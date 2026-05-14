//! Output Forwarder Service
//!
//! 负责将 PTY 输出转发给订阅了相应会话的客户端

use crate::desktop::pty::PtyOutputEvent;
use crate::desktop::session::SessionManager;
use crate::desktop::server::client_info::ClientInfo;
use crate::desktop::server::message::Message;
use crate::Result;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tokio_tungstenite::tungstenite::protocol::Message as WsMessage;
use uuid::Uuid;

/// 输出转发器
///
/// 负责将 PTY 输出转发给订阅了相应会话的客户端
pub struct OutputForwarder {
    session_manager: Arc<SessionManager>,
    clients: Arc<RwLock<HashMap<SocketAddr, ClientInfo>>>,
    client_senders: Arc<RwLock<HashMap<SocketAddr, tokio::sync::mpsc::UnboundedSender<WsMessage>>>>,
}

impl OutputForwarder {
    /// 创建新的输出转发器
    pub fn new(
        session_manager: Arc<SessionManager>,
        clients: Arc<RwLock<HashMap<SocketAddr, ClientInfo>>>,
        client_senders: Arc<RwLock<HashMap<SocketAddr, tokio::sync::mpsc::UnboundedSender<WsMessage>>>>,
    ) -> Self {
        Self {
            session_manager,
            clients,
            client_senders,
        }
    }

    /// 运行输出转发器
    pub async fn run(&self, shutdown_rx: &mut broadcast::Receiver<()>) {
        let mut output_rx = self.session_manager.subscribe_output();

        loop {
            tokio::select! {
                result = output_rx.recv() => {
                    match result {
                        Ok(event) => {
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
        // 仅解码用于检测等待输入状态，不重复编码
        let decoded_data = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            &event.data,
        ).unwrap_or_default();

        let is_waiting = crate::shared::parser::detect_waiting_input(
            &String::from_utf8_lossy(&decoded_data)
        );

        // 直接使用 PTY 事件中的 base64 数据构造 Output 消息
        let message = Message::Output {
            message_id: Uuid::new_v4().to_string(),
            session_id: event.session_id.clone(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            payload: crate::desktop::server::message::OutputPayload {
                data: event.data.clone(),
                is_waiting,
            },
        };
        let json = message.to_json()?;
        let ws_message = WsMessage::Text(json);

        // 获取所有订阅了该会话的客户端
        let clients = self.clients.read().await;
        let senders = self.client_senders.read().await;

        for (addr, client) in clients.iter() {
            if client.authenticated && client.subscribed_sessions.contains(&event.session_id) {
                if let Some(tx) = senders.get(addr) {
                    if tx.send(ws_message.clone()).is_err() {
                        tracing::debug!("Failed to send output to client {}", addr);
                    }
                }
            }
        }

        Ok(())
    }
}