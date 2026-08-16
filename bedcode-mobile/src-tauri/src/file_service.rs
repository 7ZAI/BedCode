//! 受控文件服务（移动端宿主能力，内网文件传输插件规格阶段 2）
//!
//! 移动宿主作为文件服务方：独立 actix-web HTTP 服务（随机端口 + Bearer Token），
//! 端口/token/挂载清单经现有已认证 WS 控制面公告给桌面端（规格 4.5 节）。
//!
//! 与桌面端 `src/plugin/file_service/` 同构（sandbox/upload/cipher/registry 同源），
//! 按项目惯例独立实现、不建共享 crate。移动端额外包含：
//! - [`server`]：独立 HTTP 服务（桌面端挂在现有 actix server 子路由）
//! - [`auth`]：Bearer Token 守卫（桌面端走现有 JWT 中间件）
//! - [`announce`]：WS 控制面公告/撤回
//!
//! 生命周期（规格 4.5）：
//! - 首个文件服务挂载时启动服务，挂载变更后立即 Announce
//! - 最后一个挂载摘除时关闭服务并 Withdraw
//! - 认证成功（含重连）时 resend Announce（桌面侧 peer 记录已随断连清空）
//! - 解配/token revoke 时停服务（连接已断则不发 Withdraw）

pub mod announce;
pub mod auth;
pub mod cipher;
pub mod notify;
pub mod registry;
pub mod saf_tree;
pub mod sandbox;
pub mod server;
pub mod transfer;
pub mod upload;

use registry::FileServiceRegistry;
use server::FileServiceServer;
use std::sync::Arc;

/// 文件服务门面（全局单例，见 `state::get_file_service`）
///
/// 聚合注册表与 HTTP 服务，提供挂载生命周期编排
/// （host functions 在挂载/卸载成功后调用对应方法）
pub struct FileService {
    /// 挂载注册表（mounts/peers/上传会话/策略钩子）
    pub registry: Arc<FileServiceRegistry>,
    /// 独立 HTTP 服务（随挂载启停）
    pub server: Arc<FileServiceServer>,
}

impl FileService {
    /// 创建门面（必须在 tokio runtime 上下文内调用：启动上传会话 sweeper）
    fn new() -> Arc<Self> {
        let registry = FileServiceRegistry::new();
        registry.start_background_tasks();
        let server = Arc::new(FileServiceServer::new(registry.clone()));
        Arc::new(Self { registry, server })
    }

    /// 挂载/更新 roots 成功后：确保服务已启动并立即公告（规格 4.5）
    pub async fn after_mount_changed(&self) {
        if let Err(e) = self.server.ensure_started().await {
            tracing::error!("file service ensure_started failed: {}", e);
            return;
        }
        announce::announce(&self.registry, &self.server).await;
    }

    /// 卸载成功后：无剩余挂载则停服务并撤回公告，否则重新公告
    pub async fn after_unmount(&self) {
        if self.registry.mount_count().await == 0 {
            if self.server.is_running().await {
                self.server.stop().await;
                announce::withdraw().await;
            }
        } else {
            announce::announce(&self.registry, &self.server).await;
        }
    }

    /// 认证成功后重发公告（重连后桌面侧 peer 记录已被断连清理清空，必须重发）
    ///
    /// 服务未运行/无挂载时静默跳过
    pub async fn resend_if_active(&self) {
        if self.server.is_running().await && self.registry.mount_count().await > 0 {
            tracing::info!("file service resend announce after auth success");
            announce::announce(&self.registry, &self.server).await;
        }
    }

    /// 强制关停（解配/token revoke 时调用）
    ///
    /// 停服务 + 吊销 token + 清理桌面 peer 记录；连接仍在则发 Withdraw
    pub async fn shutdown(&self) {
        if self.server.is_running().await {
            self.server.stop().await;
            announce::withdraw().await;
        } else {
            // 服务本就未运行：确保 token 无残留
            self.server.token_guard().revoke();
        }

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
