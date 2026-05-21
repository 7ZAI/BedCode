//! PTY Output Reader
//!
//! PTY 输出读取线程，使用观察者模式通知监听器

use std::io::{BufReader, Read};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use crate::desktop::enums::PtySessionStatus;
use crate::desktop::model::PtyOutputEvent;
use crate::desktop::pty::next_output_index;
use crate::desktop::traits::PtyOutputListener;

/// PTY 输出读取器
pub struct PtyReader {
    handle: Option<JoinHandle<()>>,
}

impl PtyReader {
    /// 创建并启动输出读取线程
    ///
    /// - `reader`: PTY 读取器
    /// - `output_listeners`: 观察者列表，用于通知输出事件
    /// - `lifecycle_tx`: 生命周期事件发送器
    /// - `session_id`: 会话 ID
    /// - `running`: 运行标志
    pub fn start(
        reader: Box<dyn Read + Send + 'static>,
        output_listeners: Arc<tokio::sync::Mutex<Vec<Arc<dyn PtyOutputListener>>>>,
        lifecycle_tx: tokio::sync::broadcast::Sender<PtySessionStatus>,
        session_id: String,
        running: Arc<AtomicBool>,
    ) -> Self {
        let mut buf_reader = BufReader::new(reader);

        let handle = thread::spawn(move || {
            let mut buffer = [0u8; 4096];
            let mut exit_status = PtySessionStatus::Stopped;

            while running.load(Ordering::SeqCst) {
                match buf_reader.read(&mut buffer) {
                    Ok(0) => {
                        // EOF - process exited
                        tracing::info!("PTY session ended: {}", session_id);
                        break;
                    }
                    Ok(n) => {
                        let data_preview = String::from_utf8_lossy(&buffer[..n]);
                        tracing::debug!("[PtyReader] Read {} bytes from PTY, preview: {:?}", n, data_preview.chars().take(50).collect::<String>());
                        let event = PtyOutputEvent {
                            session_id: session_id.clone(),
                            data: base64::Engine::encode(
                                &base64::engine::general_purpose::STANDARD,
                                &buffer[..n],
                            ),
                            timestamp: chrono::Utc::now(),
                            is_waiting: false,
                            index: next_output_index(),
                        };
                        tracing::debug!("[PtyReader] Created PtyOutputEvent, session_id: {}, data length: {}", event.session_id, event.data.len());

                        // 通知所有监听器（观察者模式）
                        // 注意：在同步线程中调用 async fn 不会自动执行
                        // 需要使用 tokio::spawn 在异步 runtime 中执行
                        tracing::debug!("[PtyReader] Notifying listeners for session: {}", session_id);
                        if let Ok(listeners) = output_listeners.try_lock() {
                            tracing::debug!("[PtyReader] Number of listeners: {}", listeners.len());
                            for listener in listeners.iter() {
                                let listener_name = listener.name();
                                tracing::debug!("[PtyReader] Calling on_output for listener: {}", listener_name);

                                let event_clone = event.clone();
                                let listener_clone = listener.clone();

                                // 在 tokio 异步 runtime 中 spawn 任务来执行 async on_output
                                tauri::async_runtime::spawn(async move {
                                    listener_clone.on_output(event_clone).await;
                                });
                            }
                        } else {
                            tracing::warn!("[PtyReader] Failed to acquire lock on output_listeners");
                        }
                    }
                    Err(e) => {
                        tracing::error!("PTY read error: {}", e);
                        exit_status = PtySessionStatus::Error;
                        break;
                    }
                }
            }

            // Notify lifecycle subscribers that the process has exited
            let _ = lifecycle_tx.send(exit_status);
            tracing::debug!("Output reader stopped for session: {} (status: {:?})", session_id, exit_status);
        });

        Self {
            handle: Some(handle),
        }
    }

    /// 等待线程结束
    pub fn wait(self) {
        if let Some(handle) = self.handle {
            let _ = handle.join();
        }
    }

    /// 获取内部线程句柄（用于保存到状态中）
    pub fn into_inner(self) -> JoinHandle<()> {
        self.handle.expect(" PtyReader handle is None")
    }
}