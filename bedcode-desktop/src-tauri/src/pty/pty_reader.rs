//! PTY Output Reader
//!
//! PTY 输出读取线程，使用 broadcast channel 通知监听器

use std::io::{BufReader, Read};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::enums::PtySessionStatus;
use crate::pty::next_output_index;
use crate::session::{GlobalOutputManager, OutputEvent};
use crate::system::config::AppConfig;

/// 背压暂停轮询间隔：暂停读期间每 5ms 重查一次水位/运行标志，latency 与
/// 忙等开销平衡（本地环回场景可感知的恢复延迟可忽略）
const BACKPRESSURE_POLL_INTERVAL: Duration = Duration::from_millis(5);

/// PTY 输出读取器
pub struct PtyReader {
    handle: Option<JoinHandle<()>>,
}

impl PtyReader {
    /// 创建并启动输出读取线程
    ///
    /// - `reader`: PTY 读取器
    /// - `lifecycle_tx`: 生命周期事件发送器
    /// - `session_id`: 会话 ID
    /// - `running`: 运行标志
    pub fn start(
        reader: Box<dyn Read + Send + 'static>,
        lifecycle_tx: tokio::sync::broadcast::Sender<PtySessionStatus>,
        session_id: String,
        running: Arc<AtomicBool>,
    ) -> Self {
        Self::start_with_pause(reader, lifecycle_tx, session_id, running, None)
    }

