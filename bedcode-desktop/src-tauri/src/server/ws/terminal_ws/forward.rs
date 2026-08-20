//! 输出转发层 — 将 OutputEvent 流编码为 TB v2 二进制帧并转发
//!
//! 从 `terminal_ws` 拆出的纯逻辑部分：不依赖 actor 状态，独立可测。
//! 合并/直通时序语义由 `forward_loop` 统一保证（见其文档注释与内联测试）。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::session::OutputFrame;

// ==================== Output Buffer ====================

/// 转发输出形态：TB v2 二进制帧或历史标记
#[derive(Debug)]
pub(super) enum ForwardOutput {
    Binary(Vec<u8>),
    /// 历史边界标记（05 快照协议）：新路由编码 JSON 控制帧 / 旧路由无帧直接吞掉
    /// min_seq/history_count 为 05 透传元数据（wire 上由 subscribe_ok 携带），
    /// 保留以备未来 wire 需要（如历史截断提示）
    #[allow(dead_code)]
    HistoryEnd {
        snapshot_seq: u64,
        min_seq: u64,
        history_count: usize,
    },
}

/// TB v2 帧头：magic(2) + version(1) + flags(1) + seq(8 LE) + len(4 LE) = 16 字节
// ==================== TB v2（spec §5.3，本地环回 + 新远程通道） ====================

/// TB v2 帧头：magic(2) + version(1) + flags(1) + seq(8 LE) + len(4 LE) = 16 字节
const V2_FRAME_HEADER_LEN: usize = 16;
const V2_FRAME_MAGIC: [u8; 2] = [0x54, 0x42]; // "TB"
const V2_FRAME_VERSION: u8 = 2;
/// flags bit0 = is_waiting（spec §5.3）
const V2_FRAME_FLAG_WAITING: u8 = 0x01;
/// flags 高 7 位编码「帧内事件数 - 1」：
///
/// 16B 帧头只有单条 seq（8B）+ len（4B）。合并帧含多条事件，消费端若无法
/// 得知帧内事件数，则无法推导帧末 seq——seq 缺口检测（丢帧自愈）与重播
/// 去重（跳过 ≤ last_rendered_seq 的帧）都会因合并批次边界误判。
/// 约定：`seq` = 帧内首事件 index，帧内事件数 = `(flags >> 1) + 1`，
/// 帧末 seq = `seq + count - 1`。单事件帧高 7 位 = 0，帧头与 spec 原义逐字节一致。
const V2_FRAME_FLAG_COUNT_SHIFT: u8 = 1;
/// 单帧事件数上限：flags 高 7 位 = count - 1，最大 128；超出拆分 flush
const V2_FRAME_MAX_EVENTS: usize = 128;

/// 编码 TB v2 输出帧（spec §5.3：`magic "TB" + version=2 + flags + seq(8 LE) + len(4 LE) + data`）
///
/// `seq` = 帧内首事件 index；`event_count` = 帧内事件数（1..=128）；
/// `len` = data 字节数（与 WS 帧边界一致，供解析器快速定位 data 长度）
fn encode_output_frame_v2(seq: u64, event_count: usize, is_waiting: bool, data: &[u8]) -> Vec<u8> {
    debug_assert!((1..=V2_FRAME_MAX_EVENTS).contains(&event_count));
    let flags = ((event_count - 1) as u8) << V2_FRAME_FLAG_COUNT_SHIFT
        | if is_waiting { V2_FRAME_FLAG_WAITING } else { 0 };
    let mut frame = Vec::with_capacity(V2_FRAME_HEADER_LEN + data.len());
    frame.extend_from_slice(&V2_FRAME_MAGIC);
    frame.push(V2_FRAME_VERSION);
    frame.push(flags);
    frame.extend_from_slice(&seq.to_le_bytes());
    frame.extend_from_slice(&(data.len() as u32).to_le_bytes());
    frame.extend_from_slice(data);
    frame
}

