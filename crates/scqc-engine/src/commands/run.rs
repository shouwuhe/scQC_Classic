//! `scqc-engine run` —— 全流程（SPEC §8.7）。
//!
//! 已实现：步骤 1（读取）、2（平台识别）、2b（内存预检）、3（真细胞鉴定）、4（聚类）、5（环境 RNA 诊断）、
//! 6（ddqc）、7（双细胞）、8（可选校正）、9（输出）。

use std::collections::HashMap;
use std::io::BufRead;
use std::path::Path;

use scqc_core::ambient::{
    self, AmbientInputs, AmbientProfile, CorrectedMatrix, CorrectionMethod, CorrectionState,
    DecontXParams, Rating, SoupParams, Verdict,
};
use scqc_core::doublet::{
    run_doublet, CxdsParams, DoubletClass, DoubletFinderParams, DoubletInputs, DoubletOutput,
    DoubletParams,
};
use scqc_core::embed::{build_embedding, EmbedParams, Embedding};
use scqc_core::io::BarcodeTable;
use scqc_core::io::{writer, Dataset};
use scqc_core::matrix::CountMatrix;
use scqc_core::pipeline::cancel::CancelToken;
use scqc_core::pipeline::cellcalling::{run_cell_calling, CellCallingMode, CellCallingOutput};
use scqc_core::pipeline::memory::{self, MemoryInput};
use scqc_core::pipeline::params::{RunParams, RunRecord};
use scqc_core::pipeline::plots::PlotWriter;
use scqc_core::pipeline::{Event, EventSink, Progress, Step};
use scqc_core::platform::CellCalling;
use scqc_core::platform::PlatformProfile;
use scqc_core::qc::{
    compute_basic_metrics, ddqc, BasicMetrics, DdqcParams, DdqcResult, EdReason, EmptyDropsRow,
};
use scqc_core::report::i18n::Lang as ReportLang;
use scqc_core::report::ReportDoc;
use scqc_core::{ErrorCode, Result, ScqcError};

use crate::cli::RunArgs;
use crate::progress::NdjsonSink;

