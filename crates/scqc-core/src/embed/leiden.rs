//! Leiden 社区检测（SPEC §8.6）。
//!
//! **依据：Traag VA, Waltman L, van Eck NJ (2019) "From Louvain to Leiden: guaranteeing
//! well-connected communities", Scientific Reports 9:5233，Algorithm A.1–A.4。
//! 本实现只依据论文；未阅读 leidenalg / igraph（GPL）源码（SPEC §9.2）。**
//!
//! 质量函数：带分辨率 γ 的模块度（Reichardt–Bornholdt 配置模型零模型）
//!   Q = Σ_c [ e_c / m − γ (K_c / 2m)² ]
//! 其中 e_c 为社区 c 内部边权和（每条边计一次），K_c 为社区内节点强度之和，m 为总边权。
//!
//! 三个阶段（论文 Algorithm A.1）：
//! 1. MoveNodesFast（A.2）：队列驱动的局部移动；
//! 2. RefinePartition（A.3/A.4）：在每个社区内从单点开始，只把"连接良好"的节点合并进
//!    "连接良好"的子社区（本实现取 θ→0 的贪心版本：选增益最大者）；
//! 3. 基于精化分区聚合网络，聚合网络的初始分区取自未精化的分区；重复直到不再变化。
//!
//! 随机顺序由派生 RNG 决定，结果可复现（DESIGN §12）。

use rand::seq::SliceRandom;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};

use super::snn::WeightedGraph;
use crate::rng::derive_rng;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LeidenParams {
    pub resolution: f64,
    pub seed: u64,
    pub max_rounds: usize,
}

