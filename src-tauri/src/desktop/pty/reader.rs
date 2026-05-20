//! Output Reader
//!
//! PTY 输出读取线程（当前未使用，保留用于未来重构）

use std::io::{BufReader, Read};
use std::thread::{self, JoinHandle};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::desktop::enums::PtySessionStatus;
use crate::desktop::model::PtyOutputEvent;
use crate::desktop::pty::next_output_index;
use crate::Result;

/// 输出读取器
pub struct OutputReader {
    handle: Option<JoinHandle<()>>,
}

impl OutputReader {
    /// 创建并启动输出读取线程
    pub fn start(
        reader: Box<dyn Read + Send + 'static>,
        output_tx: tokio::sync::broadcast::Sender<PtyOutputEvent>,
        lifecycle_tx: tokio::sync::broadcast::Sender<PtySessionStatus>,
        session_id: String,
        running: Arc<AtomicBool>,
    ) -> Result<Self> {
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

                        if output_tx.send(event).is_err() {
                            tracing::debug!("No output subscribers for session: {}", session_id);
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

        Ok(Self {
            handle: Some(handle),
        })
    }

    /// 等待线程结束
    pub fn wait(self) {
        if let Some(handle) = self.handle {
            let _ = handle.join();
        }
    }
}