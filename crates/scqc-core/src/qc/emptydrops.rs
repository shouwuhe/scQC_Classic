//! EmptyDrops —— 区分真细胞与空液滴（SPEC §8.1.2；DESIGN §5.4）。
//!
//! **依据：Lun ATL et al. (2019) "EmptyDrops: distinguishing cells from empty droplets in
//! droplet-based single-cell RNA sequencing data", Genome Biology 20:63, Methods 部分。
//! 本实现只依据论文，未阅读 DropletUtils（GPL-3）源码（SPEC §9.2）。**
//!
//! 算法：
//! 1. 总 UMI ≤ `lower` 的 barcode 汇总为环境池；用 Good–Turing 估计基因比例 p（零计数基因非零）。
//! 2. 在 Dirichlet-multinomial DM(α·p) 下计算每个候选 barcode（总 UMI > lower）的对数似然 L_b；
//!    α 由环境池 barcode 的极大似然估计。
//! 3. Monte Carlo：对总 UMI = t 的模拟计数向量计算 L，p_b = (1 + #{L_sim ≤ L_b}) / (niters + 1)。
//!    模拟采用 Pólya 罐子逐个抽取（DM 的等价生成过程），一次抽取到 t_max，
//!    途中在每个被检验的 t 处记录 L —— 所有 t 共用一次模拟。
//! 4. Benjamini–Hochberg 校正，FDR ≤ 阈值判为细胞。
//! 5. 总 UMI ≥ `retain`（knee）的 barcode 直接判为细胞，不检验。
//!
//! 似然公式（DM 概率质量函数，含多项式系数）：
//!   L = lnΓ(α) − lnΓ(α+t) + Σ_g [lnΓ(y_g + α p_g) − lnΓ(α p_g)] + lnΓ(t+1) − Σ_g lnΓ(y_g+1)
//! 罐子过程中对基因 g 加 1（y_g→y_g+1，t→t+1）的增量：
//!   ΔL = ln(α p_g + y_g) − ln(α + t) + ln(t+1) − ln(y_g+1)
//! 是否包含多项式系数由 `include_multinomial_coef` 控制（默认包含；金标准校准时可切换）。

use std::sync::atomic::{AtomicUsize, Ordering};

use rand::Rng;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use statrs::function::gamma::ln_gamma;

use crate::error::{ErrorCode, Result, ScqcError};
use crate::matrix::CountMatrix;
use crate::pipeline::cancel::CancelToken;
use crate::qc::goodturing::good_turing_proportions;
use crate::qc::knee::barcode_ranks;
use crate::rng::derive_rng;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmptyDropsParams {
    pub lower: u32,
    pub niters: u32,
    pub fdr: f64,
    /// 总 UMI ≥ retain 直接判为细胞；None 时取 knee
    pub retain: Option<u64>,
    pub seed: u64,
    pub include_multinomial_coef: bool,
}