struct OutputBuffer {
    data: Vec<u8>,
    /// 帧内首事件 index（TB v2 帧头 seq 来源）
    start_index: u64,
    end_index: u64,
    last_is_waiting: bool,
    /// 帧内事件数（TB v2 帧头 flags 高 7 位来源）
    event_count: usize,
}

impl OutputBuffer {
    fn new() -> Self {
        Self {
            data: Vec::new(),
            start_index: 0,
            end_index: 0,
            last_is_waiting: false,
            event_count: 0,
        }
    }

    fn append(&mut self, event: &crate::session::OutputEvent) {
        if self.data.is_empty() {
            self.start_index = event.index;
        }
        // 始终更新 end_index 为最新事件的 index
        self.end_index = event.index;
        self.data.extend_from_slice(&event.data);
        self.last_is_waiting = event.is_waiting;
        self.event_count += 1;
    }

    fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// 合并帧事件数是否已达 TB v2 单帧上限（128）
    fn at_max_events(&self) -> bool {
        self.event_count >= V2_FRAME_MAX_EVENTS
    }

    /// Flush 缓冲区为转发输出
    ///
    /// 二进制形态（RemoteV2）：帧头 seq = 首事件 index + flags 编码事件数（spec §5.3）
    fn flush(&mut self) -> ForwardOutput {
        let frame = encode_output_frame_v2(
            self.start_index,
            self.event_count,
            self.last_is_waiting,
            &self.data,
        );
        self.clear();
        ForwardOutput::Binary(frame)
    }

