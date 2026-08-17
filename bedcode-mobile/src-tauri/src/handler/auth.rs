//! Auth Handler - 认证消息处理器

use async_trait::async_trait;

use crate::model::message::Message;
use crate::enums::auth::AuthStage;
use crate::Result;

use crate::router::{ClientRouteContext, MobileEvent, ClientRouteHandler};

/// 认证消息处理器
pub struct AuthHandler;

#[async_trait]
impl ClientRouteHandler for AuthHandler {
    async fn handle(&self, message: Message, ctx: &ClientRouteContext) -> Result<Option<Message>> {
        if let Message::Auth { payload, .. } = message {
            match payload.stage {
                AuthStage::Authenticated => {
                    if let Some(session_token) = payload.session_token {
                        // 持久化为全局 token：桌面端文件服务公告约定不携带 token，
                        // 插件经 host_filesrv_get_peer 读取的 peer token 依赖此兜底；
                        // 不设置则插件 HTTP 调用无 Authorization 头，桌面端返回 401
                        crate::state::set_global_token(&session_token);
                        tracing::info!("[AuthHandler] Authenticated");
                        ctx.emit(MobileEvent::AuthSuccess {
                            session_token,
                        });

                        // 同步设备级连接状态（WS 回复路径）：HTTP 认证路径经
                        // manager::apply_auth_success 置位，WS 路径（04 事件 WS /
                        // 集成测试）由这里补齐，两路最终都落在 Authed
                        crate::state::get_connection_manager().set_authed().await;

                        // 通知插件认证成功（插件管理器未初始化时跳过——
                        // 集成测试等无插件环境路径，语义同无插件运行）
                        if let Some(pm) = crate::state::try_get_plugin_manager() {
                            pm.dispatch_lifecycle_event(
                                crate::plugin::types::PluginLifecycleEvent::AuthSuccess
                            ).await;
                        }

                        // 重发文件服务 Announce（含重连场景：桌面侧 peer 记录
                        // 已随 WS 断连清理清空，不重发对端将永远看不到服务）
                        crate::state::get_file_service().resend_if_active().await;
                    }
                }
                AuthStage::Failed => {
                    let reason = payload.error.unwrap_or_else(|| "Authentication failed".to_string());
                    tracing::warn!("[AuthHandler] AuthFailed: {}", reason);
                    ctx.emit(MobileEvent::AuthFailed { reason });
                }
                _ => {}
            }
        }
        Ok(None)
    }

    fn name(&self) -> &str {
        "AuthHandler"
    }
}

impl Default for AuthHandler {
    fn default() -> Self {
        Self
    }
}