impl Default for LeidenParams {
    fn default() -> Self {
        Self {
            resolution: 1.0,
            seed: crate::rng::DEFAULT_SEED,
            max_rounds: 20,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LeidenResult {
    /// 每个节点的社区编号（按社区大小降序重编号，从 0 开始）
    pub membership: Vec<u32>,
    pub n_communities: usize,
    pub modularity: f64,
    pub rounds: usize,
}

/// 内部工作图：CSR 邻接 + 节点强度（原图中的加权度）。
struct Graph {
    n: usize,
    ptr: Vec<usize>,
    adj: Vec<u32>,
    w: Vec<f64>,
    /// 节点强度（聚合后 = 成员强度之和）
    strength: Vec<f64>,
    /// 总边权 m（每条无向边计一次，含自环）
    m: f64,
}

impl Graph {
    fn from_weighted(g: &WeightedGraph) -> Self {
        let strength: Vec<f64> = (0..g.n)
            .map(|i| g.neighbors(i).1.iter().map(|&x| x as f64).sum())
            .collect();
        let m = strength.iter().sum::<f64>() / 2.0;
        Self {
            n: g.n,
            ptr: g.ptr.clone(),
            adj: g.adj.clone(),
            w: g.w.iter().map(|&x| x as f64).collect(),
            strength,
            m,
        }
    }

    fn neighbors(&self, i: usize) -> (&[u32], &[f64]) {
        (
            &self.adj[self.ptr[i]..self.ptr[i + 1]],
            &self.w[self.ptr[i]..self.ptr[i + 1]],
        )
    }
}

/// 分区状态：成员关系 + 社区强度和。
struct Partition {
    comm: Vec<u32>,
    k_tot: Vec<f64>,
    size: Vec<usize>,
}

impl Partition {
    fn singletons(g: &Graph) -> Self {
        Self {
            comm: (0..g.n as u32).collect(),
            k_tot: g.strength.clone(),
            size: vec![1; g.n],
        }
    }

    fn n_communities(&self) -> usize {
        self.size.iter().filter(|&&s| s > 0).count()
    }
}

/// 模块度（论文式 (1) 的 RB 形式）。
fn modularity(g: &Graph, p: &Partition, gamma: f64) -> f64 {
    if g.m <= 0.0 {
        return 0.0;
    }
    let nc = p.k_tot.len().max(p.comm.len());
    let mut e_in = vec![0.0f64; nc];
    for i in 0..g.n {
        let (nb, w) = g.neighbors(i);
        for (&j, &wij) in nb.iter().zip(w) {
            if p.comm[i] == p.comm[j as usize] {
                e_in[p.comm[i] as usize] += wij;
            }
        }
    }
    // e_in 每条内部边计了两次：e_c = e_in / 2
    let mut q = 0.0;
    for (c, &e) in e_in.iter().enumerate() {
        let k = p.k_tot.get(c).copied().unwrap_or(0.0);
        if k > 0.0 || e > 0.0 {
            q += e / (2.0 * g.m) - gamma * (k / (2.0 * g.m)).powi(2);
        }
    }
    q
}

/// Algorithm A.2：快速局部移动。返回是否有节点移动。
fn move_nodes_fast(g: &Graph, p: &mut Partition, gamma: f64, rng: &mut ChaCha8Rng) -> bool {
    let n = g.n;
    let mut order: Vec<usize> = (0..n).collect();
    order.shuffle(rng);
    let mut queue: std::collections::VecDeque<usize> = order.into_iter().collect();
    let mut in_queue = vec![true; n];
    let mut w_to: Vec<f64> = vec![0.0; p.k_tot.len().max(n)]; // 按社区编号索引
    let mut touched: Vec<u32> = Vec::new();
    let mut moved_any = false;
    let two_m = 2.0 * g.m;
    // 空社区编号池（论文 A.2：候选包含空社区）
    let mut free: Vec<usize> = (0..p.size.len()).filter(|&c| p.size[c] == 0).collect();

    while let Some(v) = queue.pop_front() {
        in_queue[v] = false;
        let c_old = p.comm[v] as usize;
        let k_v = g.strength[v];
        // 从原社区移除
        p.k_tot[c_old] -= k_v;
        p.size[c_old] -= 1;
        // 邻居社区权重
        touched.clear();
        let (nb, w) = g.neighbors(v);
        for (&u, &wvu) in nb.iter().zip(w) {
            let u = u as usize;
            if u == v {
                continue;
            }
            let c = p.comm[u];
            if w_to[c as usize] == 0.0 {
                touched.push(c);
            }
            w_to[c as usize] += wvu;
        }
        // 候选：原社区（留下）、各邻居社区、空社区（增益 0）
        let gain = |c: usize| -> f64 { w_to[c] - gamma * k_v * p.k_tot[c] / two_m };
        let mut best_c = c_old;
        let mut best_gain = gain(c_old);
        // 空社区候选：增益 0（原社区已空时留下即等价于空社区）
        let mut use_empty = false;
        if p.size[c_old] > 0 && best_gain < -1e-12 {
            best_gain = 0.0;
            use_empty = true;
        }
        // 按社区编号顺序遍历 touched，保证并列时选择确定
        touched.sort_unstable();
        for &c in &touched {
            let c = c as usize;
            let gn = gain(c);
            if gn > best_gain + 1e-12 {
                best_gain = gn;
                best_c = c;
                use_empty = false;
            }
        }
        if use_empty {
            best_c = match free.pop() {
                Some(c) => c,
                None => c_old, // 无空编号可用（理论上不会发生）：留在原社区
            };
        }
        p.comm[v] = best_c as u32;
        p.k_tot[best_c] += k_v;
        p.size[best_c] += 1;
        if best_c != c_old {
            moved_any = true;
            if p.size[c_old] == 0 {
                free.push(c_old);
            }
            for &u in nb {
                let u = u as usize;
                if u != v && p.comm[u] as usize != best_c && !in_queue[u] {
                    in_queue[u] = true;
                    queue.push_back(u);
                }
            }
        }
        for &c in &touched {
            w_to[c as usize] = 0.0;
        }
    }
    moved_any
}

/// Algorithm A.3/A.4：精化分区。返回精化后的分区（社区编号为原图节点编号空间内的值）。
fn refine_partition(g: &Graph, p: &Partition, gamma: f64, rng: &mut ChaCha8Rng) -> Partition {
    let n = g.n;
    let two_m = 2.0 * g.m;
    let mut r = Partition::singletons(g);
    // 按社区分组
    let mut members: Vec<Vec<usize>> = vec![Vec::new(); p.k_tot.len().max(n)];
    for v in 0..n {
        members[p.comm[v] as usize].push(v);
    }
    // 精化社区的内部边权 / 成员 e_u 之和
    let mut int_w = vec![0.0f64; n];
    let mut sum_e = vec![0.0f64; n];
    let mut w_to: Vec<f64> = vec![0.0; n];
    let mut touched: Vec<u32> = Vec::new();

    for s_members in members.iter().filter(|m| m.len() > 1) {
        let k_s: f64 = s_members.iter().map(|&v| g.strength[v]).sum();
        let in_s = |u: usize, comm_id: u32| p.comm[u] == comm_id;
        let s_id = p.comm[s_members[0]];
        // e_v = E(v, S − v)
        let e_of: Vec<f64> = s_members
            .iter()
            .map(|&v| {
                let (nb, w) = g.neighbors(v);
                nb.iter()
                    .zip(w)
                    .filter(|(&u, _)| u as usize != v && in_s(u as usize, s_id))
                    .map(|(_, &wvu)| wvu)
                    .sum()
            })
            .collect();
        for (i, &v) in s_members.iter().enumerate() {
            sum_e[v] = e_of[i];
        }
        let mut order: Vec<usize> = (0..s_members.len()).collect();
        order.shuffle(rng);
        for &i in &order {
            let v = s_members[i];
            if r.size[r.comm[v] as usize] != 1 {
                continue; // 只考虑仍为单点的节点
            }
            let k_v = g.strength[v];
            // 节点 v 与 S−v 连接良好？
            if e_of[i] < gamma * k_v * (k_s - k_v) / two_m {
                continue;
            }
            // v 到各精化社区（S 内）的边权
            touched.clear();
            let (nb, w) = g.neighbors(v);
            for (&u, &wvu) in nb.iter().zip(w) {
                let u = u as usize;
                if u == v || !in_s(u, s_id) {
                    continue;
                }
                let c = r.comm[u];
                if w_to[c as usize] == 0.0 {
                    touched.push(c);
                }
                w_to[c as usize] += wvu;
            }
            touched.sort_unstable();
            let c_self = r.comm[v] as usize;
            let mut best_c = c_self;
            let mut best_gain = 0.0f64; // 留在单点：增益 0
            for &c in &touched {
                let c = c as usize;
                if c == c_self {
                    continue;
                }
                // 子社区 T 与 S−T 连接良好？E(T,S−T) = Σ_{u∈T} e_u − 2·int_w[T]
                let k_t = r.k_tot[c];
                let e_t = sum_e[c] - 2.0 * int_w[c];
                if e_t < gamma * k_t * (k_s - k_t) / two_m {
                    continue;
                }
                let gn = w_to[c] - gamma * k_v * k_t / two_m;
                if gn >= 0.0 && gn > best_gain + 1e-12 {
                    best_gain = gn;
                    best_c = c;
                }
            }
            if best_c != c_self {
                // 移动 v：从单点社区 c_self 到 best_c
                r.k_tot[c_self] -= k_v;
                r.size[c_self] -= 1;
                r.comm[v] = best_c as u32;
                r.k_tot[best_c] += k_v;
                r.size[best_c] += 1;
                int_w[best_c] += w_to[best_c];
                sum_e[best_c] += e_of[i];
            }
            for &c in &touched {
                w_to[c as usize] = 0.0;
            }
        }
    }
    r
}

/// 聚合：精化分区的每个社区成为新节点。返回 (新图, 旧节点→新节点映射, 新图初始分区)。
fn aggregate(g: &Graph, refined: &Partition, p: &Partition) -> (Graph, Vec<u32>, Partition) {
    let n = g.n;
    // 精化社区重编号为 0..r
    let mut new_id = vec![u32::MAX; n];
    let mut next = 0u32;
    let mut map = vec![0u32; n];
    for (mv, &rc) in map.iter_mut().zip(&refined.comm) {
        let c = rc as usize;
        if new_id[c] == u32::MAX {
            new_id[c] = next;
            next += 1;
        }
        *mv = new_id[c];
    }
    let r = next as usize;
    let mut strength = vec![0.0f64; r];
    let mut init_comm = vec![0u32; r];
    for (v, &mv) in map.iter().enumerate() {
        strength[mv as usize] += g.strength[v];
        init_comm[mv as usize] = p.comm[v];
    }
    // 边聚合（含自环，保留以维持 m）
    let mut edges: Vec<(u32, u32, f64)> = Vec::new();
    for (v, &mv) in map.iter().enumerate() {
        let (nb, w) = g.neighbors(v);
        for (&u, &wvu) in nb.iter().zip(w) {
            edges.push((mv, map[u as usize], wvu));
        }
    }
    edges.sort_by_key(|e| (e.0, e.1));
    let mut ptr = vec![0usize; r + 1];
    let mut adj = Vec::with_capacity(edges.len());
    let mut wv = Vec::with_capacity(edges.len());
    let mut i = 0;
    while i < edges.len() {
        let (a, b, _) = edges[i];
        let mut s = 0.0;
        while i < edges.len() && edges[i].0 == a && edges[i].1 == b {
            s += edges[i].2;
            i += 1;
        }
        ptr[a as usize + 1] += 1;
        adj.push(b);
        wv.push(s);
    }
    for k in 0..r {
        ptr[k + 1] += ptr[k];
    }
    let m = g.m;
    // 未精化分区在新图上的 k_tot：按社区编号（沿用旧编号空间，长度取 n）
    let mut k_tot = vec![0.0f64; p.k_tot.len().max(r)];
    let mut size = vec![0usize; p.k_tot.len().max(r)];
    for a in 0..r {
        k_tot[init_comm[a] as usize] += strength[a];
        size[init_comm[a] as usize] += 1;
    }
    (
        Graph {
            n: r,
            ptr,
            adj,
            w: wv,
            strength,
            m,
        },
        map,
        Partition {
            comm: init_comm,
            k_tot,
            size,
        },
    )
}

pub fn leiden(graph: &WeightedGraph, params: &LeidenParams) -> LeidenResult {
    let n0 = graph.n;
    if n0 == 0 {
        return LeidenResult {
            membership: vec![],
            n_communities: 0,
            modularity: 0.0,
            rounds: 0,
        };
    }
    let mut rng = derive_rng(params.seed, "leiden", (params.resolution * 1000.0) as u64);
    let gamma = params.resolution;
    let g0 = Graph::from_weighted(graph);
    let mut g = Graph::from_weighted(graph);
    let mut p = Partition::singletons(&g);
    let mut node_to_agg: Vec<u32> = (0..n0 as u32).collect();
    let mut rounds = 0;

    loop {
        rounds += 1;
        let _moved = move_nodes_fast(&g, &mut p, gamma, &mut rng);
        let nc = p.n_communities();
        if nc == g.n || rounds >= params.max_rounds {
            break; // 每个社区只含一个（聚合）节点 → 收敛
        }
        let refined = refine_partition(&g, &p, gamma, &mut rng);
        let (g2, map, p2) = aggregate(&g, &refined, &p);
        for x in node_to_agg.iter_mut() {
            *x = map[*x as usize];
        }
        g = g2;
        p = p2;
    }
    // 回写到原始节点并按社区大小降序重编号
    let raw: Vec<u32> = node_to_agg.iter().map(|&a| p.comm[a as usize]).collect();
    let mut count: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    for &c in &raw {
        *count.entry(c).or_insert(0) += 1;
    }
    let mut order: Vec<(u32, usize)> = count.into_iter().collect();
    order.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let mut renum = std::collections::HashMap::new();
    for (i, (c, _)) in order.iter().enumerate() {
        renum.insert(*c, i as u32);
    }
    let membership: Vec<u32> = raw.iter().map(|c| renum[c]).collect();
    let final_p = {
        let mut k_tot = vec![0.0f64; order.len()];
        let mut size = vec![0usize; order.len()];
        for v in 0..n0 {
            k_tot[membership[v] as usize] += g0.strength[v];
            size[membership[v] as usize] += 1;
        }
        Partition {
            comm: membership.clone(),
            k_tot,
            size,
        }
    };
    let q = modularity(&g0, &final_p, gamma);
    LeidenResult {
        membership,
        n_communities: order.len(),
        modularity: q,
        rounds,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 两个稠密团 + 少量桥接边 → 应分成两个社区。
    #[test]
    fn two_cliques() {
        let mut edges = Vec::new();
        for a in 0..2u32 {
            for i in 0..10u32 {
                for j in i + 1..10u32 {
                    edges.push((a * 10 + i, a * 10 + j, 1.0f32));
                }
            }
        }
        edges.push((0, 10, 0.5));
        edges.push((5, 15, 0.5));
        let g = WeightedGraph::from_edges(20, &edges);
        let r = leiden(
            &g,
            &LeidenParams {
                resolution: 1.0,
                ..Default::default()
            },
        );
        assert_eq!(r.n_communities, 2, "{:?}", r.membership);
        for i in 0..10 {
            assert_eq!(r.membership[i], r.membership[0]);
            assert_eq!(r.membership[10 + i], r.membership[10]);
        }
        assert!(r.modularity > 0.3, "Q={}", r.modularity);
        // 可复现
        let r2 = leiden(
            &g,
            &LeidenParams {
                resolution: 1.0,
                ..Default::default()
            },
        );
        assert_eq!(r.membership, r2.membership);
    }

    /// 分辨率越高，社区越多（或相等）。
    #[test]
    fn resolution_monotone() {
        // 4 个团，团间弱连接
        let mut edges = Vec::new();
        for a in 0..4u32 {
            for i in 0..8u32 {
                for j in i + 1..8u32 {
                    edges.push((a * 8 + i, a * 8 + j, 1.0f32));
                }
            }
        }
        for a in 0..3u32 {
            edges.push((a * 8, (a + 1) * 8, 2.0));
            edges.push((a * 8 + 1, (a + 1) * 8 + 1, 2.0));
        }
        let g = WeightedGraph::from_edges(32, &edges);
        let lo = leiden(
            &g,
            &LeidenParams {
                resolution: 0.1,
                ..Default::default()
            },
        );
        let hi = leiden(
            &g,
            &LeidenParams {
                resolution: 1.5,
                ..Default::default()
            },
        );
        assert!(
            lo.n_communities <= hi.n_communities,
            "{} vs {}",
            lo.n_communities,
            hi.n_communities
        );
        assert_eq!(hi.n_communities, 4);
    }
}
