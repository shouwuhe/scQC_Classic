//! 统一错误类型。
//!
//! SPEC §16：所有错误必须携带足以让非程序员理解的中文提示。
//! `message_zh` 是给用户看的；`detail` 是给开发者看的（英文原始错误、路径等），
//! 只会进入 stderr 日志，不会进入界面提示（C1：崩溃信息脱敏）。

use std::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// 错误代码，固定枚举，界面据此显示中文说明（DESIGN §9.2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    /// 文件或目录无法读取
    IoRead,
    /// 文件无法写入
    IoWrite,
    /// 文件格式不符合预期
    FormatInvalid,
    /// raw 与 filtered 不匹配等输入自洽性问题
    InputMismatch,
    /// 需要用户提供信息（平台、物种等），见 C9
    NeedInput,
    /// 内存预检未通过
    OomRisk,
    /// 用户取消
    Cancelled,
    /// 参数不合法
    InvalidParam,
    /// 尚未实现的功能
    NotImplemented,
    /// 内部错误（不应发生）
    Internal,
}

/// scQC 的错误类型。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScqcError {
    pub code: ErrorCode,
    /// 面向用户的中文说明
    pub message_zh: String,
    /// 面向开发者的补充信息（可含路径与原始错误），只进 stderr
    pub detail: Option<String>,
}

impl fmt::Display for ScqcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{:?}] {}", self.code, self.message_zh)?;
        if let Some(d) = &self.detail {
            write!(f, " ({d})")?;
        }
        Ok(())
    }
}

impl std::error::Error for ScqcError {}

impl ScqcError {
    pub fn new(code: ErrorCode, message_zh: impl Into<String>) -> Self {
        Self {
            code,
            message_zh: message_zh.into(),
            detail: None,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// 读取失败：把路径放进 detail，用户提示里只给文件名。
    pub fn io_read(path: &Path, err: &std::io::Error) -> Self {
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "（未知文件）".to_string());
        Self::new(
            ErrorCode::IoRead,
            format!("无法读取文件 {name}：{}", io_hint(err)),
        )
        .with_detail(format!("{}: {err}", path.display()))
    }

    pub fn io_write(path: &Path, err: &std::io::Error) -> Self {
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "（未知文件）".to_string());
        Self::new(
            ErrorCode::IoWrite,
            format!("无法写入文件 {name}：{}", io_hint(err)),
        )
        .with_detail(format!("{}: {err}", path.display()))
    }

    pub fn format(path: &Path, line: usize, message_zh: impl Into<String>) -> Self {
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "（未知文件）".to_string());
        Self::new(
            ErrorCode::FormatInvalid,
            format!("文件 {name} 第 {line} 行格式不正确：{}", message_zh.into()),
        )
        .with_detail(path.display().to_string())
    }

    pub fn internal(message_zh: impl Into<String>) -> Self {
        Self::new(ErrorCode::Internal, message_zh)
    }
}

fn io_hint(err: &std::io::Error) -> String {
    use std::io::ErrorKind::*;
    match err.kind() {
        NotFound => "文件不存在".to_string(),
        PermissionDenied => "没有读写权限".to_string(),
        _ => format!("系统错误（{}）", err.kind()),
    }
}

pub type Result<T> = std::result::Result<T, ScqcError>;
