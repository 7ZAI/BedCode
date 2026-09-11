//! 输出转发层 — 将 OutputEvent 流编码为 TB v3 二进制帧并转发
//!
//! 从 `terminal_ws` 拆出的纯逻辑部分：不依赖 actor 状态，独立可测。
//! 合并/直通时序语义由 `forward_loop` 统一保证（见其文档注释与内联测试）。

use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::session::OutputFrame;

// ==================== Output Buffer ====================

/// 转发输出形态：TB v3 二进制帧或历史标记
#[derive(Debug)]
pub(crate) enum ForwardOutput {
    Binary(Vec<u8>),
    /// 历史边界标记（05 快照协议）：新路由编码 JSON 控制帧 / 旧路由无帧直接吞掉
    /// min_offset/history_bytes 为 05 透传元数据（wire 上由 subscribe_ok 携带），
    /// 保留以备未来 wire 需要（如历史截断提示）
    #[allow(dead_code)]
    HistoryEnd {
        snapshot_offset: u64,
        min_offset: u64,
        history_bytes: u64,
    },
}

// ==================== TB v3（spec §5.3，本地环回 + 新远程通道） ====================

/// TB v3 帧头：magic(2) + version(1) + flags(1) + start_offset(8 LE) + len(4 LE) = 16 字节
const V3_FRAME_HEADER_LEN: usize = 16;
const V3_FRAME_MAGIC: [u8; 2] = [0x54, 0x42]; // "TB"
const V3_FRAME_VERSION: u8 = 3;
/// flags bit0 = is_waiting（spec §5.3）
const V3_FRAME_FLAG_WAITING: u8 = 0x01;

// ==================== 双速传播模式（用户需求 3：实时/批次两档） ====================

/// 订阅者传播模式：realtime = 读即传（时间窗+字节窗合并，现有机制）
/// 0 = realtime（默认）；1 = batch（累计满 batch_bytes 才转发一帧）
pub(crate) const MODE_REALTIME: u8 = 0;
pub(crate) const MODE_BATCH: u8 = 1;

/// 编码 TB v3 输出帧（spec §5.3 字节化：`magic "TB" + version=3 + flags + start_offset(8 LE) + len(4 LE) + data`）
///
/// `start_offset` = 帧内首字节的会话内累计偏移；`end_offset = start_offset + len`
/// 直接可导——高 7 位不再编码事件数（payload 字节长即数量），消费端按字节区间
/// 做连续性校验（= 游标）、缺口检测（>）与跨帧裁剪（<，根治重复渲染）
fn encode_output_frame_v3(start_offset: u64, is_waiting: bool, data: &[u8]) -> Vec<u8> {
    let flags = if is_waiting { V3_FRAME_FLAG_WAITING } else { 0 };
    let mut frame = Vec::with_capacity(V3_FRAME_HEADER_LEN + data.len());
    frame.extend_from_slice(&V3_FRAME_MAGIC);
    frame.push(V3_FRAME_VERSION);
    frame.push(flags);
    frame.extend_from_slice(&start_offset.to_le_bytes());
    frame.extend_from_slice(&(data.len() as u32).to_le_bytes());
    frame.extend_from_slice(data);
    frame
}

struct OutputBuffer {
    data: Vec<u8>,
    /// 帧内首个字节的会话内偏移（TB v3 帧头 start_offset 来源）
    start_offset: u64,
    /// 帧内末尾字节偏移（end_offset = start_offset + len）
    end_offset: u64,
    last_is_waiting: bool,
}

impl OutputBuffer {
    fn new() -> Self {
        Self {
            data: Vec::new(),
            start_offset: 0,
            end_offset: 0,
            last_is_waiting: false,
        }
    }

    fn append(&mut self, event: &crate::session::OutputEvent) {
        if self.data.is_empty() {
            self.start_offset = event.start_offset;
        }
        // 始终更新 end_offset 为最新事件区间末
        self.end_offset = event.end_offset();
        self.data.extend_from_slice(&event.data);
        self.last_is_waiting = event.is_waiting;
    }

    fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Flush 缓冲区为转发输出
    ///
    /// 二进制形态（RemoteV3）：帧头 start_offset = 首字节偏移，payload 转义字节（spec §5.3）
    fn flush(&mut self) -> ForwardOutput {
        let frame = encode_output_frame_v3(self.start_offset, self.last_is_waiting, &self.data);
        self.clear();
        ForwardOutput::Binary(frame)
    }

    /// 清空缓冲（重置批次元数据）
    fn clear(&mut self) {
        self.data.clear();
        self.start_offset = 0;
        self.end_offset = 0;
        self.last_is_waiting = false;
    }
}

/// 输出转发循环 — 将 OutputEvent 流编码为 ForwardOutput 经 out_tx 送出
///
/// 三种风控组合：
/// - `flush_interval = ZERO`：零缓冲直通，每条事件立即转发（本地环回通道 /
///   合并开关关闭），模式无关
/// - realtime（默认）：有界延迟合并——字节达 `max_buffer_size` 或距上次 flush
///   超过 `flush_interval` 时 flush（先到先发），持续输出下延迟恒 ≤ flush_interval
/// - batch：订阅者退出终端页但会话未停时的按批次传输——累计满 `batch_bytes`
///   才转发一帧（无时间窗；不消费的客户端数据留在移动端 Rust 缓存，重进时
///   由历史拼接补回）。模式由 `mode: Arc<AtomicU8>` 实时切换（用户需求 3），
///   切换点即时 flush 残留：realtime→batch 遗留小批立即落盘（否则滞留到
///   下一批次），batch→realtime 恢复即时窗口
///
/// 流代数（stream generation）门控：
/// 订阅者被替换/取消订阅时旧 forward_loop 被 abort——但 abort 是异步信号，
/// 任务可能在 await 点之间已把帧投递到 actor 邮箱，Handler 仍会将其发出，
/// 旧流尾帧与新生订阅帧交错到达 → 客户端字节游标错位 → 连续性违反 → 重订阅风暴。
/// 每次转发前校验代数：订阅/取消订阅时代数递增，旧代 forward_loop 的残留帧
/// 直接丢弃，从根源杜绝旧流帧注入新订阅通道
pub(crate) async fn forward_loop(
    mut output_rx: tokio::sync::mpsc::Receiver<OutputFrame>,
    out_tx: tokio::sync::mpsc::Sender<ForwardOutput>,
    flush_interval: Duration,
    max_buffer_size: usize,
    stream_generation: Arc<AtomicU64>,
    my_gen: u64,
    mode: Arc<AtomicU8>,
    batch_bytes: usize,
) {
    let mut buffer = OutputBuffer::new();

    if flush_interval.is_zero() {
        // 零缓冲直通：每条事件立即转发，不等待（模式无关）
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
                OutputFrame::HistoryEnd {
                    snapshot_offset,
                    min_offset,
                    history_bytes,
                } => {
                    // 历史边界：先 flush 残留缓冲（保证历史字节完整落盘），
                    // 再透传标记——标记必须严格保持在历史帧之后（快照协议顺序）
                    if !buffer.is_empty() && out_tx.send(buffer.flush()).await.is_err() {
                        break;
                    }
                    if out_tx
                        .send(ForwardOutput::HistoryEnd {
                            snapshot_offset,
                            min_offset,
                            history_bytes,
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

    // 有界延迟合并（realtime）/ 批次积累（batch）双模式；模式翻转即时收尾
    let mut last_flush = tokio::time::Instant::now();
    let mut last_mode = mode.load(Ordering::SeqCst);
    loop {
        match tokio::time::timeout(flush_interval, output_rx.recv()).await {
            Ok(Some(frame)) => {
                // 流代数失效（订阅被替换/取消）：残留帧直接丢弃，不转发
                if stream_generation.load(Ordering::SeqCst) != my_gen {
                    break;
                }
                // 每次循环读取一次模式：翻转即即时 flush 残留缓冲
                let cur_mode = mode.load(Ordering::SeqCst);
                if cur_mode != last_mode {
                    last_mode = cur_mode;
                    if !buffer.is_empty() {
                        if out_tx.send(buffer.flush()).await.is_err() {
                            break;
                        }
                        last_flush = tokio::time::Instant::now();
                    }
                }
                match frame {
                    OutputFrame::Output(event) => {
                        buffer.append(&event);
                        let flush_now = if cur_mode == MODE_BATCH {
                            // batch：纯批次语义，无时间窗
                            buffer.data.len() >= batch_bytes
                        } else {
                            buffer.data.len() >= max_buffer_size || last_flush.elapsed() >= flush_interval
                        };
                        if flush_now {
                            if out_tx.send(buffer.flush()).await.is_err() {
                                break;
                            }
                            last_flush = tokio::time::Instant::now();
                        }
                    }
                    OutputFrame::HistoryEnd {
                        snapshot_offset,
                        min_offset,
                        history_bytes,
                    } => {
                        // 历史结束标记：先 flush 残留缓冲，再透传标记（顺序严格）
                        if !buffer.is_empty() {
                            if out_tx.send(buffer.flush()).await.is_err() {
                                break;
                            }
                            last_flush = tokio::time::Instant::now();
                        }
                        if out_tx
                            .send(ForwardOutput::HistoryEnd {
                                snapshot_offset,
                                min_offset,
                                history_bytes,
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
                // channel 关闭，最终 flush（batch 残留也落盘：进程正常关闭路径）
                if !buffer.is_empty() {
                    let _ = out_tx.send(buffer.flush()).await;
                }
                break;
            }
            Err(_) => {
                // 空闲超时：realtime 模式下时间窗到即 flush（空 buffer 也重置
                // 会把持续输出场景的时间窗进度抹掉——timeout 与事件同时就绪时
                // Err 分支先执行，内容 flush 将永远等不到）；batch 模式不因
                // 时间窗 flush（纯批次语义，未满批次的数据按设计滞留，重进时
                // 由历史拼接补回）；模式翻转到 realtime 时残留立即落盘
                let cur_mode = mode.load(Ordering::SeqCst);
                if cur_mode != last_mode {
                    last_mode = cur_mode;
                }
                if !buffer.is_empty() && last_mode == MODE_REALTIME {
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

    fn event(session_id: &str, data: &[u8], start_offset: u64) -> crate::session::OutputEvent {
        crate::session::OutputEvent {
            session_id: session_id.to_string(),
            data: data.to_vec(),
            start_offset,
            timestamp: 0,
            is_waiting: false,
        }
    }

    /// TB v3 二进制帧形态：帧头 start_offset 随事件并入批，字节区间自洽
    #[test]
    fn test_output_buffer_binary_flush_carries_offset_and_data() {
        let mut buf = OutputBuffer::new();
        buf.append(&event("s", b"ab", 100));
        buf.append(&event("s", b"cd", 102));

        let out = buf.flush();
        let ForwardOutput::Binary(frame) = out else {
            panic!("expected binary output");
        };
        // TB v3 帧头：magic(2) + version(1) = 3 + flags(1) + start_offset(8 LE) + len(4 LE)
        assert_eq!(&frame[0..2], b"TB");
        assert_eq!(frame[2], 3);
        assert_eq!(frame[3], 0); // 非等待、无事件数位
        let start_offset = u64::from_le_bytes(frame[4..12].try_into().unwrap());
        assert_eq!(start_offset, 100);
        let len = u32::from_le_bytes(frame[12..16].try_into().unwrap()) as usize;
        assert_eq!(len, 4);
        assert_eq!(&frame[16..16 + len], b"abcd");
        // end_offset = start_offset + len 直接可导
        assert_eq!(start_offset + len as u64, 104);
    }

    /// 单事件 flush：start_offset 为首事件偏移，len = payload 字节长
    #[test]
    fn test_output_buffer_single_event_flush() {
        let mut buf = OutputBuffer::new();
        buf.append(&event("s", b"single", 7));

        let out = buf.flush();
        let ForwardOutput::Binary(frame) = out else {
            panic!("expected binary output");
        };
        assert_eq!(&frame[0..2], b"TB");
        assert_eq!(frame[2], 3);
        assert_eq!(frame[3], 0); // 单事件非等待：高 7 位无事件数语义
        let start_offset = u64::from_le_bytes(frame[4..12].try_into().unwrap());
        assert_eq!(start_offset, 7);
        let len = u32::from_le_bytes(frame[12..16].try_into().unwrap()) as usize;
        assert_eq!(len, 6);
        assert_eq!(&frame[16..16 + len], b"single");
    }

    // ==================== forward_loop 转发循环（合并时序语义） ====================

    /// 启动 forward_loop 并返回 (事件发送端, 输出接收端)；默认 realtime 模式
    fn spawn_forward(
        flush_interval: Duration,
        max_buffer_size: usize,
    ) -> (
        tokio::sync::mpsc::Sender<OutputFrame>,
        tokio::sync::mpsc::Receiver<ForwardOutput>,
        tokio::task::JoinHandle<()>,
        Arc<AtomicU64>,
    ) {
        let (tx, out_rx, fwd, gen, _mode) =
            spawn_forward_with_mode(flush_interval, max_buffer_size, MODE_REALTIME, usize::MAX);
        (tx, out_rx, fwd, gen)
    }

    /// 启动 forward_loop（可指定模式 / 批次阈值）并返回五元组（含模式原子）
    fn spawn_forward_with_mode(
        flush_interval: Duration,
        max_buffer_size: usize,
        mode: u8,
        batch_bytes: usize,
    ) -> (
        tokio::sync::mpsc::Sender<OutputFrame>,
        tokio::sync::mpsc::Receiver<ForwardOutput>,
        tokio::task::JoinHandle<()>,
        Arc<AtomicU64>,
        Arc<AtomicU8>,
    ) {
        let (tx, rx) = tokio::sync::mpsc::channel::<OutputFrame>(128);
        let (out_tx, out_rx) = tokio::sync::mpsc::channel::<ForwardOutput>(128);
        let generation = Arc::new(AtomicU64::new(0));
        let mode = Arc::new(AtomicU8::new(mode));
        let fwd = tokio::spawn(forward_loop(
            rx,
            out_tx,
            flush_interval,
            max_buffer_size,
            generation.clone(),
            0,
            mode.clone(),
            batch_bytes,
        ));
        (tx, out_rx, fwd, generation, mode)
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
        tx.send(OutputFrame::Output(event("s", b"0123456789", 0)))
            .await
            .unwrap(); // 10 字节 > 8
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
        // TB v3 帧头解析：start_offset = 首字节偏移，payload 为原始字节
        let start_offset = u64::from_le_bytes(frame[4..12].try_into().unwrap());
        let len = u32::from_le_bytes(frame[12..16].try_into().unwrap()) as usize;
        assert_eq!(start_offset, 0);
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
        tx.send(OutputFrame::Output(event("s", b"cd", 2))).await.unwrap();
        drop(tx); // 未达时间窗/字节窗 → 关闭时合并两条最终发出

        let out = tokio::time::timeout(Duration::from_millis(100), out_rx.recv())
            .await
            .expect("final flush on channel close")
            .expect("forward_loop exited");
        let ForwardOutput::Binary(frame) = out else {
            panic!("expected binary output");
        };
        // 合并帧：start_offset = 首字节偏移(0)，len = 拼接总字节，payload = abcd
        let start_offset = u64::from_le_bytes(frame[4..12].try_into().unwrap());
        let len = u32::from_le_bytes(frame[12..16].try_into().unwrap()) as usize;
        assert_eq!(start_offset, 0);
        assert_eq!(frame[3], 0, "v3 无事件数位");
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
        let (tx, mut out_rx, fwd, generation) = spawn_forward(Duration::from_millis(20), 64 * 1024);

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
        let (tx, mut out_rx, fwd, _gen) = spawn_forward(Duration::from_millis(60_000), 64 * 1024);

        // 两条历史事件未达时间窗/字节窗，残留于缓冲
        tx.send(OutputFrame::Output(event("s", b"ab", 0))).await.unwrap();
        tx.send(OutputFrame::Output(event("s", b"cd", 2))).await.unwrap();
        tx.send(OutputFrame::HistoryEnd {
            snapshot_offset: 4,
            min_offset: 0,
            history_bytes: 4,
        })
        .await
        .unwrap();
        drop(tx);

        // 第一条 = 残留历史帧（ab+cd 合并，字节区间 [0,4)）
        let out = tokio::time::timeout(Duration::from_millis(100), out_rx.recv())
            .await
            .expect("history must flush before marker")
            .expect("forward_loop exited");
        let ForwardOutput::Binary(frame) = out else {
            panic!("expected binary output");
        };
        // 残留历史两事件合并为一帧（ab+cd，start_offset=0，len=4）
        let start_offset = u64::from_le_bytes(frame[4..12].try_into().unwrap());
        let len = u32::from_le_bytes(frame[12..16].try_into().unwrap()) as usize;
        assert_eq!(start_offset, 0);
        assert_eq!(frame[3], 0, "v3 无事件数位");
        assert_eq!(&frame[16..16 + len], b"abcd");

        // 第二条 = HistoryEnd 标记，严格在历史帧后且携带正确元数据
        let marker = tokio::time::timeout(Duration::from_millis(100), out_rx.recv())
            .await
            .expect("history end marker")
            .expect("forward_loop exited");
        match marker {
            ForwardOutput::HistoryEnd {
                snapshot_offset,
                min_offset,
                history_bytes,
            } => {
                assert_eq!(snapshot_offset, 4);
                assert_eq!(min_offset, 0);
                assert_eq!(history_bytes, 4);
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

    // ==================== TB v3（spec §5.3，新远程通道） ====================

    /// 帧头布局：magic(2) + version(1)=3 + flags(1) + start_offset(8 LE) + len(4 LE) + payload
    #[test]
    fn test_encode_output_frame_v3_header() {
        let frame = encode_output_frame_v3(100, true, b"hello");

        assert_eq!(&frame[0..2], b"TB");
        assert_eq!(frame[2], 3); // version
        assert_eq!(frame[3], V3_FRAME_FLAG_WAITING); // is_waiting
        assert_eq!(u64::from_le_bytes(frame[4..12].try_into().unwrap()), 100);
        assert_eq!(u32::from_le_bytes(frame[12..16].try_into().unwrap()), 5); // len
        assert_eq!(&frame[16..], b"hello");
        assert_eq!(frame.len(), V3_FRAME_HEADER_LEN + 5);
        // end_offset 直接可导：100 + 5
        assert_eq!(100 + 5, 105);
    }

    /// 连续字节偏移：帧内首字节偏移即 start_offset，字节区间随批次拼接
    #[test]
    fn test_output_buffer_v3_flush_merges_with_offsets() {
        let mut buf = OutputBuffer::new();
        buf.append(&event("s", b"ab", 7));
        buf.append(&event("s", b"cd", 9));
        buf.append(&event("s", b"ef", 11));

        let out = buf.flush();
        let ForwardOutput::Binary(frame) = out else {
            panic!("expected binary frame");
        };
        assert_eq!(frame[2], 3);
        assert_eq!(frame[3], 0); // v3 无事件数位
        assert_eq!(u64::from_le_bytes(frame[4..12].try_into().unwrap()), 7);
        assert_eq!(u32::from_le_bytes(frame[12..16].try_into().unwrap()), 6);
        assert_eq!(&frame[16..], b"abcdef");
        assert!(buf.is_empty()); // flush 后清空
    }

    /// TB v3 单事件帧：flags 仅 is_waiting
    #[test]
    fn test_output_buffer_v3_single_event_flush() {
        let mut buf = OutputBuffer::new();
        buf.append(&event("s", b"single", 3));

        let out = buf.flush();
        let ForwardOutput::Binary(frame) = out else {
            panic!("expected binary frame");
        };
        assert_eq!(frame[2], 3);
        assert_eq!(frame[3], 0); // 单事件、非等待
        assert_eq!(u64::from_le_bytes(frame[4..12].try_into().unwrap()), 3);
        assert_eq!(u32::from_le_bytes(frame[12..16].try_into().unwrap()), 6);
        assert_eq!(&frame[16..], b"single");
    }

    /// 长时无事件数上限（v3 删除 128 事件阈值）：仅字节窗/时间窗 flush，
    /// 大批量事件经通道关闭最终合并为一帧（帧内字节区间无空洞）
    #[tokio::test(start_paused = true)]
    async fn test_forward_loop_many_events_merge_without_count_cap() {
        let (tx, mut out_rx, fwd, _gen) = spawn_forward(Duration::from_millis(60_000), 64 * 1024);

        // 连续发送 200 条小事件（v2 时代 128 条会被事件数上限拆分；v3 不分）
        for i in 0..200u64 {
            tx.send(OutputFrame::Output(event("s", b"x", i))).await.unwrap();
        }
        drop(tx); // 未达字节窗/时间窗 → 关闭时全部合并为单帧

        let out = tokio::time::timeout(Duration::from_millis(100), out_rx.recv())
            .await
            .expect("final flush on channel close")
            .expect("forward_loop exited");
        let ForwardOutput::Binary(frame) = out else {
            panic!("expected binary frame");
        };
        // 单帧 200 字节，start_offset=0，无 count 编码
        assert_eq!(frame[2], 3);
        assert_eq!(frame[3], 0);
        assert_eq!(u64::from_le_bytes(frame[4..12].try_into().unwrap()), 0);
        assert_eq!(u32::from_le_bytes(frame[12..16].try_into().unwrap()), 200);
        assert_eq!(&frame[16..], vec![b'x'; 200].as_slice());

        // 无残留帧，forward_loop 退出
        assert!(matches!(
            tokio::time::timeout(Duration::from_millis(50), out_rx.recv()).await,
            Ok(None)
        ));
        let _ = fwd.await;
    }

    // ==================== 双速传播（realtime / batch） ====================

    /// batch 模式：未达 batch_bytes 前不转发（时间窗到也不发）；
    /// 达阈值立即整批 flush，帧内字节区间连续
    #[tokio::test(start_paused = true)]
    async fn test_forward_loop_batch_accumulates_by_bytes() {
        let (tx, mut out_rx, fwd, _gen, _mode) =
            spawn_forward_with_mode(Duration::from_millis(20), 64 * 1024, MODE_BATCH, 8);

        // 3 个小事件（共 6 字节 < 8B 阈值）：不转发
        for i in 0..3u64 {
            tx.send(OutputFrame::Output(event("s", b"ab", i * 2))).await.unwrap();
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        // 越过多个时间窗：batch 模式仍不发（纯批次语义，无时间窗）
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(
            matches!(out_rx.try_recv(), Err(tokio::sync::mpsc::error::TryRecvError::Empty)),
            "batch mode must not flush before byte threshold"
        );

        // 第 4 个小事件：累计 8 字节 = 阈值 → 立即整批 flush
        tx.send(OutputFrame::Output(event("s", b"cd", 6))).await.unwrap();
        let out = tokio::time::timeout(Duration::from_millis(100), out_rx.recv())
            .await
            .expect("batch threshold reached")
            .expect("forward_loop exited");
        let ForwardOutput::Binary(frame) = out else {
            panic!("expected binary frame");
        };
        // 帧字节区间 [0,8)：start_offset=0，len=8，内容 ababab
        assert_eq!(frame[2], 3);
        assert_eq!(u64::from_le_bytes(frame[4..12].try_into().unwrap()), 0);
        assert_eq!(u32::from_le_bytes(frame[12..16].try_into().unwrap()), 8);
        assert_eq!(&frame[16..24], b"abababcd");

        drop(tx);
        let _ = fwd.await;
    }

    /// 模式翻转：realtime → batch 时残留小批立即落盘（不滞留到下一次触发）
    #[tokio::test(start_paused = true)]
    async fn test_forward_loop_mode_switch_flushes_residual() {
        let (tx, mut out_rx, fwd, _gen, mode) =
            spawn_forward_with_mode(Duration::from_millis(60_000), 64 * 1024, MODE_REALTIME, 8);

        // realtime 下 2 字节事件未达字节窗/时间窗，残留于缓冲
        tx.send(OutputFrame::Output(event("s", b"hi", 0))).await.unwrap();
        // 让 fwd 先处理 "hi"（realtime 缓冲）再翻转，避免模式先于首帧生效
        tokio::task::yield_now().await;
        tokio::task::yield_now().await;

        // 切到 batch：残留小批立即落盘（遗留不滞留）
        mode.store(MODE_BATCH, Ordering::SeqCst);
        tx.send(OutputFrame::Output(event("s", b"!", 2))).await.unwrap();

        let out = tokio::time::timeout(Duration::from_millis(100), out_rx.recv())
            .await
            .expect("residual must flush on mode switch")
            .expect("forward_loop exited");
        let ForwardOutput::Binary(frame) = out else {
            panic!("expected binary frame");
        };
        // 残留 2 字节即时落盘（[0,2)），"!" 按 batch 语义进批次缓冲
        assert_eq!(u64::from_le_bytes(frame[4..12].try_into().unwrap()), 0);
        assert_eq!(u32::from_le_bytes(frame[12..16].try_into().unwrap()), 2);
        assert_eq!(&frame[16..18], b"hi");

        // batch 下补足阈值（8 字节）→ 整批 flush（"!" + 8 字节）
        tx.send(OutputFrame::Output(event("s", b"abcdefgh", 3))).await.unwrap();
        let out = tokio::time::timeout(Duration::from_millis(100), out_rx.recv())
            .await
            .expect("batch threshold reached")
            .expect("forward_loop exited");
        let ForwardOutput::Binary(frame) = out else {
            panic!("expected binary frame");
        };
        assert_eq!(u64::from_le_bytes(frame[4..12].try_into().unwrap()), 2);
        assert_eq!(u32::from_le_bytes(frame[12..16].try_into().unwrap()), 9);
        assert_eq!(&frame[16..25], b"!abcdefgh");

        drop(tx);
        let _ = fwd.await;
    }

    /// 模式翻转：batch → realtime 时恢复时间窗即时性（残留立即落盘）
    #[tokio::test(start_paused = true)]
    async fn test_forward_loop_batch_to_realtime_flushes_immediately() {
        let (tx, mut out_rx, fwd, _gen, mode) =
            spawn_forward_with_mode(Duration::from_millis(60_000), 64 * 1024, MODE_BATCH, 64 * 1024);

        // batch 下 3 字节 < 64KB 阈值：不发
        tx.send(OutputFrame::Output(event("s", b"abc", 0))).await.unwrap();
        assert!(matches!(out_rx.try_recv(), Err(tokio::sync::mpsc::error::TryRecvError::Empty)));

        // 切到 realtime：空闲超时（虚拟时钟提前）触发时间窗 flush
        mode.store(MODE_REALTIME, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(60_001)).await;

        let out = tokio::time::timeout(Duration::from_millis(100), out_rx.recv())
            .await
            .expect("realtime must flush residual")
            .expect("forward_loop exited");
        let ForwardOutput::Binary(frame) = out else {
            panic!("expected binary frame");
        };
        assert_eq!(u32::from_le_bytes(frame[12..16].try_into().unwrap()), 3);
        assert_eq!(&frame[16..19], b"abc");

        drop(tx);
        let _ = fwd.await;
    }
}
