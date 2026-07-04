//! PTY Output Reader
//!
//! PTY 输出读取线程，使用观察者模式通知监听器

use std::io::{BufReader, Read};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use crate::enums::PtySessionStatus;
use crate::pty::PtyOutputEvent;
use crate::pty::next_output_index;
use crate::session::{GlobalOutputManager, OutputEvent};
use crate::pty::PtyOutputListener;
use crate::system::config::AppConfig;

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
        let read_buffer_size = AppConfig::global().terminal.read_buffer_size;

        let handle = thread::spawn(move || {
            let mut buffer = vec![0u8; read_buffer_size];
            let mut exit_status = PtySessionStatus::Stopped;

            while running.load(Ordering::SeqCst) {
                match buf_reader.read(&mut buffer) {
                    Ok(0) => {
                        // EOF - process exited
                        tracing::info!("PTY session ended: {}", session_id);
                        break;
                    }
                    Ok(n) => {
                        let timestamp = chrono::Utc::now();
                        let index = next_output_index();
                        let raw_bytes = buffer[..n].to_vec();

                        // 创建 PtyOutputEvent（包含 Base64 编码，用于桌面端前端）
                        let event = PtyOutputEvent::from_bytes(
                            session_id.clone(),
                            &raw_bytes,
                            timestamp,
                            false,
                            index,
                        );

                        // 发送到 GlobalOutputManager（存储原始字节，用于移动端订阅）
                        let global_manager = GlobalOutputManager::global();
                        let output_event = OutputEvent::new(
                            session_id.clone(),
                            raw_bytes,
                            index as u64,
                            timestamp.timestamp_millis(),
                            false,
                        );
                        tauri::async_runtime::spawn(async move {
                            global_manager.on_output(output_event).await;
                        });

                        // 通知所有监听器（观察者模式，用于桌面端前端）
                        if let Ok(listeners) = output_listeners.try_lock() {
                            for listener in listeners.iter() {
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
