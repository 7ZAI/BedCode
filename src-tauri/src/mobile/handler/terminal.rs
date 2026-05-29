//! Terminal Handler - 终端消息处理器

use async_trait::async_trait;

use crate::shared::model::message::Message;
use crate::shared::enums::TerminalAction;
use crate::Result;

use crate::mobile::router::{ClientRouteContext, MobileEvent, ClientRouteHandler};

/// 终端消息处理器
pub struct TerminalHandler;

#[async_trait]
impl ClientRouteHandler for TerminalHandler {
    async fn handle(&self, message: Message, ctx: &ClientRouteContext) -> Result<Option<Message>> {
        if let Message::Terminal { session_id, payload, .. } = message {
            match payload.action {
                TerminalAction::Output { data, is_waiting, index } => {
                    tracing::debug!("[TerminalHandler] Output: session_id={}, data_len={}, is_waiting={}, index={}",
                        session_id, data.len(), is_waiting, index);
                    ctx.emit(MobileEvent::Output {
                        session_id,
                        data,
                        is_waiting,
                        index,
                    });
                }
                TerminalAction::SubscribeResponse { min_seq, max_seq, history_count } => {
                    tracing::debug!("[TerminalHandler] SubscribeResponse: session_id={}, min_seq={}, max_seq={}, history_count={}",
                        session_id, min_seq, max_seq, history_count);
                    ctx.emit(MobileEvent::SubscribeResponse {
                        session_id,
                        min_seq,
                        max_seq,
                        history_count,
                    });
                }
                TerminalAction::UnsubscribeResponse => {
                    tracing::debug!("[TerminalHandler] UnsubscribeResponse: session_id={}", session_id);
                    ctx.emit(MobileEvent::UnsubscribeResponse {
                        session_id,
                    });
                }
                // 其他动作类型（Input, Subscribe, Unsubscribe）在移动端不处理
                _ => {}
            }
        }
        Ok(None)
    }

    fn name(&self) -> &str {
        "TerminalHandler"
    }
}

impl Default for TerminalHandler {
    fn default() -> Self {
        Self
    }
}