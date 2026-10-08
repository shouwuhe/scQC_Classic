//! 流程编排基础设施：事件（进度 / 警告 / 需要输入）、取消、内存预检。
//!
//! 核心库不直接写 stdout；它通过 [`EventSink`] 把事件交给调用方（scqc-engine 把事件
//! 序列化为 NDJSON 写 stdout，SPEC §10.2）。

pub mod cancel;
pub mod cellcalling;
pub mod memory;
pub mod params;
pub mod plots;

use serde::{Deserialize, Serialize};

use crate::platform::NeedInput;

/// 流程步骤（SPEC §8.7 的 9 步；`step_index` 从 1 开始）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    Read,
    Platform,
    CellCalling,
    Cluster,
    Ambient,
    Ddqc,
    Doublet,
    Correction,
    Output,
}

impl Step {
    pub const TOTAL: u32 = 9;

    pub fn index(&self) -> u32 {
        match self {
            Step::Read => 1,
            Step::Platform => 2,
            Step::CellCalling => 3,
            Step::Cluster => 4,
            Step::Ambient => 5,
            Step::Ddqc => 6,
            Step::Doublet => 7,
            Step::Correction => 8,
            Step::Output => 9,
        }
    }

    pub fn label_zh(&self) -> &'static str {
        match self {
            Step::Read => "读取矩阵",
            Step::Platform => "平台识别",
            Step::CellCalling => "真细胞鉴定",
            Step::Cluster => "聚类与环境表达谱",
            Step::Ambient => "环境 RNA 诊断",
            Step::Ddqc => "低质量细胞过滤",
            Step::Doublet => "双细胞检测",
            Step::Correction => "环境 RNA 校正",
            Step::Output => "输出结果与报告",
        }
    }
}

/// 引擎 → 界面的事件，与 SPEC §10.2 的 NDJSON 一一对应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    Platform {
        id: String,
        species: String,
        /// 稳定的物种 id（human | mouse | other），界面据此本地化
        species_id: String,
        multiplex_n: Option<u32>,
        evidence: Vec<String>,
        /// 与 `evidence` 一一对应的语言中立版本
        evidence_codes: Vec<crate::platform::detect::CodedText>,
        user_overrides: Vec<crate::platform::detect::CodedText>,
    },
    Progress {
        step: Step,
        step_index: u32,
        total_steps: u32,
        pct: u8,
        message: String,
        elapsed_s: f64,
    },
    Warning {
        code: String,
        message: String,
    },
    NeedInput {
        field: String,
        message: String,
        options: Vec<crate::platform::detect::InputOption>,
        evidence: Vec<String>,
        evidence_codes: Vec<crate::platform::detect::CodedText>,
    },
    /// 第 1–7 步已跑完，等界面回答"这一次要不要做环境 RNA 校正"（SPEC §11.3）。
    ///
    /// 引擎在这里阻塞，收到 stdin 的一行 `{"type":"correct","method":"soup|decontx|none","force":bool}`
    /// 后再继续第 8、9 步。这样改这一个决定不必把前七步重算一遍——前七步是确定性的，
    /// 重算必然得到同样的结果，纯属浪费。
    AwaitCorrection {
        /// 环境 RNA 部分的语言中立事实：只有数字与稳定枚举 id，没有任何自然语言
        facts: serde_json::Value,
    },
    Result {
        output_dir: String,
        summary: serde_json::Value,
    },
    Error {
        code: crate::error::ErrorCode,
        message: String,
    },
}

impl From<&NeedInput> for Event {
    fn from(n: &NeedInput) -> Self {
        Event::NeedInput {
            field: n.field.clone(),
            message: n.message_zh.clone(),
            options: n.options.clone(),
            evidence: n.evidence.clone(),
            evidence_codes: n.evidence_codes.clone(),
        }
    }
}

/// 事件接收方。实现者负责序列化与输出。
pub trait EventSink: Send + Sync {
    fn emit(&self, event: Event);
}

/// 丢弃所有事件（测试用）。
pub struct NullSink;

impl EventSink for NullSink {
    fn emit(&self, _event: Event) {}
}

/// 进度上报的便捷封装：记录开始时间，按步骤发送百分比。
pub struct Progress<'a> {
    sink: &'a dyn EventSink,
    started: std::time::Instant,
}

impl<'a> Progress<'a> {
    pub fn new(sink: &'a dyn EventSink) -> Self {
        Self {
            sink,
            started: std::time::Instant::now(),
        }
    }

    pub fn report(&self, step: Step, pct: u8, message: impl Into<String>) {
        self.sink.emit(Event::Progress {
            step,
            step_index: step.index(),
            total_steps: Step::TOTAL,
            pct: pct.min(100),
            message: message.into(),
            elapsed_s: self.started.elapsed().as_secs_f64(),
        });
    }

    pub fn warn(&self, code: &str, message: impl Into<String>) {
        self.sink.emit(Event::Warning {
            code: code.to_string(),
            message: message.into(),
        });
    }

    pub fn sink(&self) -> &'a dyn EventSink {
        self.sink
    }
}
