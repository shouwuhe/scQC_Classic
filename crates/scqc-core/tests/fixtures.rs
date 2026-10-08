//! 基于 tests/fixtures 合成数据的集成测试：读写往返、中文路径、gzip、平台识别行为。
#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use scqc_core::io::{self, writer};
use scqc_core::platform::{self, ProfileId, Species, UserChoice};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

#[test]
fn read_scrna_mini_and_roundtrip() {
    let d = fixtures().join("scrna_mini");
    let ds = io::read_dataset(
        Some(&d.join("raw")),
        &d.join("filtered"),
        Some(&d.join("metrics_summary.csv")),
    )
    .unwrap();
    let raw = ds.raw.as_ref().unwrap();
    assert_eq!(raw.matrix.n_genes(), 30);
    assert_eq!(raw.matrix.n_barcodes(), 60);
    assert_eq!(ds.filtered.matrix.n_barcodes(), 20);
    assert_eq!(raw.matrix.nnz(), 1021);
    assert_eq!(ds.metrics.as_ref().unwrap().layout, "wide");

    // 往返：写出再读入，应完全一致
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("out");
    writer::write_matrix_dir(&out, &raw.matrix, &raw.features, &raw.barcodes, true).unwrap();
    let back = io::read_matrix_dir(&out).unwrap();
    assert_eq!(back.matrix.summary(), raw.matrix.summary());
    assert_eq!(back.features, raw.features);
    assert_eq!(back.barcodes, raw.barcodes);
    let a: Vec<_> = raw.matrix.iter_triplets().collect();
    let b: Vec<_> = back.matrix.iter_triplets().collect();
    assert_eq!(a, b);
}

#[test]
fn chinese_path_and_gzip() {
    // SPEC §14.4：中文路径正常工作；.gz 正常读取
    let d = fixtures().join("中文路径").join("scrna_mini_gz");
    let ds = io::read_dataset(Some(&d.join("raw")), &d.join("filtered"), None).unwrap();
    let plain = io::read_dataset(
        Some(&fixtures().join("scrna_mini/raw")),
        &fixtures().join("scrna_mini/filtered"),
        None,
    )
    .unwrap();
    assert_eq!(
        ds.raw.unwrap().matrix.summary(),
        plain.raw.unwrap().matrix.summary()
    );
}

#[test]
fn gex_data_requires_sc_sn_choice() {
    // SPEC §14.2：识别不唯一时返回 need_input，不使用默认值
    let d = fixtures().join("scrna_mini");
    let ds = io::read_dataset(
        Some(&d.join("raw")),
        &d.join("filtered"),
        Some(&d.join("metrics_summary.csv")),
    )
    .unwrap();
    let det = platform::detect(
        &ds.filtered.features,
        &ds.filtered.barcodes,
        ds.raw.as_ref().map(|r| &r.barcodes),
        ds.metrics.as_ref(),
    );
    assert_eq!(
        det.candidates,
        vec![ProfileId::TenxScRna, ProfileId::TenxSnRna]
    );
    assert_eq!(det.species.species, Some(Species::Human));
    let needs = platform::resolve(&det, &UserChoice::default())
        .err()
        .unwrap();
    assert_eq!(needs.len(), 1);
    assert_eq!(needs[0].field, "platform");
    let p = platform::resolve(
        &det,
        &UserChoice {
            platform: Some(ProfileId::TenxScRna),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(p.uses_ribo());
    assert_eq!(p.mito_guardrail, Some(0.10));
}

#[test]
fn flex_multiplex_detected_from_metrics_and_barcodes() {
    let d = fixtures().join("flex_multiplex");
    let ds = io::read_dataset(
        Some(&d.join("raw")),
        &d.join("filtered"),
        Some(&d.join("metrics_summary.csv")),
    )
    .unwrap();
    assert_eq!(ds.metrics.as_ref().unwrap().layout, "long");
    let det = platform::detect(
        &ds.filtered.features,
        &ds.filtered.barcodes,
        ds.raw.as_ref().map(|r| &r.barcodes),
        ds.metrics.as_ref(),
    );
    assert_eq!(det.candidates, vec![ProfileId::TenxFlexMultiplex]);
    assert_eq!(det.multiplex_n, Some(3));
    let p = platform::resolve(&det, &UserChoice::default()).unwrap();
    // SPEC §7.2.1【硬约束】：Flex 禁用 ribo；§7.2.5：无参考区间
    assert!(!p.uses_ribo());
    assert!(!p.show_ribo);
    assert!(p.ambient_reference.is_none());
    assert_eq!(p.multiplex_n, Some(3));
}

#[test]
fn no_metadata_requires_platform_choice() {
    let d = fixtures().join("snrna_mini");
    let ds = io::read_dataset(Some(&d.join("raw")), &d.join("filtered"), None).unwrap();
    let det = platform::detect(
        &ds.filtered.features,
        &ds.filtered.barcodes,
        ds.raw.as_ref().map(|r| &r.barcodes),
        None,
    );
    assert_eq!(det.species.species, Some(Species::Mouse));
    assert!(platform::resolve(&det, &UserChoice::default()).is_err());
    let p = platform::resolve(
        &det,
        &UserChoice {
            platform: Some(ProfileId::TenxSnRna),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(p.mito_mad_min_diff, Some(0.005));
    assert_eq!(p.mito_guardrail, Some(0.05));
}

#[test]
fn mismatched_raw_filtered_is_rejected() {
    let a = fixtures().join("scrna_mini/raw");
    let b = fixtures().join("flex_multiplex/filtered");
    let e = io::read_dataset(Some(&a), &b, None).err().unwrap();
    assert_eq!(e.code, scqc_core::ErrorCode::InputMismatch);
}

/// 报告的英文正文里不能出现中文。
///
/// 平台识别依据是唯一一处"引擎先写成中文、再交给别处翻译"的文本：`Evidence.text_zh`
/// 给旧的中文报告用，`code`/`args` 给界面和三语报告用。一旦有人加了新的 code 却忘了
/// 在 `report::i18n` 里补三语模板，英文报告就会悄悄漏出中文——这个测试就是拦它的。
#[test]
fn every_evidence_code_has_all_three_languages() {
    use scqc_core::report::i18n::{evidence_text, Lang};

    const SENTINEL: &str = "\u{1}FALLBACK\u{1}";
    let mut seen = 0usize;
    for name in ["scrna_mini", "snrna_mini", "flex_multiplex"] {
        let d = fixtures().join(name);
        let ds = io::read_dataset(Some(&d.join("raw")), &d.join("filtered"), None).unwrap();
        let det = platform::detect(
            &ds.filtered.features,
            &ds.filtered.barcodes,
            ds.raw.as_ref().map(|r| &r.barcodes),
            None,
        );
        for e in &det.evidence {
            seen += 1;
            for lang in [Lang::En, Lang::Zh, Lang::Ja] {
                let s = evidence_text(&e.code, &e.args, lang, SENTINEL);
                assert_ne!(s, SENTINEL, "证据 code `{}` 缺少 {:?} 模板", e.code, lang);
                assert!(
                    !s.contains('{'),
                    "证据 code `{}` 的 {:?} 模板占位符没替换干净：{s}",
                    e.code,
                    lang
                );
            }
            let en = evidence_text(&e.code, &e.args, Lang::En, SENTINEL);
            assert!(
                !en.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)),
                "英文证据 `{}` 里出现了汉字：{en}",
                e.code
            );
        }
    }
    assert!(seen >= 6, "没取到足够的证据条目，测试本身失效了");
}
