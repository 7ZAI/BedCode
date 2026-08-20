//! Sync Types
//!
//! 数据同步相关类型定义

use serde::{Deserialize, Serialize};

use super::plugin::PluginQuestion;
use super::summary::{SessionConfigSummary, SessionSummary};

/// `expect_response` 默认值（v2.1：intent 必须要求回执，ADR 0021 可靠性要求）
fn default_true() -> bool {
    true
}

/// 同步载荷 - 支持多种数据类型的增量同步
///
/// 用于 WebSocket 消息，向客户端推送增量数据变更
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum SyncPayload {
    // === 会话状态同步 ===
    /// 会话创建
    SessionCreated {
        session: SessionSummary,
        /// 触发操作的设备名称（桌面本地操作为空字符串）
        source_device: String,
    },
    /// 会话状态变化
    SessionStatusChanged {
        session_id: String,
        old_status: String,
        new_status: String,
        session_name: String,
    },
    /// 会话停止
    SessionStopped { session_id: String, session_name: String },
    /// 会话删除
    SessionRemoved { session_id: String, session_name: String },

    // === 会话配置同步 ===
    /// 配置创建
    ConfigCreated {
        config: SessionConfigSummary,
        /// 触发操作的设备名称（桌面本地操作为空字符串）
        source_device: String,
    },
    /// 配置更新
    ConfigUpdated {
        config: SessionConfigSummary,
        /// 触发操作的设备名称（桌面本地操作为空字符串）
        source_device: String,
    },
    /// 配置删除
    ConfigRemoved { config_id: String, config_name: String },

    // === 任务状态同步 ===
    /// Plugin 任务状态变更
    TaskStatusChanged {
        session_id: String,
        task_status: String,
        task_reason: Option<String>,
        task_questions: Option<Vec<PluginQuestion>>,
    },

    // === 会话模式同步 ===
    /// 会话自动授权模式变更
    SessionModeChanged { session_id: String, auto_approve: bool },

    // === 任务队列同步 ===
    /// 会话任务队列变更
    TaskQueueChanged {
        session_id: String,
        /// 变更后的待执行任务数量
        queue_count: i64,
        /// 触发动作：add / remove / clear / dequeue / done / update / reorder / cancel
        action: String,
        /// 关联的队列项 ID（done 广播携带）
        #[serde(default, skip_serializing_if = "Option::is_none")]
        task_id: Option<String>,
        /// 队列项状态（done 广播为 "done"）
        #[serde(default, skip_serializing_if = "Option::is_none")]
        status: Option<String>,
    },

    // === 定时自动任务同步（v6，ADR 0003） ===
    /// 定时自动任务变更（与移动端 enums/sync.rs 同名变体保持同构）
    TaskScheduledChanged {
        job_id: String,
        /// 变更后的状态：pending / creating / executed / failed / missed
        status: String,
        /// 触发动作：create / delete / trigger / missed / failed
        action: String,
    },

    // === 文件服务同步（桌面 → 移动，内网文件传输插件规格阶段 2） ===
    /// 桌面侧插件挂载点可用性变更（mount/unmount/update_roots 后由宿主自动发出）
    ///
    /// 与移动端 `enums/sync.rs` 同名变体保持同构
    FileServiceChanged {
        plugin_id: String,
        mount_path: String,
        /// true = 挂载可用（mount/update_roots），false = 已摘除（unmount）
        available: bool,
        /// 挂载支持的操作集合（unmount 时为空）
        operations: Vec<bedcode_plugin_api::FileOperation>,
    },

    // === 传输批应答同步（v2，桌面 → 移动） ===
    /// 桌面端（接收端宿主）对传输批的应答推送：批准/拒绝/超时 → 移动端发送方
    ///
    /// 与移动端 `enums/sync.rs` 同名变体保持同构；移动端收到后
    /// 经注册表双通道发布 `filesrv:transfer_approval` 供发送方插件订阅
    TransferApproval {
        /// 批 ID
        batch_id: String,
        /// "approved" | "rejected"
        decision: String,
        /// "" | "user-rejected" | "timeout"
        reason: String,
    },

    // === 文件传输意图（v2.1：服务器归零，桌面经 WS 指挥手机执行） ===
    /// 桌面发起文件传输意图（Desk→Mob，服务器归零后桌面不再直连手机，
    /// 改由手机按语意动作自行执行字节流）
    ///
    /// 与移动端 `enums/sync.rs` 同名变体保持同构（两端逐字双写）；
    /// JSON action（snake_case variant）= `file_transfer_intent`
    FileTransferIntent {
        /// 意图 ID（uuid，ACK/进度/取消全程携带）
        intent_id: String,
        /// "pull"（桌面下载手机文件→手机把本地/SAF 文件 POST 给桌面）
        /// "push"（桌面推文件给手机→手机 GET 桌面挂载文件并落盘）
        direction: String,
        /// 业务语义（手机侧通知与任务记录）：
        /// "download"（桌面下载手机文件）| "upload"（桌面推文件给手机）
        semantics: String,
        /// 批 ID（与 v2 批审批联动；pull 时桌面已自批准，手机 POST 携带免 gating 403）
        #[serde(default)]
        batch_id: Option<String>,
        /// 挂载相对路径（push：桌面挂载内路径；pull：手机本地/SAF 路径）
        relative_path: String,
        /// 字节大小（通知展示 + 断点预期）
        size: u64,
        /// 对端设备名（通知展示）
        #[serde(default)]
        device_name: String,
        /// 期望回执（ADR 0021 可靠性要求：intent 必须 expect_response ACK）
        #[serde(default = "default_true")]
        expect_response: bool,
    },
    /// 桌面取消文件传输意图（Desk→Mob）：手机中止对应 HTTP 会话；
    /// 已写字节保留，重试 = 桌面重发 intent 从断点续传
    ///
    /// JSON action（snake_case variant）= `file_transfer_cancel`
    FileTransferCancel {
        /// 意图 ID
        intent_id: String,
    },
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_transfer_intent_wire_format() {
        // 全字段序列化：snake_case 变体名即 action（`file_transfer_intent`），
        // 字段按枚举级 rename_all=snake_case 序列化
        let payload = SyncPayload::FileTransferIntent {
            intent_id: "9f1c".into(),
            direction: "push".into(),
            semantics: "upload".into(),
            batch_id: Some("b17".into()),
            relative_path: "movies/a.mp4".into(),
            size: 1234567890,
            device_name: "MyDesktop".into(),
            expect_response: true,
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"type\":\"file_transfer_intent\""));
        assert!(json.contains("\"intent_id\":\"9f1c\""));
        assert!(json.contains("\"direction\":\"push\""));
        assert!(json.contains("\"semantics\":\"upload\""));
        assert!(json.contains("\"batch_id\":\"b17\""));
        assert!(json.contains("\"relative_path\":\"movies/a.mp4\""));
        assert!(json.contains("\"size\":1234567890"));
        assert!(json.contains("\"device_name\":\"MyDesktop\""));
        assert!(json.contains("\"expect_response\":true"));
        match serde_json::from_str::<SyncPayload>(&json).unwrap() {
            SyncPayload::FileTransferIntent {
                intent_id,
                direction,
                semantics,
                batch_id,
                relative_path,
                size,
                device_name,
                expect_response,
            } => {
                assert_eq!(intent_id, "9f1c");
                assert_eq!(direction, "push");
                assert_eq!(semantics, "upload");
                assert_eq!(batch_id.as_deref(), Some("b17"));
                assert_eq!(relative_path, "movies/a.mp4");
                assert_eq!(size, 1234567890);
                assert_eq!(device_name, "MyDesktop");
                assert!(expect_response);
            }
            _ => panic!("expected FileTransferIntent"),
        }
    }

    #[test]
    fn test_file_transfer_intent_defaults_accepted() {
        // 缺省字段（batch_id/device_name/expect_response）反序列化成功：
        // 旧端/精简载荷兼容；expect_response 缺省 = true（可靠性默认要求回执）
        let json = r#"{"type":"file_transfer_intent","data":{"intent_id":"i1","direction":"pull","semantics":"download","relative_path":"a.mp4","size":10}}"#;
        match serde_json::from_str::<SyncPayload>(json).unwrap() {
            SyncPayload::FileTransferIntent {
                intent_id,
                direction,
                batch_id,
                relative_path,
                device_name,
                expect_response,
                ..
            } => {
                assert_eq!(intent_id, "i1");
                assert_eq!(direction, "pull");
                assert_eq!(batch_id, None);
                assert_eq!(relative_path, "a.mp4");
                assert_eq!(device_name, "");
                assert!(expect_response, "expect_response 缺省必须为 true");
            }
            _ => panic!("expected FileTransferIntent"),
        }
    }

    #[test]
    fn test_file_transfer_cancel_wire_format() {
        let payload = SyncPayload::FileTransferCancel {
            intent_id: "9f1c".into(),
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"type\":\"file_transfer_cancel\""));
        assert!(json.contains("\"intent_id\":\"9f1c\""));
        match serde_json::from_str::<SyncPayload>(&json).unwrap() {
            SyncPayload::FileTransferCancel { intent_id } => assert_eq!(intent_id, "9f1c"),
            _ => panic!("expected FileTransferCancel"),
        }
    }
}
