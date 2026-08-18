//! 每会话终端路由（/ws/terminal/session/{id}）JSON 控制帧协议
//!
//! spec §5.3：控制帧为简化协议——无 message_id/expect_response 请求-响应
//! 机制，连接级状态机替代（auth → auth_ok → subscribe → 快照流）。
//! 与旧路由的 `Message` 枚举（带 message_id 的完整协议）互不相干：
//! 旧路由结构不变（spec §7 兼容策略），新路由专用此类型。

use serde::{Deserialize, Serialize};

use crate::enums::special_key::KeyCombo;

/// 客户端 → 服务端控制帧
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientFrame {
    /// 首消息 JWT 认证（spec §4.3 规则与旧路由一致）
    Auth {
        token: String,
    },
    /// 订阅绑定会话（无参：连接创建即绑定，快照协议全量重播）
    Subscribe,
    /// PTY 输入（data 为 Base64；special_key 为按键组合字符串）
    Input {
        data: String,
        #[serde(default)]
        special_key: Option<KeyCombo>,
    },
}

/// 服务端 → 客户端控制帧
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerFrame {
    /// JWT 认证成功（此后客户端可发 subscribe）
    AuthOk,
    /// 订阅已建立（快照元数据；历史帧在其后按序到达）
    SubscribeOk {
        /// 订阅时刻队列最新序号（历史边界）
        snapshot_seq: u64,
        /// 队列中最早存续事件序号（环形淘汰后推进）
        min_seq: u64,
        /// 历史事件数量
        history_count: usize,
    },
    /// 历史段结束标记（此后为实时帧；空历史也必发）
    HistoryEnd {
        snapshot_seq: u64,
    },
    /// 会话停止通知（服务端主动推送，此后连接不再有输出）
    SessionStopped {
        session_id: String,
    },
    /// 错误（code 语义与旧路由 error 消息一致）
    Error {
        code: String,
        message: String,
    },
}

impl ServerFrame {
    /// 序列化为 JSON 文本帧
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| {
            r#"{"type":"error","code":"SERIALIZE_ERROR","message":"failed to serialize frame"}"#
                .to_string()
        })
    }
}

/// 解析客户端控制帧（失败返回错误描述）
pub fn parse_client_frame(text: &str) -> Result<ClientFrame, String> {
    serde_json::from_str::<ClientFrame>(text)
        .map_err(|e| format!("invalid control frame: {e}"))
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_auth_frame() {
        let frame = parse_client_frame(r#"{"type":"auth","token":"jwt-token"}"#).unwrap();
        match frame {
            ClientFrame::Auth { token } => assert_eq!(token, "jwt-token"),
            _ => panic!("expected auth frame"),
        }
    }

    #[test]
    fn parse_subscribe_frame() {
        let frame = parse_client_frame(r#"{"type":"subscribe"}"#).unwrap();
        assert!(matches!(frame, ClientFrame::Subscribe));
    }

    #[test]
    fn parse_input_frame_plain() {
        let frame = parse_client_frame(r#"{"type":"input","data":"aGVsbG8="}"#).unwrap();
        match frame {
            ClientFrame::Input { data, special_key } => {
                assert_eq!(data, "aGVsbG8=");
                assert!(special_key.is_none());
            }
            _ => panic!("expected input frame"),
        }
    }

    #[test]
    fn parse_input_frame_with_special_key() {
        let frame =
            parse_client_frame(r#"{"type":"input","data":"","special_key":"ctrl_c"}"#).unwrap();
        match frame {
            ClientFrame::Input { data, special_key } => {
                assert_eq!(data, "");
                assert!(special_key.is_some());
            }
            _ => panic!("expected input frame"),
        }
    }

    #[test]
    fn parse_unknown_type_rejected() {
        let err = parse_client_frame(r#"{"type":"unknown"}"#).unwrap_err();
        assert!(err.contains("unknown variant"), "got: {err}");
    }

    #[test]
    fn parse_malformed_rejected() {
        assert!(parse_client_frame("not json").is_err());
    }

    #[test]
    fn serialize_subscribe_ok() {
        let json = ServerFrame::SubscribeOk {
            snapshot_seq: 42,
            min_seq: 0,
            history_count: 42,
        }
        .to_json();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["type"], "subscribe_ok");
        assert_eq!(v["snapshot_seq"], 42);
        assert_eq!(v["min_seq"], 0);
        assert_eq!(v["history_count"], 42);
    }

    #[test]
    fn serialize_history_end() {
        let json = ServerFrame::HistoryEnd { snapshot_seq: 7 }.to_json();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["type"], "history_end");
        assert_eq!(v["snapshot_seq"], 7);
    }

    #[test]
    fn serialize_auth_ok_and_error() {
        let v: serde_json::Value =
            serde_json::from_str(&ServerFrame::AuthOk.to_json()).unwrap();
        assert_eq!(v["type"], "auth_ok");

        let v: serde_json::Value =
            serde_json::from_str(&ServerFrame::Error {
                code: "SESSION_NOT_FOUND".into(),
                message: "Session s-1 not found".into(),
            }
            .to_json())
            .unwrap();
        assert_eq!(v["type"], "error");
        assert_eq!(v["code"], "SESSION_NOT_FOUND");
    }

    #[test]
    fn serialize_session_stopped() {
        let json = ServerFrame::SessionStopped {
            session_id: "s-9".into(),
        }
        .to_json();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["type"], "session_stopped");
        assert_eq!(v["session_id"], "s-9");
    }
}
