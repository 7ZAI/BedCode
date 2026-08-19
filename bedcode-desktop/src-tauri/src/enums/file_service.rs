//! File Service Wire Types
//!
//! WS 控制面文件服务消息载荷（桌面端 ↔ 移动端，内网文件传输插件规格阶段 2）。
//! 与移动端 `bedcode-mobile/src-tauri/src/enums/file_service.rs` 同构（双写，
//! 两端新增/变更变体时必须同步）。
//!
//! 线格式：`{ "action": "announce", "data": { ... } }`（snake_case）。
//!
//! 方向：移动端文件服务 → 桌面端（Announce/Withdraw）。桌面端自身文件服务
//! 挂在现有 actix server（JWT 鉴权），挂载可用性变更经 SyncPayload::FileServiceChanged
//! 推送，不使用此载荷。

use serde::{Deserialize, Serialize};

use bedcode_plugin_api::FileOperation;

/// 文件服务控制面载荷（Message::FileService 的 payload）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", content = "data", rename_all = "snake_case")]
pub enum FileServicePayload {
    /// 公告：移动文件服务端口/token/挂载清单
    ///
    /// 触发时机：首个挂载启动服务、挂载集合变更、认证成功后重连
    /// （重连后桌面侧 peer 状态已被断连清理清空，移动端必须重发）
    Announce {
        /// 文件服务监听端口（0.0.0.0 绑定，IP 由桌面侧取连接 peer_addr）
        port: u16,
        /// Bearer Token（移动端内存态，配对解除即失效）
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
    /// 连接已断开时移动端不发（桌面侧断连路径已自行清理 peer 记录）
    Withdraw {},
    /// 询问对端当前文件服务状态（主动探测，数据载荷为空）
    ///
    /// 触发：插件主动发起（filesrv_query_peer host function），用于
    /// 对端状态事件遗漏/未同步时主动恢复（如先挂载后连接、广播丢失）。
    /// 响应：有挂载且服务运行 → Announce；否则 → Withdraw
    Query {},
    /// 传输批应答推送（v2）：接收端批准/拒绝/超时 → 发送端
    ///
    /// 方向：移动端（接收端宿主）→ 桌面端（发送端宿主），桌面端
    /// `terminal_ws::handle_file_service` 收到后经注册表双通道发布
    /// `filesrv:transfer_approval` 供发送方插件订阅
    TransferApproval {
        /// 批 ID
        batch_id: String,
        /// "approved" | "rejected"
        decision: String,
        /// "" | "user-rejected" | "timeout"
        reason: String,
    },
    /// 意图回执（v2.1）：手机已接受/拒绝并回传会话（或拒绝原因）
    ///
    /// 方向：移动端（responder）→ 桌面端（协调者）；桌面端
    /// `terminal_ws::handle_file_service` 收到后经注册表双通道发布
    /// `filesrv:intent_ack` 供发送方插件推进任务（accepted → 传输中 / rejected → 终态）
    IntentAck {
        /// 意图 ID（对应 SyncPayload::FileTransferIntent.intent_id）
        intent_id: String,
        /// "accepted" | "rejected"
        decision: String,
        /// accepted：期望起始偏移（断点续传真源 = 手机本地已写字节）；
        /// rejected：忽略
        #[serde(default)]
        offset: u64,
        /// 手机本地会话标识（pull：upload sid；push：本地游标 id）
        #[serde(default)]
        session_id: String,
    },
    /// 传输进度（v2.1）：执行中每推进一段回推，桌面据此乐观更新校正
    ///
    /// 方向：移动端（执行方）→ 桌面端（协调者）。载荷 = v2 progress topic
    /// 现有形状（task_id/transferred/total/bytes_per_sec/state）+ `intent_id`
    /// （`#[serde(default)]` 兼容 v1 载荷——非 intent 驱动的传统进度回推
    /// intent_id 缺省为 None）；桌面经注册表双通道发布 `filesrv:transfer_progress`
    TransferProgress {
        /// 意图 ID（缺省 None = 非 intent 驱动的传统进度回推）
        #[serde(default)]
        intent_id: Option<String>,
        /// 任务 ID
        task_id: String,
        /// 已传输字节数（含 ACK offset）
        transferred: u64,
        /// 总字节数（0 = 未知）
        total: u64,
        /// 瞬时速率（字节/秒）
        #[serde(default)]
        bytes_per_sec: u64,
        /// "running" | "completed" | "failed" | "cancelled"
        state: String,
    },
    /// 传输失败偏移上报（v2.1）：手机中途失败主动上报，接收端即断点真源
    ///
    /// 重试 = 桌面重发 intent，手机从 offset 续传；桌面经注册表双通道发布
    /// `filesrv:intent_fail`
    IntentFail {
        /// 意图 ID
        intent_id: String,
        /// 失败时刻已写字节（断点真源）
        #[serde(default)]
        offset: u64,
        /// 失败分类（桌面据此决定任务终态 reason）：
        /// "duplicate-name" = complete 409 同名已存在（沿用 v1 语义）；
        /// 缺省/其他 = 一般传输失败
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// 执行中心跳（v2.1）：手机执行中每 10s 无 progress 则回推，
    /// 桌面 30s 无任何回传判定对端失联 → 任务转「暂停-待续传」
    TransferHeartbeat {
        /// 意图 ID
        intent_id: String,
    },
    /// 目录列举请求（v2.1 list 迁移，桌面 → 手机）：浏览手机共享目录
    ///
    /// D→M 方向复用 FileServicePayload 通道（与 Query 同机制：桌面宿主直发，
    /// 手机 `handler/file_service.rs` 收口）；响应 = `FileListResponse`
    /// 回应（list_id 匹配，桌面侧 pending 带超时兑底）
    FileListRequest {
        /// 请求 ID（uuid，响应匹配）
        list_id: String,
        /// 归属插件（与挂载鉴权一致）
        plugin_id: String,
        /// 挂载路径
        mount_path: String,
        /// 相对挂载点的路径（空 = 挂载根顶层条目）
        #[serde(default)]
        path: String,
    },
    /// 目录列举响应（v2.1 list 迁移，手机 → 桌面）
    ///
    /// 与旧手机 /list HTTP 响应同构（camelCase 条目）；桌面宿主依 list_id 匹配
    /// pending 并返回给插件（前端零改动）
    FileListResponse {
        /// 请求 ID（匹配 FileListRequest.list_id）
        list_id: String,
        /// 归属插件（echo）
        plugin_id: String,
        /// 挂载路径（echo）
        mount_path: String,
        /// 当前相对路径（echo；空 = 挂载根）
        #[serde(default)]
        path: String,
        /// 条目列表（目录优先，按名称排序）
        entries: Vec<ListEntryDto>,
        /// 非空时：列表可能被 Android 存储权限过滤（对端应提示用户授权）
        #[serde(default, skip_serializing_if = "Option::is_none")]
        notice: Option<String>,
        /// 是否成功（false = list 失败，error 给原因）
        #[serde(default)]
        ok: bool,
        /// 错误原因（ok=false 时）
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
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

/// 目录条目（浏览列表，过滤 *.part 临时文件；与旧手机 /list 响应同构）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListEntryDto {
    /// 文件/目录名
    pub name: String,
    /// 字节数（目录为 0）
    pub size: u64,
    /// 修改时间（Unix 秒；读取失败为 0）
    pub mtime: u64,
    /// 是否目录
    pub is_dir: bool,
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
    fn test_intent_ack_wire_format() {
        // v2.1：手机 responder 回执 → 桌面协调者（snake_case 字段，两端同构双写）
        let payload = FileServicePayload::IntentAck {
            intent_id: "9f1c".into(),
            decision: "accepted".into(),
            offset: 4096,
            session_id: "s8".into(),
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"action\":\"intent_ack\""));
        assert!(json.contains("\"intent_id\":\"9f1c\""));
        assert!(json.contains("\"decision\":\"accepted\""));
        assert!(json.contains("\"offset\":4096"));
        assert!(json.contains("\"session_id\":\"s8\""));
        match serde_json::from_str::<FileServicePayload>(&json).unwrap() {
            FileServicePayload::IntentAck {
                intent_id,
                decision,
                offset,
                session_id,
            } => {
                assert_eq!(intent_id, "9f1c");
                assert_eq!(decision, "accepted");
                assert_eq!(offset, 4096);
                assert_eq!(session_id, "s8");
            }
            _ => panic!("expected IntentAck"),
        }
    }

    #[test]
    fn test_intent_ack_defaults_accepted() {
        // offset/session_id 缺省：rejected 回执可能不携带会话信息，反序列化需成功
        let json = r#"{"action":"intent_ack","data":{"intent_id":"i1","decision":"rejected"}}"#;
        match serde_json::from_str::<FileServicePayload>(json).unwrap() {
            FileServicePayload::IntentAck {
                intent_id,
                decision,
                offset,
                session_id,
            } => {
                assert_eq!(intent_id, "i1");
                assert_eq!(decision, "rejected");
                assert_eq!(offset, 0);
                assert_eq!(session_id, "");
            }
            _ => panic!("expected IntentAck"),
        }
    }

    #[test]
    fn test_transfer_progress_wire_format() {
        // v2.1：执行中进度回推 = v2 progress 载荷形状 + intent_id（逐字对齐移动端）
        let payload = FileServicePayload::TransferProgress {
            intent_id: Some("9f1c".into()),
            task_id: "t1".into(),
            transferred: 5000,
            total: 12000,
            bytes_per_sec: 2048,
            state: "running".into(),
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"action\":\"transfer_progress\""));
        assert!(json.contains("\"intent_id\":\"9f1c\""));
        assert!(json.contains("\"task_id\":\"t1\""));
        assert!(json.contains("\"transferred\":5000"));
        assert!(json.contains("\"total\":12000"));
        assert!(json.contains("\"state\":\"running\""));
        match serde_json::from_str::<FileServicePayload>(&json).unwrap() {
            FileServicePayload::TransferProgress {
                intent_id,
                task_id,
                transferred,
                total,
                bytes_per_sec,
                state,
            } => {
                assert_eq!(intent_id.as_deref(), Some("9f1c"));
                assert_eq!(task_id, "t1");
                assert_eq!(transferred, 5000);
                assert_eq!(total, 12000);
                assert_eq!(bytes_per_sec, 2048);
                assert_eq!(state, "running");
            }
            _ => panic!("expected TransferProgress"),
        }
    }

    #[test]
    fn test_transfer_progress_without_intent_defaults() {
        // v1 载荷（无 intent_id）：缺省解析为 None（非 intent 驱动进度回推）
        let json = r#"{"action":"transfer_progress","data":{"task_id":"t1","transferred":1,"total":2,"bytes_per_sec":0,"state":"running"}}"#;
        match serde_json::from_str::<FileServicePayload>(json).unwrap() {
            FileServicePayload::TransferProgress {
                intent_id,
                task_id,
                state,
                ..
            } => {
                assert_eq!(intent_id, None);
                assert_eq!(task_id, "t1");
                assert_eq!(state, "running");
            }
            _ => panic!("expected TransferProgress"),
        }
    }

    #[test]
    fn test_intent_fail_wire_format() {
        let payload = FileServicePayload::IntentFail {
            intent_id: "9f1c".into(),
            offset: 4096,
            reason: None,
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"action\":\"intent_fail\""));
        assert!(json.contains("\"intent_id\":\"9f1c\""));
        assert!(json.contains("\"offset\":4096"));
        match serde_json::from_str::<FileServicePayload>(&json).unwrap() {
            FileServicePayload::IntentFail {
                intent_id,
                offset,
                reason,
            } => {
                assert_eq!(intent_id, "9f1c");
                assert_eq!(offset, 4096);
                assert_eq!(reason, None);
            }
            _ => panic!("expected IntentFail"),
        }
    }

