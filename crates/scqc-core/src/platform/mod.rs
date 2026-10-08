//! Platform Profile —— 平台差异抽象（SPEC §7，DESIGN §3.4 / §5.2 / §6）。
//!
//! **本目录是整个 crate 中唯一允许出现平台名字符串（scRNA / snRNA / Flex）的地方。**
//! 算法模块只读 [`PlatformProfile`] 的字段（CI 用 grep 强制，见 .github/workflows/ci.yml）。

pub mod builtin;
pub mod detect;
pub mod profile;
pub mod species;

pub use detect::{detect, resolve, Detection, NeedInput, UserChoice};
pub use profile::*;
pub use species::{Species, SpeciesRule};
