pub mod credits;
pub mod detect;
pub mod report_build;
pub mod run;
pub mod verify;
pub mod version;

use scqc_core::io::Dataset;
use scqc_core::pipeline::{Event, EventSink};
use scqc_core::platform::{self, PlatformProfile, ProfileId, Species, UserChoice};
use scqc_core::{ErrorCode, Result, ScqcError};

use crate::cli::{InputArgs, PlatformArg, SpeciesArg};

/// 把命令行参数转成 `UserChoice`。物种为 other 时要求给出前缀。
pub fn user_choice(args: &InputArgs) -> Result<UserChoice> {
    let platform = match args.platform {
        PlatformArg::Auto => None,
        PlatformArg::TenxScRna => Some(ProfileId::TenxScRna),
        PlatformArg::TenxSnRna => Some(ProfileId::TenxSnRna),
        PlatformArg::TenxFlexSingleplex => Some(ProfileId::TenxFlexSingleplex),
        PlatformArg::TenxFlexMultiplex => Some(ProfileId::TenxFlexMultiplex),
    };
    let species = match args.species {
        SpeciesArg::Auto => None,
        SpeciesArg::Human => Some(Species::Human),
        SpeciesArg::Mouse => Some(Species::Mouse),
        SpeciesArg::Other => {
            let mito_prefix = args.mito_prefix.clone().ok_or_else(|| {
                ScqcError::new(
                    ErrorCode::InvalidParam,
                    "--species other 时必须提供 --mito-prefix（线粒体基因名前缀）",
                )
            })?;
            let ribo_prefixes = args
                .ribo_prefixes
                .as_deref()
                .unwrap_or("")
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            Some(Species::Other {
                mito_prefix,
                ribo_prefixes,
                mito_guardrail: args.mito_guardrail,
            })
        }
    };
    Ok(UserChoice {
        platform,
        species,
        multiplex_n: args.multiplex_n,
    })
}

/// 读取数据集。
pub fn load_dataset(args: &InputArgs) -> Result<Dataset> {
    scqc_core::io::read_dataset(args.raw.as_deref(), &args.filtered, args.metrics.as_deref())
}

/// 稳定的物种 id（界面用自己的词典渲染，不解析中文标签）。
pub fn species_id(s: &scqc_core::platform::Species) -> &'static str {
    match s {
        scqc_core::platform::Species::Human => "human",
        scqc_core::platform::Species::Mouse => "mouse",
        scqc_core::platform::Species::Other { .. } => "other",
    }
}

/// 识别 + 合成 Profile；信息不足时发出 need_input 事件并返回 NeedInput 错误（退出码 3）。
pub fn resolve_profile(
    ds: &Dataset,
    args: &InputArgs,
    sink: &dyn EventSink,
) -> Result<(platform::Detection, PlatformProfile)> {
    let choice = user_choice(args)?;
    let det = platform::detect(
        &ds.filtered.features,
        &ds.filtered.barcodes,
        ds.raw.as_ref().map(|r| &r.barcodes),
        ds.metrics.as_ref(),
    );
    match platform::resolve(&det, &choice) {
        Ok(profile) => {
            sink.emit(Event::Platform {
                id: profile.id.as_str().to_string(),
                species: profile.species.label_zh(),
                species_id: species_id(&profile.species).to_string(),
                multiplex_n: profile.multiplex_n,
                evidence: profile.evidence.clone(),
                evidence_codes: profile.evidence_codes.clone(),
                user_overrides: profile.user_overrides.clone(),
            });
            Ok((det, profile))
        }
        Err(needs) => {
            for n in &needs {
                sink.emit(Event::from(n));
            }
            let fields: Vec<&str> = needs.iter().map(|n| n.field.as_str()).collect();
            Err(ScqcError::new(
                ErrorCode::NeedInput,
                format!(
                    "需要用户提供：{}（不会使用默认值继续，见 SPEC C9）",
                    fields.join("、")
                ),
            ))
        }
    }
}
