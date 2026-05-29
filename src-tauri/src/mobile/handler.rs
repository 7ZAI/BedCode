//! Mobile Message Handler
//!
//! 实现 MessageHandler trait，处理来自服务器的各类消息

use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;

use tokio::sync::{broadcast, mpsc};
use tracing;
use crate::shared::websocket::{
    ClientMessageHandler, HandlerResult, MessageHandler, WsClientEvent,
};
use crate::shared::model::message::Message;
use crate::shared::enums::{SyncPayload, TerminalAction, TerminalPayload};
use crate::shared::enums::auth::AuthStage;
use crate::shared::enums::sumary::{SessionConfigSummary, SessionSummary};
use tokio_tungstenite::tungstenite::protocol::Message as WsMsg;

/// Mobile 业务事件（发送给前端）
#[derive(Debug, Clone)]
pub enum MobileEvent {
    /// 连接成功
    Connected,
    /// 断开连接
    Disconnected,
    /// 收到输出
    Output {
        session_id: String,
        data: String,
        is_waiting: bool,
        /// 全局递增索引，用于去重
        index: usize,
    },
    /// 订阅响应
    SubscribeResponse {
        session_id: String,
        min_seq: u64,
        max_seq: u64,
        history_count: usize,
    },
    /// 取消订阅响应
    UnsubscribeResponse {
        session_id: String,
    },
    /// 认证成功
    AuthSuccess {
        device_id: String,
        session_token: String,
    },
    /// 认证失败
    AuthFailed {
        reason: String,
    },
    /// 配对请求
    PairingRequest {
        device_name: String,
    },
    /// 配对码验证
    PairingVerified,
    /// 错误
    Error {
        message: String,
    },
    /// 服务器关闭
    ServerClosed {
        reason: String,
    },
    /// 确认响应（服务端默认响应）
    Ack {
        request_id: String,
    },

    // === 同步数据事件 ===
    /// 会话创建同步
    SyncSessionCreated {
        session: SessionSummary,
        source_device: String,
    },
    /// 会话状态变化同步
    SyncSessionStatusChanged {
        session_id: String,
        old_status: String,
        new_status: String,
        session_name: String,
    },
    /// 会话停止同步
    SyncSessionStopped {
        session_id: String,
        session_name: String,
    },
    /// 会话删除同步
    SyncSessionRemoved {
        session_id: String,
        session_name: String,
    },
    /// 配置创建同步
    SyncConfigCreated {
        config: SessionConfigSummary,
        source_device: String,
    },
    /// 配置更新同步
    SyncConfigUpdated {
        config: SessionConfigSummary,
        source_device: String,
    },
    /// 配置删除同步
    SyncConfigRemoved {
        config_id: String,
        config_name: String,
    },
}

/// Mobile 消息处理器
///
/// 处理来自服务器的消息：
/// 1. 发送 WsClientEvent::TextMessage 到 ws_event_tx（用于 send_and_wait 响应匹配）
/// 2. 发送 MobileEvent 到 event_tx（用于业务层事件订阅）
pub struct MobileHandler {
    /// 业务事件发送器（发送 MobileEvent 给前端）
    event_tx: broadcast::Sender<MobileEvent>,
    /// WebSocket 事件发送器（发送 WsClientEvent 用于 send_and_wait 响应匹配）
    ws_event_tx: Option<broadcast::Sender<WsClientEvent>>,
}

impl MobileHandler {
    pub fn new() -> Arc<Self> {
        let (event_tx, _) = broadcast::channel(1024);
        Arc::new(Self { event_tx, ws_event_tx: None })
    }

    /// 设置 WebSocket 事件发送器（由 ConnectionManager 在 set_handler 前调用）
    pub fn with_ws_event_tx(mut self: Arc<Self>, tx: broadcast::Sender<WsClientEvent>) -> Arc<Self> {
        // 需要解包 Arc 来修改内部字段
        let inner = Arc::try_unwrap(self).unwrap_or_else(|arc| {
            // 如果 Arc 有多个引用，克隆内部数据创建新实例
            (*arc).clone()
        });
        Arc::new(Self {
            event_tx: inner.event_tx,
            ws_event_tx: Some(tx),
        })
    }

    /// 订阅事件
    pub fn subscribe(&self) -> broadcast::Receiver<MobileEvent> {
        self.event_tx.subscribe()
    }

    /// 发送事件
    fn send_event(&self, event: MobileEvent) {
        tracing::info!("[MobileHandler] send_event: {:?}", event);
        if let Err(e) = self.event_tx.send(event) {
            tracing::error!("[MobileHandler] Failed to send event: {}", e);
        }
    }
}

/// 处理原始 WebSocket 消息，解析后委托给 ClientMessageHandler
impl MessageHandler for MobileHandler {
    fn handle(
        &self,
        raw_message: WsMsg,
        _addr: SocketAddr,
        _client_id: Option<&str>,
        _sender: Option<mpsc::Sender<WsMsg>>,
    ) {
        // 只处理 Text 类型
        let text = match raw_message {
            WsMsg::Text(text) => text,
            WsMsg::Binary(data) => {
                tracing::debug!("[MobileHandler] Binary received: {} bytes", data.len());
                return;
            }
            _ => return,
        };

        tracing::info!("[MobileHandler] Raw text received: {}...", &text[..text.len().min(200)]);

        // 解析为 Message
        let parsed: Option<Message> = serde_json::from_str(&text).ok();

        if let Some(ref message) = parsed {
            tracing::info!("[MobileHandler] Message parsed: message_id={:?}", message.message_id());
            // 获取 message_id 用于响应匹配
            let message_id = message.message_id().map(|s| s.to_string());

            // 发送 TextMessage 事件到 ws_event_tx（用于 send_and_wait 响应匹配）
            // 这是关键：让 WsClient::send_and_wait 能收到响应
            if let Some(ref ws_event_tx) = self.ws_event_tx {
                let _ = ws_event_tx.send(WsClientEvent::TextMessage {
                    message_id,
                    content: text.clone(),
                });
            }
        }

        // 如果解析成功，通过 spawn 委托给 ClientMessageHandler
        if let Some(message) = parsed {
            let self_clone = self.clone();
            tokio::spawn(async move {
                let _ = <Self as ClientMessageHandler>::handle(&self_clone, message).await;
            });
        }
    }
}

