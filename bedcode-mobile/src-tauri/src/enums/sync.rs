//! Sync Types
//!
//! 数据同步相关类型定义

use serde::{Deserialize, Serialize};

use super::plugin::PluginQuestion;
use super::sumary::{SessionConfigSummary, SessionSummary};

/// 文件传输意图载荷（强类型：wire 内联字段的项目类型，供 responder/通知跨模块
/// 传参；serde 往返与 `SyncPayload::FileTransferIntent` 变体字段逐字一致）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileTransferIntent {
    /// 意图 ID（uuid；ACK/进度/取消全程携带）
    pub intent_id: String,
    /// "pull"（桌面下载手机文件）| "push"（桌面推文件给手机）
    pub direction: String,
    /// 业务语义："download"（桌面下载手机文件）| "upload"（桌面推文件给手机）
    pub semantics: String,
    /// 批 ID（与 v2 批审批联动，pull 且 ask 时桌面已自批准随 intent 下发）
    #[serde(default)]
    pub batch_id: Option<String>,
    /// 挂载相对路径（push：桌面挂载内路径；pull：手机本地/SAF 路径）
    pub relative_path: String,
    /// 字节大小（通知展示 + 断点预期）
    pub size: u64,
    /// 对端设备名（通知展示）
    #[serde(default)]
    pub device_name: String,
    /// 期望回执（ADR 0021 可靠性要求：intent 必须 expect_response ACK）
    #[serde(default = "default_true")]
    pub expect_response: bool,
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
        /// 关联的队列项 ID（done 广播携带，供预设任务完成匹配）
        #[serde(default)]
        task_id: Option<String>,
        /// 队列项状态（done 广播为 "done"）
        #[serde(default)]
        status: Option<String>,
    },

    // === 定时自动任务同步（v6，ADR 0003） ===
    /// 定时自动任务变更（与桌面端 enums/sync.rs 同名变体保持同构）
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
    /// 与桌面端 `enums/sync.rs` 同名变体保持同构
    FileServiceChanged {
        plugin_id: String,
        mount_path: String,
        /// true = 挂载可用（mount/update_roots），false = 已摘除（unmount）
        available: bool,
        /// 挂载支持的操作集合（unmount 时为空）
        operations: Vec<bedcode_plugin_api_mobile::FileOperation>,
    },

    // === 传输批应答（v2，桌面 → 移动，发送端=移动） ===
    /// 传输批应答推送（接收端批准/拒绝/超时 → 发送端）
    ///
    /// 移动端作为发送方时收到（对端桌面接收方经 WS 推送）；宿主发布
    /// `filesrv:transfer_approval` 双通道事件，发送方插件据此调度批内任务。
    /// 与桌面端 `enums/sync.rs` 同名变体保持同构（逐字一致）
    TransferApproval {
        /// 批 ID
        batch_id: String,
        /// "approved" | "rejected"
        decision: String,
        /// "" | "user-rejected" | "timeout"
        reason: String,
    },

    // === 文件传输意图（v2.1 服务器归零，桌面 → 移动） ===
    /// 桌面发起文件传输意图（服务器归零后桌面经 WS 指挥手机执行）
    ///
    /// 移动端 responder 据此按方向执行对应语意动作（`file_transfer_intent`）：
    /// - pull：手机用 UploadClient 把本地/SAF 文件 POST 给桌面（免审批，仅信息性通知）
    /// - push：手机用 DownloadClient GET 桌面 + Range 落 SAF（ask 策略须先经用户确认）
    /// 与桌面端 `enums/sync.rs` 同名变体保持同构（逐字一致）
    FileTransferIntent {
        /// 意图 ID（uuid；ACK/进度/取消全程携带）
        intent_id: String,
        /// "pull"（桌面下载手机文件）| "push"（桌面推文件给手机）
        direction: String,
        /// 业务语义："download"（桌面下载手机文件）| "upload"（桌面推文件给手机）
        semantics: String,
        /// 批 ID（与 v2 批审批联动，pull 且 ask 时桌面已自批准随 intent 下发）
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

    /// 桌面取消文件传输意图（`file_transfer_cancel`）
    ///
    /// 移动端 responder 中止对应 HTTP 会话；已写字节保留，重试 = 桌面重发 intent
    FileTransferCancel {
        /// 意图 ID
        intent_id: String,
    },
}

/// `expect_response` 缺省值（ADR 0021：intent 默认必须回执）
fn default_true() -> bool {
    true
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_transfer_intent_wire_format() {
        // v2.1：跨端推送逐字一致（snake_case action + data 载荷字段）
        let payload = SyncPayload::FileTransferIntent {
            intent_id: "9f1c-1234".to_string(),
            direction: "push".to_string(),
            semantics: "upload".to_string(),
            batch_id: Some("b17".to_string()),
            relative_path: "movies/a.mp4".to_string(),
            size: 1234567890,
            device_name: "MyDesktop".to_string(),
            expect_response: true,
        };
        let json = serde_json::to_string(&payload).unwrap();
        // SyncPayload 的 serde tag = "type"（adjacently tagged {type, data}），
        // 与 FileServicePayload 的 "action" 区分；两端同构（对齐既有变体命名）
        assert!(json.contains("\"type\":\"file_transfer_intent\""));
        assert!(json.contains("\"intent_id\":\"9f1c-1234\""));
        assert!(json.contains("\"direction\":\"push\""));
        assert!(json.contains("\"semantics\":\"upload\""));
        assert!(json.contains("\"batch_id\":\"b17\""));
        assert!(json.contains("\"relative_path\":\"movies/a.mp4\""));
        assert!(json.contains("\"size\":1234567890"));
        assert!(json.contains("\"device_name\":\"MyDesktop\""));
        assert!(json.contains("\"expect_response\":true"));
        let back: SyncPayload = serde_json::from_str(&json).unwrap();
        match back {
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
                assert_eq!(intent_id, "9f1c-1234");
                assert_eq!(direction, "push");
                assert_eq!(semantics, "upload");
                assert_eq!(batch_id.as_deref(), Some("b17"));
                assert_eq!(relative_path, "movies/a.mp4");
                assert_eq!(size, 1234567890);
                assert_eq!(device_name, "MyDesktop");
                assert!(expect_response);
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn test_file_transfer_intent_optional_fields_default() {
        // 旧端二进制不发 batch_id/device_name/expect_response 时缺省值
        let payload = SyncPayload::FileTransferIntent {
            intent_id: "i1".to_string(),
            direction: "pull".to_string(),
            semantics: "download".to_string(),
            batch_id: None,
            relative_path: "local/a.txt".to_string(),
            size: 42,
            device_name: String::new(),
            expect_response: true,
        };
        // 序列化时 Option::None / 空串照常输出；反序列化缺省 expect_response=true
        let json = serde_json::to_string(&payload).unwrap();
        let back: SyncPayload = serde_json::from_str(&json).unwrap();
        match back {
            SyncPayload::FileTransferIntent {
                batch_id,
                device_name,
                expect_response,
                ..
            } => {
                assert!(batch_id.is_none());
                assert_eq!(device_name, "");
                assert!(expect_response);
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn test_file_transfer_cancel_wire_format() {
        let payload = SyncPayload::FileTransferCancel {
            intent_id: "9f1c-1234".to_string(),
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"type\":\"file_transfer_cancel\""));
        assert!(json.contains("\"intent_id\":\"9f1c-1234\""));
        assert!(matches!(
            serde_json::from_str::<SyncPayload>(&json).unwrap(),
            SyncPayload::FileTransferCancel { intent_id } if intent_id == "9f1c-1234"
        ));
    }
}
