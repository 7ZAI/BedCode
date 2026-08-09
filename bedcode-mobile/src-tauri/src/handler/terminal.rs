//! Terminal Handler - 终端消息处理器

use async_trait::async_trait;

use crate::model::message::Message;
use crate::enums::TerminalAction;
use crate::Result;

use crate::router::{ClientRouteContext, MobileEvent, ClientRouteHandler};

/// 终端消息处理器
pub struct TerminalHandler;

#[async_trait]
impl ClientRouteHandler for TerminalHandler {
    async fn handle(&self, message: Message, ctx: &ClientRouteContext) -> Result<Option<Message>> {
        if let Message::Terminal { session_id, payload, .. } = message {
            match payload.action {
                TerminalAction::Output { data, is_waiting, index, end_index, start_offset, end_offset } => {
                    // 输出事件：直接转发，高频操作不记录详细日志
                    ctx.emit(MobileEvent::Output {
                        session_id,
                        data,
                        is_waiting,
                        index: index as u64,
                        end_index: end_index.map(|ei| ei as u64),
                        start_offset,
                        end_offset,
                    });
                }
                TerminalAction::SubscribeResponse { min_seq, max_seq, history_count, mode, min_offset, max_offset } => {
                    tracing::info!(
                        "[TerminalHandler] SubscribeResponse: session_id={}, seq_range={}-{}, history={}",
                        session_id, min_seq, max_seq, history_count
                    );
                    // SubscribeResponse 不转发到前端，仅记录日志
                }
                TerminalAction::UnsubscribeResponse => {
                    tracing::debug!("[TerminalHandler] UnsubscribeResponse: session_id={}", session_id);
                    // UnsubscribeResponse 不转发到前端，仅记录日志
                }
                // 其他动作类型（Input, Subscribe, Unsubscribe）在移动端不处理
                _ => {
                    tracing::warn!("[TerminalHandler] Unhandled action type: {:?}", payload.action);
                }
            }
        } else {
            tracing::warn!("[TerminalHandler] Message is not Terminal type");
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