    /// 带可注入背压判定的读取器启动（默认走 GlobalOutputManager 未 ack
    /// 水位）；测试注入可控闭包驱动暂停/恢复，验证「暂停零读取、恢复全量
    /// 无丢失」的背压契约
    pub fn start_with_pause(
        reader: Box<dyn Read + Send + 'static>,
        lifecycle_tx: tokio::sync::broadcast::Sender<PtySessionStatus>,
        session_id: String,
        running: Arc<AtomicBool>,
        pause_check: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
    ) -> Self {
        let pause = pause_check.unwrap_or_else(|| {
            let sid = session_id.clone();
            Arc::new(move || GlobalOutputManager::global().should_pause(&sid))
        });
        let mut buf_reader = BufReader::new(reader);
        let read_buffer_size = AppConfig::global().terminal.read_buffer_size;

        // 有序输出队列：PTY 读线程按 read 顺序 blocking_send，单消费者任务顺序
        // on_output——根治「spawn 并发 on_output 乱序」竞态（多任务在 index 分配
        // 与广播之间互相插入 → 队列 push/broadcast 顺序与事件产生顺序错乱 → 字节
        // 错位残渣）。队列满时 blocking_send 阻塞读线程（背压：数据留 PTY 内核管道）。
        let (output_tx, mut output_rx) = tokio::sync::mpsc::channel::<OutputEvent>(16384);
        let consumer_session_id = session_id.clone();
        tauri::async_runtime::spawn(async move {
            let global_manager = GlobalOutputManager::global();
            while let Some(event) = output_rx.recv().await {
                global_manager.on_output(event).await;
            }
            tracing::debug!("PTY output consumer exited: {}", consumer_session_id);
        });

        let handle = thread::spawn(move || {
            let mut buffer = vec![0u8; read_buffer_size];
            let mut exit_status = PtySessionStatus::Stopped;

            while running.load(Ordering::SeqCst) {
                // 背压门：未 ack 字节超水位 → 暂停读。暂停只发生在两次 read 之间
                //（绝不中断半途 read）→ 零字节丢失；期间 PTY 内核管道缓冲积聚，
                // 子进程写满即自然阻塞——这是真正的背压向产生端传导
                if pause() {
                    thread::sleep(BACKPRESSURE_POLL_INTERVAL);
                    continue;
                }
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

                        // 经有序队列顺序发送（单消费者顺序 on_output，消除 spawn 并发
                        // 乱序）。队列关闭 = 消费者已退出（应用关闭中），读循环退出
                        let output_event = OutputEvent::new(
                            session_id.clone(),
                            raw_bytes,
                            index as u64,
                            timestamp.timestamp_millis(),
                            false,
                        );
                        if output_tx.blocking_send(output_event).is_err() {
                            tracing::debug!("PTY output queue closed, reader exiting");
                            break;
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

        Self { handle: Some(handle) }
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
    use std::sync::atomic::AtomicBool;
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

    /// 记录每次 read 实际读入字节数的 Reader（背压暂停判定用）
    struct RecordingReader {
        data: Vec<u8>,
        pos: usize,
        reads: Arc<std::sync::Mutex<Vec<usize>>>,
    }

    impl Read for RecordingReader {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.pos >= self.data.len() {
                return Ok(0); // EOF
            }
            let n = std::cmp::min(buf.len(), self.data.len() - self.pos);
            buf[..n].copy_from_slice(&self.data[self.pos..self.pos + n]);
            self.pos += n;
            self.reads.lock().unwrap().push(n);
            Ok(n)
        }
    }

    #[test]
    fn reads_output_and_reports_stopped_on_eof() {
        let (lifecycle_tx, mut lifecycle_rx) = broadcast::channel(8);
        let running = Arc::new(AtomicBool::new(true));

        // 负载超过默认 read_buffer_size（4096）→ 强制分多次 read → 多轮输出
        let payload: Vec<u8> = (0..9000u32).map(|i| (i % 251) as u8).collect();
        let reader: Box<dyn Read + Send + 'static> = Box::new(MemoryReader {
            data: std::io::Cursor::new(payload),
        });

        let pty_reader = PtyReader::start(reader, lifecycle_tx, "test-session".to_string(), running);
        pty_reader.wait(); // EOF 后线程自行退出，join 返回

        // 生命周期：EOF → Stopped
        let status = lifecycle_rx.try_recv().expect("lifecycle event should be sent");
        assert_eq!(status, PtySessionStatus::Stopped);
    }

    #[test]
    fn backpressure_pause_blocks_reads_and_resume_drains_without_loss() {
        let (lifecycle_tx, mut lifecycle_rx) = broadcast::channel(8);
        let running = Arc::new(AtomicBool::new(true));
        let paused = Arc::new(AtomicBool::new(true)); // 初始暂停（模拟未 ack 超水位）
        let pause_check: Arc<dyn Fn() -> bool + Send + Sync> = {
            let paused = paused.clone();
            Arc::new(move || paused.load(Ordering::SeqCst))
        };

        // 负载超过 read_buffer_size（4096）→ 多次 read
        let payload: Vec<u8> = (0..9000u32).map(|i| (i % 251) as u8).collect();
        let reads = Arc::new(std::sync::Mutex::new(Vec::new()));
        let reader: Box<dyn Read + Send + 'static> = Box::new(RecordingReader {
            data: payload.clone(),
            pos: 0,
            reads: reads.clone(),
        });

        let pty_reader = PtyReader::start_with_pause(
            reader,
            lifecycle_tx,
            "test-session".to_string(),
            running.clone(),
            Some(pause_check),
        );

        // 暂停中：等过多个轮询间隔（5ms），不得发生任何 read
        std::thread::sleep(std::time::Duration::from_millis(30));
        assert_eq!(reads.lock().unwrap().len(), 0, "paused reader must not read");
        assert!(running.load(Ordering::SeqCst));

        // 恢复（ack 推进）：全量字节被读入（零字节丢失），EOF 后线程退出
        paused.store(false, Ordering::SeqCst);
        pty_reader.wait();

        let total: usize = reads.lock().unwrap().iter().sum();
        assert_eq!(total, payload.len(), "resume must drain all bytes without loss");
        let status = lifecycle_rx.try_recv().expect("lifecycle event should be sent");
        assert_eq!(status, PtySessionStatus::Stopped);
    }

    #[test]
    fn empty_input_exits_with_stopped_lifecycle() {
        let (lifecycle_tx, mut lifecycle_rx) = broadcast::channel(8);
        let running = Arc::new(AtomicBool::new(true));

        let reader: Box<dyn Read + Send + 'static> = Box::new(MemoryReader {
            data: std::io::Cursor::new(Vec::new()),
        });

        let pty_reader = PtyReader::start(reader, lifecycle_tx, "empty".to_string(), running);
        pty_reader.wait();

        let status = lifecycle_rx.try_recv().expect("lifecycle event should be sent");
        assert_eq!(status, PtySessionStatus::Stopped);
    }
}
