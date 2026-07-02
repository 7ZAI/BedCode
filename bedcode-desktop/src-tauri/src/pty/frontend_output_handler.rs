//! Frontend Output Handler
//!
//! 向前端发送 PTY 输出事件的 Handler 实现

use crate::pty::PtyOutputEvent;
use crate::pty::PtyOutputHandler;
use async_trait::async_trait;
use tauri::{AppHandle, Emitter};

/// 向前端发送 PTY 输出事件的 Handler
pub struct FrontendOutputHandler {
    name: String,
    app_handle: AppHandle,
}

impl FrontendOutputHandler {
    pub fn new(app_handle: AppHandle) -> Self {
        Self {
            name: "FrontendOutputHandler".to_string(),
            app_handle,
        }
    }

    pub fn with_name(app_handle: AppHandle, name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            app_handle,
        }
    }
}

#[async_trait]
impl PtyOutputHandler for FrontendOutputHandler {
    async fn handle(&self, event: PtyOutputEvent) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match self.app_handle.emit("pty-output", &event) {
            Ok(()) => Ok(()),
            Err(e) => {
                tracing::error!("[FrontendOutputHandler] Failed to emit pty-output event: {}", e);
                Err(Box::new(e))
            }
        }
    }

    fn name(&self) -> &str {
        &self.name
    }
}