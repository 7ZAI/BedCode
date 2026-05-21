//! Output Forwarder Service
//!
//! 负责将 PTY 输出转发给已认证的 WebSocket 客户端

use crate::desktop::pty::PtyOutputEvent;
use crate::desktop::session::SessionManager;
use crate::desktop::server::message::Message;
use crate::desktop::websocket_manager::WebSocketManager;
use crate::Result;
use std::sync::Arc;
use tokio::sync::broadcast;
use uuid::Uuid;

/// 输出转发器
///
/// 负责将 PTY 输出转发给已认证的 WebSocket 客户端
pub struct OutputForwarder {
    session_manager: Arc<SessionManager>,
}

impl OutputForwarder {
    /// 创建新的输出转发器
    pub fn new(session_manager: Arc<SessionManager>) -> Self {
        Self { session_manager }
    }

    /// 运行输出转发器
    pub async fn run(&self, shutdown_rx: &mut broadcast::Receiver<()>) {
        let mut output_rx = self.session_manager.subscribe_output();

        loop {
            tokio::select! {
                result = output_rx.recv() => {
                    match result {
                        Ok(event) => {
                            tracing::debug!(
                                "[OutputForwarder] Received output for session {}, data_len={}",
                                event.session_id, event.data.len()
                            );
                            // 转发输出给订阅了该会话的客户端
                            if let Err(e) = self.forward_output(&event).await {
                                tracing::error!("Failed to forward output: {}", e);
                            }
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            tracing::debug!("Output channel closed");
                            break;
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!("Output channel lagged {} messages", n);
                        }
                    }
                }
                _ = shutdown_rx.recv() => {
                    tracing::info!("Output forwarder shutting down");
                    break;
                }
            }
        }
    }

    /// 转发PTY输出到订阅的客户端
    async fn forward_output(&self, event: &PtyOutputEvent) -> Result<()> {
        // 仅解码用于检测等待输入状态
        let decoded_data = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            &event.data,
        ).unwrap_or_default();

        let is_waiting = crate::shared::parser::detect_waiting_input(
            &String::from_utf8_lossy(&decoded_data)
        );

        // 构造输出消息
        let message = Message::Output {
            message_id: Uuid::new_v4().to_string(),
            session_id: event.session_id.clone(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            payload: crate::desktop::server::message::OutputPayload {
                data: event.data.clone(),
                is_waiting,
                index: event.index,
            },
        };
        let json = message.to_json()?;

        // 通过 WebSocketManager 发送消息给已认证的客户端
        // 注意：由于订阅状态未持久化，当前简化为发送给所有已认证客户端
        // 客户端会根据 sessionId 自行过滤
        let ws_manager = WebSocketManager::global();

        // 获取所有已认证的客户端
        let clients = ws_manager.list_authenticated_clients().await;

        let mut sent_count = 0;
        for client in clients {
            // 直接发送给每个客户端
            if ws_manager.send_text_to_client(&client.client_id, &json).await.is_ok() {
                sent_count += 1;
            }
        }

        tracing::debug!(
            "[OutputForwarder] Forwarded output for session {}, sent to {} clients",
            event.session_id, sent_count
        );

        Ok(())
    }
}