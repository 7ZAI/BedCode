//! Event Forwarder
//!
//! 将 SessionManager 的内部事件统一转发到 Tauri 前端
//! 注意：PTY 输出事件由 FrontendOutputHandler 直接发送 pty-output，不在此转发

use crate::desktop::session::SessionManager;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

/// 事件转发器 - 将 SessionManager 的状态/重启事件转发到 Tauri 前端
/// PTY 输出事件由 FrontendOutputHandler 通过 pty-output 事件发送
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
        // PTY 输出事件由 FrontendOutputHandler 直接处理，无需在此转发
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
}