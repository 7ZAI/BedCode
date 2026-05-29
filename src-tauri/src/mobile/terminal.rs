//! Mobile Terminal Manager
//!
//! 移动端终端缓冲区管理 - 将复杂的输出管理逻辑从前端移至 Rust 后端
//!
//! 职责：
//! - 环形缓冲区存储输出历史
//! - 按字节限制缓冲区大小
//! - Base64 解码
//! - 增量数据返回
//! - 断线重连后历史数据恢复

use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 单个输出事件
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalOutputEvent {
    /// 会话 ID
    pub session_id: String,
    /// 解码后的输出数据
    pub data: String,
    /// 是否等待输入
    pub is_waiting: bool,
    /// 全局递增索引
    pub index: usize,
    /// 时间戳
    pub timestamp: i64,
}

/// 终端输出历史响应
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalHistory {
    /// 历史事件列表
    pub events: Vec<TerminalOutputEvent>,
    /// 当前写入索引位置
    pub current_index: usize,
    /// 缓冲区总事件数
    pub total_count: usize,
}

/// 终端增量输出（用于增量渲染）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalIncrementalOutput {
    /// 新增的事件列表
    pub events: Vec<TerminalOutputEvent>,
    /// 当前写入索引位置
    pub current_index: usize,
    /// 是否为首次（完整历史）
    pub is_initial: bool,
}

/// 环形缓冲区 - 存储终端输出历史
#[derive(Clone)]
pub struct TerminalBuffer {
    /// 事件列表（固定容量，满了覆盖最旧的）
    events: VecDeque<TerminalOutputEvent>,
    /// 缓冲区容量（按事件数量）
    capacity: usize,
    /// 缓冲区容量（按字节数）
    byte_capacity: usize,
    /// 当前写入索引（全局递增）
    write_index: usize,
    /// 当前缓冲区的实际字节大小
    current_bytes: usize,
}

impl TerminalBuffer {
    /// 创建新的终端缓冲区
    pub fn new(capacity: usize, byte_capacity: usize) -> Self {
        Self {
            events: VecDeque::with_capacity(capacity),
            capacity,
            byte_capacity,
            write_index: 0,
            current_bytes: 0,
        }
    }

    /// 写入事件（自动 Base64 解码）
    pub fn push(&mut self, session_id: String, data_base64: String, is_waiting: bool) {
        // Base64 解码
        let data = decode_base64(&data_base64);
        let data_len = data.len();

        // 计算如果添加新数据后的总字节数
        let new_bytes = self.current_bytes + data_len;

        // 如果超过字节限制，先清理旧数据
        while new_bytes > self.byte_capacity && !self.events.is_empty() {
            if let Some(old_event) = self.events.pop_front() {
                self.current_bytes -= old_event.data.len();
                self.write_index -= 1; // 回退索引
            }
        }

        // 如果事件数超过容量，移除最旧的
        if self.events.len() >= self.capacity {
            if let Some(old_event) = self.events.pop_front() {
                self.current_bytes -= old_event.data.len();
            }
        }

        let event = TerminalOutputEvent {
            session_id,
            data,
            is_waiting,
            index: self.write_index,
            timestamp: chrono::Utc::now().timestamp_millis(),
        };

        self.current_bytes += event.data.len();
        self.events.push_back(event);
        self.write_index += 1;
    }

    /// 写入事件（使用外部指定的全局索引，不更新 write_index）
    pub fn push_with_index(&mut self, session_id: String, data_base64: String, is_waiting: bool, global_index: usize) {
        // Base64 解码
        let data = decode_base64(&data_base64);
        let data_len = data.len();

        // 计算如果添加新数据后的总字节数
        let new_bytes = self.current_bytes + data_len;

        // 如果超过字节限制，先清理旧数据
        while new_bytes > self.byte_capacity && !self.events.is_empty() {
            if let Some(old_event) = self.events.pop_front() {
                self.current_bytes -= old_event.data.len();
                // 注意：这里也不回退 write_index，因为使用的是全局索引
            }
        }

        // 如果事件数超过容量，移除最旧的
        if self.events.len() >= self.capacity {
            if let Some(old_event) = self.events.pop_front() {
                self.current_bytes -= old_event.data.len();
            }
        }

        let event = TerminalOutputEvent {
            session_id,
            data,
            is_waiting,
            index: global_index,  // 使用外部传入的全局索引
            timestamp: chrono::Utc::now().timestamp_millis(),
        };

        self.current_bytes += event.data.len();
        self.events.push_back(event);
        // 注意：不更新 write_index，因为使用的是全局索引
    }

