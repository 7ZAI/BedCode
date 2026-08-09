//! Frontend Output Handler
//!
//! 向前端发送 PTY 输出事件
//! 通过 broadcast channel 订阅输出，在独立 task 中 recv 循环消费并 emit Tauri 事件
//! 事件名按 session 分 channel：pty-output-{session_id}，避免多会话时无意义 IPC
//!
//! 输出经过插件 TerminalHandler 管道处理后再转发到前端

use crate::pty::PtyOutputEvent;
use crate::system::error_boundary::spawn_with_error_boundary;
use tauri::{AppHandle, Emitter};
use tokio::sync::broadcast;

/// 向前端发送 PTY 输出事件的 Handler
///
/// 启动一个后台 task 从 broadcast receiver 循环接收 PtyOutputEvent，
/// 通过插件 TerminalHandler 管道处理后，emit 到前端
pub struct FrontendOutputHandler;

impl FrontendOutputHandler {
    /// 启动输出转发 task
    pub fn spawn(app_handle: AppHandle, mut rx: broadcast::Receiver<PtyOutputEvent>) {
        spawn_with_error_boundary("frontend_output_handler", async move {
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        let event = Self::process_through_plugins(event).await;
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

    /// 通过插件 TerminalHandler 管道处理输出
    ///
    /// 将 Base64 编码的输出数据解码，依次调用所有 Rust terminal handler 的 `on_output`，
    /// 如果任一 handler 修改了数据，使用修改后的数据重新编码
    async fn process_through_plugins(event: PtyOutputEvent) -> PtyOutputEvent {
        let ctx = crate::system::app_context::AppContext::global();
        let plugin_host = ctx.plugin_host();

        // 无 terminal handler 时直接透传：避免每次输出都做
        // base64 解码 + UTF-8 校验 + 字符串拷贝（绝大多数运行场景无插件）
        if !plugin_host.has_terminal_handlers().await {
            return event;
        }

        // 解码 Base64 输出数据
        let bytes = match event.decode_data() {
            Some(b) => b,
            None => return event,
        };
        let text = match String::from_utf8(bytes) {
            Ok(t) => t,
            Err(_) => return event, // 非 UTF-8 输出（二进制数据），跳过插件处理
        };

        // 通过插件管道处理
        let processed = plugin_host.process_terminal_output(&event.session_id, &text).await;

        // 如果数据未被修改，直接返回原始事件
        if processed == text {
            return event;
        }

        // 用修改后的数据重建事件
        PtyOutputEvent::from_bytes(
            event.session_id.clone(),
            processed.as_bytes(),
            event.timestamp,
            event.is_waiting,
            event.index,
        )
    }
}
