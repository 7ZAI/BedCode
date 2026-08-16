//! 宿主能力接口 — 按功能域划分的 trait 定义
//!
//! 本模块是插件可见的全部宿主能力的**接口契约**：
//! - [`WasmHost`](crate::wasm_host::WasmHost) 以 WASM import 后端实现全部子 trait
//! - 插件业务代码可依赖 `impl HostStorage + HostLog` 等抽象组合，而非具体类型，
//!   便于单元测试时 mock
//!
//! 各子 trait 按功能域一一对应 WIT `bedcode:plugin` world 的 import 接口
//! （storage / database / terminal / events / http / fs / log / bus / config /
//! file_service / transfer）。session 能力已随组件迁移删除（内置插件零使用，
//! 见迁移 spec §3.2/D-Q3）—— WIT 为单一事实来源，编译期杜绝残存调用。
//!
//! 错误语义见 [`HostError`]：仅承载状态码与通用描述，
//! 详细错误原因记录在宿主日志（WIT `result<T, string>` 透传后含真实消息）。

pub mod bus;
pub mod config;
pub mod database;
pub mod events;
pub mod file_service;
pub mod fs;
pub mod http;
pub mod log;
pub mod storage;
pub mod terminal;
pub mod transfer;

pub use bus::HostBus;
pub use config::{ConfigKey, HostConfig};
pub use database::HostDatabase;
pub use events::HostEvents;
pub use file_service::HostFileService;
pub use fs::HostFs;
pub use http::HostHttp;
pub use log::HostLog;
pub use storage::HostStorage;
pub use terminal::HostTerminal;
pub use transfer::HostTransfer;

/// 宿主调用错误
///
/// 插件侧可见的错误类型。`code` 为宿主返回的状态码（通常为 -1），
/// `message` 为通用描述 —— 详细失败原因（权限拒绝、SQL 错误等）
/// 记录在宿主 tracing 日志中，按 plugin_id 检索。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostError {
    /// 宿主状态码（-1 = 调用失败，-2 = 能力不可用）
    pub code: i32,
    /// 错误描述
    pub message: String,
}

impl HostError {
    /// 能力在当前上下文不可用（如宿主未实现该能力）
    pub const CODE_UNSUPPORTED: i32 = -2;

    /// 构造"调用失败"错误
    pub fn call_failed(api: &str) -> Self {
        Self {
            code: -1,
            message: format!("{} failed (see host log for details)", api),
        }
    }

    /// 构造"能力不可用"错误
    pub fn unsupported(api: &str) -> Self {
        Self {
            code: Self::CODE_UNSUPPORTED,
            message: format!("{} is not available in this context", api),
        }
    }

    /// 构造自定义错误
    pub fn custom(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for HostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "host error {}: {}", self.code, self.message)
    }
}

impl std::error::Error for HostError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_call_failed() {
        // 默认调用失败：code=-1，消息含 API 名（详细原因在宿主日志）
        let e = HostError::call_failed("db_execute");
        assert_eq!(e.code, -1);
        assert_eq!(e.message, "db_execute failed (see host log for details)");
    }

    #[test]
    fn test_unsupported() {
        // 能力不可用：code 必须等于 CODE_UNSUPPORTED（-2），与调用失败区分
        let e = HostError::unsupported("fs_read");
        assert_eq!(e.code, HostError::CODE_UNSUPPORTED);
        assert_eq!(e.code, -2);
        assert_eq!(e.message, "fs_read is not available in this context");
    }

    #[test]
    fn test_custom() {
        let e = HostError::custom(42, "custom failure");
        assert_eq!(e.code, 42);
        assert_eq!(e.message, "custom failure");
    }

    #[test]
    fn test_display() {
        // Display 格式被宿主日志引用，锁定格式防破坏性改动
        let e = HostError::custom(-1, "boom");
        assert_eq!(e.to_string(), "host error -1: boom");
    }

    #[test]
    fn test_error_trait_and_partial_eq() {
        // 可放入 anyhow 错误链（插件侧 `?` 透传）
        let e = HostError::call_failed("x");
        let err: anyhow::Error = e.into();
        assert!(err.to_string().contains("x failed"));
        assert_eq!(HostError::call_failed("x"), HostError::call_failed("x"));
    }
}

/// 宿主能力聚合 trait
///
/// 需要全部宿主能力的插件入口可用 `&impl HostApi` 作为参数类型；
/// 只需部分能力的业务函数建议用更细的子 trait 组合（如 `impl HostStorage + HostLog`），
/// 保持依赖面最小、可测试性最好。
///
/// blanket impl：任何实现了全部子 trait 的类型自动获得此 trait。
pub trait HostApi:
    HostStorage
    + HostDatabase
    + HostTerminal
    + HostEvents
    + HostHttp
    + HostFs
    + HostLog
    + HostBus
    + HostFileService
    + HostTransfer
    + HostConfig
{
}

impl<T> HostApi for T where
    T: HostStorage
        + HostDatabase
        + HostTerminal
        + HostEvents
        + HostHttp
        + HostFs
        + HostLog
        + HostBus
        + HostFileService
        + HostTransfer
        + HostConfig
{
}
