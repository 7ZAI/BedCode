//! 受控文件服务（移动端宿主能力，内网文件传输插件规格阶段 2）
//!
//! v2.1 服务器归零后，移动宿主不再运行任何 HTTP 服务端：不再独立 actix-web
//! 服务、不再 Bearer Token、不再监听端口。挂载/共享目录清单仍经 WS 控制面
//! 公告给桌面（`announce`，仅携带 mounts，port/token 置 0）供桌面浏览与
//! 拉取；大文件数据流由手机作为 HTTP client 主动发起（`client` 模块，桌面为
//! 唯一 HTTP server），目录列举改经 `FileServicePayload::FileListRequest/Response`
//! WS 往返（`list` 模块，HTTP 无关）。
//!
//! 与桌面端 `src/plugin/file_service/` 同构（sandbox/upload/cipher/registry 同源），
//! 按项目惯例独立实现、不建共享 crate。移动端额外包含：
//! - [`client`]：双向 HTTP client 传输栈（下载 GET+Range / 上传 POST+PUT session）
//! - [`responder`]：intent 传输响应器（状态机 + Approved 门 + 心跳）
//! - [`list`]：HTTP 无关的目录列举（WS list 迁移）
//! - [`announce`]：WS 控制面挂载公告/撤回（port/token 已置 0）
//!
//! 生命周期（v2.1）：
//! - 挂载变更/认证成功后重新公告挂载清单（桌面据此可 browse + pull）
//! - 末个挂载摘除时 Withdraw
//! - 解配/token revoke 时 Withdraw + 清理桌面 peer 记录

pub mod announce;
pub mod cipher;
pub mod client;
pub mod list;
pub mod notify;
pub mod registry;
pub mod responder;
pub mod saf_tree;
pub mod sandbox;
pub mod transfer;
pub mod upload;

use registry::FileServiceRegistry;
use std::sync::Arc;

/// 文件服务门面（全局单例，见 `state::get_file_service`）
///
/// 聚合注册表；挂载生命周期为纯注册表 + 公告编排（服务器归零后无 HTTP server）
pub struct FileService {
    /// 挂载注册表（mounts/peers/上传会话/策略钩子）
    pub registry: Arc<FileServiceRegistry>,
}

impl FileService {
    /// 创建门面（必须在 tokio runtime 上下文内调用：启动上传会话 sweeper）
    fn new() -> Arc<Self> {
        let registry = FileServiceRegistry::new();
        registry.start_background_tasks();
        Arc::new(Self { registry })
    }

    /// 挂载/更新 roots 成功后：重新公告挂载清单（无挂载时静默跳过）
    pub async fn after_mount_changed(&self) {
        announce::announce(&self.registry).await;
    }

    /// 卸载成功后：无剩余挂载则撤回公告，否则重新公告
    pub async fn after_unmount(&self) {
        if self.registry.mount_count().await == 0 {
            announce::withdraw().await;
        } else {
            announce::announce(&self.registry).await;
        }
    }

    /// 认证成功后重发公告（重连后桌面侧 peer 记录已被断连清理清空，必须重发）
    ///
    /// 无挂载时静默跳过
    pub async fn resend_if_active(&self) {
        if self.registry.mount_count().await > 0 {
            tracing::info!("file service resend announce after auth success");
            announce::announce(&self.registry).await;
        }
    }

    /// 强制关停（解配/token revoke 时调用）
    ///
    /// 撤回挂载公告 + 清理桌面 peer 记录；连接仍在则发 Withdraw
    pub async fn shutdown(&self) {
        announce::withdraw().await;

        // 清理桌面端 peer 记录并推送 online=false（解配 = 对端不可达）
        if let Some(peer_id) = crate::handler::sync::desktop_peer_id().await {
            self.registry.remove_peer(&peer_id).await;
        }
    }
}

// ==================== State 单例 ====================

static FILE_SERVICE: std::sync::OnceLock<Arc<FileService>> = std::sync::OnceLock::new();

/// 获取文件服务单例（首次调用时创建；必须在 tokio runtime 上下文内）
pub fn get_file_service() -> Arc<FileService> {
    FILE_SERVICE
        .get_or_init(FileService::new)
        .clone()
}

/// 获取 intent 传输响应器全局单例
pub fn get_responder() -> Arc<responder::IntentResponder> {
    responder::get_responder()
}
