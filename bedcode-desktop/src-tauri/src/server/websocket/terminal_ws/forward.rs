//! 输出转发层 — 合帧决策与 TB v3 二进制帧编码
//!
//! 从 `terminal_ws` 拆出的纯逻辑部分：不依赖 actor 状态，独立可测。
//! 合帧决策（`OutputBuffer::should_flush` / 连续性切批）是**纯决策单元**，
//! 由订阅者执行体（`super::subscriber`）在拉取模型下统一调用——历史上它由
//! 推送模型的 `forward_loop` 驱动，该循环已随「背压下移」重构下线。

use std::time::Duration;

// ==================== Output Buffer ====================

/// 转发输出形态：TB v3 二进制帧或控制帧（历史边界 / 重同步 / 终止）
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
    /// 重同步信号（spec §4.7）：订阅者游标早于环驻留起点 → 客户端清屏 +
    /// 以 `min_offset` 重锚，随后服务端从 `min_offset` 连续重播。
    /// 只增不改：老客户端忽略未知控制帧后退化为既有「缺口 → 重拼接」自愈路径
    Resync {
        /// 环当前驻留起点（客户端重锚点）
        min_offset: u64,
        /// 重锚后的历史边界（此后 HistoryEnd 以此为界）
        snapshot_offset: u64,
    },
    /// 终止该订阅链路（僵尸订阅者回收）：先尽力下发 error 控制帧，再关闭连接
    Terminate {
        code: String,
        message: String,
    },
}

// ==================== TB v3（spec §5.3，本地环回 + 新远程通道） ====================

/// TB v3 帧头：magic(2) + version(1) + flags(1) + start_offset(8 LE) + len(4 LE) = 16 字节
pub(crate) const V3_FRAME_HEADER_LEN: usize = 16;
const V3_FRAME_MAGIC: [u8; 2] = [0x54, 0x42]; // "TB"
const V3_FRAME_VERSION: u8 = 3;
/// flags bit0 = is_waiting（spec §5.3）
const V3_FRAME_FLAG_WAITING: u8 = 0x01;

// ==================== 双速传播模式（用户需求 3：实时/批次两档） ====================

/// 订阅者传播模式常量定义在 session 层（订阅者状态的一部分），此处重导出
/// 以保持既有引用点（control_frame / terminal_ws）不变
pub(crate) use crate::session::{MODE_BATCH, MODE_REALTIME};

