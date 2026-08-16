//! PTY Output Event
//!
//! PTY 输出事件数据结构

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// PTY 输出事件
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PtyOutputEvent {
    pub session_id: String,
    /// Base64 编码的输出数据（用于 JSON 序列化）
    pub data: String,
    pub timestamp: DateTime<Utc>,
    /// 是否等待用户输入（用于插件会话）
    #[serde(default)]
    pub is_waiting: bool,
    /// 全局递增索引，用于去重（桌面端 + 移动端统一计数）
    #[serde(default)]
    pub index: usize,
}

impl PtyOutputEvent {
    /// 从原始字节数据创建事件
    pub fn from_bytes(session_id: String, bytes: &[u8], timestamp: DateTime<Utc>, is_waiting: bool, index: usize) -> Self {
        Self {
            session_id,
            data: base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                bytes,
            ),
            timestamp,
            is_waiting,
            index,
        }
    }

    /// 解码为原始字节
    pub fn decode_data(&self) -> Option<Vec<u8>> {
        base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            &self.data,
        ).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn from_bytes_base64_encodes_known_value() {
        let ts = Utc::now();
        let event = PtyOutputEvent::from_bytes("s1".to_string(), b"hello", ts, true, 42);
        // base64("hello") 的已知字面量，独立于实现计算
        assert_eq!(event.data, "aGVsbG8=");
        assert_eq!(event.session_id, "s1");
        assert_eq!(event.is_waiting, true);
        assert_eq!(event.index, 42);
        assert_eq!(event.timestamp, ts);
    }

    #[test]
    fn decode_data_round_trips_original_bytes() {
        // 包含非 UTF-8 与边界字节，验证二进制安全
        let bytes: Vec<u8> = vec![0u8, 1, 2, 255, 254, 128, 65, 0];
        let event = PtyOutputEvent::from_bytes("s1".to_string(), &bytes, Utc::now(), false, 0);
        assert_eq!(event.decode_data(), Some(bytes));
    }

    #[test]
    fn decode_data_returns_none_for_invalid_base64() {
        let event = PtyOutputEvent {
            session_id: "s1".to_string(),
            data: "!!!not-base64!!!".to_string(),
            timestamp: Utc::now(),
            is_waiting: false,
            index: 0,
        };
        assert!(event.decode_data().is_none());
    }
}