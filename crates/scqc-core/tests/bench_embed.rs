//! 5 万细胞规模的嵌入基准（SPEC §13：聚类 < 90 s）。默认忽略；运行：
//! `cargo test --release -p scqc-core --test bench_embed -- --ignored --nocapture`
#![allow(clippy::unwrap_used)]

use rand::Rng;
use scqc_core::embed::{knn, leiden, snn};
use scqc_core::rng::derive_rng;

#[test]
#[ignore]
fn bench_knn_snn_leiden_50k() {
    let n = 50_000;
    let dim = 50;
    let mut rng = derive_rng(1, "bench", 0);
    // 12 个高斯簇
    let centers: Vec<Vec<f32>> = (0..12)
        .map(|_| (0..dim).map(|_| rng.gen::<f32>() * 20.0 - 10.0).collect())
        .collect();
    let mut data = vec![0.0f32; n * dim];
    for i in 0..n {
        let c = &centers[i % 12];
        for d in 0..dim {
            data[i * dim + d] = c[d] + (rng.gen::<f32>() - 0.5) * 3.0;
        }
    }
    let t = std::time::Instant::now();
    let g = knn::knn_graph(&data, n, dim, &knn::KnnParams::default());
    let t_knn = t.elapsed().as_secs_f64();
    let t = std::time::Instant::now();
    let s = snn::snn_graph(&g, 1.0 / 15.0);
    let t_snn = t.elapsed().as_secs_f64();
    let t = std::time::Instant::now();
    let r = leiden::leiden(
        &s,
        &leiden::LeidenParams {
            resolution: 1.0,
            ..Default::default()
        },
    );
    let t_leiden = t.elapsed().as_secs_f64();
    eprintln!(
        "BENCH n={n}: knn {t_knn:.1}s, snn {t_snn:.1}s, leiden {t_leiden:.1}s → {} 簇, Q={:.3}",
        r.n_communities, r.modularity
    );
    assert!(r.n_communities >= 12);
    assert!(t_knn + t_snn + t_leiden < 90.0, "too slow");
}