pub fn run(args: RunArgs) -> Result<()> {
    if args.progress_format != "ndjson" {
        return Err(ScqcError::new(
            ErrorCode::InvalidParam,
            "--progress-format 目前只支持 ndjson",
        ));
    }
    let mut params = match &args.params {
        Some(p) => RunParams::from_file(p)?,
        None => RunParams::default(),
    };
    if let Some(seed) = args.seed {
        params.seed = seed;
    }
    if args.threads > 0 {
        params.threads = args.threads;
    }
    if let Some(c) = &args.correct {
        params.correction.method = c.trim().to_ascii_lowercase();
    }
    if args.force_correct {
        params.correction.force = true;
    }
    params.validate()?;
    if params.threads > 0 {
        // 线程数只影响速度，不影响结果（DESIGN §12）。
        let _ = rayon::ThreadPoolBuilder::new()
            .num_threads(params.threads)
            .build_global();
    }

    // 输出目录先建（C7：不覆盖）
    let out = args.output.as_path();
    writer::create_output_dir(out)?;

    // 结果目录只放"人要看要用"的东西：报告、矩阵、逐细胞指标、本次参数。
    // 各步骤的中间产物与界面用的绘图数据全部收进 details/，免得结果目录一打开几十个文件。
    let detail_dir = out.join("details");
    std::fs::create_dir_all(&detail_dir).map_err(|e| ScqcError::io_write(&detail_dir, &e))?;

    let sink = NdjsonSink::new();
    let progress = Progress::new(&sink);
    let cancel = CancelToken::new();
    let answers = spawn_stdin_listener(cancel.clone());
    let started_at = now_utc();
    let t0 = std::time::Instant::now();

    // ---- 1. 读取 ----
    progress.report(Step::Read, 0, "读取 filtered / raw 矩阵");
    let ds = super::load_dataset(&args.input)?;
    cancel.checkpoint()?;
    progress.report(Step::Read, 100, "读取完成");
    if let Some(ng) = &ds.non_gex {
        progress.warn("NON_GEX_FEATURES_EXCLUDED", ng.note_zh.clone());
        tracing::info!(
            "非基因表达特征已排除：{} / {} 个（占细胞 UMI {:.1}%、空液滴 UMI {:.1}%）",
            ng.n_features_dropped,
            ng.n_features_total,
            ng.umi_fraction_filtered * 100.0,
            ng.umi_fraction_raw * 100.0
        );
    }
    if let Some(al) = &ds.feature_alignment {
        progress.warn("FEATURE_SET_ALIGNED", al.note_zh.clone());
        tracing::info!(
            "基因列表对齐：raw {} 个、filtered {} 个 → 交集 {} 个；raw 丢弃 UMI {}（{:.2}%）",
            al.n_raw,
            al.n_filtered,
            al.n_common,
            al.raw_dropped_umi,
            al.raw_dropped_umi_fraction * 100.0
        );
    }
    if ds.raw.is_none() {
        progress.warn(
            "RAW_MISSING",
            "缺少 raw 矩阵，真细胞鉴定与环境 RNA 诊断不可用（SPEC §6.1）",
        );
    }
    let t_read = t0.elapsed().as_secs_f64();

    // ---- 2. 平台识别 ----
    progress.report(Step::Platform, 0, "识别平台");
    let (det, mut profile) = super::resolve_profile(&ds, &args.input, &sink)?;
    if let Some(g) = args.input.mito_guardrail {
        if !(0.0..=1.0).contains(&g) {
            return Err(ScqcError::new(
                ErrorCode::InvalidParam,
                "--mito-guardrail 必须在 0 与 1 之间",
            ));
        }
        if profile.mito_guardrail != Some(g) {
            progress.warn(
                "MITO_GUARDRAIL_OVERRIDDEN",
                format!(
                    "线粒体阈值护栏由用户指定为 {:.2}%（Profile 原值 {}）",
                    g * 100.0,
                    profile
                        .mito_guardrail
                        .map(|v| format!("{:.2}%", v * 100.0))
                        .unwrap_or_else(|| "无".into())
                ),
            );
            profile.mito_guardrail = Some(g);
            profile
                .user_overrides
                .push(scqc_core::platform::detect::CodedText::new(
                    "override_mito_guardrail",
                    [],
                ));
        }
    }
    writer::write_json(&detail_dir.join("platform_profile.json"), &profile)?;
    writer::write_json(&detail_dir.join("detection.json"), &det)?;
    writer::write_json(&detail_dir.join("raw_input_summary.json"), &ds.summary())?;
    progress.report(
        Step::Platform,
        100,
        format!("平台：{}", profile.id.label_zh()),
    );

    // ---- 2b. 内存预检 ----
    let mem_in = MemoryInput {
        raw_nnz: ds.raw.as_ref().map(|r| r.matrix.nnz()).unwrap_or(0),
        raw_barcodes: ds.raw.as_ref().map(|r| r.matrix.n_barcodes()).unwrap_or(0),
        filtered_nnz: ds.filtered.matrix.nnz(),
        n_cells: ds.filtered.matrix.n_barcodes(),
        n_genes: ds.filtered.matrix.n_genes(),
        n_hvg: params.embed.n_hvg,
    };
    let mem = memory::precheck(&mem_in)?;
    tracing::info!(
        "内存预检通过：预估 {:.2} GB，可用 {:.2} GB",
        mem.estimated_bytes as f64 / 1e9,
        mem.available_bytes as f64 / 1e9
    );
    writer::write_json(&detail_dir.join("memory_estimate.json"), &mem)?;

    // ---- 共用环境表达谱（SPEC §8.6 只计算一次）----
    let ambient_profile: Option<AmbientProfile> = ds.raw.as_ref().map(|raw| {
        let lower = match profile.cell_calling {
            CellCalling::EmptyDrops { lower } => params.cell_calling.lower.unwrap_or(lower),
            CellCalling::TrustUpstream => 100,
        };
        let upper = params.ambient.umi_upper_bound.unwrap_or(lower) as u64;
        let totals = raw.matrix.column_sums();
        AmbientProfile::from_raw(&raw.matrix, &totals, upper)
    });
    if let Some(ap) = &ambient_profile {
        tracing::info!(
            "环境表达谱：{} 个空液滴（总 UMI ≤ {}），合计 {} UMI",
            ap.n_droplets,
            ap.umi_upper,
            ap.total_umi
        );
    }

    // ---- 3. 真细胞鉴定 ----
    progress.report(Step::CellCalling, 0, "真细胞鉴定");
    let t1 = std::time::Instant::now();
    let cc = run_cell_calling(
        &ds,
        &profile,
        &params,
        ambient_profile.as_ref(),
        &cancel,
        &progress,
    )?;
    let t_cellcalling = t1.elapsed().as_secs_f64();
    writer::write_json(&detail_dir.join("cell_calling.json"), &cc.summary)?;
    write_deltas(&detail_dir.join("cells_rescued.csv"), &cc.rescued)?;
    write_deltas(&detail_dir.join("cells_lost.csv"), &cc.lost)?;
    if cc.summary.n_rescued > 0 {
        tracing::info!(
            "Cell Ranger 判空、本工具判细胞：{} 个（基因数中位数 {}，高表达 {}）",
            cc.summary.n_rescued,
            cc.summary.rescued_median_genes,
            cc.summary.rescued_top_genes.join("/")
        );
    }
    cancel.checkpoint()?;

    // ---- 细胞矩阵（本工具判定的细胞集合，raw 计数）----
    let cells_matrix = cells_count_matrix(&ds, &cc)?;

    // ---- 4. 聚类（精细 + 粗，两套分辨率，只算一次）----
    progress.report(Step::Cluster, 0, "聚类");
    let t2 = std::time::Instant::now();
    let embed_params = EmbedParams {
        n_hvg: params.embed.n_hvg,
        n_pcs: params.embed.n_pcs,
        k: params.embed.k,
        res_fine: params.embed.res_fine,
        res_coarse: params.embed.res_coarse,
        seed: params.seed,
        ..Default::default()
    };
    let gene_candidates = ds.filtered.features.gene_expression_indices();
    let embedding = build_embedding(
        &cells_matrix,
        &gene_candidates,
        &embed_params,
        &cancel,
        &|pct, msg| progress.report(Step::Cluster, pct, msg),
    )?;
    let t_cluster = t2.elapsed().as_secs_f64();
    let embed_summary = embedding.summary(&embed_params);
    if embedding.coarse.n_communities > 100 {
        progress.warn(
            "CLUSTERING_FRAGMENTED",
            format!(
                "粗聚类得到 {} 个簇（通常为 10–30 个）。这通常意味着数据里有大量高度相似的细胞群或批次结构；\
                 环境 RNA 估计按簇分组计算，簇数过多会降低估计质量并显著增加内存占用",
                embedding.coarse.n_communities
            ),
        );
    }
    writer::write_json(&detail_dir.join("embedding.json"), &embed_summary)?;
    tracing::info!(
        "聚类完成：HVG {}，PC {}，精细 {} 簇（Q={:.3}），粗 {} 簇（Q={:.3}），耗时 {:.1} s",
        embed_summary.n_hvg,
        embed_summary.n_pcs,
        embed_summary.fine_n_clusters,
        embed_summary.fine_modularity,
        embed_summary.coarse_n_clusters,
        embed_summary.coarse_modularity,
        t_cluster
    );
    cancel.checkpoint()?;

    // ---- 5. 环境 RNA 诊断（只诊断，不校正）----
    let mut t_ambient = 0.0;
    let ambient_out = match &ambient_profile {
        Some(ap) if !ap.is_empty() => {
            progress.report(Step::Ambient, 0, "环境 RNA 诊断");
            let t3 = std::time::Instant::now();
            let mito_genes = profile.species.rule().gene_sets(&ds.filtered.features).mito;
            let inputs = AmbientInputs {
                cells: &cells_matrix,
                coarse_cluster: &embedding.coarse.membership,
                profile: ap,
                gene_names: &ds.filtered.features.name,
                mito_genes: &mito_genes,
                estimators: &profile.ambient_estimators,
                reference: profile.ambient_reference,
                decontx_params: DecontXParams {
                    max_iter: params.ambient.decontx_max_iter as usize,
                    tol: params.ambient.decontx_tol,
                    ..Default::default()
                },
                soup_params: SoupParams {
                    excl_factor: params.ambient.soup_excl_factor,
                    ..Default::default()
                },
            };
            let res = ambient::run_ambient(&inputs, &cancel, &|pct, msg| {
                progress.report(Step::Ambient, pct, msg)
            })?;
            t_ambient = t3.elapsed().as_secs_f64();
            writer::write_json(&detail_dir.join("ambient_estimates.json"), &res.estimates)?;
            let c = &res.estimates.consensus;
            let fmt_rho = |v: Option<f64>| {
                v.map(|x| format!("{:.1}%", x * 100.0))
                    .unwrap_or_else(|| "不可用".into())
            };
            tracing::info!(
                "环境 RNA：SoupX 式 {}，DecontX {}，线粒体法 {}；判定 {:?}，评级 {:?}，校正 {:?}",
                fmt_rho(c.rho_soup),
                fmt_rho(c.rho_decontx),
                fmt_rho(c.rho_mito),
                c.verdict,
                c.rating,
                c.correction
            );
            if res.soup.as_ref().map(|s| s.at_boundary).unwrap_or(false) {
                progress.warn(
                    "AMBIENT_ESTIMATOR_UNRELIABLE",
                    "SoupX 式估计的后验众数落在搜索区间端点，该估计不可信，已不参与一致性判定；\
                     本次仅以 DecontX 的结果为准",
                );
            }
            if res.decontx.as_ref().map(|d| !d.converged).unwrap_or(false) {
                progress.warn(
                    "AMBIENT_DECONTX_NOT_CONVERGED",
                    format!(
                        "DecontX 在 {} 次迭代内未收敛，污染度估计可能不稳定（可增大 ambient.decontx_max_iter 重跑）",
                        params.ambient.decontx_max_iter
                    ),
                );
            }
            if c.verdict == Verdict::Disagree {
                progress.warn(
                    "AMBIENT_METHODS_DISAGREE",
                    format!(
                        "SoupX 式估计 {}，DecontX 估计 {}，分歧显著（相对差 {:.0}%）",
                        fmt_rho(c.rho_soup),
                        fmt_rho(c.rho_decontx),
                        c.relative_diff.unwrap_or(0.0) * 100.0
                    ),
                );
            }
            if c.rating == ambient::Rating::Severe {
                progress.warn(
                    "AMBIENT_SEVERE",
                    "污染度显著高于参考范围（> 20%），建议核查样本解离流程与细胞活率，如条件允许考虑重新制备文库",
                );
            }
            Some(res)
        }
        _ => {
            progress.warn(
                "AMBIENT_SKIPPED",
                "缺少 raw 矩阵或空液滴，环境 RNA 诊断不可用",
            );
            None
        }
    };
    cancel.checkpoint()?;

    // ---- 6. 低质量细胞过滤（ddqc，平台修正由 Profile 字段给出）----
    progress.report(Step::Ddqc, 0, "低质量细胞过滤（ddqc）");
    let t4 = std::time::Instant::now();
    let gene_sets = profile.species.rule().gene_sets(&ds.filtered.features);
    let cell_metrics = compute_basic_metrics(&cells_matrix, &gene_sets);
    let ddqc_params = DdqcParams {
        dims: profile.ddqc_dimensions.clone(),
        mad_multiplier: params.ddqc.mad_multiplier,
        coarse_min_genes: params.ddqc.coarse_min_genes,
        coarse_max_mito: params.ddqc.coarse_max_mito,
        mito_upper_bound: profile.mito_guardrail,
        mito_min_diff: profile.mito_mad_min_diff,
        fixed_thresholds_are_hard: params.ddqc.fixed_thresholds_are_hard,
        fixed_min_genes: params.ddqc.fixed_min_genes,
        fixed_max_mito: params.ddqc.fixed_max_mito,
        ..Default::default()
    };
    if !profile.uses_ribo() {
        tracing::info!("ddqc：percent.ribo 维度已由 Platform Profile 禁用");
    }
    if profile.mito_guardrail.is_none() {
        tracing::info!("ddqc：线粒体维度不加护栏（Profile 未标定），仅用簇内自适应阈值");
    }
    let ddqc_res = ddqc(&cell_metrics, &embedding.fine.membership, &ddqc_params);
    let t_ddqc = t4.elapsed().as_secs_f64();
    writer::write_json(&detail_dir.join("ddqc.json"), &ddqc_res)?;
    tracing::info!(
        "ddqc：{} 个细胞中剔除 {}（粗过滤 {}；其中固定阈值硬下限拦下 {}）；\
         单用固定阈值会剔除 {}；自适应阈值比固定阈值多保留 {} 个（基因数中位数 {}）",
        ddqc_res.n_cells,
        ddqc_res.n_removed_total,
        ddqc_res.n_removed_coarse,
        ddqc_res.n_removed_by_fixed,
        ddqc_res.fixed_n_removed,
        ddqc_res.n_extra_kept,
        ddqc_res.n_extra_kept_median_genes
    );
    // 真实 Flex 数据暴露的问题：线粒体比例被探针集压低到 0.1% 量级时，
    // 「中位数 + 2×MAD」会落在噪声里，把健康细胞当成低质量细胞删掉（SPEC §7.2.4"未标定"的后果）。
    {
        use scqc_core::platform::DdqcDim;
        let mito_removed = ddqc_res
            .removed_by_dim
            .iter()
            .find(|(d, _)| *d == DdqcDim::Mito)
            .map(|(_, n)| *n)
            .unwrap_or(0);
        let noisy: Vec<&scqc_core::qc::ddqc::ClusterThreshold> = ddqc_res
            .thresholds
            .iter()
            .filter(|t| t.dim == DdqcDim::Mito && t.n_removed > 0 && t.threshold < 0.01)
            .collect();
        if mito_removed > 0 && !noisy.is_empty() {
            let worst = noisy
                .iter()
                .map(|t| t.threshold)
                .fold(f64::INFINITY, f64::min);
            let n_noisy: usize = noisy.iter().map(|t| t.n_removed).sum();
            progress.warn(
                "DDQC_MITO_THRESHOLD_AT_NOISE",
                format!(
                    "有 {} 个簇的线粒体阈值低于 1%（最低 {:.3}%），据此剔除了 {} 个细胞。\
                     线粒体比例在本数据里整体极低时，「中位数 + MAD」会落在噪声量级，可能误删健康细胞；\
                     建议用 --mito-guardrail 设定一个合理下限（如 0.05 = 5%）后重跑对比",
                    noisy.len(),
                    worst * 100.0,
                    n_noisy
                ),
            );
        }
    }
    progress.report(
        Step::Ddqc,
        100,
        format!("剔除 {} 个低质量细胞", ddqc_res.n_removed_total),
    );

    // ---- 过滤状态：对 cells_matrix 各列的保留标记与剔除记录 ----
    let mut filt = FilterState::new(cells_matrix.n_barcodes());
    let cell_barcodes: Vec<String> = cells_barcodes(&ds, &cc);
    for (j, fail) in ddqc_res.fail.iter().enumerate() {
        if let Some(f) = fail {
            let dim = format!("{:?}", f.dim).to_lowercase();
            let detail = if f.coarse {
                format!(
                    "粗过滤：{dim} {} {}",
                    f.comparator,
                    fmt_thr(f.dim, f.threshold)
                )
            } else {
                format!(
                    "簇 {} 内 {dim} 偏离中位数 {} 超过 {}×MAD（阈值 {}，观测 {}）",
                    f.cluster.map(|c| c.to_string()).unwrap_or_default(),
                    fmt_thr(f.dim, f.median),
                    params.ddqc.mad_multiplier,
                    fmt_thr(f.dim, f.threshold),
                    fmt_thr(f.dim, f.observed)
                )
            };
            filt.remove(
                j,
                Removal {
                    barcode: cell_barcodes[j].clone(),
                    step: "ddqc".into(),
                    metric: dim,
                    comparator: f.comparator.clone(),
                    threshold: f.threshold,
                    observed: f.observed,
                    cluster: f.cluster.map(|c| format!("fine_{c}")).unwrap_or_default(),
                    detail_zh: detail,
                },
            );
        }
    }
    cancel.checkpoint()?;

    // ---- 7. 双细胞检测（在低质量过滤之后，每个样本独立；SPEC §8.5.5）----
    progress.report(Step::Doublet, 0, "双细胞检测（cxds + DoubletFinder）");
    let t5 = std::time::Instant::now();
    let kept_after_ddqc: Vec<usize> = (0..cells_matrix.n_barcodes())
        .filter(|&j| filt.keep[j])
        .collect();
    let dbl_matrix = cells_matrix.select_columns(&kept_after_ddqc)?;
    // 样本分组：多重化时按 Probe Barcode 分组，否则为单一样本
    let (dbl_sample, dbl_labels) = sample_groups(&kept_after_ddqc, &cell_barcodes);
    let n_loaded = params.doublet.n_loaded.unwrap_or(kept_after_ddqc.len());
    let mut rate_profile = profile.clone();
    if let Some(r) = params.doublet.rate_per_1000 {
        rate_profile.doublet_rate_per_1000 = r;
    }
    let expected_rate = rate_profile.expected_doublet_rate(n_loaded);
    tracing::info!(
        "预期双细胞率 {:.4}（{}/1000 × n_loaded {} ÷ 多重化重数 {}）；样本数 {}",
        expected_rate,
        rate_profile.doublet_rate_per_1000,
        n_loaded,
        profile.multiplex_n.unwrap_or(1),
        dbl_labels.len()
    );
    if expected_rate > 0.15 {
        progress.warn(
            "DOUBLET_RATE_HIGH",
            format!(
                "按当前 n_loaded（{}）算出的预期双细胞率为 {:.1}%，明显高于常规单通道实验。\
                 若本样本是多个上机通道合并而来，请把 n_loaded 改为**单个通道**的上样细胞数，否则会误删大量真细胞",
                n_loaded,
                expected_rate * 100.0
            ),
        );
    }
    let dbl_params = DoubletParams {
        cxds: CxdsParams {
            ntop: params.doublet.ntop,
            ..Default::default()
        },
        df: DoubletFinderParams {
            p_n: params.doublet.p_n,
            n_hvg: params.embed.n_hvg,
            n_pcs: params.embed.n_pcs,
            seed: params.seed,
            ..Default::default()
        },
    };
    let dbl = run_doublet(
        &DoubletInputs {
            cells: &dbl_matrix,
            gene_candidates: &gene_candidates,
            sample: &dbl_sample,
            sample_labels: &dbl_labels,
            expected_rate,
        },
        &dbl_params,
        &cancel,
        &|pct, msg| progress.report(Step::Doublet, pct, msg),
    )?;
    let t_doublet = t5.elapsed().as_secs_f64();
    writer::write_json(&detail_dir.join("doublet.json"), &dbl.summary)?;
    tracing::info!(
        "双细胞：高置信 {}（剔除）、边界 {}（{}）、cxds 判定 {}、DoubletFinder 判定 {}、一致性 Jaccard {:.3}；耗时 {:.1} s",
        dbl.summary.n_high_confidence,
        dbl.summary.n_borderline,
        if params.doublet.remove_borderline { "剔除" } else { "保留并标注" },
        dbl.summary.cxds_n_called,
        dbl.summary.df_n_called,
        dbl.summary.agreement_jaccard,
        t_doublet
    );
    for (k, &j) in kept_after_ddqc.iter().enumerate() {
        let cls = dbl.class[k];
        let remove = match cls {
            DoubletClass::HighConfidence => true,
            DoubletClass::Borderline => params.doublet.remove_borderline,
            DoubletClass::Singlet => false,
        };
        if remove {
            let sample = if dbl_labels.len() > 1 {
                dbl_labels[dbl_sample[k] as usize].clone()
            } else {
                String::new()
            };
            filt.remove(
                j,
                Removal {
                    barcode: cell_barcodes[j].clone(),
                    step: "doublet".into(),
                    metric: if cls == DoubletClass::HighConfidence {
                        "doublet_class".into()
                    } else {
                        "doublet_class_borderline".into()
                    },
                    comparator: "==".into(),
                    threshold: expected_rate,
                    observed: dbl.df_pann[k],
                    cluster: sample,
                    detail_zh: if cls == DoubletClass::HighConfidence {
                        format!(
                            "cxds 与 DoubletFinder 均判为双细胞（高置信）；cxds 得分 {:.1}，pANN {:.3}",
                            dbl.cxds_score[k], dbl.df_pann[k]
                        )
                    } else {
                        format!(
                            "仅一个方法判为双细胞（边界，按参数剔除）；cxds {}，DoubletFinder {}",
                            if dbl.cxds_call[k] { "双细胞" } else { "单细胞" },
                            if dbl.df_call[k] { "双细胞" } else { "单细胞" }
                        )
                    },
                },
            );
        }
    }
    progress.report(
        Step::Doublet,
        100,
        format!(
            "高置信双细胞 {} 个，边界 {} 个",
            dbl.summary.n_high_confidence, dbl.summary.n_borderline
        ),
    );
    cancel.checkpoint()?;

    // ---- 细胞表与基础指标 ----
    let dbl_by_pos: HashMap<usize, usize> = kept_after_ddqc
        .iter()
        .enumerate()
        .map(|(k, &j)| (j, k))
        .collect();
    let table = build_cell_table(
        &ds,
        &profile,
        &cc,
        &embedding,
        ambient_out.as_ref(),
        &ddqc_res,
        &filt,
        &dbl,
        &dbl_by_pos,
    )?;
    write_qc_metrics(&out.join("qc_metrics.csv"), &table, profile.show_ribo)?;

    // ---- 8. 环境 RNA 校正（可选，单一方法，条件触发；C8）----
    let final_cols: Vec<usize> = (0..cells_matrix.n_barcodes())
        .filter(|&j| filt.keep[j])
        .collect();
    let mut t_correction = 0.0;

    // 第 1–7 步到此为止。开了 --await-correction 就在这里停下来问一次：
    // 前七步是确定性的，为了改"要不要校正"这一个决定而重跑一遍，跑出来的必然是同样的结果。
    // 所以停在这里等答复，而不是让界面重新起一次进程（SPEC §11.3）。
    if args.await_correction {
        if let Some(ao) = ambient_out.as_ref() {
            let c = &ao.estimates.consensus;
            let facts = serde_json::json!({
                "ambient_available": true,
                "rho_soup": ao.estimates.soup.as_ref().map(|x| x.rho_global),
                "rho_soup_unreliable": ao
                    .estimates
                    .soup
                    .as_ref()
                    .map(|x| x.at_boundary)
                    .unwrap_or(false),
                "rho_decontx": ao.estimates.decontx.as_ref().map(|x| x.rho_global),
                "rho_mito": ao
                    .estimates
                    .mito
                    .as_ref()
                    .filter(|x| x.usable)
                    .map(|x| x.rho_global),
                "ambient_verdict": verdict_id(c.verdict),
                "ambient_rating": rating_id(c.rating),
                "ambient_correction_state": correction_state_id(c.correction),
                "ambient_reference_mean": c.reference.map(|r| r.mean),
                "ambient_reference_grey_below": c.reference.map(|r| r.grey_below),
                "ambient_reference_discourage_above": c.reference.map(|r| r.discourage_above),
                "ambient_top_genes": ao
                    .estimates
                    .top_affected_genes
                    .iter()
                    .take(12)
                    .map(|g| serde_json::json!({"gene": g.gene, "fraction": g.ambient_fraction}))
                    .collect::<Vec<_>>(),
            });
            progress.sink().emit(Event::AwaitCorrection { facts });
            match answers.recv() {
                Ok(a) => {
                    params.correction.method = a.method;
                    params.correction.force = a.force;
                }
                // stdin 关了（命令行里直接跑、或界面崩了）：按"不校正"继续，不要卡死
                Err(_) => {
                    tracing::info!("等待校正决定时 stdin 已关闭，按不校正继续");
                    params.correction.method = "none".into();
                }
            }
            cancel.checkpoint()?;
        } else {
            tracing::info!("无环境表达谱，跳过校正决定询问");
        }
    }

    let corrected: Option<CorrectedMatrix> = match CorrectionMethod::parse(
        &params.correction.method,
    ) {
        None => {
            tracing::info!("步骤 8：未启用环境 RNA 校正（correction.method = none）");
            None
        }
        Some(method) => {
            progress.report(
                Step::Correction,
                0,
                format!("环境 RNA 校正（{}）", method.label_zh()),
            );
            let t6 = std::time::Instant::now();
            let res = run_correction(
                method,
                &params,
                &cells_matrix,
                &final_cols,
                &embedding,
                ambient_profile.as_ref(),
                ambient_out.as_ref(),
                &progress,
            );
            t_correction = t6.elapsed().as_secs_f64();
            if let Some(c) = &res {
                writer::write_json(&detail_dir.join("correction.json"), &c.summary)?;
                tracing::info!(
                    "校正完成：{}，ρ 均值 {:.4}，总 UMI {:.0} → {:.0}（扣除 {:.2}%），Top 基因 {}；耗时 {:.1} s",
                    c.summary.method_zh,
                    c.summary.rho_mean,
                    c.summary.total_before,
                    c.summary.total_after,
                    c.summary.removed_fraction * 100.0,
                    c.summary
                        .top_genes
                        .iter()
                        .take(5)
                        .map(|g| ds.filtered.features.name[g.gene_index].clone())
                        .collect::<Vec<_>>()
                        .join("/"),
                    t_correction
                );
                progress.report(
                    Step::Correction,
                    100,
                    format!("扣除 {:.2}% 的 UMI", c.summary.removed_fraction * 100.0),
                );
            }
            res
        }
    };
    cancel.checkpoint()?;

    // ---- 9. 输出 ----
    progress.report(Step::Output, 0, "写出矩阵、绘图数据与参数");
    // 绘图数据：knee 曲线
    let mut plots = PlotWriter::new(&detail_dir)?;
    let knee_data: Vec<f32> = cc
        .knee
        .curve_rank
        .iter()
        .zip(&cc.knee.curve_total)
        .flat_map(|(r, t)| [*r as f32, *t as f32])
        .collect();
    plots.write_f32(
        "knee_curve",
        &["rank", "total_umi"],
        cc.knee.curve_rank.len(),
        &knee_data,
        "barcode-rank 曲线（唯一 total 折叠后的平均秩与总 UMI）",
    )?;
    plots.write_f32(
        "pca",
        &(0..embedding.pca.k)
            .map(|i| format!("PC{}", i + 1))
            .collect::<Vec<_>>()
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>(),
        embedding.n_cells,
        &embedding.pca.scores,
        "本工具判定细胞的主成分得分（行序与 cells_passed.txt 一致）",
    )?;
    // 绘图数据：逐细胞指标（界面画分布图用；比解析 qc_metrics.csv 快得多）
    // 列：总 UMI / 基因数 / 线粒体比例 / 核糖体比例 / 本工具判为细胞 / 通过全部过滤
    let m = &table.metrics;
    let mut qc_data: Vec<f32> = Vec::with_capacity(table.rows.len() * 6);
    for (i, r) in table.rows.iter().enumerate() {
        qc_data.push(m.total_umi[i] as f32);
        qc_data.push(m.n_genes[i] as f32);
        qc_data.push(m.pct_mito[i] as f32);
        qc_data.push(if profile.show_ribo {
            m.pct_ribo[i] as f32
        } else {
            f32::NAN
        });
        qc_data.push(if r.final_status == "not_called" {
            0.0
        } else {
            1.0
        });
        qc_data.push(if r.final_status == "passed" { 1.0 } else { 0.0 });
    }
    plots.write_f32(
        "qc_metrics",
        &[
            "total_umi",
            "n_genes",
            "pct_mito",
            "pct_ribo",
            "is_cell",
            "passed",
        ],
        table.rows.len(),
        &qc_data,
        "逐细胞质控指标（行序与 qc_metrics.csv 一致）",
    )?;
    plots.finish()?;

    // 通过 / 剔除列表与清洗后矩阵
    let passed: Vec<String> = table
        .rows
        .iter()
        .filter(|r| r.final_status == "passed")
        .map(|r| r.barcode.clone())
        .collect();
    writer::write_lines(&detail_dir.join("cells_passed.txt"), &passed)?;
    let mut removed =
        vec!["barcode,step,metric,comparator,threshold,observed,cluster,detail_zh".to_string()];
    for r in &table.rows {
        if r.final_status == "removed" && r.ed.as_ref().map(|e| !e.called).unwrap_or(false) {
            let (fdr, thr) = (
                r.ed.as_ref().map(|e| e.fdr).unwrap_or(f64::NAN),
                params.cell_calling.fdr,
            );
            removed.push(format!(
                "{},cell_calling,emptydrops_fdr,>,{thr:.6},{fdr:.6},,EmptyDrops 未拒绝「来自环境」的假设（FDR {fdr:.4} > {thr}）判为空液滴；Cell Ranger 曾判为细胞",
                r.barcode
            ));
        }
    }
    for rm in &filt.removals {
        removed.push(format!(
            "{},{},{},{},{},{},{},{}",
            rm.barcode,
            rm.step,
            rm.metric,
            rm.comparator,
            fmt_f(rm.threshold),
            fmt_f(rm.observed),
            rm.cluster,
            rm.detail_zh.replace(',', "，")
        ));
    }
    writer::write_lines(&detail_dir.join("cells_removed.csv"), &removed)?;
    let kept_cols: Vec<usize> = (0..cells_matrix.n_barcodes())
        .filter(|&j| filt.keep[j])
        .collect();
    let final_matrix = cells_matrix.select_columns(&kept_cols)?;
    let final_called: Vec<usize> = kept_cols.iter().map(|&j| cc.called_cols[j]).collect();
    write_final_matrix(
        out,
        &ds,
        &cc,
        &passed,
        &final_called,
        corrected.as_ref(),
        &params,
        &progress,
    )?;

    let record = RunRecord {
        scqc_version: scqc_core::VERSION.to_string(),
        git_hash: option_env!("SCQC_GIT_HASH")
            .unwrap_or("unknown")
            .to_string(),
        started_at_utc: started_at,
        platform_id: profile.id.as_str().to_string(),
        params: params.clone(),
    };
    writer::write_json(&out.join("run_params.json"), &record)?;
    // 结果目录的说明。只写英文：这份目录会整个发给下游和合作者。
    writer::write_lines(
        &out.join("README.txt"),
        &[
            format!("scQC {} - single-cell QC results.", scqc_core::VERSION),
            String::new(),
            "report.html       Standalone report. Open in any browser; nothing else is required."
                .to_string(),
            "matrix/           Filtered count matrix (Matrix Market). This is the output to use              downstream."
                .to_string(),
            "qc_metrics.csv    Per-cell metrics for every barcode that was called a cell."
                .to_string(),
            "run_params.json   Every parameter used, for reproducing this run.".to_string(),
            "details/          Per-step results, the barcode lists behind each decision, and the              plot data read by the scQC desktop app."
                .to_string(),
            String::new(),
            "Input files were never modified.".to_string(),
        ],
    )?;

    // ---- 报告（SPEC §12 九节；report_data.json 供逐数核对）----
    progress.report(Step::Output, 60, "生成报告");
    let final_metrics = compute_basic_metrics(&final_matrix, &gene_sets);
    let n_after_ddqc = kept_after_ddqc.len();
    let n_removed_doublet = filt.removals.iter().filter(|r| r.step == "doublet").count();
    let n_removed_ddqc = filt.removals.iter().filter(|r| r.step == "ddqc").count();
    let n_removed_cc = table
        .rows
        .iter()
        .filter(|r| {
            r.final_status == "removed" && r.ed.as_ref().map(|e| !e.called).unwrap_or(false)
        })
        .count();
    let title = args
        .output
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "sample".into());
    let params_json = serde_json::to_string_pretty(&record).unwrap_or_else(|_| "{}".to_string());
    let report_inputs = super::report_build::ReportInputs {
        title,
        ds: &ds,
        profile: &profile,
        record: &record,
        cc: &cc,
        embedding: &embedding,
        ambient: ambient_out.as_ref(),
        ambient_skipped_reason: if ds.raw.is_none() {
            "未提供 raw 矩阵，无法构建环境表达谱，环境 RNA 诊断不可用。".into()
        } else {
            "raw 矩阵中没有可用的空液滴，环境 RNA 诊断不可用。".into()
        },
        ddqc: &ddqc_res,
        doublet: &dbl,
        n_loaded,
        rate_per_1000: rate_profile.doublet_rate_per_1000,
        corrected: corrected.as_ref(),
        correction_forced: corrected.is_some()
            && !matches!(
                ambient_out
                    .as_ref()
                    .map(|a| a.estimates.consensus.correction),
                Some(CorrectionState::Available) | Some(CorrectionState::Unrated)
            ),
        final_metrics: &final_metrics,
        n_after_ddqc,
        n_after_doublet: n_after_ddqc - n_removed_doublet,
        n_final: passed.len(),
        n_removed_by_step: vec![
            (
                "真细胞鉴定（Cell Ranger 细胞被判为空液滴）".into(),
                n_removed_cc,
            ),
            ("低质量细胞过滤（ddqc）".into(), n_removed_ddqc),
            ("双细胞（高置信）".into(), n_removed_doublet),
        ],
        warnings: sink.warnings(),
        elapsed_s: t0.elapsed().as_secs_f64(),
        params_json,
    };
    // 报告按"英语 + 用户语言"输出两份完整正文（SPEC §12）；
    // report_data.json 取英语那一份，它是给机器逐数核对的，语言不该影响字段。
    let second = ReportLang::parse(&args.report_lang).unwrap_or(ReportLang::En);
    let report_data = super::report_build::build(&report_inputs, ReportLang::En);
    writer::write_json(&detail_dir.join("report_data.json"), &report_data)?;
    let mut docs = vec![ReportDoc::new(ReportLang::En, report_data.clone())];
    if second != ReportLang::En {
        docs.push(ReportDoc::new(
            second,
            super::report_build::build(&report_inputs, second),
        ));
    }
    let html = scqc_core::report::render_html_multi(&docs)?;
    std::fs::write(out.join("report.html"), html)
        .map_err(|e| ScqcError::io_write(&out.join("report.html"), &e))?;
    progress.report(Step::Output, 100, "完成");

    let summary = serde_json::json!({
        "platform": profile.id.as_str(),
        "species": profile.species.label_zh(),
        "n_barcodes_raw": ds.raw.as_ref().map(|r| r.matrix.n_barcodes()),
        "n_cellranger_cells": ds.filtered.matrix.n_barcodes(),
        "n_genes": ds.filtered.matrix.n_genes(),
        "cell_calling": &cc.summary,
        "n_passed": passed.len(),
        "median_umi": table.metrics.median_total_umi(),
        "median_pct_mito": table.metrics.median_pct_mito(),
        "clustering": &embed_summary,
        "ambient": ambient_out.as_ref().map(|a| &a.estimates.consensus),
        "ddqc": {"n_cells": ddqc_res.n_cells, "n_removed": ddqc_res.n_removed_total, "n_removed_coarse": ddqc_res.n_removed_coarse,
                 "fixed_n_removed": ddqc_res.fixed_n_removed, "n_extra_kept": ddqc_res.n_extra_kept, "removed_by_dim": &ddqc_res.removed_by_dim},
        "doublet": {"expected_rate": dbl.summary.expected_rate, "n_samples": dbl.summary.n_samples, "n_high_confidence": dbl.summary.n_high_confidence,
                    "n_borderline": dbl.summary.n_borderline, "cxds_n_called": dbl.summary.cxds_n_called, "df_n_called": dbl.summary.df_n_called,
                    "agreement_jaccard": dbl.summary.agreement_jaccard, "remove_borderline": params.doublet.remove_borderline},
        "correction": corrected.as_ref().map(|c| serde_json::json!({"method": c.method.as_str(), "rho_mean": c.summary.rho_mean,
                    "total_before": c.summary.total_before, "total_after": c.summary.total_after, "removed_fraction": c.summary.removed_fraction})),
        "timing_s": {"read": t_read, "cell_calling": t_cellcalling, "cluster": t_cluster, "ambient": t_ambient, "ddqc": t_ddqc, "doublet": t_doublet, "correction": t_correction, "total": t0.elapsed().as_secs_f64()},
        "steps_implemented": ["read", "platform", "memory_precheck", "cell_calling", "cluster", "ambient", "ddqc", "doublet", "correction", "output"],
    });
    sink.emit(Event::Result {
        output_dir: out.display().to_string(),
        summary,
    });
    Ok(())
}

