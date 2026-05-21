//! Event Forwarder
//!
//! 将 SessionManager 的内部事件统一转发到 Tauri 前端

use crate::desktop::session::SessionManager;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

/// 事件转发器 - 将 SessionManager 的事件转发到 Tauri 前端
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
        self.forward_restart_events();
        self.forward_output_events();
    }

    /// 转发会话状态变化事件
    fn forward_status_events(&self) {
        let app_handle = self.app_handle.clone();
        let mut rx = self.session_manager.subscribe_status();
        tauri::async_runtime::spawn(async move {
            while let Ok(event) = rx.recv().await {
                if let Err(e) = app_handle.emit("session-status-changed", &event) {
                    tracing::error!("Failed to emit session-status-changed event: {}", e);
                }
            }
        });
    }

    /// 转发会话重启事件
    fn forward_restart_events(&self) {
        let app_handle = self.app_handle.clone();
        let mut rx = self.session_manager.subscribe_restart();
        tauri::async_runtime::spawn(async move {
            while let Ok(event) = rx.recv().await {
                if let Err(e) = app_handle.emit("session-restarted", &event) {
                    tracing::error!("Failed to emit session-restarted event: {}", e);
                }
            }
        });
    }

    /// 转发 PTY 输出事件
    fn forward_output_events(&self) {
        let app_handle = self.app_handle.clone();
        let mut rx = self.session_manager.subscribe_output();
        tauri::async_runtime::spawn(async move {
            while let Ok(event) = rx.recv().await {
                if let Err(e) = app_handle.emit("session-output", &event) {
                    tracing::error!("Failed to emit session-output event: {}", e);
                }
            }
        });
    }
}