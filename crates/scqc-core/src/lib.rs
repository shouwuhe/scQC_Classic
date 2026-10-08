//! scqc-core —— scQC 的纯算法库。
//!
//! 【硬约束 SPEC §5】本 crate 不依赖 Electron、Node 或任何 UI 相关的东西，
//! 必须能独立编译、独立测试、独立运行。
//!
//! 模块划分见 DESIGN.md §2。所有平台差异只存在于 [`platform::PlatformProfile`]
//! 的字段中，算法模块只读字段、不问平台名（SPEC §16）。

pub mod ambient;
pub mod doublet;
pub mod embed;
pub mod error;
pub mod io;
pub mod matrix;
pub mod pipeline;
pub mod platform;
pub mod qc;
pub mod report;
pub mod rng;

pub use error::{ErrorCode, Result, ScqcError};

/// 引擎版本号（与 Cargo.toml 一致），写入 `run_params.json` 与 `platform_profile.json`。
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
