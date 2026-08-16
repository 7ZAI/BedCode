//! 宿主能力：传输引擎（断点续传的文件上传/下载）
//!
//! 宿主托管实际字节搬运（HTTP/1.1 + Range / upload session），
//! 插件只负责任务编排（状态机、队列、偏移持久化）。
//! 进度经 Tauri 事件 `plugin:transfer:progress` 与消息总线
//! `transfer:{task_id}` 双通道推送，均为 [`crate::types::TransferProgress`]。
//! 与桌面端 SDK `host/transfer.rs` 同构。

use super::HostError;
use crate::types::TransferRequest;

/// 插件传输引擎宿主能力
///
/// 需要 `transfer` 权限。local_path 经宿主 fs 授权检查
/// （下载 = 写授权，上传 = 读授权），未授权拒绝启动。
pub trait HostTransfer {
    /// 启动传输任务，返回宿主生成的 task_id
    ///
    /// 任务在宿主后台异步执行；完成/失败/取消均推送最终
    /// [`crate::types::TransferProgress`]（携带最终偏移，供续传持久化）
    fn transfer_start(&self, request: &TransferRequest) -> Result<String, HostError>;

    /// 取消传输任务
    ///
    /// 宿主停止任务后推送 `Cancelled` 终态进度（含最终偏移）。
    /// 任务不存在（已完成/已取消）也视为成功
    fn transfer_cancel(&self, task_id: &str) -> Result<(), HostError>;
}
