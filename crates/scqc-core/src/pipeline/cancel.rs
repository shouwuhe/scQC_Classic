//! 取消标志（SPEC §10.2：在下一个检查点干净退出）。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::error::{ErrorCode, Result, ScqcError};

/// 可在线程间共享的取消令牌。
#[derive(Debug, Clone, Default)]
pub struct CancelToken {
    flag: Arc<AtomicBool>,
}

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }

    /// 检查点：已取消则返回 `Cancelled` 错误。
    pub fn checkpoint(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(ScqcError::new(ErrorCode::Cancelled, "已按用户要求取消运行"))
        } else {
            Ok(())
        }
    }
}
