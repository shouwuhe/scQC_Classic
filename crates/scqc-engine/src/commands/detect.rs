//! `scqc-engine detect` —— 只做平台识别（DESIGN §5.2），便于在终端验证。

use scqc_core::pipeline::{Event, Step};
use scqc_core::Result;

use crate::cli::InputArgs;
use crate::progress::NdjsonSink;

pub fn run(args: InputArgs) -> Result<()> {
    let sink = NdjsonSink::new();
    let progress = scqc_core::pipeline::Progress::new(&sink);
    progress.report(Step::Read, 0, "读取矩阵");
    let ds = super::load_dataset(&args)?;
    progress.report(Step::Read, 100, "读取完成");
    progress.report(Step::Platform, 0, "识别平台");
    let (det, profile) = super::resolve_profile(&ds, &args, &sink)?;
    let summary = serde_json::json!({
        "input": ds.summary(),
        "detection": det,
        "profile": profile,
    });
    crate::progress::emit(&Event::Result {
        output_dir: String::new(),
        summary,
    });
    Ok(())
}
