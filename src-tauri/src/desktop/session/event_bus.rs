//! Event Bus
//!
//! 统一事件广播 - 整合 output/status/restart 三个独立广播通道

use crate::desktop::model::{SessionRestartEvent, SessionStatusEvent};
use crate::desktop::pty::PtyOutputEvent;
use tokio::sync::broadcast;

/// 会话事件类型
#[derive(Debug, Clone)]
pub enum SessionEvent {
    Output(PtyOutputEvent),
    StatusChanged(SessionStatusEvent),
    Restarted(SessionRestartEvent),
}

/// 会话事件总线 trait
pub trait SessionEventBus: Send + Sync {
    fn publish(&self, event: SessionEvent);
    fn subscribe(&self) -> broadcast::Receiver<SessionEvent>;
    fn output_sender(&self) -> broadcast::Sender<PtyOutputEvent>;
    fn status_sender(&self) -> broadcast::Sender<SessionStatusEvent>;
    fn restart_sender(&self) -> broadcast::Sender<SessionRestartEvent>;
}

/// 会话事件总线实现
pub struct DefaultSessionEventBus {
    output_tx: broadcast::Sender<PtyOutputEvent>,
    status_tx: broadcast::Sender<SessionStatusEvent>,
    restart_tx: broadcast::Sender<SessionRestartEvent>,
    event_tx: broadcast::Sender<SessionEvent>,
}

impl DefaultSessionEventBus {
    pub fn new() -> Self {
        let (output_tx, _) = broadcast::channel(2048);
        let (status_tx, _) = broadcast::channel(64);
        let (restart_tx, _) = broadcast::channel(64);
        let (event_tx, _) = broadcast::channel(256);

        Self {
            output_tx,
            status_tx,
            restart_tx,
            event_tx,
        }
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
            SessionEvent::Output(e) => {
                if self.output_tx.receiver_count() > 0 {
                    let _ = self.output_tx.send(e.clone());
                }
            }
            SessionEvent::StatusChanged(e) => {
                if self.status_tx.receiver_count() > 0 {
                    let _ = self.status_tx.send(e.clone());
                }
            }
            SessionEvent::Restarted(e) => {
                if self.restart_tx.receiver_count() > 0 {
                    let _ = self.restart_tx.send(e.clone());
                }
            }
        }
        let _ = self.event_tx.send(event);
    }

    fn subscribe(&self) -> broadcast::Receiver<SessionEvent> {
        self.event_tx.subscribe()
    }

    fn output_sender(&self) -> broadcast::Sender<PtyOutputEvent> {
        self.output_tx.clone()
    }

    fn status_sender(&self) -> broadcast::Sender<SessionStatusEvent> {
        self.status_tx.clone()
    }

    fn restart_sender(&self) -> broadcast::Sender<SessionRestartEvent> {
        self.restart_tx.clone()
    }
}