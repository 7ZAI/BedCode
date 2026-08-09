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

                        // 通知插件认证成功
                        {
                            let pm = crate::state::get_plugin_manager();
                            pm.dispatch_lifecycle_event(
                                crate::plugin::types::PluginLifecycleEvent::AuthSuccess
                            ).await;
                        }

                        // 重发文件服务 Announce（含重连场景：桌面侧 peer 记录
                        // 已随 WS 断连清理清空，不重发对端将永远看不到服务）
                        crate::state::get_file_service().resend_if_active().await;
                    }
                }
                AuthStage::VerifyCode => {
                    tracing::info!("[AuthHandler] PairingVerified");
                    ctx.emit(MobileEvent::PairingVerified);

                    // 配对成功 = 对端可达：补发文件服务公告（挂载早于连接的
                    // 场景下首次 announce 因连接未建立被跳过，不重发对端将永远看不到服务）
                    crate::state::get_file_service().resend_if_active().await;
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