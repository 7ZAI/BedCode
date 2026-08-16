//! PTY Output Reader
//!
//! PTY 输出读取线程，使用 broadcast channel 通知监听器

use std::io::{BufReader, Read};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use crate::enums::PtySessionStatus;
use crate::pty::PtyOutputEvent;
use crate::pty::next_output_index;
use crate::session::{GlobalOutputManager, OutputEvent};
use crate::system::config::AppConfig;

/// PTY 输出读取器
pub struct PtyReader {
    handle: Option<JoinHandle<()>>,
}

impl PtyReader {
    /// 创建并启动输出读取线程
    ///
    /// - `reader`: PTY 读取器
    /// - `output_broadcast`: 输出事件广播器，替代旧的 Mutex<Vec<Listener>>
    ///   broadcast send 不可阻塞，无 receiver 时自动丢弃，避免 try_lock 失败丢数据
    /// - `lifecycle_tx`: 生命周期事件发送器
    /// - `session_id`: 会话 ID
    /// - `running`: 运行标志
    pub fn start(
        reader: Box<dyn Read + Send + 'static>,
        output_broadcast: tokio::sync::broadcast::Sender<PtyOutputEvent>,
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

                        // 通过 broadcast channel 通知所有监听器（桌面端前端）
                        // broadcast::send 不阻塞，无 receiver 时返回 Err 但不影响后续操作
                        let receiver_count = output_broadcast.receiver_count();
                        if receiver_count > 0 {
                            if let Err(e) = output_broadcast.send(event) {
                                // 无活跃 receiver 时正常，不 warn
                                tracing::debug!(
                                    "[PtyReader] broadcast send failed (no receivers or lagged): {}", e
                                );
                            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enums::PtySessionStatus;
    use std::io::Read;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use tokio::sync::broadcast;

    /// 内存 Reader：一次性吐出数据后 EOF（模拟 PTY 主设备读取）
    struct MemoryReader {
        data: std::io::Cursor<Vec<u8>>,
    }

    impl Read for MemoryReader {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.data.read(buf)
        }
    }

    /// 收集 channel 中已就绪的全部事件
    /// （PtyReader 线程在 wait() 返回前已完成所有 send，同步 try_recv 即可取到）
    fn drain_events(rx: &mut broadcast::Receiver<PtyOutputEvent>) -> Vec<PtyOutputEvent> {
        let mut events = Vec::new();
        loop {
            match rx.try_recv() {
                Ok(ev) => events.push(ev),
                Err(_) => break,
            }
        }
        events
    }

    #[test]
    fn reads_output_broadcasts_events_and_reports_stopped_on_eof() {
        let (output_tx, mut output_rx) = broadcast::channel(64);
        let (lifecycle_tx, mut lifecycle_rx) = broadcast::channel(8);
        let running = Arc::new(AtomicBool::new(true));

        // 负载超过默认 read_buffer_size（4096）→ 强制分多次 read → 多个事件
        let payload: Vec<u8> = (0..9000u32).map(|i| (i % 251) as u8).collect();
        let reader: Box<dyn Read + Send + 'static> = Box::new(MemoryReader {
            data: std::io::Cursor::new(payload.clone()),
        });

        let pty_reader = PtyReader::start(
            reader,
            output_tx,
            lifecycle_tx,
            "test-session".to_string(),
            running,
        );
        pty_reader.wait(); // EOF 后线程自行退出，join 返回

        // 生命周期：EOF → Stopped
        let status = lifecycle_rx
            .try_recv()
            .expect("lifecycle event should be sent");
        assert_eq!(status, PtySessionStatus::Stopped);

        // 输出事件：拼接后必须还原原始字节，且 index 严格递增
        let events = drain_events(&mut output_rx);
        assert!(!events.is_empty(), "expected at least one output event");
        let mut reconstructed = Vec::new();
        let mut last_index: Option<usize> = None;
        for ev in &events {
            assert_eq!(ev.session_id, "test-session");
            assert!(!ev.is_waiting);
            if let Some(prev) = last_index {
                assert!(ev.index > prev, "index must be strictly increasing");
            }
            last_index = Some(ev.index);
            reconstructed.extend_from_slice(&ev.decode_data().expect("valid base64"));
        }
        assert_eq!(reconstructed, payload);
    }

    #[test]
    fn empty_input_exits_with_stopped_lifecycle_without_events() {
        let (output_tx, mut output_rx) = broadcast::channel(64);
        let (lifecycle_tx, mut lifecycle_rx) = broadcast::channel(8);
        let running = Arc::new(AtomicBool::new(true));

        let reader: Box<dyn Read + Send + 'static> = Box::new(MemoryReader {
            data: std::io::Cursor::new(Vec::new()),
        });

        let pty_reader = PtyReader::start(
            reader,
            output_tx,
            lifecycle_tx,
            "empty".to_string(),
            running,
        );
        pty_reader.wait();

        let status = lifecycle_rx
            .try_recv()
            .expect("lifecycle event should be sent");
        assert_eq!(status, PtySessionStatus::Stopped);
        assert!(drain_events(&mut output_rx).is_empty());
    }
}
