//! Event Bus
//!
//! 统一事件广播 - 会话状态广播通道
//! SessionEventBus trait 已内联到此文件
//!
//! v21 起「会话重启」不再有内核广播通道：重启编排归 `com.bedcode.session` 插件
//! （`remove` + `create-with-spec`），前端 `session-restarted` 事件由插件在 Created
//! 生命周期之后经 `host-events.emit` 补发。

use crate::session::SessionStatusEvent;
use crate::system::config::AppConfig;
use tokio::sync::broadcast;

/// 会话事件类型
#[derive(Debug, Clone)]
pub enum SessionEvent {
    StatusChanged(SessionStatusEvent),
}

/// 会话事件总线
pub trait SessionEventBus: Send + Sync {
    fn publish(&self, event: SessionEvent);
    fn subscribe(&self) -> broadcast::Receiver<SessionEvent>;
    fn status_sender(&self) -> broadcast::Sender<SessionStatusEvent>;
}

/// 会话事件总线实现
pub struct DefaultSessionEventBus {
    status_tx: broadcast::Sender<SessionStatusEvent>,
    event_tx: broadcast::Sender<SessionEvent>,
}

impl DefaultSessionEventBus {
    pub fn new() -> Self {
        let config = AppConfig::global();
        let (status_tx, _) = broadcast::channel(config.channels.status_broadcast_capacity);
        let (event_tx, _) = broadcast::channel(config.channels.event_broadcast_capacity);

        Self { status_tx, event_tx }
    }
}

impl Default for DefaultSessionEventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionEventBus for DefaultSessionEventBus {
    fn publish(&self, event: SessionEvent) {
        match &event {
            SessionEvent::StatusChanged(e) => {
                if self.status_tx.receiver_count() > 0 {
                    let _ = self.status_tx.send(e.clone());
                }
            }
        }
        let _ = self.event_tx.send(event);
    }

    fn subscribe(&self) -> broadcast::Receiver<SessionEvent> {
        self.event_tx.subscribe()
    }

    fn status_sender(&self) -> broadcast::Sender<SessionStatusEvent> {
        self.status_tx.clone()
    }
}
