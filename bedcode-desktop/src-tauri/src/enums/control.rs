//! Control Types
//!
//! 会话控制、会话配置和终端消息类型定义

use serde::{Deserialize, Serialize};

use super::special_key::KeyCombo;
use super::summary::{QuickActionSummary, SessionConfigSummary, SessionSummary};

// ==================== Session Control ====================

/// 会话控制载荷
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionControlPayload {
    /// 控制动作
    pub action: SessionControlAction,
}

/// 会话控制动作
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionControlAction {
    /// 列出会话
    ListSessions,
    /// 会话列表响应
    SessionList { sessions: Vec<SessionSummary> },
    /// 启动会话
    StartSession { config_id: String },
    /// 停止会话
    StopSession { session_id: String },
    /// 删除会话
    RemoveSession { session_id: String },
    /// 调整终端大小（force：覆盖确认后置位，见正统渲染端裁决）
    ResizeSession {
        session_id: String,
        cols: u16,
        rows: u16,
        #[serde(default)]
        force: bool,
    },
    /// 会话变更通知 (created/stopped/removed)
    SessionChanged {
        change_type: String,
        session: SessionSummary,
    },
}

// ==================== Session Config ====================

/// 会话配置载荷
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfigPayload {
    /// 配置动作
    pub action: SessionConfigAction,
}

/// 会话配置动作
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionConfigAction {
    /// 列出会话配置
    ListSessionConfigs,
    /// 会话配置列表响应
    SessionConfigList { configs: Vec<SessionConfigSummary> },
    /// 列出快捷指令
    ListQuickActions,
    /// 快捷指令列表响应
    QuickActionList { actions: Vec<QuickActionSummary> },
}

// ==================== Terminal ====================

/// 终端载荷
///
/// 统一的终端消息类型，包含输出、输入、订阅/取消订阅等操作
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalPayload {
    /// 终端动作
    pub action: TerminalAction,
}

