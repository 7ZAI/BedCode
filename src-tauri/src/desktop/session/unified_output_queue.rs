//! Unified Output Queue
//!
//! 统一输出队列 - 环形缓冲区存储 PTY 输出历史
//! 支持范围查询和自动覆盖最旧数据

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use serde::{Deserialize, Serialize};
use crate::shared::system::config::AppConfig;

/// 输出事件
///
/// 注意：`data` 存储原始字节数据，在发送到 WebSocket 时才进行 Base64 编码
/// 这样可以避免在缓冲合并时多次编解码
#[derive(Debug, Clone)]
pub struct OutputEvent {
    /// 会话 ID
    pub session_id: String,
    /// 原始字节数据（未经 Base64 编码）
    pub data: Vec<u8>,
    /// 全局递增序号
    pub index: u64,
    /// 时间戳（毫秒）
    pub timestamp: i64,
    /// 是否等待输入
    pub is_waiting: bool,
}

/// 用于 JSON 序列化的临时结构（包含 Base64 编码的数据）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputEventSerialized {
    pub session_id: String,
    /// Base64 编码的输出数据
    pub data: String,
    pub index: u64,
    pub timestamp: i64,
    pub is_waiting: bool,
}

impl OutputEvent {
    /// 创建新的输出事件
    pub fn new(session_id: String, data: Vec<u8>, index: u64, timestamp: i64, is_waiting: bool) -> Self {
        Self {
            session_id,
            data,
            index,
            timestamp,
            is_waiting,
        }
    }

    /// 编码为可序列化的结构（用于 WebSocket 发送）
    pub fn to_serialized(&self) -> OutputEventSerialized {
        OutputEventSerialized {
            session_id: self.session_id.clone(),
            data: base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                &self.data,
            ),
            index: self.index,
            timestamp: self.timestamp,
            is_waiting: self.is_waiting,
        }
    }

    /// 获取 Base64 编码的数据
    pub fn data_base64(&self) -> String {
        base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            &self.data,
        )
    }
}

/// 统一输出队列（环形缓冲区）
pub struct UnifiedOutputQueue {
    /// 事件缓冲区
    buffer: VecDeque<OutputEvent>,
    /// 容量（默认 10000 条）
    capacity: usize,
    /// 当前最大序号
    max_seq: AtomicU64,
    /// 当前最小可用序号（用于判断是否被覆盖）
    min_seq: AtomicU64,
    /// 总生产数量（用于统计）
    total_produced: AtomicU64,
}

impl UnifiedOutputQueue {
    /// 创建新队列
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: VecDeque::with_capacity(capacity),
            capacity,
            max_seq: AtomicU64::new(0),
            min_seq: AtomicU64::new(0),
            total_produced: AtomicU64::new(0),
        }
    }

    /// 获取最大序号
    pub fn max_seq(&self) -> u64 {
        self.max_seq.load(Ordering::SeqCst)
    }

    /// 获取最小可用序号
    pub fn min_seq(&self) -> u64 {
        self.min_seq.load(Ordering::SeqCst)
    }

    /// 获取缓冲区长度
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// 检查是否为空
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// 推入新事件
    pub fn push(&mut self, event: OutputEvent) {
        // 更新 max_seq
        self.max_seq.store(event.index, Ordering::SeqCst);
        self.total_produced.fetch_add(1, Ordering::SeqCst);

        // 如果满了，移除最旧的并更新 min_seq
        if self.buffer.len() >= self.capacity {
            if let Some(old) = self.buffer.pop_front() {
                self.min_seq.store(old.index + 1, Ordering::SeqCst);
            }
        }

        self.buffer.push_back(event);
    }

    /// 获取范围数据 [start_seq, max_seq]
    /// 如果 start_seq < min_seq，自动从 min_seq 开始
    pub fn get_range(&self, start_seq: u64) -> Vec<OutputEvent> {
        let min_seq = self.min_seq.load(Ordering::SeqCst);
        let actual_start = start_seq.max(min_seq);

        self.buffer
            .iter()
            .filter(|e| e.index >= actual_start)
            .cloned()
            .collect()
    }
}

impl Default for UnifiedOutputQueue {
    fn default() -> Self {
        let config = AppConfig::global();
        Self::new(config.channels.global_queue_capacity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_event(index: u64) -> OutputEvent {
        OutputEvent::new(
            "test".to_string(),
            b"test".to_vec(), // 原始字节
            index,
            Utc::now().timestamp_millis(),
            false,
        )
    }

    #[test]
    fn test_push_and_get_range() {
        let mut queue = UnifiedOutputQueue::new(10);

        for i in 0..5 {
            queue.push(make_event(i));
        }

        let events = queue.get_range(0);
        assert_eq!(events.len(), 5);
        assert_eq!(events[0].index, 0);
        assert_eq!(events[4].index, 4);
    }

    #[test]
    fn test_overflow_updates_min_seq() {
        let mut queue = UnifiedOutputQueue::new(3);

        for i in 0..5 {
            queue.push(make_event(i));
        }

        // 应该只保留 index 2, 3, 4
        assert_eq!(queue.min_seq(), 2);
        assert_eq!(queue.max_seq(), 4);
        assert_eq!(queue.len(), 3);

        // get_range(0) 应自动从 min_seq=2 开始
        let events = queue.get_range(0);
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].index, 2);
    }

    #[test]
    fn test_get_range_from_middle() {
        let mut queue = UnifiedOutputQueue::new(10);

        for i in 0..10 {
            queue.push(make_event(i));
        }

        let events = queue.get_range(5);
        assert_eq!(events.len(), 5);
        assert_eq!(events[0].index, 5);
    }
}