// ---------------------------------------------------------------------------
// 过滤状态与剔除记录（cells_removed.csv 的内存形式）
// ---------------------------------------------------------------------------

struct Removal {
    barcode: String,
    step: String,
    metric: String,
    comparator: String,
    threshold: f64,
    observed: f64,
    cluster: String,
    detail_zh: String,
}

struct FilterState {
    /// 对 cells_matrix 各列：是否仍保留
    keep: Vec<bool>,
    removals: Vec<Removal>,
}

impl FilterState {
    fn new(n: usize) -> Self {
        Self {
            keep: vec![true; n],
            removals: Vec::new(),
        }
    }

    fn remove(&mut self, j: usize, r: Removal) {
        if self.keep[j] {
            self.keep[j] = false;
            self.removals.push(r);
        }
    }
}

fn fmt_thr(dim: scqc_core::platform::DdqcDim, v: f64) -> String {
    use scqc_core::platform::DdqcDim;
    if v.is_nan() {
        return String::new();
    }
    match dim {
        DdqcDim::Mito | DdqcDim::Ribo => format!("{:.2}%", v * 100.0),
        _ => format!("{v:.0}"),
    }
}

/// cells_matrix 各列对应的 barcode。
fn cells_barcodes(ds: &Dataset, cc: &CellCallingOutput) -> Vec<String> {
    let barcodes = match (&ds.raw, cc.summary.mode) {
        (Some(raw), CellCallingMode::EmptyDrops | CellCallingMode::TrustUpstream) => &raw.barcodes,
        _ => &ds.filtered.barcodes,
    };
    cc.called_cols
        .iter()
        .map(|&c| barcodes.barcode[c].clone())
        .collect()
}