/// 编码 TB v3 输出帧（spec §5.3 字节化：`magic "TB" + version=3 + flags + start_offset(8 LE) + len(4 LE) + data`）
///
/// `start_offset` = 帧内首字节的会话内累计偏移；`end_offset = start_offset + len`
/// 直接可导——高 7 位不再编码事件数（payload 字节长即数量），消费端按字节区间
/// 做连续性校验（= 游标）、缺口检测（>）与跨帧裁剪（<，根治重复渲染）
pub(crate) fn encode_output_frame_v3(start_offset: u64, is_waiting: bool, data: &[u8]) -> Vec<u8> {
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

/// 合帧缓冲（纯决策 + 编码单元，无 IO）
///
/// 订阅者执行体（环拉取路径）使用：
/// - 并入（`append_slice` / `append`）+ 连续性切批判定（`is_contiguous_with`）
/// - 合帧触发判定（`should_flush`：字节窗 / 时间窗 / 双速模式）
/// - 帧编码（`flush` → TB v3 帧头区间 = 负载）
///
/// 把「合帧决策」从循环时序里剥出来，是为了让双速语义与帧区间契约
/// 可以脱离通道/计时器被单测（见模块内 tests）。
pub(crate) struct OutputBuffer {
    data: Vec<u8>,
    /// 帧内首个字节的会话内偏移（TB v3 帧头 start_offset 来源）
    pub(crate) start_offset: u64,
    /// 帧内末尾字节偏移（end_offset = start_offset + len）
    pub(crate) end_offset: u64,
    last_is_waiting: bool,
}

impl OutputBuffer {
    pub(crate) fn new() -> Self {
        Self {
            data: Vec::new(),
            start_offset: 0,
            end_offset: 0,
            last_is_waiting: false,
        }
    }

    /// 按事件并入（生产路径走 `append_slice`；此处供单测构造区间语义一致的批次）
    #[cfg(test)]
    pub(crate) fn append(&mut self, event: &crate::session::OutputEvent) {
        self.append_slice(event.start_offset, &event.data, event.is_waiting);
    }

    /// 按字节区间并入一段负载（环上 `read_at` 返回的单块视图直接可用）
    pub(crate) fn append_slice(&mut self, start_offset: u64, data: &[u8], end_is_waiting: bool) {
        if self.data.is_empty() {
            self.start_offset = start_offset;
        }
        // 始终更新 end_offset 为最新并入区间末
        self.end_offset = start_offset + data.len() as u64;
        self.data.extend_from_slice(data);
        self.last_is_waiting = end_is_waiting;
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// 帧内负载字节数（合帧阈值判定依据）
    pub(crate) fn len(&self) -> usize {
        self.data.len()
    }

    /// 事件/切片是否与当前批次连续（可安全并入同一帧）。
    ///
    /// 判据：缓冲区为空（恒可并入）；或区间首 == 缓冲尾（首尾相接）。
    /// 带洞（区间首越过缓冲尾——上游丢块 / 截断重播）与重叠（早于缓冲尾）
    /// 都必须切批：合并后帧头仅记录 `start_offset` 与 `len`，消费端据此
    /// 推导的区间会与真实负载错位 → 跨帧裁剪/去重静默丢字节、转义序列
    /// 被切在半途（渲染错位、空白间隔）。
    pub(crate) fn is_contiguous_with(&self, start_offset: u64) -> bool {
        self.data.is_empty() || start_offset == self.end_offset
    }

    /// 合帧触发判定（纯函数，新旧路径共用同一语义）
    ///
    /// - `flush_interval = ZERO`：零缓冲直通，恒立即 flush（本地环回通道）
    /// - batch：仅字节窗（满 `batch_bytes`，无时间窗）
    /// - realtime：字节窗（`max_buffer_size`）或时间窗（距上次 flush ≥ `flush_interval`）
    pub(crate) fn should_flush(
        &self,
        mode: u8,
        batch_bytes: usize,
        max_buffer_size: usize,
        since_last_flush: Duration,
        flush_interval: Duration,
    ) -> bool {
        if self.data.is_empty() {
            return false;
        }
        if flush_interval.is_zero() {
            return true;
        }
        if mode == MODE_BATCH {
            return self.data.len() >= batch_bytes;
        }
        self.data.len() >= max_buffer_size || since_last_flush >= flush_interval
    }

    /// Flush 缓冲区为转发输出
    ///
    /// 二进制形态（RemoteV3）：帧头 start_offset = 首字节偏移，payload 转义字节（spec §5.3）
    pub(crate) fn flush(&mut self) -> ForwardOutput {
        let frame = encode_output_frame_v3(self.start_offset, self.last_is_waiting, &self.data);
        self.clear();
        ForwardOutput::Binary(frame)
    }

    /// 清空缓冲（重置批次元数据）
    pub(crate) fn clear(&mut self) {
        self.data.clear();
        self.start_offset = 0;
        self.end_offset = 0;
        self.last_is_waiting = false;
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

    // ==================== 批次连续性（帧区间必须与负载一致） ====================

    /// 连续性判定：空缓冲恒连续；首尾相接连续；带洞/重叠均不连续
    #[test]
    fn output_buffer_contiguity_check() {
        let mut buf = OutputBuffer::new();
        assert!(buf.is_contiguous_with(100), "空缓冲恒可并入");
        buf.append(&event("s", b"ab", 100)); // [100,102)
        assert!(buf.is_contiguous_with(102), "区间首 == 缓冲尾：连续");
        assert!(!buf.is_contiguous_with(104), "越过缓冲尾（带洞）：不得并入");
        assert!(!buf.is_contiguous_with(101), "早于缓冲尾（重叠）：不得并入");
    }

    /// 合帧触发纯决策：零间隔恒直通 / batch 仅字节窗 / realtime 字节窗或时间窗
    #[test]
    fn output_buffer_should_flush_decision_matrix() {
        let mut buf = OutputBuffer::new();
        // 空缓冲恒不 flush（避免空帧）
        assert!(!buf.should_flush(MODE_REALTIME, 8, 8, Duration::from_secs(99), Duration::from_millis(30)));

        buf.append_slice(0, b"abc", false); // 3 字节
                                            // 零缓冲直通：恒立即 flush（本地环回通道，模式无关）
        assert!(buf.should_flush(MODE_BATCH, 8, 8, Duration::ZERO, Duration::ZERO));
        // batch：未满 batch_bytes 不 flush（不受时间窗影响）
        assert!(!buf.should_flush(MODE_BATCH, 8, 8, Duration::from_secs(99), Duration::from_millis(30)));
        // realtime：未达字节窗但已达时间窗 → flush
        assert!(buf.should_flush(
            MODE_REALTIME,
            8,
            8,
            Duration::from_millis(31),
            Duration::from_millis(30)
        ));
        // realtime：时间窗未到且字节窗未达 → 不 flush
        assert!(!buf.should_flush(MODE_REALTIME, 8, 8, Duration::from_millis(1), Duration::from_millis(30)));
        // 字节窗达标：立即 flush（不等时间窗）
        assert!(buf.should_flush(MODE_REALTIME, 8, 3, Duration::ZERO, Duration::from_millis(30)));
        assert!(buf.should_flush(MODE_BATCH, 3, usize::MAX, Duration::ZERO, Duration::from_millis(30)));
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
}
