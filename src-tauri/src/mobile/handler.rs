//! Mobile Message Handler
//!
//! 实现 ClientMessageHandler trait，处理来自服务器的各类消息

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use tokio::sync::broadcast;
use tracing;
use crate::shared::websocket::{
    ClientMessageHandler, HandlerResult,
};
use crate::shared::model::message::Message;

/// 输出消息的 payload 数据结构
#[derive(Debug, Clone, serde::Deserialize)]
pub struct OutputPayloadData {
    pub data: String,
    pub is_waiting: bool,
    pub index: usize,
}

/// Mobile 消息类型（与桌面端协商的业务协议）
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MobileMessage {
    /// 认证相关
    Auth {
        stage: String,
        device_id: Option<String>,
        device_fingerprint: Option<String>,
        device_name: Option<String>,
        session_token: Option<String>,
        pairing_code: Option<String>,
        qr_token: Option<String>,
        error: Option<String>,
    },
    /// 控制消息
    Control {
        action: serde_json::Value,
    },
    /// 输入消息
    Input {
        data: String,
        special_key: Option<String>,
    },
    /// 输出消息
    Output {
        session_id: String,
        payload: OutputPayloadData,
    },
    /// 订阅响应
    SubscribeResponse {
        session_id: String,
        current_max_seq: u64,
        history_count: usize,
    },
    /// 取消订阅响应
    UnsubscribeResponse {
        session_id: String,
    },
    /// 错误
    Error {
        message: Option<String>,
        code: Option<String>,
    },
    /// 服务器关闭
    ServerClosed {
        reason: Option<String>,
    },
}

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
        current_max_seq: u64,
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
}

/// Mobile 消息处理器
pub struct MobileHandler {
    event_tx: broadcast::Sender<MobileEvent>,
}

impl MobileHandler {
    pub fn new() -> Arc<Self> {
        let (event_tx, _) = broadcast::channel(1024);
        Arc::new(Self { event_tx })
    }

    /// 订阅事件
    pub fn subscribe(&self) -> broadcast::Receiver<MobileEvent> {
        self.event_tx.subscribe()
    }

    /// 发送事件
    fn send_event(&self, event: MobileEvent) {
        let _ = self.event_tx.send(event);
    }

    /// 解析消息载荷为 MobileMessage
    fn parse_message(message: &Message) -> Option<MobileMessage> {
        let json = message.to_json().ok()?;
        serde_json::from_str(&json).ok()
    }
}

impl ClientMessageHandler for MobileHandler {
    fn handle(
        &self,
        message: Message,
    ) -> Pin<Box<dyn Future<Output = HandlerResult> + Send + '_>> {
        let self_clone = self.clone();
        Box::pin(async move {
            // 直接使用 Message 类型，通过 to_json 获取 JSON 字符串
            let json = message.to_json().ok();
            let mobile_msg: MobileMessage = match json {
                Some(j) => serde_json::from_str(&j)
                    .map_err(|e| crate::AppError::Parse(format!("Failed to parse mobile message: {}", e)))?,
                None => return Ok(None),
            };

            match mobile_msg {
                MobileMessage::Output { session_id, payload } => {
                    let is_waiting = payload.is_waiting;
                    let data_len = payload.data.len();
                    let index = payload.index;
                    tracing::debug!("[MobileHandler] Output received: session_id={}, data_len={}, is_waiting={}, index={}", session_id, data_len, is_waiting, index);
                    self_clone.send_event(MobileEvent::Output {
                        session_id: session_id.clone(),
                        data: payload.data.clone(),
                        is_waiting,
                        index,
                    });
                    tracing::debug!("[MobileHandler] Output event sent: session_id={}", session_id);
                }
                MobileMessage::Auth { stage, device_id, session_token, error, .. } => {
                    match stage.as_str() {
                        "authenticated" => {
                            if let (Some(device_id), Some(session_token)) = (device_id, session_token) {
                                self_clone.send_event(MobileEvent::AuthSuccess {
                                    device_id,
                                    session_token,
                                });
                            }
                        }
                        "verify_code" => {
                            self_clone.send_event(MobileEvent::PairingVerified);
                        }
                        "error" => {
                            let reason = error.unwrap_or_else(|| "Authentication failed".to_string());
                            self_clone.send_event(MobileEvent::AuthFailed { reason });
                        }
                        _ => {}
                    }
                }
                MobileMessage::ServerClosed { reason } => {
                    let reason = reason.unwrap_or_else(|| "Unknown".to_string());
                    self_clone.send_event(MobileEvent::ServerClosed { reason });
                }
                MobileMessage::SubscribeResponse { session_id, current_max_seq, history_count } => {
                    tracing::debug!("[MobileHandler] SubscribeResponse: session_id={}, current_max_seq={}, history_count={}",
                        session_id, current_max_seq, history_count);
                    self_clone.send_event(MobileEvent::SubscribeResponse {
                        session_id,
                        current_max_seq,
                        history_count,
                    });
                }
                MobileMessage::UnsubscribeResponse { session_id } => {
                    tracing::debug!("[MobileHandler] UnsubscribeResponse: session_id={}", session_id);
                    self_clone.send_event(MobileEvent::UnsubscribeResponse {
                        session_id,
                    });
                }
                MobileMessage::Error { message, code } => {
                    let msg = message.or(code).unwrap_or_else(|| "Unknown error".to_string());
                    self_clone.send_event(MobileEvent::Error { message: msg });
                }
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
        Self { event_tx }
    }
}

impl Clone for MobileHandler {
    fn clone(&self) -> Self {
        Self {
            event_tx: self.event_tx.clone(),
        }
    }
}