//! Terminal Handler - 终端消息处理器
//!
//! 处理 `Message::Terminal` 消息，包括输入、订阅、取消订阅等操作
//! 委托给具体的服务层处理

use crate::desktop::server::message::{Message as BusinessMessage, TerminalAction, TerminalPayload};
use crate::desktop::server::router::handler::RouteHandler;
use crate::desktop::server::services::terminal_service::handle_input;
use crate::desktop::session::{GlobalOutputManager, OutputEvent, SessionManager};
use crate::shared::enums::{TerminalAction as SharedTerminalAction, TerminalPayload as SharedTerminalPayload};
use crate::shared::model::message::Message;
use crate::shared::system::config::AppConfig;
use crate::shared::websocket::server::context::RouteContext;
use crate::Result;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{timeout, Duration};
use tracing::info;

/// 输出缓冲区
///
/// 累积多条 PTY 输出，减少 WebSocket 消息数量
/// 存储原始字节，flush 时统一编码为 Base64
struct OutputBuffer {
    /// 累积的原始字节数据
    data: Vec<u8>,
    /// 起始索引（用于前端去重）
    start_index: u64,
    /// 最后一条的 waiting 状态
    last_is_waiting: bool,
}

impl OutputBuffer {
    fn new() -> Self {
        Self {
            data: Vec::new(),
            start_index: 0,
            last_is_waiting: false,
        }
    }

    /// 追加一条输出事件
    fn append(&mut self, event: &OutputEvent) {
        // 第一条事件记录起始索引
        if self.is_empty() {
            self.start_index = event.index;
        }
        // 直接拼接原始字节
        self.data.extend_from_slice(&event.data);
        self.last_is_waiting = event.is_waiting;
    }

    /// 是否为空
    fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// 清空缓冲区
    fn clear(&mut self) {
        self.data.clear();
    }
}

/// 终端消息处理器
///
/// 处理所有终端相关操作：
/// - Input: 输入数据到 PTY
/// - Subscribe: 订阅会话输出
/// - Unsubscribe: 取消订阅
pub struct TerminalHandler {
    session_manager: Option<Arc<SessionManager>>,
}

impl TerminalHandler {
    pub fn new(session_manager: Option<Arc<SessionManager>>) -> Self {
        Self { session_manager }
    }
}