/// 终端动作
///
/// 终端相关的所有操作类型：
/// - Input: 客户端输入发送 (客户端 → 服务端)
/// - Subscribe: 订阅会话输出 (客户端 → 服务端)
/// - SubscribeResponse: 订阅响应 (服务端 → 客户端)
/// - Unsubscribe: 取消订阅 (客户端 → 服务端)
/// - UnsubscribeResponse: 取消订阅响应 (服务端 → 客户端)
///
/// PTY 输出不再经 JSON 文本帧（v2 base64 Output action 已随 JoinSession 链
/// 删除），统一走 TB v3 二进制帧（server/ws/terminal_ws/forward.rs）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TerminalAction {
    /// 输入消息 (客户端 → 服务端)
    /// 客户端发送输入到 PTY
    Input {
        /// 输入数据
        data: String,
        /// 特殊键
        #[serde(skip_serializing_if = "Option::is_none")]
        special_key: Option<KeyCombo>,
    },

    /// 订阅输出 (客户端 → 服务端)
    /// 客户端订阅会话输出，实现增量同步
    Subscribe,

    /// 订阅响应 (服务端 → 客户端)
    /// TB v3：字段名保留旧协议（增量演进），值承载字节语义——
    /// min_seq = min_offset（最早存续字节）、max_seq = snapshot_offset（订阅时刻
    /// 累计字节）、history_count = history_bytes（驻留历史总字节）
    SubscribeResponse {
        /// 环形保留区间最小字节偏移（更早头部已被淘汰）
        min_seq: u64,
        /// 订阅时刻累计字节数（历史边界）
        max_seq: u64,
        /// 驻留历史总字节数
        history_count: usize,
    },

    /// 取消订阅 (客户端 → 服务端)
    Unsubscribe,

    /// 取消订阅响应 (服务端 → 客户端)
    UnsubscribeResponse,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_summary() -> SessionSummary {
        SessionSummary {
            id: "sess-1".to_string(),
            name: "dev".to_string(),
            status: "running".to_string(),
            created_at: "2025-01-01T00:00:00Z".to_string(),
            started_at: Some("2025-01-01T00:00:00Z".to_string()),
            session_type: Some("pty".to_string()),
            config_id: Some("cfg-1".to_string()),
            task_status: None,
            task_reason: None,
        }
    }

    /// SessionControlAction 全变体 serde 往返（票据 23：跨端协议表面零覆盖）
    #[test]
    fn session_control_action_roundtrip_all_variants() {
        let cases: Vec<(SessionControlAction, &str)> = vec![
            (SessionControlAction::ListSessions, "list_sessions"),
            (
                SessionControlAction::SessionList {
                    sessions: vec![sample_summary()],
                },
                "session_list",
            ),
            (
                SessionControlAction::StartSession {
                    config_id: "c1".to_string(),
                },
                "start_session",
            ),
            (
                SessionControlAction::StopSession {
                    session_id: "s1".to_string(),
                },
                "stop_session",
            ),
            (
                SessionControlAction::RemoveSession {
                    session_id: "s1".to_string(),
                },
                "remove_session",
            ),
            (
                SessionControlAction::ResizeSession {
                    session_id: "s1".to_string(),
                    cols: 120,
                    rows: 40,
                    force: false,
                },
                "resize_session",
            ),
            (
                SessionControlAction::SessionChanged {
                    change_type: "created".to_string(),
                    session: sample_summary(),
                },
                "session_changed",
            ),
        ];
        for (action, expected_type) in cases {
            let json = serde_json::to_string(&action).unwrap();
            // type 标签锁（移动端 TS 依赖）
            assert!(
                json.contains(&format!("\"type\":\"{expected_type}\"")),
                "标签缺失: {json}"
            );
            let back: SessionControlAction = serde_json::from_str(&json).unwrap();
            let json2 = serde_json::to_string(&back).unwrap();
            assert_eq!(json, json2, "变体往返不一致: {json}");
        }
    }

    /// 未知变体拒绝（协议错位不得静默吞掉）
    #[test]
    fn session_control_action_unknown_variant_rejected() {
        assert!(serde_json::from_str::<SessionControlAction>(r#"{"type":"bogus_action"}"#).is_err());
    }

    /// SessionConfigAction 往返 + 标签锁
    #[test]
    fn session_config_action_roundtrip() {
        let cases: Vec<(SessionConfigAction, &str)> = vec![
            (SessionConfigAction::ListSessionConfigs, "list_session_configs"),
            (SessionConfigAction::ListQuickActions, "list_quick_actions"),
        ];
        for (action, expected_type) in cases {
            let json = serde_json::to_string(&action).unwrap();
            assert!(
                json.contains(&format!("\"type\":\"{expected_type}\"")),
                "标签缺失: {json}"
            );
            let back: SessionConfigAction = serde_json::from_str(&json).unwrap();
            assert_eq!(serde_json::to_string(&back).unwrap(), json, "往返不一致: {json}");
        }
    }

    /// TerminalAction 全变体往返（含特殊键与订阅响应字段）
    #[test]
    fn terminal_action_roundtrip_all_variants() {
        let cases = vec![
            TerminalAction::Input {
                data: "ls".to_string(),
                special_key: None,
            },
            TerminalAction::Input {
                data: "".to_string(),
                special_key: Some(KeyCombo::parse("enter").unwrap()),
            },
            TerminalAction::Subscribe,
            TerminalAction::SubscribeResponse {
                min_seq: 10,
                max_seq: 100,
                history_count: 90,
            },
            TerminalAction::Unsubscribe,
            TerminalAction::UnsubscribeResponse,
        ];
        for action in cases {
            let json = serde_json::to_string(&action).unwrap();
            let back: TerminalAction = serde_json::from_str(&json).unwrap();
            assert_eq!(serde_json::to_string(&back).unwrap(), json, "变体往返不一致: {json}");
        }
    }

    /// TerminalAction 标签锁（TB v3 二进制帧之外的 JSON 控制帧契约）
    #[test]
    fn terminal_action_wire_labels_locked() {
        assert_eq!(
            serde_json::to_string(&TerminalAction::Subscribe).unwrap(),
            r#"{"type":"subscribe"}"#
        );
        assert_eq!(
            serde_json::to_string(&TerminalAction::Unsubscribe).unwrap(),
            r#"{"type":"unsubscribe"}"#
        );
    }
}