/// 双细胞检测的样本分组：barcode 含 Probe Barcode（多重化）时按其分组，否则单一样本。
/// 返回（每个 `cols` 位置的样本编号，样本标签）。
fn sample_groups(cols: &[usize], barcodes: &[String]) -> (Vec<u32>, Vec<String>) {
    let probes: Vec<Option<String>> = cols
        .iter()
        .map(|&j| BarcodeTable::parse_probe_barcode(&barcodes[j]))
        .collect();
    if probes.iter().any(|p| p.is_none()) {
        return (vec![0; cols.len()], vec!["sample".to_string()]);
    }
    let mut labels: Vec<String> = probes.iter().flatten().cloned().collect();
    labels.sort_unstable();
    labels.dedup();
    let idx: HashMap<&str, u32> = labels
        .iter()
        .enumerate()
        .map(|(i, l)| (l.as_str(), i as u32))
        .collect();
    let sample = probes
        .iter()
        .map(|p| p.as_deref().and_then(|l| idx.get(l).copied()).unwrap_or(0))
        .collect();
    (sample, labels)
}

// ---------------------------------------------------------------------------
// 细胞表：qc_metrics.csv 的内存形式（DESIGN §10.3）
// ---------------------------------------------------------------------------

struct EdCols {
    log_prob: f64,
    p_value: f64,
    fdr: f64,
    called: bool,
    reason: EdReason,
}

