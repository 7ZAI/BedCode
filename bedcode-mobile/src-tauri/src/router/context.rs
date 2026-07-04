//! Client Route Context - 路由上下文
//!
//! 包含事件发送器，供 handler 发送业务事件

use std::sync::Arc;
use tokio::sync::broadcast;

use super::MobileEvent;

/// 客户端路由上下文
pub struct ClientRouteContext {
    /// 业务事件发送器（发送 MobileEvent 给前端）
    event_tx: broadcast::Sender<MobileEvent>,
}

impl ClientRouteContext {
    pub fn new(event_tx: broadcast::Sender<MobileEvent>) -> Arc<Self> {
        Arc::new(Self { event_tx })
    }

    /// 发送业务事件
    pub fn emit(&self, event: MobileEvent) {
        // 高频输出事件用 debug 级别，避免日志刷屏
        match &event {
            MobileEvent::Output { .. } => {
                tracing::debug!("[ClientRouteContext] emit: Output event");
            }
            _ => {
                tracing::info!("[ClientRouteContext] emit: {:?}", event);
            }
        }
        if let Err(e) = self.event_tx.send(event) {
            tracing::error!("[ClientRouteContext] Failed to send event: {}", e);
        }
    }
}