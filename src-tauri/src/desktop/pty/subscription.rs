//! PTY Output Subscription Module
//!
//! 提供 PTY 输出的订阅消费机制，满足"持久化订阅 + 实时广播"场景

use crate::desktop::model::PtyOutputEvent;
use std::sync::atomic::{AtomicU64, Ordering};

/// 环形缓冲区 - 存储最近 N 条 PTY 输出消息
pub struct OutputRingBuffer {
    buffer: Vec<Option<PtyOutputEvent>>,
    capacity: usize,
    head: usize,
    count: usize,
    max_seq: AtomicU64,
    total_produced: AtomicU64,
}

impl OutputRingBuffer {
    /// 创建新的环形缓冲区
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: vec![None; capacity],
            capacity,
            head: 0,
            count: 0,
            max_seq: AtomicU64::new(0),
            total_produced: AtomicU64::new(0),
        }
    }

    /// 推送新消息
    pub fn push(&mut self, event: PtyOutputEvent) {
        let index = event.index as u64;

        // 更新 max_seq
        self.max_seq.store(index, Ordering::SeqCst);
        self.total_produced.fetch_add(1, Ordering::SeqCst);

        // 写入环形缓冲区
        self.buffer[self.head] = Some(event);
        self.head = (self.head + 1) % self.capacity;

        if self.count < self.capacity {
            self.count += 1;
        }
    }

    /// 获取从 start_seq 之后的所有消息（包括 start_seq）
    pub fn get_since(&self, start_seq: u64) -> Vec<PtyOutputEvent> {
        if self.count == 0 {
            return vec![];
        }

        let mut result = Vec::new();

        // 遍历缓冲区，从最旧到最新
        for i in 0..self.count {
            let idx = (self.head + self.capacity - self.count + i) % self.capacity;
            if let Some(ref event) = self.buffer[idx] {
                if (event.index as u64) >= start_seq {
                    result.push(event.clone());
                }
            }
        }

        result
    }

    /// 获取当前最大序号
    pub fn max_seq(&self) -> u64 {
        self.max_seq.load(Ordering::SeqCst)
    }

    /// 获取历史总消息数
    pub fn total_produced(&self) -> u64 {
        self.total_produced.load(Ordering::SeqCst)
    }

    /// 获取缓冲区容量
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// 获取当前消息数
    pub fn len(&self) -> usize {
        self.count
    }

    /// 检查是否为空
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// 清空缓冲区
    pub fn clear(&mut self) {
        for i in 0..self.capacity {
            self.buffer[i] = None;
        }
        self.head = 0;
        self.count = 0;
        self.max_seq.store(0, Ordering::SeqCst);
        self.total_produced.store(0, Ordering::SeqCst);
    }
}

impl Default for OutputRingBuffer {
    fn default() -> Self {
        Self::new(10000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_event(index: usize) -> PtyOutputEvent {
        PtyOutputEvent {
            session_id: "test".to_string(),
            data: format!("data{}", index),
            timestamp: Utc::now(),
            is_waiting: false,
            index,
        }
    }

    #[test]
    fn test_ring_buffer_push_and_get() {
        let mut buffer = OutputRingBuffer::new(10);

        for i in 0..5 {
            buffer.push(make_event(i));
        }

        let messages = buffer.get_since(0);
        assert_eq!(messages.len(), 5);
    }

    #[test]
    fn test_ring_buffer_wrap_around() {
        let mut buffer = OutputRingBuffer::new(3);

        for i in 0..5 {
            buffer.push(make_event(i));
        }

        // 应该只保留最新的 3 条 (index 2, 3, 4)
        let messages = buffer.get_since(0);
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].index, 2);
    }

    #[test]
    fn test_get_since_start() {
        let mut buffer = OutputRingBuffer::new(10);

        for i in 0..10 {
            buffer.push(make_event(i));
        }

        // 从 index 5 开始获取
        let messages = buffer.get_since(5);
        assert_eq!(messages.len(), 5); // 5, 6, 7, 8, 9
        assert_eq!(messages[0].index, 5);
    }

    #[test]
    fn test_empty_buffer() {
        let buffer = OutputRingBuffer::new(10);
        assert!(buffer.is_empty());
        assert_eq!(buffer.get_since(0).len(), 0);
    }

    #[test]
    fn test_max_seq() {
        let mut buffer = OutputRingBuffer::new(10);

        buffer.push(make_event(5));
        assert_eq!(buffer.max_seq(), 5);

        buffer.push(make_event(10));
        assert_eq!(buffer.max_seq(), 10);
    }

    #[test]
    fn test_total_produced() {
        let mut buffer = OutputRingBuffer::new(3);

        buffer.push(make_event(0));
        buffer.push(make_event(1));
        buffer.push(make_event(2));
        buffer.push(make_event(3)); // 触发环覆盖

        // total_produced 仍然累加
        assert_eq!(buffer.total_produced(), 4);
    }
}