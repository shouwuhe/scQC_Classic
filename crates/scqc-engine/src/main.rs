//! scqc-engine —— 命令行引擎（SPEC §10）。
//!
//! 【硬约束】stdout 只输出 NDJSON；人类可读日志走 stderr。
//! 退出码（DESIGN §9.3）：0 成功 · 1 运行错误 · 2 参数错误 · 3 need_input · 4 OOM_RISK · 130 取消。

mod cli;
mod commands;
mod progress;

use clap::Parser;
use scqc_core::ErrorCode;

fn main() {
    let args = cli::Cli::parse();
    init_logging(args.verbose);

    let code = match run(args) {
        Ok(()) => 0,
        Err(e) => {
            // 所有错误都以 NDJSON error 事件报告一次（界面读 stdout），stderr 记录详情。
            tracing::error!(detail = ?e.detail, "{}", e.message_zh);
            progress::emit_error(&e);
            match e.code {
                ErrorCode::NeedInput => 3,
                ErrorCode::OomRisk => 4,
                ErrorCode::Cancelled => 130,
                ErrorCode::InvalidParam => 2,
                _ => 1,
            }
        }
    };
    std::process::exit(code);
}

fn run(args: cli::Cli) -> scqc_core::Result<()> {
    match args.command {
        cli::Command::Version => commands::version::run(),
        cli::Command::Detect(a) => commands::detect::run(a),
        cli::Command::Run(a) => commands::run::run(a),
        cli::Command::Verify(a) => commands::verify::run(a),
        cli::Command::Credits(a) => commands::credits::run(a),
    }
}

fn init_logging(verbose: bool) {
    use tracing_subscriber::EnvFilter;
    // 第三方库（hnsw_rs 等）的日志只保留 warn 级别
    let default = if verbose {
        "debug,hnsw_rs=warn,annembed=warn"
    } else {
        "info,hnsw_rs=warn"
    };
    let filter = EnvFilter::try_from_env("SCQC_LOG").unwrap_or_else(|_| EnvFilter::new(default));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr) // 【硬约束】日志永远走 stderr
        .with_target(false)
        .init();
}
