//! WS 目录列举请求 pending（v2.1 list 迁移）
//!
//! 服务器归零后桌面不再直连手机 `/list` 端点：`filesrv_list_remote` host fn
//! 发起 `FileServicePayload::FileListRequest` 后在此登记等待方（list_id →
//! oneshot），`terminal_ws::handle_file_service` 收到 `FileListResponse` 时
//! 按 list_id resolve；发起侧带超时兜底清除（防泄漏）。
//!
//! 多设备场景：`WsSessionRegistry` 广播给全部已认证客户端，取首个匹配
//! list_id 的响应（单配对为主场景；多手机并发 list 由前端串行调用避免）。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use tokio::sync::oneshot;

use crate::enums::FileServicePayload;

static PENDING: OnceLock<Mutex<HashMap<String, oneshot::Sender<FileServicePayload>>>> = OnceLock::new();

fn pending() -> &'static Mutex<HashMap<String, oneshot::Sender<FileServicePayload>>> {
    PENDING.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 登记等待方（filesrv_list_remote 发起请求后调用）
pub(crate) fn register(list_id: String, tx: oneshot::Sender<FileServicePayload>) {
    pending().lock().unwrap().insert(list_id, tx);
}

/// 取出并移除对应 list_id 的等待方（terminal_ws 收到响应时调用）；
/// 不存在（已超时清除）返回 None
pub(crate) fn take(list_id: &str) -> Option<oneshot::Sender<FileServicePayload>> {
    pending().lock().unwrap().remove(list_id)
}

/// 移除并丢弃等待方（发起侧超时后清理，防泄漏）
pub(crate) fn drop_pending(list_id: &str) {
    pending().lock().unwrap().remove(list_id);
}