impl ClientMessageHandler for MobileHandler {
    fn handle(
        &self,
        message: Message,
    ) -> Pin<Box<dyn Future<Output = HandlerResult> + Send + '_>> {
        let self_clone = self.clone();
        Box::pin(async move {
            // 直接使用 Message 类型处理消息，避免不必要的序列化
            match message {
                Message::Terminal { session_id, payload, .. } => {
                    // 处理终端消息的各种动作
                    match payload.action {
                        TerminalAction::Output { data, is_waiting, index } => {
                            tracing::debug!("[MobileHandler] Output received: session_id={}, data_len={}, is_waiting={}, index={}", session_id, data.len(), is_waiting, index);
                            self_clone.send_event(MobileEvent::Output {
                                session_id: session_id.clone(),
                                data,
                                is_waiting,
                                index,
                            });
                        }
                        TerminalAction::SubscribeResponse { min_seq, max_seq, history_count } => {
                            tracing::debug!("[MobileHandler] SubscribeResponse: session_id={}, min_seq={}, max_seq={}, history_count={}",
                                session_id, min_seq, max_seq, history_count);
                            self_clone.send_event(MobileEvent::SubscribeResponse {
                                session_id,
                                min_seq,
                                max_seq,
                                history_count,
                            });
                        }
                        TerminalAction::UnsubscribeResponse => {
                            tracing::debug!("[MobileHandler] UnsubscribeResponse: session_id={}", session_id);
                            self_clone.send_event(MobileEvent::UnsubscribeResponse {
                                session_id,
                            });
                        }
                        // 其他动作类型（Input, Subscribe, Unsubscribe）在移动端不处理
                        _ => {}
                    }
                }
                Message::Auth { payload, .. } => {
                    match payload.stage {
                        AuthStage::Authenticated => {
                            if let (Some(device_id), Some(session_token)) = (payload.device_id, payload.session_token) {
                                self_clone.send_event(MobileEvent::AuthSuccess {
                                    device_id,
                                    session_token,
                                });
                            }
                        }
                        AuthStage::VerifyCode => {
                            self_clone.send_event(MobileEvent::PairingVerified);
                        }
                        AuthStage::Failed => {
                            let reason = payload.error.unwrap_or_else(|| "Authentication failed".to_string());
                            self_clone.send_event(MobileEvent::AuthFailed { reason });
                        }
                        _ => {}
                    }
                }
                Message::ServerClosed { reason, .. } => {
                    self_clone.send_event(MobileEvent::ServerClosed { reason });
                }
                Message::Error { message, code, .. } => {
                    let msg = if !message.is_empty() { message } else { code };
                    self_clone.send_event(MobileEvent::Error { message: msg });
                }
                Message::Ack { request_id, .. } => {
                    tracing::debug!("[MobileHandler] Ack received for request_id={}", request_id);
                    self_clone.send_event(MobileEvent::Ack { request_id });
                }
                Message::SyncData { payload, .. } => {
                    tracing::info!("[MobileHandler] SyncData received: {:?}", payload);
                    match payload {
                        SyncPayload::SessionCreated { session, source_device } => {
                            self_clone.send_event(MobileEvent::SyncSessionCreated {
                                session,
                                source_device,
                            });
                        }
                        SyncPayload::SessionStatusChanged { session_id, old_status, new_status, session_name } => {
                            self_clone.send_event(MobileEvent::SyncSessionStatusChanged {
                                session_id,
                                old_status,
                                new_status,
                                session_name,
                            });
                        }
                        SyncPayload::SessionStopped { session_id, session_name } => {
                            self_clone.send_event(MobileEvent::SyncSessionStopped {
                                session_id,
                                session_name,
                            });
                        }
                        SyncPayload::SessionRemoved { session_id, session_name } => {
                            self_clone.send_event(MobileEvent::SyncSessionRemoved {
                                session_id,
                                session_name,
                            });
                        }
                        SyncPayload::ConfigCreated { config, source_device } => {
                            self_clone.send_event(MobileEvent::SyncConfigCreated {
                                config,
                                source_device,
                            });
                        }
                        SyncPayload::ConfigUpdated { config, source_device } => {
                            tracing::info!("[MobileHandler] ConfigUpdated: config_id={}, source={}", config.id, source_device);
                            self_clone.send_event(MobileEvent::SyncConfigUpdated {
                                config,
                                source_device,
                            });
                        }
                        SyncPayload::ConfigRemoved { config_id, config_name } => {
                            self_clone.send_event(MobileEvent::SyncConfigRemoved {
                                config_id,
                                config_name,
                            });
                        }
                    }
                }
                // 其他消息类型不处理
                _ => {}
            }

            Ok(None)
        })
    }

    fn name(&self) -> &str {
        "MobileHandler"
    }
}

impl Default for MobileHandler {
    fn default() -> Self {
        let (event_tx, _) = broadcast::channel(1024);
        Self { event_tx, ws_event_tx: None }
    }
}

impl Clone for MobileHandler {
    fn clone(&self) -> Self {
        Self {
            event_tx: self.event_tx.clone(),
            ws_event_tx: self.ws_event_tx.clone(),
        }
    }
}