    /// 清空缓冲（重置批次元数据）
    fn clear(&mut self) {
        self.data.clear();
        self.start_index = 0;
        self.end_index = 0;
        self.event_count = 0;
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
    mut output_rx: tokio::sync::mpsc::Receiver<OutputFrame>,
    out_tx: tokio::sync::mpsc::Sender<ForwardOutput>,
    flush_interval: Duration,
    max_buffer_size: usize,
    stream_generation: Arc<AtomicU64>,
    my_gen: u64,
) {
    let mut buffer = OutputBuffer::new();

    if flush_interval.is_zero() {
        // 零缓冲直通：每条事件立即转发，不等待
        while let Some(frame) = output_rx.recv().await {
            if stream_generation.load(Ordering::SeqCst) != my_gen {
                break;
            }
            match frame {
                OutputFrame::Output(event) => {
                    buffer.append(&event);
                    if out_tx.send(buffer.flush()).await.is_err() {
                        break;
                    }
                }
                OutputFrame::HistoryEnd { snapshot_seq, min_seq, history_count } => {
                    // 历史边界：先 flush 残留缓冲（保证历史字节完整落盘），
                    // 再透传标记——标记必须严格保持在历史帧之后（快照协议顺序）
                    if !buffer.is_empty() && out_tx.send(buffer.flush()).await.is_err() {
                        break;
                    }
                    if out_tx
                        .send(ForwardOutput::HistoryEnd {
                            snapshot_seq,
                            min_seq,
                            history_count,
                        })
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            }
        }
        return;
    }

    // 有界延迟合并：时间窗 / 字节窗双条件，先到先发
    let mut last_flush = tokio::time::Instant::now();
    loop {
        match tokio::time::timeout(flush_interval, output_rx.recv()).await {
            Ok(Some(frame)) => {
                // 流代数失效（订阅被替换/取消）：残留帧直接丢弃，不转发
                if stream_generation.load(Ordering::SeqCst) != my_gen {
                    break;
                }
                match frame {
                    OutputFrame::Output(event) => {
                        buffer.append(&event);
                        if buffer.data.len() >= max_buffer_size
                            || buffer.at_max_events()
                            || last_flush.elapsed() >= flush_interval
                        {
                            if out_tx.send(buffer.flush()).await.is_err() {
                                break;
                            }
                            last_flush = tokio::time::Instant::now();
                        }
                    }
                    OutputFrame::HistoryEnd { snapshot_seq, min_seq, history_count } => {
                        // 历史结束标记：先 flush 残留缓冲，再透传标记（顺序严格）
                        if !buffer.is_empty() {
                            if out_tx.send(buffer.flush()).await.is_err() {
                                break;
                            }
                            last_flush = tokio::time::Instant::now();
                        }
                        if out_tx
                            .send(ForwardOutput::HistoryEnd {
                                snapshot_seq,
                                min_seq,
                                history_count,
                            })
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            }
            Ok(None) => {
                // channel 关闭，最终 flush
                if !buffer.is_empty() {
                    let _ = out_tx.send(buffer.flush()).await;
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
                    if out_tx.send(buffer.flush()).await.is_err() {
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

    fn event(session_id: &str, data: &[u8], index: u64) -> crate::session::OutputEvent {
        crate::session::OutputEvent {
            session_id: session_id.to_string(),
            data: data.to_vec(),
            index,
            timestamp: 0,
            is_waiting: false,
        }
    }

    /// TB v2 二进制帧形态：帧头随事件并入批
    #[test]
    fn test_output_buffer_binary_flush_carries_seq_and_data() {
        let mut buf = OutputBuffer::new();
        buf.append(&event("s", b"ab", 3));
        buf.append(&event("s", b"cd", 4));

        let out = buf.flush();
        let ForwardOutput::Binary(frame) = out else {
            panic!("expected binary output");
        };
        // TB v2 帧头：magic(2) + version(1) + flags(1) + seq(8 LE) + len(4 LE)
        assert_eq!(&frame[0..2], b"TB");
        assert_eq!(frame[2], 2);
        // flags：count-1 = 1 → 高 7 位编码 1
        assert_eq!(frame[3], 2);
        let seq = u64::from_le_bytes(frame[4..12].try_into().unwrap());
        assert_eq!(seq, 3);
        let len = u32::from_le_bytes(frame[12..16].try_into().unwrap()) as usize;
        assert_eq!(len, 4);
        assert_eq!(&frame[16..16 + len], b"abcd");
    }

    /// 单事件 flush：count = 1，seq 为事件索引
    #[test]
    fn test_output_buffer_single_event_flush() {
        let mut buf = OutputBuffer::new();
        buf.append(&event("s", b"single", 7));

        let out = buf.flush();
        let ForwardOutput::Binary(frame) = out else {
            panic!("expected binary output");
        };
        assert_eq!(&frame[0..2], b"TB");
        assert_eq!(frame[2], 2);
        assert_eq!(frame[3], 0); // count-1 = 0
        let seq = u64::from_le_bytes(frame[4..12].try_into().unwrap());
        assert_eq!(seq, 7);
        let len = u32::from_le_bytes(frame[12..16].try_into().unwrap()) as usize;
        assert_eq!(len, 6);
        assert_eq!(&frame[16..16 + len], b"single");
    }

    // ==================== forward_loop 转发循环（合并时序语义） ====================

    /// 启动 forward_loop 并返回 (事件发送端, 输出接收端)
    fn spawn_forward(
        flush_interval: Duration,
        max_buffer_size: usize,
    ) -> (
        tokio::sync::mpsc::Sender<OutputFrame>,
        tokio::sync::mpsc::Receiver<ForwardOutput>,
        tokio::task::JoinHandle<()>,
        Arc<AtomicU64>,
    ) {
        let (tx, rx) = tokio::sync::mpsc::channel::<OutputFrame>(128);
        let (out_tx, out_rx) = tokio::sync::mpsc::channel::<ForwardOutput>(128);
        let generation = Arc::new(AtomicU64::new(0));
        let fwd = tokio::spawn(forward_loop(
            rx,
            out_tx,
            flush_interval,
            max_buffer_size,
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
        let (tx, mut out_rx, fwd, _gen) = spawn_forward(Duration::from_millis(20), 64 * 1024);

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
                let ForwardOutput::Binary(_) = out else {
                    panic!("expected text output");
                };
                messages += 1;
            }
            let _ = res_tx.send((messages, first_at)).await;
        });

        // 每 5ms 一条小事件，共 30 条（持续 150ms，间隔远小于 20ms 时间窗）
        for i in 0..30u64 {
            tx.send(OutputFrame::Output(event("s", b"x", i))).await.unwrap();
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
        let (tx, mut out_rx, fwd, _gen) = spawn_forward(Duration::from_millis(500), 8);

        let start = tokio::time::Instant::now();
        tx.send(OutputFrame::Output(event("s", b"0123456789", 0))).await.unwrap(); // 10 字节 > 8
        drop(tx);

        let out = tokio::time::timeout(Duration::from_millis(50), out_rx.recv())
            .await
            .expect("byte window must flush immediately")
            .expect("forward_loop exited");
        // 立即 flush：虚拟时间几乎未流逝，不等到 500ms 时间窗
        assert!(start.elapsed() < Duration::from_millis(100));

        let ForwardOutput::Binary(frame) = out else {
            panic!("expected binary output");
        };
        // TB v2 帧头解析：seq = 首事件 index，payload 为原始字节
        let seq = u64::from_le_bytes(frame[4..12].try_into().unwrap());
        let len = u32::from_le_bytes(frame[12..16].try_into().unwrap()) as usize;
        assert_eq!(seq, 0);
        assert_eq!(&frame[16..16 + len], b"0123456789");
        let _ = fwd.await;
    }

    /// 空闲 flush：单条小事件后无后续，≤ flush_interval 后发出（不无限滞留）
    ///
    /// 注意保持 sender 存活：drop 会触发 final flush 路径（立即发出），
    /// 而非空闲超时路径
    #[tokio::test(start_paused = true)]
    async fn test_forward_loop_idle_flush_within_interval() {
        let (tx, mut out_rx, fwd, _gen) = spawn_forward(Duration::from_millis(30), 64 * 1024);

        let start = tokio::time::Instant::now();
        tx.send(OutputFrame::Output(event("s", b"hi", 0))).await.unwrap();

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
        assert!(matches!(out, ForwardOutput::Binary(_)));

        drop(tx); // 关闭通道，让循环退出
        let _ = fwd.await;
    }

    /// 零间隔（直通模式）：每条事件立即转发，消息数 = 事件数，无合并
    #[tokio::test]
    async fn test_forward_loop_zero_interval_passthrough() {
        let (tx, mut out_rx, fwd, _gen) = spawn_forward(Duration::ZERO, 64 * 1024);

        for i in 0..5u64 {
            tx.send(OutputFrame::Output(event("s", b"x", i))).await.unwrap();
        }
        drop(tx);

        let mut messages = 0;
        while let Some(out) = out_rx.recv().await {
            let ForwardOutput::Binary(_) = out else {
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
        let (tx, mut out_rx, fwd, _gen) = spawn_forward(Duration::from_millis(60_000), 64 * 1024);

        tx.send(OutputFrame::Output(event("s", b"ab", 0))).await.unwrap();
        tx.send(OutputFrame::Output(event("s", b"cd", 1))).await.unwrap();
        drop(tx); // 未达时间窗/字节窗 → 关闭时合并两条最终发出

        let out = tokio::time::timeout(Duration::from_millis(100), out_rx.recv())
            .await
            .expect("final flush on channel close")
            .expect("forward_loop exited");
        let ForwardOutput::Binary(frame) = out else {
            panic!("expected binary output");
        };
        // 合并帧：seq = 首事件 index(0)，count = 2（flags 高 7 位 = 1），payload = abcd
        let seq = u64::from_le_bytes(frame[4..12].try_into().unwrap());
        let len = u32::from_le_bytes(frame[12..16].try_into().unwrap()) as usize;
        assert_eq!(seq, 0);
        assert_eq!(frame[3], 2, "merged two events");
        assert_eq!(&frame[16..16 + len], b"abcd");

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
            spawn_forward(Duration::from_millis(20), 64 * 1024);

        // 订阅被替换：代数递增，旧 forward_loop 立即失效
        generation.fetch_add(1, Ordering::SeqCst);

        tx.send(OutputFrame::Output(event("s", b"stale", 0))).await.unwrap();
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
        let (tx, mut out_rx, fwd, _gen) = spawn_forward(Duration::ZERO, 64 * 1024);

        tx.send(OutputFrame::Output(event("s", b"live", 0))).await.unwrap();
        drop(tx);

        let out = tokio::time::timeout(Duration::from_millis(100), out_rx.recv())
            .await
            .expect("current generation must forward")
            .expect("forward_loop exited");
        assert!(matches!(out, ForwardOutput::Binary(_)));
        let _ = fwd.await;
    }

    /// HistoryEnd 到达时先 flush 残留缓冲，标记严格保持在历史帧之后（快照协议顺序）
    #[tokio::test]
    async fn test_forward_loop_history_end_flushes_and_orders() {
        let (tx, mut out_rx, fwd, _gen) =
            spawn_forward(Duration::from_millis(60_000), 64 * 1024);

        // 两条历史事件未达时间窗/字节窗，残留于缓冲
        tx.send(OutputFrame::Output(event("s", b"ab", 0))).await.unwrap();
        tx.send(OutputFrame::Output(event("s", b"cd", 1))).await.unwrap();
        tx.send(OutputFrame::HistoryEnd {
            snapshot_seq: 1,
            min_seq: 0,
            history_count: 2,
        })
        .await
        .unwrap();
        drop(tx);

        // 第一条 = 残留历史帧（ab+cd 合并，合成游标 [0,4)）
        let out = tokio::time::timeout(Duration::from_millis(100), out_rx.recv())
            .await
            .expect("history must flush before marker")
            .expect("forward_loop exited");
        let ForwardOutput::Binary(frame) = out else {
            panic!("expected binary output");
        };
        // 残留历史两事件合并为一帧（ab+cd，count=2）
        let len = u32::from_le_bytes(frame[12..16].try_into().unwrap()) as usize;
        assert_eq!(frame[3], 2, "two events merged");
        assert_eq!(&frame[16..16 + len], b"abcd");

        // 第二条 = HistoryEnd 标记，严格在历史帧后且携带正确元数据
        let marker = tokio::time::timeout(Duration::from_millis(100), out_rx.recv())
            .await
            .expect("history end marker")
            .expect("forward_loop exited");
        match marker {
            ForwardOutput::HistoryEnd {
                snapshot_seq,
                min_seq,
                history_count,
            } => {
                assert_eq!(snapshot_seq, 1);
                assert_eq!(min_seq, 0);
                assert_eq!(history_count, 2);
            }
            _ => panic!("expected HistoryEnd marker"),
        }

        // 标记后无更多帧（通道已关闭）
        assert!(matches!(
            tokio::time::timeout(Duration::from_millis(50), out_rx.recv()).await,
            Ok(None)
        ));
        let _ = fwd.await;
    }

    // ==================== TB v2（spec §5.3，新远程通道） ====================

    /// 帧头布局：magic(2) + version(1)=2 + flags(1) + seq(8 LE) + len(4 LE) + payload
    #[test]
    fn test_encode_output_frame_v2_header() {
        let frame = encode_output_frame_v2(100, 1, true, b"hello");

        assert_eq!(&frame[0..2], b"TB");
        assert_eq!(frame[2], 2); // version
        assert_eq!(frame[3], V2_FRAME_FLAG_WAITING); // 单事件帧：is_waiting
        assert_eq!(u64::from_le_bytes(frame[4..12].try_into().unwrap()), 100);
        assert_eq!(u32::from_le_bytes(frame[12..16].try_into().unwrap()), 5); // len
        assert_eq!(&frame[16..], b"hello");
        assert_eq!(frame.len(), V2_FRAME_HEADER_LEN + 5);
    }

    /// 合并帧 flags 高 7 位编码事件数 - 1（帧末 seq = seq + count - 1）
    #[test]
    fn test_encode_output_frame_v2_event_count_flags() {
        let frame = encode_output_frame_v2(5, 4, false, b"abcd");
        // 4 事件 → (4-1) << 1 = 0b110 = 6；非等待 → bit0 = 0
        assert_eq!(frame[3], 6);
        let frame = encode_output_frame_v2(5, 128, true, b"x");
        // 128 事件 → (128-1) << 1 = 254；等待 → bit0 = 1 → 255
        assert_eq!(frame[3], 255);
    }

    /// TB v2 合并 flush：seq = 首事件 index，flags 编码事件数，data 为拼接字节
    #[test]
    fn test_output_buffer_v2_flush_merges_with_seq() {
        let mut buf = OutputBuffer::new();
        buf.append(&event("s", b"ab", 7));
        buf.append(&event("s", b"cd", 8));
        buf.append(&event("s", b"ef", 9));

        let out = buf.flush();
        let ForwardOutput::Binary(frame) = out else {
            panic!("expected binary frame");
        };
        assert_eq!(frame[2], 2);
        // 3 事件 → (3-1) << 1 = 4
        assert_eq!(frame[3], 4);
        assert_eq!(u64::from_le_bytes(frame[4..12].try_into().unwrap()), 7);
        assert_eq!(u32::from_le_bytes(frame[12..16].try_into().unwrap()), 6);
        assert_eq!(&frame[16..], b"abcdef");
        assert!(buf.is_empty()); // flush 后清空
    }

    /// TB v2 单事件帧：flags 仅 is_waiting（与 spec 原义逐字节一致）
    #[test]
    fn test_output_buffer_v2_single_event_flush() {
        let mut buf = OutputBuffer::new();
        buf.append(&event("s", b"single", 3));

        let out = buf.flush();
        let ForwardOutput::Binary(frame) = out else {
            panic!("expected binary frame");
        };
        assert_eq!(frame[2], 2);
        assert_eq!(frame[3], 0); // 单事件、非等待
        assert_eq!(u64::from_le_bytes(frame[4..12].try_into().unwrap()), 3);
        assert_eq!(u32::from_le_bytes(frame[12..16].try_into().unwrap()), 6);
        assert_eq!(&frame[16..], b"single");
    }

    /// 事件数上限：合并批次达到 128 条立即 flush（即使未达字节/时间窗）
    #[tokio::test(start_paused = true)]
    async fn test_forward_loop_v2_event_count_cap_flushes() {
        let (tx, mut out_rx, fwd, _gen) =
            spawn_forward(Duration::from_millis(60_000), 64 * 1024);

        // 连续发送 128 条小事件（时间窗 60s 未到、字节窗 64KB 未到）
        for i in 0..128u64 {
            tx.send(OutputFrame::Output(event("s", b"x", i))).await.unwrap();
        }
        drop(tx);

        let out = tokio::time::timeout(Duration::from_millis(100), out_rx.recv())
            .await
            .expect("event count cap must flush immediately")
            .expect("forward_loop exited");
        let ForwardOutput::Binary(frame) = out else {
            panic!("expected binary frame");
        };
        // 128 事件 → flags 高 7 位 = 127（0b1111111 << 1 = 254）
        assert_eq!(frame[3], 254);
        assert_eq!(u64::from_le_bytes(frame[4..12].try_into().unwrap()), 0);
        assert_eq!(u32::from_le_bytes(frame[12..16].try_into().unwrap()), 128);
        assert_eq!(&frame[16..], vec![b'x'; 128].as_slice());

        // 事件数清零：批次后通道关闭 → 无残留帧，forward_loop 退出
        assert!(matches!(
            tokio::time::timeout(Duration::from_millis(50), out_rx.recv()).await,
            Ok(None)
        ));
        let _ = fwd.await;
    }
}