    /// 获取所有事件（从最旧到最新）
    pub fn get_all(&self) -> Vec<TerminalOutputEvent> {
        self.events.iter().cloned().collect()
    }

    /// 获取增量数据（从指定索引之后）
    pub fn get_incremental(&self, from_index: usize) -> Vec<TerminalOutputEvent> {
        self.events
            .iter()
            .filter(|e| e.index > from_index)
            .cloned()
            .collect()
    }

    /// 获取当前写入位置
    pub fn current_index(&self) -> usize {
        self.write_index
    }

    /// 获取事件数量
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// 检查是否为空
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// 清空缓冲区
    pub fn clear(&mut self) {
        self.events.clear();
        self.write_index = 0;
        self.current_bytes = 0;
    }
}

/// Base64 解码（支持 UTF-8 多字节字符）
/// 使用 lossy 转换确保即使有非 UTF-8 字节也能正确显示
fn decode_base64(encoded: &str) -> String {
    // 使用标准 base64 解码
    let decoded = base64_decode(encoded);

    // 使用 lossy 转换，将无效 UTF-8 字节替换为替换字符而不是返回原始 Base64
    String::from_utf8_lossy(&decoded).to_string()
}

/// 标准 Base64 解码（使用标准库实现）
fn base64_decode(input: &str) -> Vec<u8> {
    // 移除填充字符并添加必要的填充
    let input = input.trim_end_matches('=');
    let mut result = Vec::with_capacity(input.len() * 3 / 4);

    let alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut buffer: u32 = 0;
    let mut bits_collected = 0;

    for c in input.chars() {
        let value = match alphabet.find(c) {
            Some(v) => v as u32,
            None => continue, // 跳过非 base64 字符
        };

        buffer = (buffer << 6) | value;
        bits_collected += 6;

        if bits_collected >= 8 {
            bits_collected -= 8;
            result.push((buffer >> bits_collected) as u8);
            buffer &= (1 << bits_collected) - 1;
        }
    }

    result
}

/// 终端管理器 - 管理所有会话的终端缓冲区
pub struct TerminalManager {
    /// 会话 ID -> 终端缓冲区
    buffers: Arc<RwLock<std::collections::HashMap<String, TerminalBuffer>>>,
    /// 默认容量（事件数）
    default_capacity: usize,
    /// 默认字节容量
    default_byte_capacity: usize,
    /// 订阅者的写入索引（session_id -> last_index）
    subscribers: Arc<RwLock<std::collections::HashMap<String, usize>>>,
}

