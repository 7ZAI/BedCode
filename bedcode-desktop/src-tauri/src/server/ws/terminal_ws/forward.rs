//! 输出转发层 — 将 OutputEvent 流编码为 WS 帧（文本 JSON / 二进制帧）并转发
//!
//! 从 `terminal_ws` 拆出的纯逻辑部分：不依赖 actor 状态，独立可测。
//! 合并/直通时序语义由 `forward_loop` 统一保证（见其文档注释与内联测试）。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::session::OutputEvent;
use crate::server::message::Message;

// ==================== Output Buffer ====================

/// 转发输出形态：文本 JSON（移动端 WS）或二进制帧（桌面端本地 WS）
#[derive(Debug)]
pub(super) enum ForwardOutput {
    Text(String),
    Binary(Vec<u8>),
}

/// 二进制帧头：magic(2) + version(1) + flags(1) + start_offset(8 LE) + end_offset(8 LE) = 20 字节
const BINARY_FRAME_HEADER_LEN: usize = 20;
const BINARY_FRAME_MAGIC: [u8; 2] = [0x54, 0x42]; // "TB"
const BINARY_FRAME_VERSION: u8 = 1;
const BINARY_FRAME_FLAG_WAITING: u8 = 0x01;

/// 编码输出二进制帧（客户端据此做字节级连续性校验）
fn encode_output_frame(start_offset: u64, end_offset: u64, is_waiting: bool, data: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(BINARY_FRAME_HEADER_LEN + data.len());
    frame.extend_from_slice(&BINARY_FRAME_MAGIC);
    frame.push(BINARY_FRAME_VERSION);
    frame.push(if is_waiting { BINARY_FRAME_FLAG_WAITING } else { 0 });
    frame.extend_from_slice(&start_offset.to_le_bytes());
    frame.extend_from_slice(&end_offset.to_le_bytes());
    frame.extend_from_slice(data);
    frame
}

/// 输出缓冲区 — 累积多条 PTY 输出，减少 WS 消息数量
struct OutputBuffer {
    data: Vec<u8>,
    start_index: u64,
    end_index: u64,
    start_offset: u64,
    end_offset: u64,
    last_is_waiting: bool,
    /// true = 二进制帧输出（本地通道）；false = base64 JSON（移动端通道）
    binary: bool,
}

impl OutputBuffer {
    fn new(binary: bool) -> Self {
        Self {
            data: Vec::new(),
            start_index: 0,
            end_index: 0,
            start_offset: 0,
            end_offset: 0,
            last_is_waiting: false,
            binary,
        }
    }

    fn append(&mut self, event: &crate::session::OutputEvent) {
        if self.data.is_empty() {
            self.start_index = event.index;
            self.start_offset = event.start_offset;
        }
        // 始终更新 end_index 为最新事件的 index
        self.end_index = event.index;
        self.end_offset = event.end_offset;
        self.data.extend_from_slice(&event.data);
        self.last_is_waiting = event.is_waiting;
    }

    fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Flush 缓冲区为转发输出
    ///
    /// 文本形态：合并多条事件时，index 为起始索引，end_index 为结束索引，
    /// 前端可用 end_index 精确更新去重游标，支持增量同步
    /// 二进制形态：帧携带 [start_offset, end_offset)，客户端校验连续性
    fn flush(&mut self, session_id: &str) -> ForwardOutput {
        if self.binary {
            let frame = encode_output_frame(
                self.start_offset,
                self.end_offset,
                self.last_is_waiting,
                &self.data,
            );
            self.data.clear();
            ForwardOutput::Binary(frame)
        } else {
            let data_base64 = base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                &self.data,
            );
            // 仅在合并了多条事件（end_index > start_index）时附带 end_index
            let end_index = if self.end_index > self.start_index {
                Some(self.end_index as usize)
            } else {
                None
            };
            let message = Message::output_from_base64(
                session_id,
                &data_base64,
                self.last_is_waiting,
                self.start_index as usize,
                end_index,
                Some(self.start_offset),
                Some(self.end_offset),
            );
            self.data.clear();
            ForwardOutput::Text(message.to_json().unwrap_or_default())
        }
    }
}

