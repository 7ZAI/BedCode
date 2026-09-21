//! Event Forwarder
//!
//! 将 SessionManager 的内部事件统一转发到 Tauri 前端

use crate::session::SessionManager;
use crate::system::constants::event;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

/// 事件转发器 - 将 SessionManager 的状态事件转发到 Tauri 前端
///
/// 重启事件（`session-restarted`）自 v21 起由 `com.bedcode.terminal-session` 插件在 Created
/// 生命周期之后经 `host-events.emit` 补发，内核不再有该广播通道。
pub struct EventForwarder {
    app_handle: AppHandle,
    session_manager: Arc<SessionManager>,
}

impl EventForwarder {
    pub fn new(app_handle: AppHandle, session_manager: Arc<SessionManager>) -> Self {
        Self {
            app_handle,
            session_manager,
        }
    }

    /// 启动所有事件监听和转发
    pub fn start(&self) {
        self.forward_status_events();
    }

    /// 转发会话状态变化事件
    fn forward_status_events(&self) {
        let app_handle = self.app_handle.clone();
        let mut rx = self.session_manager.subscribe_status();
        tauri::async_runtime::spawn(async move {
            while let Ok(event) = rx.recv().await {
                if let Err(e) = app_handle.emit(event::SESSION_STATUS_CHANGED, &event) {
                    tracing::error!("Failed to emit {} event: {}", event::SESSION_STATUS_CHANGED, e);
                }
            }
        });
    }
}