impl TerminalManager {
    /// 创建新的终端管理器
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            buffers: Arc::new(RwLock::new(std::collections::HashMap::new())),
            default_capacity: 10000,
            default_byte_capacity: 500_000, // 500KB
            subscribers: Arc::new(RwLock::new(std::collections::HashMap::new())),
        })
    }

    /// 获取或创建会话缓冲区
    async fn get_or_create_buffer(&self, session_id: &str) -> Arc<RwLock<TerminalBuffer>> {
        let mut buffers = self.buffers.write().await;

        if !buffers.contains_key(session_id) {
            buffers.insert(
                session_id.to_string(),
                TerminalBuffer::new(self.default_capacity, self.default_byte_capacity),
            );
        }

        // 克隆缓冲区并包装成 Arc<RwLock>
        let buffer = buffers.get(session_id).unwrap().clone();
        Arc::new(RwLock::new(buffer))
    }

    /// 写入输出数据到缓冲区，并返回解码后的数据和新事件的索引
    pub async fn write_output_and_get(&self, session_id: &str, data_base64: String, is_waiting: bool) -> (String, usize) {
        let mut buffers = self.buffers.write().await;

        let decoded_data = decode_base64(&data_base64);
        let data_len = decoded_data.len();

        if let Some(buffer) = buffers.get_mut(session_id) {
            // 计算如果添加新数据后的总字节数
            let new_bytes = buffer.current_bytes + data_len;

            // 如果超过字节限制，先清理旧数据
            while new_bytes > buffer.byte_capacity && !buffer.events.is_empty() {
                if let Some(old_event) = buffer.events.pop_front() {
                    buffer.current_bytes -= old_event.data.len();
                    buffer.write_index -= 1;
                }
            }

            // 如果事件数超过容量，移除最旧的
            if buffer.events.len() >= buffer.capacity {
                if let Some(old_event) = buffer.events.pop_front() {
                    buffer.current_bytes -= old_event.data.len();
                }
            }

            let event_index = buffer.write_index;
            let event = TerminalOutputEvent {
                session_id: session_id.to_string(),
                data: decoded_data.clone(),
                is_waiting,
                index: event_index,
                timestamp: chrono::Utc::now().timestamp_millis(),
            };

            buffer.current_bytes += event.data.len();
            buffer.events.push_back(event);
            buffer.write_index += 1;

            (decoded_data, event_index)
        } else {
            // 创建新缓冲区
            let mut buffer = TerminalBuffer::new(self.default_capacity, self.default_byte_capacity);
            buffer.push(session_id.to_string(), data_base64, is_waiting);
            buffers.insert(session_id.to_string(), buffer);

            // 返回解码后的数据和新事件的索引
            let index = 0;
            (decoded_data, index)
        }
    }

    /// 写入输出数据到缓冲区（不返回数据）
    pub async fn write_output(&self, session_id: &str, data_base64: String, is_waiting: bool) {
        let mut buffers = self.buffers.write().await;

        if let Some(buffer) = buffers.get_mut(session_id) {
            buffer.push(session_id.to_string(), data_base64, is_waiting);
        } else {
            // 创建新缓冲区
            let mut buffer = TerminalBuffer::new(self.default_capacity, self.default_byte_capacity);
            buffer.push(session_id.to_string(), data_base64, is_waiting);
            buffers.insert(session_id.to_string(), buffer);
        }
    }

    /// 写入输出数据到缓冲区（使用外部指定的全局索引）
    pub async fn write_output_with_index(&self, session_id: &str, data_base64: String, is_waiting: bool, global_index: usize) {
        let mut buffers = self.buffers.write().await;

        let decoded_data = decode_base64(&data_base64);
        let data_len = decoded_data.len();

        if let Some(buffer) = buffers.get_mut(session_id) {
            // 计算如果添加新数据后的总字节数
            let new_bytes = buffer.current_bytes + data_len;

            // 如果超过字节限制，先清理旧数据
            while new_bytes > buffer.byte_capacity && !buffer.events.is_empty() {
                if let Some(old_event) = buffer.events.pop_front() {
                    buffer.current_bytes -= old_event.data.len();
                    buffer.write_index -= 1;
                }
            }

            // 如果事件数超过容量，移除最旧的
            if buffer.events.len() >= buffer.capacity {
                if let Some(old_event) = buffer.events.pop_front() {
                    buffer.current_bytes -= old_event.data.len();
                }
            }

            // 使用外部传入的全局索引，而不是缓冲区自己的索引
            let event = TerminalOutputEvent {
                session_id: session_id.to_string(),
                data: decoded_data,
                is_waiting,
                index: global_index,
                timestamp: chrono::Utc::now().timestamp_millis(),
            };

            buffer.current_bytes += event.data.len();
            buffer.events.push_back(event);
            // 注意：不更新 write_index，因为使用的是全局索引
        } else {
            // 创建新缓冲区
            let mut buffer = TerminalBuffer::new(self.default_capacity, self.default_byte_capacity);
            buffer.push_with_index(session_id.to_string(), data_base64, is_waiting, global_index);
            buffers.insert(session_id.to_string(), buffer);
        }
    }

    /// 获取完整历史数据
    pub async fn get_history(&self, session_id: &str) -> TerminalHistory {
        let buffers = self.buffers.read().await;

        match buffers.get(session_id) {
            Some(buffer) => TerminalHistory {
                events: buffer.get_all(),
                current_index: buffer.current_index(),
                total_count: buffer.len(),
            },
            None => TerminalHistory {
                events: vec![],
                current_index: 0,
                total_count: 0,
            },
        }
    }

    /// 订阅终端（记录当前索引位置，用于增量获取）
    pub async fn subscribe(&self, session_id: &str) -> usize {
        let mut subscribers = self.subscribers.write().await;

        let buffers = self.buffers.read().await;
        let current_index = buffers
            .get(session_id)
            .map(|b| b.current_index())
            .unwrap_or(0);

        subscribers.insert(session_id.to_string(), current_index);

        tracing::debug!(
            "[TerminalManager] Subscribed to session {}, current_index={}",
            session_id,
            current_index
        );

        current_index
    }

    /// 取消订阅
    pub async fn unsubscribe(&self, session_id: &str) {
        let mut subscribers = self.subscribers.write().await;
        subscribers.remove(session_id);
        tracing::debug!("[TerminalManager] Unsubscribed from session {}", session_id);
    }

    /// 获取订阅者的增量数据
    pub async fn get_incremental(&self, session_id: &str) -> Option<TerminalIncrementalOutput> {
        let subscribers = self.subscribers.read().await;
        let last_index = *subscribers.get(session_id)?;

        let buffers = self.buffers.read().await;
        let buffer = buffers.get(session_id)?;

        let current_index = buffer.current_index();

        // 如果没有新数据，返回 None
        if current_index <= last_index {
            return None;
        }

        let events = buffer.get_incremental(last_index);

        Some(TerminalIncrementalOutput {
            events,
            current_index,
            is_initial: false,
        })
    }

    /// 更新订阅者的索引位置（在增量数据消费后调用）
    pub async fn update_subscriber_index(&self, session_id: &str, index: usize) {
        let mut subscribers = self.subscribers.write().await;
        subscribers.insert(session_id.to_string(), index);
        tracing::debug!(
            "[TerminalManager] Updated subscriber {} index to {}",
            session_id,
            index
        );
    }

    /// 清空会话缓冲区
    pub async fn clear_buffer(&self, session_id: &str) {
        let mut buffers = self.buffers.write().await;
        if let Some(buffer) = buffers.get_mut(session_id) {
            buffer.clear();
        }
        // 同时清除订阅者
        let mut subscribers = self.subscribers.write().await;
        subscribers.remove(session_id);
        tracing::debug!("[TerminalManager] Cleared buffer for session {}", session_id);
    }

    /// 清除所有缓冲区（断开连接时调用）
    pub async fn clear_all(&self) {
        let mut buffers = self.buffers.write().await;
        buffers.clear();
        let mut subscribers = self.subscribers.write().await;
        subscribers.clear();
        tracing::debug!("[TerminalManager] Cleared all buffers");
    }
}