#[async_trait]
impl RouteHandler for TerminalHandler {
    async fn handle(
        &self,
        message: BusinessMessage,
        ctx: &RouteContext,
    ) -> Result<Option<BusinessMessage>> {
        // 解析 Terminal 消息
        let (session_id, action, _message_id) = match message {
            BusinessMessage::Terminal {
                message_id,
                session_id,
                payload,
                ..
            } => (session_id, payload.action, message_id),
            _ => return Ok(None),
        };

        // 根据动作类型分发处理
        match action {
            TerminalAction::Input { data, special_key } => {
                // 委托给 input_service 处理
                let payload = TerminalPayload {
                    action: TerminalAction::Input { data, special_key },
                };
                handle_input(&session_id, payload, &self.session_manager).await
            }
            TerminalAction::Subscribe { start_seq: _ } => {
                let global_manager = GlobalOutputManager::global();

                // 创建 OutputEvent 到 WebSocket 的转发通道
                let (output_tx, mut output_rx) = mpsc::channel::<OutputEvent>(256);

                // 获取 WebSocket 发送通道
                let ws_sender = ctx
                    .connection_manager
                    .get_sender(ctx.connection_id)
                    .await;

                match ws_sender {
                    Some(sender) => {
                        let session_id_clone = session_id.clone();
                        let client_id = ctx.client_id.clone();

                        // 从配置读取缓冲参数
                        let config = AppConfig::global();
                        let flush_interval = Duration::from_millis(config.terminal.flush_interval_ms);
                        let max_buffer_size = config.terminal.max_buffer_size;

                        // 启动转发任务：将 OutputEvent 转换为 Message 并发送到 WebSocket
                        // 使用缓冲机制减少 WebSocket 消息数量
                        tokio::spawn(async move {
                            let mut buffer = OutputBuffer::new();

                            loop {
                                match timeout(flush_interval, output_rx.recv()).await {
                                    Ok(Some(event)) => {
                                        buffer.append(&event);
                                        // 达到最大缓冲大小，立即 flush
                                        if buffer.data.len() >= max_buffer_size {
                                            if flush_buffer(&mut buffer, &sender, &session_id_clone, &client_id).await {
                                                break;
                                            }
                                        }
                                    }
                                    Ok(None) => {
                                        // channel 关闭，flush 剩余数据后退出
                                        if !buffer.is_empty() {
                                            flush_buffer(&mut buffer, &sender, &session_id_clone, &client_id).await;
                                        }
                                        break;
                                    }
                                    Err(_) => {
                                        // 超时，flush 缓冲区
                                        if !buffer.is_empty() {
                                            if flush_buffer(&mut buffer, &sender, &session_id_clone, &client_id).await {
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                            tracing::debug!(
                                "[TerminalHandler] Output forwarder stopped for client {}",
                                client_id
                            );
                        });

                        // 订阅
                        match global_manager
                            .subscribe(&session_id, &ctx.client_id, output_tx)
                            .await
                        {
                            Some(response) => {
                                info!(
                                    "Client {} subscribed to session {}, min_seq={}, max_seq={}, history_count={}",
                                    ctx.client_id, session_id, response.min_seq, response.max_seq, response.history_count
                                );
                                Ok(Some(BusinessMessage::subscribe_response(
                                    &session_id,
                                    response.min_seq,
                                    response.max_seq,
                                    response.history_count,
                                )))
                            }
                            None => {
                                tracing::warn!(
                                    "[TerminalHandler] Session {} not found for subscribe",
                                    session_id
                                );
                                Ok(Some(BusinessMessage::error(
                                    "SESSION_NOT_FOUND",
                                    &format!("Session {} not found", session_id),
                                )))
                            }
                        }
                    }
                    None => {
                        tracing::warn!(
                            "[TerminalHandler] No WebSocket sender for client {}",
                            ctx.client_id
                        );
                        Ok(Some(BusinessMessage::error(
                            "NO_CONNECTION",
                            "WebSocket connection not found",
                        )))
                    }
                }
            }
            TerminalAction::Unsubscribe => {
                let global_manager = GlobalOutputManager::global();

                if global_manager
                    .unsubscribe(&session_id, &ctx.client_id)
                    .await
                {
                    info!(
                        "Client {} unsubscribed from session {}",
                        ctx.client_id, session_id
                    );
                    Ok(Some(BusinessMessage::unsubscribe_response(&session_id)))
                } else {
                    Ok(Some(BusinessMessage::error(
                        "SESSION_NOT_FOUND",
                        &format!("Session {} not found", session_id),
                    )))
                }
            }
            // 其他动作类型不需要处理（如 Output, SubscribeResponse, UnsubscribeResponse）
            _ => Ok(None),
        }
    }
}

/// Flush 缓冲区到 WebSocket
///
/// 返回 true 表示发送失败（连接已断开），应退出转发任务
async fn flush_buffer(
    buffer: &mut OutputBuffer,
    sender: &tokio::sync::mpsc::Sender<tokio_tungstenite::tungstenite::Message>,
    session_id: &str,
    client_id: &str,
) -> bool {
    if buffer.is_empty() {
        return false;
    }

    // 将原始字节编码为 Base64
    let data_base64 = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        &buffer.data,
    );

    let message = Message::output_from_base64(
        session_id,
        &data_base64,
        buffer.last_is_waiting,
        buffer.start_index as usize,
    );

    if let Ok(json) = message.to_json() {
        if sender
            .send(tokio_tungstenite::tungstenite::Message::Text(json))
            .await
            .is_err()
        {
            tracing::warn!(
                "[TerminalHandler] Failed to send output to client {}",
                client_id
            );
            return true;
        }
    }

    buffer.clear();
    false
}