struct CellRow {
    barcode: String,
    cellranger_called: bool,
    ed: Option<EdCols>,
    cluster_fine: Option<u32>,
    cluster_coarse: Option<u32>,
    rho_soup: Option<f64>,
    rho_decontx: Option<f64>,
    rho_mito: Option<f64>,
    ddqc_pass: Option<bool>,
    ddqc_fail_dim: String,
    cxds_score: Option<f64>,
    cxds_call: Option<bool>,
    df_pann: Option<f64>,
    df_call: Option<bool>,
    doublet_class: Option<DoubletClass>,
    /// "passed" | "removed" | "not_called"（raw 中候选但两边都不判为细胞）
    final_status: &'static str,
}

struct CellTable {
    rows: Vec<CellRow>,
    metrics: BasicMetrics,
}

/// 表的范围：EmptyDrops 模式下 = raw 中 total > lower 的全部候选（含 CR 细胞）；
/// 其他模式 = 被判为细胞的 barcode。
#[allow(clippy::too_many_arguments)]
fn build_cell_table(
    ds: &Dataset,
    profile: &PlatformProfile,
    cc: &CellCallingOutput,
    emb: &Embedding,
    amb: Option<&ambient::AmbientOutput>,
    dq: &DdqcResult,
    filt: &FilterState,
    dbl: &DoubletOutput,
    dbl_by_pos: &HashMap<usize, usize>,
) -> Result<CellTable> {
    // 被判定细胞的列索引 → 在 cells_matrix / 聚类中的位置
    let cluster_of: HashMap<usize, usize> = cc
        .called_cols
        .iter()
        .enumerate()
        .map(|(pos, &c)| (c, pos))
        .collect();
    let cr_set: std::collections::HashSet<&str> = ds
        .filtered
        .barcodes
        .barcode
        .iter()
        .map(|s| s.as_str())
        .collect();
    let sets = profile.species.rule();

    let (cols, matrix, barcodes, ed_by_col): (
        Vec<usize>,
        &scqc_core::matrix::CountMatrix,
        &[String],
        HashMap<usize, &EmptyDropsRow>,
    ) = match (cc.summary.mode, &ds.raw, &cc.emptydrops) {
        (CellCallingMode::EmptyDrops, Some(raw), Some(ed)) => {
            let map: HashMap<usize, &EmptyDropsRow> = ed.rows.iter().map(|r| (r.col, r)).collect();
            let mut cols: Vec<usize> = ed.rows.iter().map(|r| r.col).collect();
            cols.sort_unstable();
            (cols, &raw.matrix, &raw.barcodes.barcode, map)
        }
        (CellCallingMode::TrustUpstream, Some(raw), _) => (
            cc.called_cols.clone(),
            &raw.matrix,
            &raw.barcodes.barcode,
            HashMap::new(),
        ),
        _ => (
            cc.called_cols.clone(),
            &ds.filtered.matrix,
            &ds.filtered.barcodes.barcode,
            HashMap::new(),
        ),
    };
    let sub = matrix.select_columns(&cols)?;
    let gene_sets = sets.gene_sets(&ds.filtered.features);
    let metrics = compute_basic_metrics(&sub, &gene_sets);
    let called: std::collections::HashSet<usize> = cc.called_cols.iter().copied().collect();

    let rows = cols
        .iter()
        .map(|&c| {
            let bc = barcodes[c].clone();
            let in_cr = cr_set.contains(bc.as_str());
            let ed = ed_by_col.get(&c).map(|r| EdCols {
                log_prob: r.log_prob,
                p_value: r.p_value,
                fdr: r.fdr,
                called: r.called,
                reason: r.reason,
            });
            let is_called = called.contains(&c);
            let pos = cluster_of.get(&c).copied();
            let (ddqc_pass, ddqc_fail_dim) = match pos {
                Some(p) => (
                    Some(dq.pass[p]),
                    dq.fail[p]
                        .as_ref()
                        .map(|f| format!("{:?}", f.dim).to_lowercase())
                        .unwrap_or_default(),
                ),
                None => (None, String::new()),
            };
            let (cluster_fine, cluster_coarse, rho_soup, rho_decontx, rho_mito) =
                match cluster_of.get(&c) {
                    Some(&pos) => (
                        Some(emb.fine.membership[pos]),
                        Some(emb.coarse.membership[pos]),
                        amb.and_then(|a| a.soup.as_ref()).map(|s| s.rho_global),
                        amb.and_then(|a| a.decontx.as_ref())
                            .map(|d| d.rho_per_cell[pos]),
                        amb.and_then(|a| a.mito.as_ref())
                            .filter(|m| m.usable)
                            .map(|m| m.rho_per_cell[pos]),
                    ),
                    None => (None, None, None, None, None),
                };
            let dk = pos.and_then(|p| dbl_by_pos.get(&p).copied());
            let (cxds_score, cxds_call, df_pann, df_call, doublet_class) = match dk {
                Some(k) => (
                    Some(dbl.cxds_score[k]),
                    Some(dbl.cxds_call[k]),
                    Some(dbl.df_pann[k]),
                    Some(dbl.df_call[k]),
                    Some(dbl.class[k]),
                ),
                None => (None, None, None, None, None),
            };
            let final_status = if is_called {
                match pos {
                    Some(p) if !filt.keep[p] => "removed",
                    _ => "passed",
                }
            } else if in_cr {
                "removed"
            } else {
                "not_called"
            };
            CellRow {
                barcode: bc,
                cellranger_called: in_cr,
                ed,
                cluster_fine,
                cluster_coarse,
                rho_soup,
                rho_decontx,
                rho_mito,
                ddqc_pass,
                ddqc_fail_dim,
                cxds_score,
                cxds_call,
                df_pann,
                df_call,
                doublet_class,
                final_status,
            }
        })
        .collect();
    Ok(CellTable { rows, metrics })
}

