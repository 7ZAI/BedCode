//! Session Event Bus Trait
//!
//! 会话事件总线 trait 定义

use crate::desktop::model::{SessionRestartEvent, SessionStatusEvent};
use crate::desktop::pty::PtyOutputEvent;
use tokio::sync::broadcast;

pub enum SessionEvent {
    Output(PtyOutputEvent),
    StatusChanged(SessionStatusEvent),
    Restarted(SessionRestartEvent),
}

pub trait SessionEventBus: Send + Sync {
    fn publish(&self, event: SessionEvent);
    fn subscribe(&self) -> broadcast::Receiver<SessionEvent>;
    fn output_sender(&self) -> broadcast::Sender<PtyOutputEvent>;
    fn status_sender(&self) -> broadcast::Sender<SessionStatusEvent>;
    fn restart_sender(&self) -> broadcast::Sender<SessionRestartEvent>;
}