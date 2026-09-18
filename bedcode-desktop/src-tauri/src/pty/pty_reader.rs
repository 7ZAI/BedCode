//! PTY Output Reader
//!
//! PTY 输出读取线程：`read → 有序队列 → 单消费者入环`。
//!
//! **源零等待（spec §4.1/§4.6 背压下移）**：读取路径不含任何暂停/水位判定，
//! 也不感知任何订阅者。数据量超出会话环容量时由环淘汰最旧（订阅者各自按
//! 游标拉取），慢订阅者的节流只发生在它自己的订阅者执行体里——单个慢消费者
//! 不再能冻结源产出（历史教训：会话级共享水位 + 源侧暂停 = 多订阅者场景下
//! 一个端 ack 滞留就冻结整条链路）。

use std::io::{BufReader, Read};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use crate::enums::PtySessionStatus;
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
    /// - `lifecycle_tx`: 生命周期事件发送器
    /// - `session_id`: 会话 ID
    /// - `running`: 运行标志
    pub fn start(
        reader: Box<dyn Read + Send + 'static>,
        lifecycle_tx: tokio::sync::broadcast::Sender<PtySessionStatus>,
        session_id: String,
        running: Arc<AtomicBool>,
    ) -> Self {
        let mut buf_reader = BufReader::new(reader);
        let read_buffer_size = AppConfig::global().terminal.read_buffer_size;

        // 有序输出队列：PTY 读线程按 read 顺序 blocking_send，单消费者任务顺序
        // on_output——根治「spawn 并发 on_output 乱序」竞态（多任务在 index 分配
        // 与广播之间互相插入 → 队列 push/broadcast 顺序与事件产生顺序错乱 → 字节
        // 错位残渣）。队列满时 blocking_send 阻塞读线程——这是唯一残留的源侧
        // 等待，仅在入环消费者彻底停摆时发生（正常路径 on_output 只做入环 + 通告
        // 水印，无 await 等待任何订阅者）
        let (output_tx, mut output_rx) = tokio::sync::mpsc::channel::<OutputEvent>(16384);
        let consumer_session_id = session_id.clone();
        tauri::async_runtime::spawn(async move {
            let global_manager = GlobalOutputManager::global();
            while let Some(event) = output_rx.recv().await {
                global_manager.on_output(event).await;
            }
            tracing::debug!(session_id = %consumer_session_id, "PTY output consumer exited");
        });

        let handle = thread::spawn(move || {
            let mut buffer = vec![0u8; read_buffer_size];
            let mut exit_status = PtySessionStatus::Stopped;

            while running.load(Ordering::SeqCst) {
                match buf_reader.read(&mut buffer) {
                    Ok(0) => {
                        // EOF - process exited
                        tracing::info!(session_id = %session_id, "PTY session ended");
                        break;
                    }
                    Ok(n) => {
                        let timestamp = chrono::Utc::now();
                        let raw_bytes = buffer[..n].to_vec();

                        // 经有序队列顺序发送（单消费者顺序 on_output，消除 spawn 并发
                        // 乱序）。队列关闭 = 消费者已退出（应用关闭中），读循环退出
                        let output_event = OutputEvent::new(
                            session_id.clone(),
                            raw_bytes,
                            0, // start_offset 由 on_output 在串行临界区内按 max_offset 分配
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
            // （发送失败 = 无订阅者，属正常终止路径；warn 保留可观测性）
            if let Err(e) = lifecycle_tx.send(exit_status) {
                tracing::warn!(session_id = %session_id, %e, "PTY lifecycle event dropped (no subscribers)");
            }
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

    /// 记录每次 read 实际读入字节数的 Reader
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

    /// 全局唯一会话 ID（避免测试间通过全局单例互相干扰）
    fn unique_session_id(prefix: &str) -> String {
        format!(
            "{prefix}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )
    }

    /// 轮询会话环直至累计字节达到期望值（消费者任务是异步的）
    async fn drain_ring_until(session: &Arc<crate::session::SessionOutputManager>, want: usize) -> Vec<u8> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let ring_arc = session.ring();
            let ring = ring_arc.read().await;
            let (min, max) = ring.watermarks();
            if max.saturating_sub(min) as usize >= want {
                return ring.range(min, max);
            }
            drop(ring);
            if std::time::Instant::now() >= deadline {
                return Vec::new();
            }
            tokio::task::yield_now().await;
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

    /// 数据投递断言（票据 04 + 拉取模型）：消费者任务硬编码
    /// `GlobalOutputManager::global()`，用全局唯一 session_id 注册；产出字节
    /// 必须完整按序落入该会话输出环（订阅者随后按游标从环上拉取）。
    ///
    /// 零订阅者场景同时是「源不依赖消费者」的回归护栏：没有任何订阅者时
    /// 产出照常全量入环。
    #[test]
    fn output_bytes_reach_session_ring_complete_and_in_order_without_subscribers() {
        use crate::session::GlobalOutputManager;

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let sid = unique_session_id("itest-delivery");
            let manager = GlobalOutputManager::global();
            manager.register_session(&sid).await;

            let (lifecycle_tx, _rx) = broadcast::channel(8);
            let running = Arc::new(AtomicBool::new(true));
            let payload: Vec<u8> = (0..9000u32).map(|i| (i % 251) as u8).collect();
            let reader: Box<dyn Read + Send + 'static> = Box::new(MemoryReader {
                data: std::io::Cursor::new(payload.clone()),
            });
            let pty_reader = PtyReader::start(reader, lifecycle_tx, sid.clone(), running);
            pty_reader.wait();

            // 无任何订阅者：产出仍须完整按序入环
            let session = manager.session(&sid).await.expect("会话管理器");
            assert_eq!(
                session.pull_subscriber_count().await,
                0,
                "本用例刻意零订阅者（源产出不得依赖消费者）"
            );
            let collected = drain_ring_until(&session, payload.len()).await;
            assert_eq!(collected, payload, "PTY 输出必须完整按序落入会话环（票据 04）");

            manager.unregister_session(&sid).await;
        });
    }

    /// Err 分支（票据 04）：假 Reader 返回 Err → 生命周期事件为 Error
    #[test]
    fn reader_error_reports_error_lifecycle() {
        struct ErrReader;
        impl Read for ErrReader {
            fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("injected read error"))
            }
        }

        let (lifecycle_tx, mut lifecycle_rx) = broadcast::channel(8);
        let running = Arc::new(AtomicBool::new(true));
        let reader: Box<dyn Read + Send + 'static> = Box::new(ErrReader);
        let pty_reader = PtyReader::start(reader, lifecycle_tx, "err-session".to_string(), running);
        pty_reader.wait();

        let status = lifecycle_rx.try_recv().expect("lifecycle event should be sent");
        assert_eq!(status, PtySessionStatus::Error, "读错误应上报 Error 生命周期");
    }

    /// running=false（启动即退出）：读线程不读任何字节，生命周期报 Stopped
    #[test]
    fn running_flag_cleared_before_read_exits_without_reading() {
        let (lifecycle_tx, mut lifecycle_rx) = broadcast::channel(8);
        let running = Arc::new(AtomicBool::new(false));
        let reads = Arc::new(std::sync::Mutex::new(Vec::new()));
        let reader: Box<dyn Read + Send + 'static> = Box::new(RecordingReader {
            data: b"never read".to_vec(),
            pos: 0,
            reads: reads.clone(),
        });

        let pty_reader = PtyReader::start(reader, lifecycle_tx, "stop-session".to_string(), running);
        pty_reader.wait();

        assert!(reads.lock().unwrap().is_empty(), "running=false 时不得发生任何 read");
        let status = lifecycle_rx.try_recv().expect("lifecycle event should be sent");
        assert_eq!(status, PtySessionStatus::Stopped, "running=false 退出应报 Stopped");
    }
}
