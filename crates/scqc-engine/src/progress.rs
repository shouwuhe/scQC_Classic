//! NDJSON 输出（SPEC §10.2）。每个事件一行 JSON 写 stdout 并立即 flush。
//!
//! 这是整个引擎中**唯一**允许写 stdout 的地方；CI 用 grep 拦截其他 `println!`。

use std::io::Write;
use std::sync::Mutex;

use scqc_core::pipeline::{Event, EventSink};
use scqc_core::ScqcError;

pub struct NdjsonSink {
    lock: Mutex<()>,
    /// 本次运行发出的全部 warning（报告"运行中的警示"用）
    warnings: Mutex<Vec<(String, String)>>,
}

impl NdjsonSink {
    pub fn new() -> Self {
        Self {
            lock: Mutex::new(()),
            warnings: Mutex::new(Vec::new()),
        }
    }

    pub fn warnings(&self) -> Vec<(String, String)> {
        self.warnings
            .lock()
            .map(|w| w.clone())
            .unwrap_or_else(|p| p.into_inner().clone())
    }
}

impl Default for NdjsonSink {
    fn default() -> Self {
        Self::new()
    }
}

impl EventSink for NdjsonSink {
    fn emit(&self, event: Event) {
        if let Event::Warning { code, message } = &event {
            let mut w = self.warnings.lock().unwrap_or_else(|p| p.into_inner());
            w.push((code.clone(), message.clone()));
        }
        write_event(&self.lock, &event);
    }
}

fn write_event(lock: &Mutex<()>, event: &Event) {
    // 序列化失败只可能是内部 bug；此时写一条最简错误事件而不是 panic。
    let line = serde_json::to_string(event).unwrap_or_else(|_| {
        r#"{"type":"error","code":"INTERNAL","message":"事件序列化失败"}"#.to_string()
    });
    let _guard = lock.lock().unwrap_or_else(|p| p.into_inner());
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    // stdout 已关闭（界面退出）时无处可写，忽略即可。
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

static GLOBAL_LOCK: Mutex<()> = Mutex::new(());

/// 在没有 sink 的上下文（如 main 的错误出口）里写一条事件。
pub fn emit(event: &Event) {
    write_event(&GLOBAL_LOCK, event);
}

pub fn emit_error(e: &ScqcError) {
    emit(&Event::Error {
        code: e.code,
        message: e.message_zh.clone(),
    });
}
