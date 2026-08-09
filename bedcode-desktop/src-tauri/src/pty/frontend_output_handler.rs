//! Frontend Output Handler
//!
//! 向前端发送 PTY 输出事件（兼容通道：插件输出变换已迁移到
//! SessionOutputManager::on_output 统一真源，此处仅透传原始事件）
//! 通过 broadcast channel 订阅输出，在独立 task 中 recv 循环消费并 emit Tauri 事件
//! 事件名按 session 分 channel：pty-output-{session_id}，避免多会话时无意义 IPC

use crate::pty::PtyOutputEvent;
use crate::system::error_boundary::spawn_with_error_boundary;
use tauri::{AppHandle, Emitter};
use tokio::sync::broadcast;

/// 向前端发送 PTY 输出事件的 Handler
///
/// 启动一个后台 task 从 broadcast receiver 循环接收 PtyOutputEvent 并 emit 到前端
pub struct FrontendOutputHandler;

impl FrontendOutputHandler {
    /// 启动输出转发 task
    pub fn spawn(app_handle: AppHandle, mut rx: broadcast::Receiver<PtyOutputEvent>) {
        spawn_with_error_boundary("frontend_output_handler", async move {
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        let event_name = format!("pty-output-{}", event.session_id);
                        if let Err(e) = app_handle.emit(&event_name, &event) {
                            tracing::error!("[FrontendOutputHandler] Failed to emit {}: {}", event_name, e);
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(count)) => {
                        tracing::warn!("[FrontendOutputHandler] Lagged {} events, some output may be dropped", count);
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        tracing::info!("[FrontendOutputHandler] Broadcast channel closed, exiting");
                        break;
                    }
                }
            }
        });
    }
}
