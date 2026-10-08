//! `scqc-engine credits --lang <en|zh|ja>`
//!
//! 打印本工具**用到的每一个算法的论文出处**与实现方式说明。
//!
//! 为什么由引擎出而不是界面写死：这些条目与报告第 8 节、`docs/methods.md` 同源于
//! `scqc_core::report::methods`。三处同源意味着加一个算法只改一个地方，
//! 不会出现"报告里引了、版权页漏了"这种最难被发现的错。

use scqc_core::pipeline::Event;
use scqc_core::report::i18n::Lang;
use scqc_core::report::methods;
use scqc_core::Result;

use crate::cli::CreditsArgs;
use crate::progress;

pub fn run(args: CreditsArgs) -> Result<()> {
    let lang = Lang::parse(&args.lang).unwrap_or(Lang::En);
    let entries: Vec<serde_json::Value> = methods::entries(lang)
        .into_iter()
        .map(|m| {
            serde_json::json!({
                "key": m.key,
                "name": m.name,
                "citation": m.citation,
                "license_note": m.license_note,
            })
        })
        .collect();
    let summary = serde_json::json!({
        "lang": lang.code(),
        "version": scqc_core::VERSION,
        "git_hash": option_env!("SCQC_GIT_HASH").unwrap_or("unknown"),
        "methods": entries,
        "limitations_global": methods::global_limitations(lang),
    });
    progress::emit(&Event::Result {
        output_dir: String::new(),
        summary,
    });
    Ok(())
}
