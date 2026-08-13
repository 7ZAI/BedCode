//! File Service Wire Types
//!
//! WS 控制面文件服务消息载荷（移动端 ↔ 桌面端，内网文件传输插件规格阶段 2）。
//! 与桌面端 `bedcode-desktop/src-tauri/src/enums/file_service.rs` 同构（双写，
//! 两端新增/变更变体时必须同步）。
//!
//! 线格式：`{ "action": "announce", "data": { ... } }`（snake_case）。

use serde::{Deserialize, Serialize};

use bedcode_plugin_api_mobile::FileOperation;

/// 文件服务控制面载荷（Message::FileService 的 payload）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", content = "data", rename_all = "snake_case")]
pub enum FileServicePayload {
    /// 公告：移动文件服务端口/token/挂载清单
    ///
    /// 触发时机：首个挂载启动服务、挂载集合变更、认证成功后重连
    /// （重连后桌面侧 peer 状态已被断连清理清空，必须重发）
    Announce {
        /// 文件服务监听端口（0.0.0.0 绑定，IP 由桌面侧取连接 peer_addr）
        port: u16,
        /// Bearer Token（内存态，配对解除即失效）
        token: String,
        /// 对端真实设备名（用户设置名，获取不到时为兜底名），供文件传输展示
        ///
        /// #[serde(default)]：旧端二进制不发此字段，缺省时反序列化仍成功，
        /// 否则整个 Announce 解析失败、对端共享目录判定为不可用
        #[serde(default)]
        device_name: String,
        /// 当前全部挂载清单
        mounts: Vec<MountAnnouncement>,
    },
    /// 撤回：末个挂载摘除、服务停止
    ///
    /// 连接已断开时不发（桌面侧断连路径已自行清理 peer 记录）
    Withdraw {},
    /// 询问对端当前文件服务状态（主动探测，数据载荷为空）
    ///
    /// 触发：插件主动发起（filesrv_query_peer host function），用于
    /// 对端状态事件遗漏/未同步时主动恢复（如先挂载后连接、广播丢失）。
    /// 响应：有挂载且服务运行 → Announce；否则 → Withdraw
    Query {},
    /// 传输批应答推送（v2）：接收端批准/拒绝/超时 → 发送端
    ///
    /// 发送端宿主收到后发布 `filesrv:transfer_approval`（双通道），
    /// 发送方插件据此把批内 waiting-approval 任务转为 queued / rejected。
    /// 与桌面端 `enums/file_service.rs` 同名变体保持同构（逐字一致）
    TransferApproval {
        /// 批 ID
        batch_id: String,
        /// "approved" | "rejected"
        decision: String,
        /// "" | "user-rejected" | "timeout"
        reason: String,
    },
}

/// 单个挂载的公告信息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MountAnnouncement {
    /// 挂载所属插件 ID（URL 第一段：/{plugin_id}/{mount_path}/...）
    pub plugin_id: String,
    /// 挂载点名称（URL 第二段）
    pub mount_path: String,
    /// 该挂载支持的操作集合
    pub operations: Vec<FileOperation>,
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_announce_wire_format() {
        let payload = FileServicePayload::Announce {
            port: 41234,
            token: "tok".to_string(),
            device_name: "my-phone".to_string(),
            mounts: vec![MountAnnouncement {
                plugin_id: "com.bedcode.file-transfer".to_string(),
                mount_path: "files".to_string(),
                operations: vec![FileOperation::List, FileOperation::Download],
            }],
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"action\":\"announce\""));
        assert!(json.contains("\"device_name\":\"my-phone\""));
        let back: FileServicePayload = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, FileServicePayload::Announce { port: 41234, .. }));
    }

    #[test]
    fn test_withdraw_wire_format() {
        let payload = FileServicePayload::Withdraw {};
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"action\":\"withdraw\""));
        assert!(matches!(
            serde_json::from_str::<FileServicePayload>(&json).unwrap(),
            FileServicePayload::Withdraw {}
        ));
    }

    #[test]
    fn test_query_wire_format() {
        let payload = FileServicePayload::Query {};
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"action\":\"query\""));
        assert!(matches!(
            serde_json::from_str::<FileServicePayload>(&json).unwrap(),
            FileServicePayload::Query {}
        ));
    }

    #[test]
    fn test_transfer_approval_wire_format() {
        // v2：跨端推送逐字一致（snake_case action + data 载荷字段）
        let payload = FileServicePayload::TransferApproval {
            batch_id: "b1".to_string(),
            decision: "rejected".to_string(),
            reason: "user-rejected".to_string(),
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"action\":\"transfer_approval\""));
        assert!(json.contains("\"batch_id\":\"b1\""));
        assert!(json.contains("\"decision\":\"rejected\""));
        assert!(json.contains("\"reason\":\"user-rejected\""));
        let back: FileServicePayload = serde_json::from_str(&json).unwrap();
        assert!(matches!(
            back,
            FileServicePayload::TransferApproval {
                batch_id,
                decision,
                reason,
            } if batch_id == "b1" && decision == "rejected" && reason == "user-rejected"
        ));
    }
}