    #[test]
    fn test_intent_fail_reason_wire_format() {
        // v2.1：complete 409 duplicate-name 分类随 fail 上报（桌面据此置任务终态 reason）
        let json =
            r#"{"action":"intent_fail","data":{"intent_id":"i9","offset":5,"reason":"duplicate-name"}}"#;
        match serde_json::from_str::<FileServicePayload>(json).unwrap() {
            FileServicePayload::IntentFail {
                intent_id,
                offset,
                reason,
            } => {
                assert_eq!(intent_id, "i9");
                assert_eq!(offset, 5);
                assert_eq!(reason.as_deref(), Some("duplicate-name"));
            }
            _ => panic!("expected IntentFail"),
        }
    }

    #[test]
    fn test_transfer_heartbeat_wire_format() {
        let payload = FileServicePayload::TransferHeartbeat {
            intent_id: "9f1c".into(),
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"action\":\"transfer_heartbeat\""));
        assert!(json.contains("\"intent_id\":\"9f1c\""));
        match serde_json::from_str::<FileServicePayload>(&json).unwrap() {
            FileServicePayload::TransferHeartbeat { intent_id } => assert_eq!(intent_id, "9f1c"),
            _ => panic!("expected TransferHeartbeat"),
        }
    }

    #[test]
    fn test_transfer_approval_wire_format() {
        // v2：接收端应答 → 发送端（snake_case 字段，两端同构双写）
        let payload = FileServicePayload::TransferApproval {
            batch_id: "b1".into(),
            decision: "approved".into(),
            reason: String::new(),
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"action\":\"transfer_approval\""));
        assert!(json.contains("\"batch_id\":\"b1\""));
        assert!(json.contains("\"decision\":\"approved\""));
        let back: FileServicePayload = serde_json::from_str(&json).unwrap();
        match back {
            FileServicePayload::TransferApproval {
                batch_id,
                decision,
                reason,
            } => {
                assert_eq!(batch_id, "b1");
                assert_eq!(decision, "approved");
                assert_eq!(reason, "");
            }
            _ => panic!("expected TransferApproval"),
        }
    }

    #[test]
    fn test_file_list_request_wire_format() {
        let payload = FileServicePayload::FileListRequest {
            list_id: "li-1".to_string(),
            plugin_id: "com.bedcode.file-transfer".to_string(),
            mount_path: "files".to_string(),
            path: "dir/sub".to_string(),
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"action\":\"file_list_request\""));
        assert!(json.contains("\"list_id\":\"li-1\""));
        assert!(json.contains("\"mount_path\":\"files\""));
        assert!(json.contains("\"path\":\"dir/sub\""));
        match serde_json::from_str::<FileServicePayload>(&json).unwrap() {
            FileServicePayload::FileListRequest {
                list_id,
                plugin_id,
                mount_path,
                path,
            } => {
                assert_eq!(list_id, "li-1");
                assert_eq!(plugin_id, "com.bedcode.file-transfer");
                assert_eq!(mount_path, "files");
                assert_eq!(path, "dir/sub");
            }
            _ => panic!("expected FileListRequest"),
        }
    }

    #[test]
    fn test_file_list_response_wire_format() {
        let payload = FileServicePayload::FileListResponse {
            list_id: "li-1".to_string(),
            plugin_id: "com.bedcode.file-transfer".to_string(),
            mount_path: "files".to_string(),
            path: String::new(),
            entries: vec![ListEntryDto {
                name: "a.mp4".into(),
                size: 123,
                mtime: 0,
                is_dir: false,
            }],
            notice: None,
            ok: true,
            error: None,
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"action\":\"file_list_response\""));
        assert!(json.contains("\"entries\":[{\"name\":\"a.mp4\",\"size\":123,\"mtime\":0,\"isDir\":false}]"));
        assert!(!json.contains("\"notice\""));
        assert!(!json.contains("\"error\""));
        match serde_json::from_str::<FileServicePayload>(&json).unwrap() {
            FileServicePayload::FileListResponse {
                list_id,
                entries,
                ok,
                ..
            } => {
                assert_eq!(list_id, "li-1");
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0].name, "a.mp4");
                assert!(ok);
            }
            _ => panic!("expected FileListResponse"),
        }
    }

    #[test]
    fn test_file_list_response_error_roundtrip() {
        let payload = FileServicePayload::FileListResponse {
            list_id: "li-2".to_string(),
            plugin_id: "com.bedcode.file-transfer".to_string(),
            mount_path: "files".to_string(),
            path: "nope".to_string(),
            entries: vec![],
            notice: None,
            ok: false,
            error: Some("not found".to_string()),
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"ok\":false"));
        assert!(json.contains("\"error\":\"not found\""));
        match serde_json::from_str::<FileServicePayload>(&json).unwrap() {
            FileServicePayload::FileListResponse { ok, error, .. } => {
                assert!(!ok);
                assert_eq!(error.as_deref(), Some("not found"));
            }
            _ => panic!("expected FileListResponse"),
        }
    }
}
