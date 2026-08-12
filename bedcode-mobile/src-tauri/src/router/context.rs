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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn emit_delivers_event_to_receiver() {
        // 核心职责：emit 的事件必须能被接收者收到
        let (tx, mut rx) = broadcast::channel(16);
        let ctx = ClientRouteContext::new(tx);

        ctx.emit(MobileEvent::PairingRequest);
        ctx.emit(MobileEvent::Paired);

        // 先 drop 掉构造时未使用的接收端避免干扰？broadcast::channel 返回的
        // rx 就是我们持有的唯一接收者，收两条
        assert!(matches!(rx.recv().await, Ok(MobileEvent::PairingRequest)));
        assert!(matches!(rx.recv().await, Ok(MobileEvent::Paired)));
    }

    #[tokio::test]
    async fn emit_does_not_panic_without_receiver() {
        // 无接收者时 send 返回 Err，emit 内部吞掉只记日志，必须不 panic
        let (tx, rx) = broadcast::channel(4);
        drop(rx); // 模拟无接收者
        let ctx = ClientRouteContext::new(tx);

        ctx.emit(MobileEvent::PairingRequest);
        ctx.emit(MobileEvent::Error {
            message: "x".to_string(),
        });
    }

    #[tokio::test]
    async fn emit_preserves_order_across_types() {
        // 事件序对前端状态机重要（如 PairingRequest 必须先于 Paired）
        let (tx, mut rx) = broadcast::channel(16);
        let ctx = ClientRouteContext::new(tx);

        ctx.emit(MobileEvent::AuthSuccess {
            session_token: "tok".to_string(),
        });
        ctx.emit(MobileEvent::ServerClosed {
            reason: "bye".to_string(),
        });
        ctx.emit(MobileEvent::Ack {
            request_id: "r1".to_string(),
        });

        assert!(matches!(
            rx.recv().await,
            Ok(MobileEvent::AuthSuccess { .. })
        ));
        assert!(matches!(
            rx.recv().await,
            Ok(MobileEvent::ServerClosed { .. })
        ));
        assert!(matches!(rx.recv().await, Ok(MobileEvent::Ack { .. })));
    }

    #[tokio::test]
    async fn new_returns_usable_arc_context() {
        // new() 的返回值必须可直接用于 handler 侧（Arc<Self> 语义）
        let (tx, mut rx) = broadcast::channel(2);
        let ctx = ClientRouteContext::new(tx);
        ctx.emit(MobileEvent::Paired);
        assert!(matches!(rx.recv().await, Ok(MobileEvent::Paired)));
    }
}