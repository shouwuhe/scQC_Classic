//! `scqc-engine version`

use scqc_core::pipeline::Event;
use scqc_core::Result;

use crate::progress;

pub fn run() -> Result<()> {
    let summary = serde_json::json!({
        "version": scqc_core::VERSION,
        "git_hash": option_env!("SCQC_GIT_HASH").unwrap_or("unknown"),
        "target": option_env!("SCQC_TARGET").unwrap_or(std::env::consts::ARCH),
        "os": std::env::consts::OS,
    });
    progress::emit(&Event::Result {
        output_dir: String::new(),
        summary,
    });
    Ok(())
}
