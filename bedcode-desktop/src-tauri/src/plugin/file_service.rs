//! 插件文件服务（宿主通用能力，规格第 4 节）
//!
//! 插件通过 host_filesrv_mount 将用户配置的允许目录挂载为 HTTP 端点
//! （/api/plugins/{plugin_id}/{mount}/**，自动经过宿主 JWT 鉴权）。
//! 宿主强制目录沙箱（sandbox）、上传策略钩子（registry）、临时文件
//! 生命周期（upload），并预留传输加密缝（cipher）。
//!
//! 服务面无删除/改名/移动/覆盖端点；上传同名即拒由插件钩子实现。
//! 挂载随插件生命周期：deactivate/停用/卸载立即摘除。
//!
//! 本模块为纯 re-export 壳，无独立逻辑；沙箱/上传/注册表/加密的实现
//! 与测试均在四个子模块内（sandbox / upload / registry / cipher 各自的
//! `#[cfg(test)] mod tests`），故本文件不设测试模块。

pub mod cipher;
pub mod registry;
pub mod sandbox;
pub mod transfer;
pub mod upload;

pub use cipher::{PassthroughCipher, TransportCipher};
pub use registry::{FileServiceRegistry, HookTarget, MountEntry};
pub use transfer::{BatchError, BatchState, RejectReason, TransferBatch};
pub use upload::{UploadSession, UploadSessionError, UploadSessionManager};