impl Default for TerminalManager {
    fn default() -> Self {
        Self {
            buffers: Arc::new(RwLock::new(std::collections::HashMap::new())),
            default_capacity: 10000,
            default_byte_capacity: 500_000,
            subscribers: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }
}

// ==================== Singleton ====================

static TERMINAL_MANAGER: std::sync::OnceLock<Arc<TerminalManager>> = std::sync::OnceLock::new();

/// 获取终端管理器单例
pub fn get_terminal_manager() -> Arc<TerminalManager> {
    TERMINAL_MANAGER.get_or_init(|| TerminalManager::new()).clone()
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base64_decode() {
        // "Hello" 的 Base64 编码
        let encoded = "SGVsbG8=";
        let decoded = decode_base64(encoded);
        assert_eq!(decoded, "Hello");
    }

    #[test]
    fn test_base64_decode_chinese() {
        // "你好" 的 Base64 编码
        let encoded = "5L2g5aW9";
        let decoded = decode_base64(encoded);
        assert_eq!(decoded, "你好");
    }

    #[tokio::test]
    async fn test_terminal_buffer_write_and_read() {
        let mut buffer = TerminalBuffer::new(100, 1000);

        buffer.push("session1".to_string(), "aGVsbG8=".to_string(), false); // "hello"
        buffer.push("session1".to_string(), "d29ybGQ=".to_string(), false); // "world"

        let events = buffer.get_all();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].data, "hello");
        assert_eq!(events[1].data, "world");
    }

    #[tokio::test]
    async fn test_terminal_buffer_byte_limit() {
        let mut buffer = TerminalBuffer::new(100, 10); // 10 字节限制

        // 5 字节
        buffer.push("s1".to_string(), "YWRtaW4=".to_string(), false); // "admin"
        // 5 字节
        buffer.push("s1".to_string(), "dXNlcg==".to_string(), false); // "user"
        // 5 字节，会触发清理
        buffer.push("s1".to_string(), "Z3Vlc3Q=".to_string(), false); // "guest"

        // 应该只剩下最新的数据
        let events = buffer.get_all();
        assert!(events.len() <= 2);
    }

    #[tokio::test]
    async fn test_terminal_manager() {
        let manager = TerminalManager::new();

        // 写入数据
        manager.write_output("session1", "aGVsbG8=".to_string(), false).await;

        // 获取历史
        let history = manager.get_history("session1").await;
        assert_eq!(history.events.len(), 1);
        assert_eq!(history.events[0].data, "hello");
    }

    #[tokio::test]
    async fn test_incremental_output() {
        let manager = TerminalManager::new();

        // 订阅
        let initial_index = manager.subscribe("session1").await;
        assert_eq!(initial_index, 0);

        // 写入两条数据
        manager.write_output("session1", "YWRtaW4=".to_string(), false).await; // "admin"
        manager.write_output("session1", "dXNlcg==".to_string(), false).await; // "user"

        // 获取增量
        let incremental = manager.get_incremental("session1").await;
        assert!(incremental.is_some());

        let inc = incremental.unwrap();
        assert_eq!(inc.events.len(), 2);
        assert!(!inc.is_initial);
    }
}