fn fmt_f(v: f64) -> String {
    if v.is_nan() {
        String::new()
    } else {
        format!("{v:.6}")
    }
}

fn write_qc_metrics(path: &Path, t: &CellTable, show_ribo: bool) -> Result<()> {
    let mut lines = Vec::with_capacity(t.rows.len() + 1);
    lines.push(
        "barcode,total_umi,n_genes,pct_mito,pct_ribo,cellranger_called,ed_logprob,ed_pvalue,ed_fdr,ed_called,ed_reason,cluster_fine,cluster_coarse,rho_soup,rho_decontx,rho_mito,ddqc_pass,ddqc_fail_dim,cxds_score,cxds_call,df_pann,df_call,doublet_class,final_status"
            .to_string(),
    );
    for (i, r) in t.rows.iter().enumerate() {
        let ribo = if show_ribo {
            fmt_f(t.metrics.pct_ribo[i])
        } else {
            String::new()
        };
        let (lp, pv, fdr, called, reason) = match &r.ed {
            Some(e) => (
                fmt_f(e.log_prob),
                fmt_f(e.p_value),
                fmt_f(e.fdr),
                e.called.to_string(),
                match e.reason {
                    EdReason::AboveRetain => "above_retain",
                    EdReason::Tested => "tested",
                }
                .to_string(),
            ),
            None => (
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
            ),
        };
        let cf = r.cluster_fine.map(|c| c.to_string()).unwrap_or_default();
        let cc = r.cluster_coarse.map(|c| c.to_string()).unwrap_or_default();
        let opt = |v: Option<f64>| v.map(fmt_f).unwrap_or_default();
        let (rs, rd, rm) = (opt(r.rho_soup), opt(r.rho_decontx), opt(r.rho_mito));
        let dp = r.ddqc_pass.map(|b| b.to_string()).unwrap_or_default();
        let optb = |v: Option<bool>| v.map(|b| b.to_string()).unwrap_or_default();
        let (cs, cc_call, pann, dfc) = (
            opt(r.cxds_score),
            optb(r.cxds_call),
            opt(r.df_pann),
            optb(r.df_call),
        );
        let dclass = r.doublet_class.map(|c| c.as_str()).unwrap_or_default();
        lines.push(format!(
            "{},{},{},{},{ribo},{},{lp},{pv},{fdr},{called},{reason},{cf},{cc},{rs},{rd},{rm},{dp},{},{cs},{cc_call},{pann},{dfc},{dclass},{}",
            r.barcode,
            t.metrics.total_umi[i],
            t.metrics.n_genes[i],
            fmt_f(t.metrics.pct_mito[i]),
            r.cellranger_called,
            r.ddqc_fail_dim,
            r.final_status
        ));
    }
    writer::write_lines(path, &lines)
}