/// 输出转发循环 — 将 OutputEvent 流编码为 ForwardOutput 经 out_tx 送出
///
/// 两种模式：
/// - `flush_interval = ZERO`：零缓冲直通，每条事件立即转发（本地环回通道 / 合并开关关闭）
/// - 有界延迟合并：字节达 `max_buffer_size` 或距上次 flush 超过 `flush_interval` 时
///   flush（先到先发），持续输出下延迟恒 ≤ flush_interval。不能用 timeout 重计时代替
///   时间窗——持续输出时 timeout 永不触发，flush 会退化成仅容量触发，慢速输出
///   延迟 = 容量/速率（可达数百 ms）
///
/// 流代数（stream generation）门控：
/// 订阅者被替换/取消订阅时旧 forward_loop 被 abort——但 abort 是异步信号，
/// 任务可能在 await 点之间已把帧投递到 actor 邮箱，Handler 仍会将其发出，
/// 旧流尾帧与新生订阅帧交错到达 → 客户端字节游标错位 → 连续性违反 → 重订阅风暴。
/// 每次转发前校验代数：订阅/取消订阅时代数递增，旧代 forward_loop 的残留帧
/// 直接丢弃，从根源杜绝旧流帧注入新订阅通道
pub(super) async fn forward_loop(
    mut output_rx: tokio::sync::mpsc::Receiver<crate::session::OutputEvent>,
    out_tx: tokio::sync::mpsc::Sender<ForwardOutput>,
    flush_interval: Duration,
    max_buffer_size: usize,
    binary: bool,
    session_id: String,
    stream_generation: Arc<AtomicU64>,
    my_gen: u64,
) {
    let mut buffer = OutputBuffer::new(binary);

    if flush_interval.is_zero() {
        // 零缓冲直通：每条事件立即转发，不等待
        while let Some(event) = output_rx.recv().await {
            if stream_generation.load(Ordering::SeqCst) != my_gen {
                break;
            }
            buffer.append(&event);
            if out_tx.send(buffer.flush(&session_id)).await.is_err() {
                break;
            }
        }
        return;
    }

    // 有界延迟合并：时间窗 / 字节窗双条件，先到先发
    let mut last_flush = tokio::time::Instant::now();
    loop {
        match tokio::time::timeout(flush_interval, output_rx.recv()).await {
            Ok(Some(event)) => {
                // 流代数失效（订阅被替换/取消）：残留帧直接丢弃，不转发
                if stream_generation.load(Ordering::SeqCst) != my_gen {
                    break;
                }
                buffer.append(&event);
                if buffer.data.len() >= max_buffer_size
                    || last_flush.elapsed() >= flush_interval
                {
                    if out_tx.send(buffer.flush(&session_id)).await.is_err() {
                        break;
                    }
                    last_flush = tokio::time::Instant::now();
                }
            }
            Ok(None) => {
                // channel 关闭，最终 flush
                if !buffer.is_empty() {
                    let _ = out_tx.send(buffer.flush(&session_id)).await;
                }
                break;
            }
            Err(_) => {
                // 空闲超时，flush 缓冲区；仅在确有内容发出时重置时间窗——
                // 空 buffer 也重置会把持续输出场景的时间窗进度抹掉（timeout 与
                // 事件同时就绪时 Err 分支先执行，内容 flush 将永远等不到）
                if !buffer.is_empty() {
                    if stream_generation.load(Ordering::SeqCst) != my_gen {
                        break;
                    }
                    if out_tx.send(buffer.flush(&session_id)).await.is_err() {
                        break;
                    }
                    last_flush = tokio::time::Instant::now();
                }
            }
        }
    }
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    fn event(session_id: &str, data: &[u8], index: u64, start_offset: u64, end_offset: u64) -> crate::session::OutputEvent {
        crate::session::OutputEvent {
            session_id: session_id.to_string(),
            data: data.to_vec(),
            index,
            start_offset,
            end_offset,
            timestamp: 0,
            is_waiting: false,
        }
    }

    /// 帧头布局：magic(2) + version(1) + flags(1) + start(8 LE) + end(8 LE) + payload
    #[test]
    fn test_encode_output_frame_header() {
        let frame = encode_output_frame(100, 106, true, b"hello");

        assert_eq!(&frame[0..2], b"TB");
        assert_eq!(frame[2], 1); // version
        assert_eq!(frame[3], BINARY_FRAME_FLAG_WAITING); // is_waiting
        assert_eq!(u64::from_le_bytes(frame[4..12].try_into().unwrap()), 100);
        assert_eq!(u64::from_le_bytes(frame[12..20].try_into().unwrap()), 106);
        assert_eq!(&frame[20..], b"hello");
        assert_eq!(frame.len(), BINARY_FRAME_HEADER_LEN + 5);
    }

    #[test]
    fn test_encode_output_frame_non_waiting_flag() {
        let frame = encode_output_frame(0, 1, false, b"x");
        assert_eq!(frame[3], 0);
    }

    /// 二进制形态：合并多条事件为一个帧，偏移取首事件 start / 尾事件 end
    #[test]
    fn test_output_buffer_binary_flush_merges_with_offsets() {
        let mut buf = OutputBuffer::new(true);
        buf.append(&event("s", b"ab", 0, 10, 12));
        buf.append(&event("s", b"cd", 1, 12, 14));
        buf.append(&event("s", b"ef", 2, 14, 16));

        let out = buf.flush("s");
        let ForwardOutput::Binary(frame) = out else {
            panic!("expected binary frame");
        };
        assert_eq!(u64::from_le_bytes(frame[4..12].try_into().unwrap()), 10);
        assert_eq!(u64::from_le_bytes(frame[12..20].try_into().unwrap()), 16);
        assert_eq!(&frame[20..], b"abcdef");
        assert!(buf.is_empty()); // flush 后清空
    }

    /// 文本形态（移动端兼容）：base64 JSON 携带 start_offset/end_offset 与 end_index
    #[test]
    fn test_output_buffer_text_flush_carries_offsets() {
        let mut buf = OutputBuffer::new(false);
        buf.append(&event("s", b"ab", 3, 40, 42));
        buf.append(&event("s", b"cd", 4, 42, 44));

        let out = buf.flush("s");
        let ForwardOutput::Text(json) = out else {
            panic!("expected text output");
        };
        let msg: Message = Message::from_json(&json).unwrap();
        let Message::Terminal { payload, .. } = msg else {
            panic!("expected terminal message");
        };
        let crate::enums::TerminalAction::Output { data, index, end_index, start_offset, end_offset, .. } = payload.action else {
            panic!("expected output action");
        };
        assert_eq!(index, 3);
        assert_eq!(end_index, Some(4));
        assert_eq!(start_offset, Some(40));
        assert_eq!(end_offset, Some(44));
        // 解码 base64 校验数据
        let decoded = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &data).unwrap();
        assert_eq!(decoded, b"abcd");
    }

    /// 单事件 flush：end_index 为 None，偏移取事件自身
    #[test]
    fn test_output_buffer_single_event_flush() {
        let mut buf = OutputBuffer::new(false);
        buf.append(&event("s", b"single", 7, 100, 106));

        let out = buf.flush("s");
        let ForwardOutput::Text(json) = out else {
            panic!("expected text output");
        };
        let msg: Message = Message::from_json(&json).unwrap();
        let Message::Terminal { payload, .. } = msg else {
            panic!("expected terminal message");
        };
        let crate::enums::TerminalAction::Output { index, end_index, start_offset, end_offset, .. } = payload.action else {
            panic!("expected output action");
        };
        assert_eq!(index, 7);
        assert_eq!(end_index, None);
        assert_eq!(start_offset, Some(100));
        assert_eq!(end_offset, Some(106));
    }

    // ==================== forward_loop 转发循环（合并时序语义） ====================

    /// 启动 forward_loop 并返回 (事件发送端, 输出接收端)
    fn spawn_forward(
        flush_interval: Duration,
        max_buffer_size: usize,
        binary: bool,
    ) -> (
        tokio::sync::mpsc::Sender<crate::session::OutputEvent>,
        tokio::sync::mpsc::Receiver<ForwardOutput>,
        tokio::task::JoinHandle<()>,
        Arc<AtomicU64>,
    ) {
        let (tx, rx) = tokio::sync::mpsc::channel::<crate::session::OutputEvent>(128);
        let (out_tx, out_rx) = tokio::sync::mpsc::channel::<ForwardOutput>(128);
        let generation = Arc::new(AtomicU64::new(0));
        let fwd = tokio::spawn(forward_loop(
            rx,
            out_tx,
            flush_interval,
            max_buffer_size,
            binary,
            "s".into(),
            generation.clone(),
            0,
        ));
        (tx, out_rx, fwd, generation)
    }

    /// 持续输出（事件间隔 < flush_interval）：合并生效且首条消息延迟有界（≤ 时间窗）
    ///
    /// 使用 tokio 虚拟时钟（start_paused）精确控制事件节奏，避免真实定时器
    /// 精度（Windows 上 ~15ms 抖动）干扰批次断言；消费任务与发送并行，
    /// 记录的是消息实际发出的时刻而非测试开始接收的时刻
    #[tokio::test(start_paused = true)]
    async fn test_forward_loop_sustained_output_bounded_delay_and_merging() {
        let (tx, mut out_rx, fwd, _gen) = spawn_forward(Duration::from_millis(20), 64 * 1024, false);

        let start = tokio::time::Instant::now();
        let (res_tx, mut res_rx) = tokio::sync::mpsc::channel::<(usize, Option<Duration>)>(4);
        // 并行消费：记录每条消息的实际发出时刻（虚拟时钟）
        let collector = tokio::spawn(async move {
            let mut messages = 0;
            let mut first_at: Option<Duration> = None;
            while let Some(out) = out_rx.recv().await {
                if first_at.is_none() {
                    first_at = Some(start.elapsed());
                }
                let ForwardOutput::Text(_) = out else {
                    panic!("expected text output");
                };
                messages += 1;
            }
            let _ = res_tx.send((messages, first_at)).await;
        });

        // 每 5ms 一条小事件，共 30 条（持续 150ms，间隔远小于 20ms 时间窗）
        for i in 0..30u64 {
            tx.send(event("s", b"x", i, i, i + 1)).await.unwrap();
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        drop(tx);

        let (messages, first_at) = res_rx.recv().await.expect("collector finished");
        let _ = collector.await;
        let _ = fwd.await;

        // 合并生效：30 条事件按 20ms 窗聚合成 ~7-8 批（5ms 间隔 → 每批 4-5 条）
        assert!(messages < 12, "expected merging, got {messages} messages");
        // 有界延迟：首批事件累积满 20ms 时间窗时发出（虚拟时钟精确）
        let first = first_at.expect("at least one message");
        assert!(
            first >= Duration::from_millis(15) && first <= Duration::from_millis(25),
            "first message delayed {first:?}, expected ~20ms"
        );
    }

    /// 字节窗：单条大事件 ≥ max_buffer_size 时立即 flush，不等待时间窗
    #[tokio::test(start_paused = true)]
    async fn test_forward_loop_byte_window_flushes_immediately() {
        let (tx, mut out_rx, fwd, _gen) = spawn_forward(Duration::from_millis(500), 8, false);

        let start = tokio::time::Instant::now();
        tx.send(event("s", b"0123456789", 0, 0, 10)).await.unwrap(); // 10 字节 > 8
        drop(tx);

        let out = tokio::time::timeout(Duration::from_millis(50), out_rx.recv())
            .await
            .expect("byte window must flush immediately")
            .expect("forward_loop exited");
        // 立即 flush：虚拟时间几乎未流逝，不等到 500ms 时间窗
        assert!(start.elapsed() < Duration::from_millis(100));

        let ForwardOutput::Text(json) = out else {
            panic!("expected text output");
        };
        let msg: Message = Message::from_json(&json).unwrap();
        let Message::Terminal { payload, .. } = msg else {
            panic!("expected terminal message");
        };
        let crate::enums::TerminalAction::Output { data, index, .. } = payload.action else {
            panic!("expected output action");
        };
        assert_eq!(index, 0);
        let decoded = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &data).unwrap();
        assert_eq!(decoded, b"0123456789");
        let _ = fwd.await;
    }

    /// 空闲 flush：单条小事件后无后续，≤ flush_interval 后发出（不无限滞留）
    ///
    /// 注意保持 sender 存活：drop 会触发 final flush 路径（立即发出），
    /// 而非空闲超时路径
    #[tokio::test(start_paused = true)]
    async fn test_forward_loop_idle_flush_within_interval() {
        let (tx, mut out_rx, fwd, _gen) = spawn_forward(Duration::from_millis(30), 64 * 1024, false);

        let start = tokio::time::Instant::now();
        tx.send(event("s", b"hi", 0, 0, 2)).await.unwrap();

        let out = tokio::time::timeout(Duration::from_millis(80), out_rx.recv())
            .await
            .expect("idle flush within interval")
            .expect("forward_loop exited");
        // 空闲 flush 由时间窗触发：虚拟时间 ≈30ms（而非无限滞留）
        let elapsed = start.elapsed();
        assert!(
            (Duration::from_millis(25)..=Duration::from_millis(60)).contains(&elapsed),
            "idle flush elapsed: {elapsed:?}"
        );
        assert!(matches!(out, ForwardOutput::Text(_)));

        drop(tx); // 关闭通道，让循环退出
        let _ = fwd.await;
    }

    /// 零间隔（直通模式）：每条事件立即转发，消息数 = 事件数，无合并
    #[tokio::test]
    async fn test_forward_loop_zero_interval_passthrough() {
        let (tx, mut out_rx, fwd, _gen) = spawn_forward(Duration::ZERO, 64 * 1024, false);

        for i in 0..5u64 {
            tx.send(event("s", b"x", i, i, i + 1)).await.unwrap();
        }
        drop(tx);

        let mut messages = 0;
        while let Some(out) = out_rx.recv().await {
            let ForwardOutput::Text(_) = out else {
                panic!("expected text output");
            };
            messages += 1;
        }
        let _ = fwd.await;
        assert_eq!(messages, 5, "passthrough must forward each event unchanged");
    }

    /// 通道关闭：未达时间窗/字节窗的残留缓冲最终 flush（合并语义收尾）
    #[tokio::test]
    async fn test_forward_loop_final_flush_on_channel_close() {
        let (tx, mut out_rx, fwd, _gen) = spawn_forward(Duration::from_millis(60_000), 64 * 1024, false);

        tx.send(event("s", b"ab", 0, 0, 2)).await.unwrap();
        tx.send(event("s", b"cd", 1, 2, 4)).await.unwrap();
        drop(tx); // 未达时间窗/字节窗 → 关闭时合并两条最终发出

        let out = tokio::time::timeout(Duration::from_millis(100), out_rx.recv())
            .await
            .expect("final flush on channel close")
            .expect("forward_loop exited");
        let ForwardOutput::Text(json) = out else {
            panic!("expected text output");
        };
        let msg: Message = Message::from_json(&json).unwrap();
        let Message::Terminal { payload, .. } = msg else {
            panic!("expected terminal message");
        };
        let crate::enums::TerminalAction::Output { data, index, end_index, start_offset, end_offset, .. } = payload.action else {
            panic!("expected output action");
        };
        assert_eq!(index, 0);
        assert_eq!(end_index, Some(1), "merged two events");
        assert_eq!(start_offset, Some(0));
        assert_eq!(end_offset, Some(4));
        let decoded = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &data).unwrap();
        assert_eq!(decoded, b"abcd");

        // 循环在关闭后退出：recv 返回 Ok(None)（通道关闭）而非超时
        assert!(
            matches!(
                tokio::time::timeout(Duration::from_millis(50), out_rx.recv()).await,
                Ok(None)
            ),
            "forward_loop must exit after channel close"
        );
        let _ = fwd.await;
    }

    /// 流代数门控：代数递增（订阅被替换/取消）后，旧 forward_loop 的残留帧
    /// 不再转发——abort 是异步信号，旧任务在 await 点之间仍可能拿到帧，
    /// 代数校验保证这些帧被丢弃，杜绝旧流注入新订阅通道（移动端连续性
    /// 违反风暴的根源）
    #[tokio::test]
    async fn test_forward_loop_generation_gate_drops_stale_frames() {
        let (tx, mut out_rx, fwd, generation) =
            spawn_forward(Duration::from_millis(20), 64 * 1024, false);

        // 订阅被替换：代数递增，旧 forward_loop 立即失效
        generation.fetch_add(1, Ordering::SeqCst);

        tx.send(event("s", b"stale", 0, 0, 5)).await.unwrap();
        drop(tx);

        // 旧代 forward_loop 应丢弃残留帧并退出：无任何消息发出
        assert!(
            matches!(
                tokio::time::timeout(Duration::from_millis(100), out_rx.recv()).await,
                Ok(None)
            ),
            "stale forward_loop must drop buffered frames"
        );
        let _ = fwd.await;
    }

    /// 代数未变：正常转发不受影响（门控仅在替换/取消订阅时生效）
    #[tokio::test]
    async fn test_forward_loop_generation_gate_passthrough_when_current() {
        let (tx, mut out_rx, fwd, _gen) = spawn_forward(Duration::ZERO, 64 * 1024, false);

        tx.send(event("s", b"live", 0, 0, 4)).await.unwrap();
        drop(tx);

        let out = tokio::time::timeout(Duration::from_millis(100), out_rx.recv())
            .await
            .expect("current generation must forward")
            .expect("forward_loop exited");
        assert!(matches!(out, ForwardOutput::Text(_)));
        let _ = fwd.await;
    }
}