impl Default for EmptyDropsParams {
    fn default() -> Self {
        Self {
            lower: 100,
            niters: 10_000,
            fdr: 0.01,
            retain: None,
            seed: crate::rng::DEFAULT_SEED,
            include_multinomial_coef: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdReason {
    /// 总 UMI ≥ retain，直接判为细胞
    AboveRetain,
    /// 经 Monte Carlo 检验
    Tested,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmptyDropsRow {
    /// 在 raw 矩阵中的列索引
    pub col: usize,
    pub total: u64,
    pub log_prob: f64,
    pub p_value: f64,
    pub fdr: f64,
    pub called: bool,
    pub reason: EdReason,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmptyDropsResult {
    pub params: EmptyDropsParams,
    pub retain: u64,
    pub alpha: f64,
    pub n_ambient_barcodes: usize,
    pub n_ambient_umi: u64,
    pub n_candidates: usize,
    pub n_tested: usize,
    pub n_called: usize,
    /// Good–Turing 环境表达谱 p_g（和为 1），供后续模块共用（DESIGN §5.6）
    pub ambient_profile: Vec<f64>,
    /// 候选 barcode（总 UMI > lower）的检验结果
    pub rows: Vec<EmptyDropsRow>,
}

/// 运行 EmptyDrops。`on_progress(pct)` 在 Monte Carlo 阶段每完成约 5% 调用一次。
/// `shared_profile`：流程共用的环境谱（SPEC §8.6 只计算一次）；其 `umi_upper` 必须等于 `lower`，
/// 否则本函数自行汇总。
pub fn empty_drops(
    matrix: &CountMatrix,
    params: &EmptyDropsParams,
    cancel: &CancelToken,
    on_progress: &(dyn Fn(u8) + Sync),
    shared_profile: Option<&crate::ambient::AmbientProfile>,
) -> Result<EmptyDropsResult> {
    if params.niters == 0 {
        return Err(ScqcError::new(ErrorCode::InvalidParam, "niters 必须大于 0"));
    }
    let n_genes = matrix.n_genes();
    let totals = matrix.column_sums();
    let lower = params.lower as u64;

    // ---- 1. 环境池与 Good–Turing 谱 ----
    let ambient_cols: Vec<usize> = (0..matrix.n_barcodes())
        .filter(|&j| totals[j] > 0 && totals[j] <= lower)
        .collect();
    if ambient_cols.is_empty() {
        return Err(ScqcError::new(
            ErrorCode::InputMismatch,
            format!("raw 矩阵中没有总 UMI ≤ {lower} 的空液滴，无法估计环境表达谱（是否误用了 filtered 矩阵？）"),
        ));
    }
    let (p, n_ambient_umi) = match shared_profile {
        Some(pr) if pr.umi_upper == lower && pr.good_turing.len() == n_genes => {
            (pr.good_turing.clone(), pr.total_umi)
        }
        _ => {
            let mut ambient_counts = vec![0u64; n_genes];
            let mut n_ambient_umi = 0u64;
            for &j in &ambient_cols {
                let (idx, val) = matrix.column(j);
                for (&g, &v) in idx.iter().zip(val) {
                    ambient_counts[g as usize] += v as u64;
                    n_ambient_umi += v as u64;
                }
            }
            (good_turing_proportions(&ambient_counts), n_ambient_umi)
        }
    };
    cancel.checkpoint()?;

    // ---- 2. α 的极大似然估计（环境池上）----
    let alpha = estimate_alpha(matrix, &ambient_cols, &totals, &p);
    tracing::info!(
        alpha,
        n_ambient = ambient_cols.len(),
        "EmptyDrops：环境谱与 α 估计完成"
    );
    cancel.checkpoint()?;

    // ---- 5. retain（knee）----
    let retain = match params.retain {
        Some(r) => r,
        None => barcode_ranks(&totals, lower).knee_total,
    };

    // ---- 候选 barcode 与观测似然 ----
    let candidates: Vec<usize> = (0..matrix.n_barcodes())
        .filter(|&j| totals[j] > lower)
        .collect();
    let coef = params.include_multinomial_coef;
    let log_probs: Vec<f64> = candidates
        .par_iter()
        .map(|&j| {
            let (idx, val) = matrix.column(j);
            dm_log_prob(idx, val, totals[j], alpha, &p, coef)
        })
        .collect();

    let mut rows: Vec<EmptyDropsRow> = candidates
        .iter()
        .zip(&log_probs)
        .map(|(&j, &lp)| {
            let above = totals[j] >= retain;
            EmptyDropsRow {
                col: j,
                total: totals[j],
                log_prob: lp,
                p_value: if above { 0.0 } else { f64::NAN },
                fdr: if above { 0.0 } else { f64::NAN },
                called: above,
                reason: if above {
                    EdReason::AboveRetain
                } else {
                    EdReason::Tested
                },
            }
        })
        .collect();

    // ---- 3. Monte Carlo ----
    let tested: Vec<usize> = (0..rows.len())
        .filter(|&r| rows[r].reason == EdReason::Tested)
        .collect();
    if !tested.is_empty() {
        let t_max = tested.iter().map(|&r| rows[r].total).max().unwrap_or(0) as usize;
        // total → 该 total 的被检验行索引
        let mut by_total: Vec<Vec<usize>> = vec![Vec::new(); t_max + 1];
        for &r in &tested {
            by_total[rows[r].total as usize].push(r);
        }
        let obs: Vec<f64> = rows.iter().map(|r| r.log_prob).collect();
        let niters = params.niters as usize;
        let done = AtomicUsize::new(0);
        let report_every = (niters / 20).max(1);

        // 每次迭代独立派生 RNG；计数为整数，归约顺序不影响结果（DESIGN §12）。
        let counts: Vec<u32> = (0..niters)
            .into_par_iter()
            .fold(
                || McState::new(rows.len(), n_genes),
                |mut st, iter| {
                    if cancel.is_cancelled() {
                        return st;
                    }
                    let mut rng = derive_rng(params.seed, "emptydrops", iter as u64);
                    st.tree.reset(&p, alpha);
                    st.y.fill(0);
                    let mut ll = 0.0f64;
                    for t in 0..t_max {
                        // 抽取一个基因：P(g) ∝ α p_g + y_g
                        let w_total = st.tree.total();
                        let u = rng.gen::<f64>() * w_total;
                        let g = st.tree.find(u);
                        let yg = st.y[g] as f64;
                        ll += (alpha * p[g] + yg).ln() - (alpha + t as f64).ln();
                        if coef {
                            ll += ((t + 1) as f64).ln() - (yg + 1.0).ln();
                        }
                        st.y[g] += 1;
                        st.tree.add(g, 1.0);
                        let t_now = t + 1;
                        for &r in &by_total[t_now] {
                            if ll <= obs[r] {
                                st.counts[r] += 1;
                            }
                        }
                    }
                    let d = done.fetch_add(1, Ordering::Relaxed) + 1;
                    if d.is_multiple_of(report_every) {
                        on_progress(((d * 100) / niters) as u8);
                    }
                    st
                },
            )
            .map(|st| st.counts)
            .reduce(
                || vec![0u32; rows.len()],
                |mut a, b| {
                    for (x, y) in a.iter_mut().zip(b) {
                        *x += y;
                    }
                    a
                },
            );
        cancel.checkpoint()?;

        // p 值与 BH
        for &r in &tested {
            rows[r].p_value = (1.0 + counts[r] as f64) / (niters as f64 + 1.0);
        }
        let pvals: Vec<f64> = tested.iter().map(|&r| rows[r].p_value).collect();
        let fdrs = benjamini_hochberg(&pvals);
        for (k, &r) in tested.iter().enumerate() {
            rows[r].fdr = fdrs[k];
            rows[r].called = fdrs[k] <= params.fdr;
        }
    }

    let n_called = rows.iter().filter(|r| r.called).count();
    Ok(EmptyDropsResult {
        params: params.clone(),
        retain,
        alpha,
        n_ambient_barcodes: ambient_cols.len(),
        n_ambient_umi,
        n_candidates: rows.len(),
        n_tested: tested.len(),
        n_called,
        ambient_profile: p,
        rows,
    })
}

/// 每线程的 Monte Carlo 工作区。
struct McState {
    counts: Vec<u32>,
    y: Vec<u32>,
    tree: Fenwick,
}

impl McState {
    fn new(n_rows: usize, n_genes: usize) -> Self {
        Self {
            counts: vec![0; n_rows],
            y: vec![0; n_genes],
            tree: Fenwick::new(n_genes),
        }
    }
}

/// Fenwick（二叉索引）树，支持前缀和查询与按累积权重查找，O(log G)。
struct Fenwick {
    n: usize,
    tree: Vec<f64>,
    top: usize,
}

impl Fenwick {
    fn new(n: usize) -> Self {
        let mut top = 1;
        while top * 2 <= n {
            top *= 2;
        }
        Self {
            n,
            tree: vec![0.0; n + 1],
            top,
        }
    }

    /// 重置为权重 α p_g（线性时间构建）。
    #[allow(clippy::needless_range_loop)]
    fn reset(&mut self, p: &[f64], alpha: f64) {
        for i in 1..=self.n {
            self.tree[i] = alpha * p[i - 1];
        }
        for i in 1..=self.n {
            let j = i + (i & i.wrapping_neg());
            if j <= self.n {
                self.tree[j] += self.tree[i];
            }
        }
    }

    fn add(&mut self, idx: usize, delta: f64) {
        let mut i = idx + 1;
        while i <= self.n {
            self.tree[i] += delta;
            i += i & i.wrapping_neg();
        }
    }

    fn total(&self) -> f64 {
        let mut i = self.n;
        let mut s = 0.0;
        while i > 0 {
            s += self.tree[i];
            i -= i & i.wrapping_neg();
        }
        s
    }

    /// 最小的 idx 使前缀和 > u（u < total）。
    fn find(&self, mut u: f64) -> usize {
        let mut pos = 0usize;
        let mut step = self.top;
        while step > 0 {
            let next = pos + step;
            if next <= self.n && self.tree[next] <= u {
                pos = next;
                u -= self.tree[next];
            }
            step /= 2;
        }
        pos.min(self.n - 1)
    }
}

/// DM 对数概率（见模块文档公式）。
fn dm_log_prob(idx: &[u32], val: &[u32], total: u64, alpha: f64, p: &[f64], coef: bool) -> f64 {
    let mut ll = ln_gamma(alpha) - ln_gamma(alpha + total as f64);
    if coef {
        ll += ln_gamma(total as f64 + 1.0);
    }
    for (&g, &y) in idx.iter().zip(val) {
        let ap = alpha * p[g as usize];
        ll += ln_gamma(y as f64 + ap) - ln_gamma(ap);
        if coef {
            ll -= ln_gamma(y as f64 + 1.0);
        }
    }
    ll
}

/// 在环境池 barcode 上对 α 做极大似然（黄金分割搜索 log α ∈ [ln 0.01, ln 1e5]）。
fn estimate_alpha(matrix: &CountMatrix, cols: &[usize], totals: &[u64], p: &[f64]) -> f64 {
    let ll = |log_alpha: f64| -> f64 {
        let alpha = log_alpha.exp();
        // 先并行算出每列的值，再顺序求和 → 结果与线程数无关
        let parts: Vec<f64> = cols
            .par_iter()
            .map(|&j| {
                let (idx, val) = matrix.column(j);
                dm_log_prob(idx, val, totals[j], alpha, p, false)
            })
            .collect();
        parts.iter().sum()
    };
    let (mut a, mut b) = (0.01f64.ln(), 1e5f64.ln());
    let phi = (5f64.sqrt() - 1.0) / 2.0;
    let mut c = b - phi * (b - a);
    let mut d = a + phi * (b - a);
    let (mut fc, mut fd) = (ll(c), ll(d));
    for _ in 0..40 {
        if fc > fd {
            b = d;
            d = c;
            fd = fc;
            c = b - phi * (b - a);
            fc = ll(c);
        } else {
            a = c;
            c = d;
            fc = fd;
            d = a + phi * (b - a);
            fd = ll(d);
        }
        if (b - a).abs() < 1e-4 {
            break;
        }
    }
    ((a + b) / 2.0).exp()
}

/// Benjamini–Hochberg：返回与输入同序的调整 p 值。
pub fn benjamini_hochberg(p: &[f64]) -> Vec<f64> {
    let n = p.len();
    if n == 0 {
        return Vec::new();
    }
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&i, &j| p[i].partial_cmp(&p[j]).unwrap_or(std::cmp::Ordering::Equal));
    let mut adj = vec![0.0; n];
    let mut running_min = 1.0f64;
    for k in (0..n).rev() {
        let i = order[k];
        let v = (p[i] * n as f64 / (k + 1) as f64).min(1.0);
        running_min = running_min.min(v);
        adj[i] = running_min;
    }
    adj
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::matrix::Triplet;
    use crate::pipeline::cancel::CancelToken;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    #[test]
    fn bh_matches_known_values() {
        let adj = benjamini_hochberg(&[0.01, 0.04, 0.03, 0.5]);
        // 排序 p: 0.01,0.03,0.04,0.5 → 0.04, 0.06, 0.0533→min 0.0533, 0.5；再取后缀最小
        assert!((adj[0] - 0.04).abs() < 1e-12);
        assert!((adj[2] - 0.05333333).abs() < 1e-6);
        assert!((adj[1] - 0.05333333).abs() < 1e-6);
        assert!((adj[3] - 0.5).abs() < 1e-12);
    }

    #[test]
    fn fenwick_sampling_matches_weights() {
        let p = vec![0.1, 0.2, 0.7];
        let mut f = Fenwick::new(3);
        f.reset(&p, 10.0);
        assert!((f.total() - 10.0).abs() < 1e-12);
        assert_eq!(f.find(0.5), 0);
        assert_eq!(f.find(1.5), 1);
        assert_eq!(f.find(9.99), 2);
        f.add(0, 5.0);
        assert_eq!(f.find(5.5), 0);
        assert_eq!(f.find(6.5), 1);
    }

    #[test]
    fn incremental_ll_matches_closed_form() {
        // 用罐子过程生成一个向量，增量似然应等于闭式 DM 概率
        let p = vec![0.5, 0.3, 0.2];
        let alpha = 4.0;
        let mut f = Fenwick::new(3);
        f.reset(&p, alpha);
        let mut y = [0u32; 3];
        let mut ll = 0.0;
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        for t in 0..25usize {
            let u = rng.gen::<f64>() * f.total();
            let g = f.find(u);
            ll += (alpha * p[g] + y[g] as f64).ln() - (alpha + t as f64).ln()
                + ((t + 1) as f64).ln()
                - (y[g] as f64 + 1.0).ln();
            y[g] += 1;
            f.add(g, 1.0);
        }
        let idx: Vec<u32> = (0..3).filter(|&g| y[g] > 0).map(|g| g as u32).collect();
        let val: Vec<u32> = idx.iter().map(|&g| y[g as usize]).collect();
        let closed = dm_log_prob(&idx, &val, 25, alpha, &p, true);
        assert!((ll - closed).abs() < 1e-9, "{ll} vs {closed}");
    }

    /// 合成数据：细胞有明显的基因偏好，空液滴按环境谱采样；EmptyDrops 应找回细胞、排除空液滴。
    #[test]
    fn separates_cells_from_ambient() {
        let n_genes = 200;
        let mut rng = ChaCha8Rng::seed_from_u64(7);
        let ambient: Vec<f64> = (0..n_genes).map(|g| 1.0 + (g % 7) as f64).collect();
        let s: f64 = ambient.iter().sum();
        let ambient: Vec<f64> = ambient.iter().map(|v| v / s).collect();
        let mut triplets = Vec::new();
        let mut col = 0u32;
        let sample = |rng: &mut ChaCha8Rng, w: &[f64], n: u64, col: u32, out: &mut Vec<Triplet>| {
            let mut counts = vec![0u32; w.len()];
            for _ in 0..n {
                let mut u = rng.gen::<f64>();
                let mut g = 0;
                while g + 1 < w.len() && u > w[g] {
                    u -= w[g];
                    g += 1;
                }
                counts[g] += 1;
            }
            for (g, &c) in counts.iter().enumerate() {
                if c > 0 {
                    out.push(Triplet {
                        row: g as u32,
                        col,
                        value: c,
                    });
                }
            }
        };
        // 3000 个空液滴：5–60 UMI；200 个"中等空液滴"：120–300 UMI（环境谱）
        for _ in 0..3000 {
            let n = 5 + rng.gen_range(0..56);
            sample(&mut rng, &ambient, n, col, &mut triplets);
            col += 1;
        }
        let n_empty_big = 200;
        for _ in 0..n_empty_big {
            let n = 120 + rng.gen_range(0..180);
            sample(&mut rng, &ambient, n, col, &mut triplets);
            col += 1;
        }
        // 60 个真细胞：150–400 UMI，只表达 20 个基因
        let mut cell_w = vec![0.0; n_genes];
        for w in cell_w.iter_mut().take(20) {
            *w = 0.05;
        }
        let n_cells = 60;
        let first_cell = col;
        for _ in 0..n_cells {
            let n = 150 + rng.gen_range(0..250);
            sample(&mut rng, &cell_w, n, col, &mut triplets);
            col += 1;
        }
        let m = CountMatrix::from_triplets(n_genes, col as usize, triplets).unwrap();
        let params = EmptyDropsParams {
            niters: 2000,
            retain: Some(u64::MAX),
            ..Default::default()
        };
        let res = empty_drops(&m, &params, &CancelToken::new(), &|_| {}, None).unwrap();
        let called_cells = res
            .rows
            .iter()
            .filter(|r| r.called && r.col as u32 >= first_cell)
            .count();
        let called_empty = res
            .rows
            .iter()
            .filter(|r| r.called && (r.col as u32) < first_cell)
            .count();
        assert_eq!(called_cells, n_cells, "全部真细胞都应被找回");
        assert!(called_empty <= 4, "误判的空液滴应很少：{called_empty}");
        assert!((res.ambient_profile.iter().sum::<f64>() - 1.0).abs() < 1e-9);
    }
}