fn write_deltas(path: &Path, v: &[scqc_core::pipeline::cellcalling::CellDelta]) -> Result<()> {
    let mut lines = vec!["barcode,total_umi,n_genes,ed_fdr".to_string()];
    for c in v {
        lines.push(format!(
            "{},{},{},{}",
            c.barcode,
            c.total_umi,
            c.n_genes,
            fmt_f(c.fdr)
        ));
    }
    writer::write_lines(path, &lines)
}

/// 本工具判定的细胞集合的计数矩阵（列序 = `cc.called_cols`）。
fn cells_count_matrix(ds: &Dataset, cc: &CellCallingOutput) -> Result<CountMatrix> {
    let matrix = match (&ds.raw, cc.summary.mode) {
        (Some(raw), CellCallingMode::EmptyDrops | CellCallingMode::TrustUpstream) => &raw.matrix,
        _ => &ds.filtered.matrix,
    };
    matrix.select_columns(&cc.called_cols)
}

/// `filtered/`：本工具判定的细胞集合（C7：新目录，不动输入）。
/// 被判定细胞列（原矩阵列索引）对应的 barcode 表。
fn subset_barcodes(ds: &Dataset, cc: &CellCallingOutput, cols: &[usize]) -> BarcodeTable {
    let barcodes = match (&ds.raw, cc.summary.mode) {
        (Some(raw), CellCallingMode::EmptyDrops | CellCallingMode::TrustUpstream) => &raw.barcodes,
        _ => &ds.filtered.barcodes,
    };
    BarcodeTable {
        barcode: cols.iter().map(|&c| barcodes.barcode[c].clone()).collect(),
        probe_barcode: cols
            .iter()
            .map(|&c| barcodes.probe_barcode[c].clone())
            .collect(),
    }
}

/// 步骤 8：条件触发的单一方法校正。返回 None 表示跳过（已发 warning）。
#[allow(clippy::too_many_arguments)]
fn run_correction(
    method: CorrectionMethod,
    params: &RunParams,
    cells_matrix: &CountMatrix,
    final_cols: &[usize],
    embedding: &Embedding,
    ambient_profile: Option<&AmbientProfile>,
    ambient_out: Option<&ambient::AmbientOutput>,
    progress: &Progress<'_>,
) -> Option<CorrectedMatrix> {
    let (Some(ap), Some(ao)) = (ambient_profile, ambient_out) else {
        progress.warn(
            "CORRECTION_SKIPPED",
            "未提供 raw 矩阵，无环境表达谱，无法校正；已跳过",
        );
        return None;
    };
    let state = ao.estimates.consensus.correction;
    let allowed = matches!(state, CorrectionState::Available | CorrectionState::Unrated);
    if !allowed && !params.correction.force {
        progress.warn(
            "CORRECTION_SKIPPED",
            format!(
                "步骤 5 的建议为「{:?}」（{}），未执行校正；如确需校正请显式加 --force-correct，并在结果解读中注明",
                state, ao.estimates.consensus.reason_zh
            ),
        );
        return None;
    }
    if !allowed {
        progress.warn(
            "CORRECTION_FORCED",
            format!(
                "步骤 5 的建议为「{:?}」（{}），用户强制执行校正；校正结果的可信度下降，报告已注明",
                state, ao.estimates.consensus.reason_zh
            ),
        );
    }
    if state == CorrectionState::Unrated {
        tracing::info!("该平台无环境 RNA 参考基准，校正与否由用户判断（SPEC §8.3.2）");
    }
    let sub = match cells_matrix.select_columns(final_cols) {
        Ok(m) => m,
        Err(e) => {
            progress.warn("CORRECTION_SKIPPED", e.message_zh);
            return None;
        }
    };
    let cluster: Vec<u32> = final_cols
        .iter()
        .map(|&j| embedding.coarse.membership[j])
        .collect();
    match method {
        CorrectionMethod::Soup => {
            let Some(soup) = ao.soup.as_ref().filter(|s| s.rho_global.is_finite()) else {
                progress.warn(
                    "CORRECTION_SKIPPED",
                    "SoupX 式估计不可用，无法做 SoupX 式校正",
                );
                return None;
            };
            let rho = vec![soup.rho_global; final_cols.len()];
            Some(ambient::correct::correct_soup(&sub, &cluster, ap, &rho))
        }
        CorrectionMethod::DecontX => {
            let Some(dx) = ao.decontx.as_ref() else {
                progress.warn(
                    "CORRECTION_SKIPPED",
                    "DecontX 估计不可用，无法做 DecontX 式校正",
                );
                return None;
            };
            // 只取最终保留细胞的 θ；簇谱在这些细胞上重算
            let est = scqc_core::ambient::DecontXResult {
                rho_per_cell: final_cols.iter().map(|&j| dx.rho_per_cell[j]).collect(),
                ..dx.clone()
            };
            let dxp = DecontXParams {
                seed: params.seed,
                ..Default::default()
            };
            Some(ambient::correct::correct_decontx(
                &sub, &cluster, ap, &est, &dxp,
            ))
        }
    }
}

