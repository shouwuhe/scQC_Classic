//! 质控指标与过滤算法：metrics（基础指标）、knee、emptydrops、ddqc。

pub mod ddqc;
pub mod emptydrops;
pub mod goodturing;
pub mod knee;
pub mod metrics;

pub use ddqc::{ddqc, DdqcParams, DdqcResult};
pub use emptydrops::{empty_drops, EdReason, EmptyDropsParams, EmptyDropsResult, EmptyDropsRow};
pub use knee::{barcode_ranks, KneeResult};
pub use metrics::{compute_basic_metrics, BasicMetrics, GeneSets};