/// **唯一的结果矩阵**（`matrix/`）：最终保留的细胞 × 输入里的全部特征类型。
///
/// 以前这里写三份（`filtered/`、`filtered_all_features/`、`decontaminated/`），
/// 打开结果目录的人分不清该拿哪一份。现在只留一份：
///   * 全部特征类型都在（抗体捕获、HTO 不参与质控，但按同一批细胞原值带回，C10 不变）；
///   * 执行了环境 RNA 清理时，Gene Expression 那部分写清理后的计数，非 GEX 特征不动。
///
/// 未清理的计数没有丢：输入矩阵一个字节没动（C7），`details/cells_passed.txt` 给出这批
/// barcode，据此可以从输入里还原出未清理的版本。
///
/// 重读 filtered 目录而不是全程留着两份矩阵，是为了不让峰值内存翻倍。
/// 被 scQC 从 raw 里找回的细胞不在 Cell Ranger 的 filtered 矩阵里，拿不到它们的抗体计数；
/// 这种情况如实写进 README 与警告，而不是悄悄少写几列。
#[allow(clippy::too_many_arguments)]
fn write_final_matrix(
    out: &Path,
    ds: &Dataset,
    cc: &CellCallingOutput,
    passed: &[String],
    final_called: &[usize],
    corrected: Option<&CorrectedMatrix>,
    params: &RunParams,
    progress: &Progress<'_>,
) -> Result<()> {
    let dir = out.join("matrix");

    // 没有非 GEX 特征时不必重读整个目录：质控用的那份矩阵就是全部特征
    if ds.non_gex.is_none() {
        let bc = subset_barcodes(ds, cc, final_called);
        match corrected {
            Some(c) => writer::write_real_matrix_dir(
                &dir,
                c.n_genes,
                c.n_cells,
                &c.entries,
                &ds.filtered.features,
                &bc,
                params.output.gzip,
                if c.summary.integer_output { 0 } else { 3 },
            )?,
            None => {
                let sub = ds.filtered.matrix.select_columns(final_called)?;
                writer::write_matrix_dir(
                    &dir,
                    &sub,
                    &ds.filtered.features,
                    &bc,
                    params.output.gzip,
                )?
            }
        }
        return write_matrix_readme(&dir, ds, corrected, 0);
    }

    let full = scqc_core::io::read_matrix_dir(&ds.filtered.dir)?;
    let index: HashMap<&str, usize> = full
        .barcodes
        .barcode
        .iter()
        .enumerate()
        .map(|(i, b)| (b.as_str(), i))
        .collect();
    let mut cols = Vec::with_capacity(passed.len());
    let mut missing = 0usize;
    for b in passed {
        match index.get(b.as_str()) {
            Some(&j) => cols.push(j),
            None => missing += 1,
        }
    }
    if cols.is_empty() {
        return Ok(());
    }
    let sub = full.matrix.select_columns(&cols)?;
    let bc = BarcodeTable {
        barcode: cols
            .iter()
            .map(|&c| full.barcodes.barcode[c].clone())
            .collect(),
        probe_barcode: cols
            .iter()
            .map(|&c| full.barcodes.probe_barcode[c].clone())
            .collect(),
    };

    match corrected {
        None => writer::write_matrix_dir(&dir, &sub, &full.features, &bc, params.output.gzip)?,
        Some(c) => {
            let row_of: HashMap<&str, u32> = full
                .features
                .id
                .iter()
                .enumerate()
                .map(|(i, id)| (id.as_str(), i as u32))
                .collect();
            let gex_row: Vec<Option<u32>> = ds
                .filtered
                .features
                .id
                .iter()
                .map(|id| row_of.get(id.as_str()).copied())
                .collect();
            let is_gex: std::collections::HashSet<u32> =
                gex_row.iter().flatten().copied().collect();

            let mut entries: Vec<(u32, u32, f32)> = Vec::with_capacity(sub.nnz());
            for j in 0..bc.barcode.len() {
                let (rows, vals) = sub.column(j);
                for (&r, &v) in rows.iter().zip(vals) {
                    if !is_gex.contains(&r) {
                        entries.push((r, j as u32, v as f32));
                    }
                }
            }
            for &(g, j, v) in &c.entries {
                if let Some(Some(r)) = gex_row.get(g as usize) {
                    entries.push((*r, j, v));
                }
            }
            entries.sort_unstable_by_key(|&(r, j, _)| (j, r));
            writer::write_real_matrix_dir(
                &dir,
                full.features.len(),
                bc.barcode.len(),
                &entries,
                &full.features,
                &bc,
                params.output.gzip,
                if c.summary.integer_output { 0 } else { 3 },
            )?
        }
    }

    write_matrix_readme(&dir, ds, corrected, missing)?;
    if missing > 0 {
        progress.warn(
            "ALL_FEATURES_PARTIAL",
            format!(
                "matrix/ 少了 {missing} 个细胞的非基因表达特征：它们由本工具从 raw 中找回，Cell Ranger 的 filtered 矩阵里没有这些 barcode"
            ),
        );
    }
    Ok(())
}

/// 结果矩阵的 README。**只写英文**：这份文件会跟着矩阵一起发给下游和合作者。
fn write_matrix_readme(
    dir: &Path,
    ds: &Dataset,
    corrected: Option<&CorrectedMatrix>,
    missing: usize,
) -> Result<()> {
    let mut lines = vec![
        "scQC filtered count matrix (Matrix Market format).".to_string(),
        "Cells: barcodes retained after cell calling, low-quality filtering and doublet removal."
            .to_string(),
    ];
    match &ds.non_gex {
        Some(ng) => {
            let types: Vec<String> = ng.types.iter().map(|(t, n)| format!("{t} ({n})")).collect();
            lines.push(format!(
                "Features: Gene Expression plus {}.",
                types.join(", ")
            ));
            lines.push(
                "Quality control used Gene Expression features only; other feature types are carried over unchanged for the same set of cells."
                    .to_string(),
            );
        }
        None => lines.push("Features: Gene Expression.".to_string()),
    }
    if let Some(c) = corrected {
        lines.push(format!(
            "Ambient RNA removal was applied to the Gene Expression counts ({}).",
            match c.summary.method {
                CorrectionMethod::Soup => "global subtraction, SoupX-style",
                CorrectionMethod::DecontX => "per-cell mixture model, DecontX-style",
            }
        ));
        lines.push(if c.summary.integer_output {
            "Values are integer UMI counts.".to_string()
        } else {
            "Values are non-integer (3 decimal places); round them if your downstream tool requires integers."
                .to_string()
        });
        lines.push(
            "Every ambient RNA removal method biases expression, more strongly for highly expressed genes. The uncorrected counts can be reconstructed from the input matrix and details/cells_passed.txt; the input files were never modified."
                .to_string(),
        );
    }
    if missing > 0 {
        lines.push(format!(
            "Note: {missing} retained cells were recovered from the raw matrix by scQC and are absent from the Cell Ranger filtered matrix, so they carry no non-Gene-Expression counts."
        ));
    }
    writer::write_lines(&dir.join("README.txt"), &lines)
}

/// 下面三个映射与 `report_data.json.ui` 里用的是同一套稳定 id：
/// 界面在"等校正决定"和"看报告"两个时刻拿到的必须是同一个词，否则会渲染出两种说法。
fn verdict_id(v: Verdict) -> &'static str {
    match v {
        Verdict::Agree => "agree",
        Verdict::Disagree => "disagree",
        Verdict::Single => "single",
        Verdict::Unavailable => "unavailable",
    }
}

fn rating_id(r: Rating) -> &'static str {
    match r {
        Rating::Normal => "normal",
        Rating::Elevated => "elevated",
        Rating::Severe => "severe",
        Rating::Unrated => "unrated",
        Rating::Unavailable => "unavailable",
    }
}

fn correction_state_id(c: CorrectionState) -> &'static str {
    match c {
        CorrectionState::Greyed => "greyed",
        CorrectionState::Available => "available",
        CorrectionState::Discouraged => "discouraged",
        CorrectionState::Unrated => "unrated",
    }
}

/// 界面对"要不要校正"的回答（`--await-correction` 模式下用）。
struct CorrectAnswer {
    /// none | soup | decontx
    method: String,
    force: bool,
}

/// stdin 监听（DESIGN §9.5）。一条通道两种消息：
///   `{"type":"cancel"}`                                    → 置位取消令牌
///   `{"type":"correct","method":"soup","force":false}`     → 校正决定（见 Event::AwaitCorrection）
///
/// Windows 上 Electron 无法发送 SIGTERM，因此 stdin 是主通道。
fn spawn_stdin_listener(token: CancelToken) -> std::sync::mpsc::Receiver<CorrectAnswer> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            let Ok(line) = line else { break };
            if line.contains("\"cancel\"") {
                tracing::info!("收到取消请求");
                token.cancel();
                break;
            }
            if line.contains("\"correct\"") {
                let v: serde_json::Value = match serde_json::from_str(&line) {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::warn!("stdin 收到无法解析的一行，已忽略：{e}");
                        continue;
                    }
                };
                let method = v
                    .get("method")
                    .and_then(|m| m.as_str())
                    .unwrap_or("none")
                    .trim()
                    .to_ascii_lowercase();
                let force = v.get("force").and_then(|f| f.as_bool()).unwrap_or(false);
                tracing::info!("收到校正决定：method={method} force={force}");
                if tx.send(CorrectAnswer { method, force }).is_err() {
                    break;
                }
            }
        }
    });
    rx
}

fn now_utc() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (y, mo, d, h, mi, s) = civil_from_unix(secs);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

/// Unix 秒 → 公历（Howard Hinnant 的 civil_from_days 算法）。
fn civil_from_unix(secs: u64) -> (i64, u32, u32, u32, u32, u32) {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (
        y,
        m,
        d,
        (rem / 3600) as u32,
        ((rem % 3600) / 60) as u32,
        (rem % 60) as u32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_date() {
        assert_eq!(civil_from_unix(1_789_084_800), (2026, 9, 11, 0, 0, 0));
        assert_eq!(civil_from_unix(0), (1970, 1, 1, 0, 0, 0));
    }
}
