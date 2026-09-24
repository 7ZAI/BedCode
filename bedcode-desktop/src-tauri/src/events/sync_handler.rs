//! Sync Event Handler
//!
//! 同步事件处理器：把 [`HostSyncEvent`] 折成 `SyncData` WebSocket 消息并广播。
//!
//! **专项票 03 后本处理器只剩传输面三件事**：折载荷、按源设备排除、广播。
//! 此前这里是 11 个变体的业务 match（逐字段搬运 + `format!("{:?}")` 重格式化状态 +
//! 「缺字段就 warn」的生产者兜底），那是会话语义在宿主的残留面；票 02 把
//! `SyncEvent` 与 `SyncPayload` 对齐成同一 wire 后，「搬运」退化成一次机械折算，
//! 必填自足性由类型与插件产出口保证。判据锁见
//! `sync_handler_does_not_interpret_session_variants`。

use super::app_event::AppEvent;
use super::matcher::EventHandler;
use crate::enums::SyncPayload;
use crate::events::HostSyncEvent;
use crate::server::websocket::message::Message;
use crate::server::websocket::WebSocketManager;

/// 同步事件处理器
///
/// 由 `EventMatcher` 在 [`HostSyncEvent`] 事件源上驱动；广播失败只留痕不重试
/// （移动端靠重连后的全量拉取自愈，与迁移前一致）。
pub struct SyncEventHandler {
    ws_manager: &'static (dyn SyncBroadcaster + Send + Sync),
}

/// 同步广播抽象（票据 22）：把 `WebSocketManager` 的广播能力抽为 trait，
/// 测试注入 Fake 记录广播调用，覆盖事件→消息映射与排除语义。
///
/// 生产实现 `WebSocketManager`（`&'static` 单例），`&'static WebSocketManager`
/// 自动 coerce 到 `&'static dyn SyncBroadcaster`，调用点零改动。
#[async_trait::async_trait]
pub trait SyncBroadcaster: Send + Sync {
    /// 向所有已认证客户端广播
    async fn broadcast(&self, message: &Message) -> crate::Result<()>;
    /// 向除指定设备外的所有已认证客户端广播（基于设备名称）
    async fn broadcast_sync_to_others(&self, exclude_device_name: &str, message: &Message) -> crate::Result<()>;
}

#[async_trait::async_trait]
impl SyncBroadcaster for crate::server::websocket::WebSocketManager {
    async fn broadcast(&self, message: &Message) -> crate::Result<()> {
        crate::server::websocket::WebSocketManager::broadcast(self, message).await
    }

    async fn broadcast_sync_to_others(&self, exclude_device_name: &str, message: &Message) -> crate::Result<()> {
        crate::server::websocket::WebSocketManager::broadcast_sync_to_others(self, exclude_device_name, message).await
    }
}

impl SyncEventHandler {
    /// 创建新的同步事件处理器
    pub fn new(ws_manager: &'static WebSocketManager) -> Self {
        let ws_manager: &'static (dyn SyncBroadcaster + Send + Sync) = ws_manager;
        Self { ws_manager }
    }

    /// 异步处理事件
    ///
    /// 零业务分支：载荷由 `HostSyncEvent` 折算（同 wire 的机械转换），源设备由
    /// 信封字段给出，其余一概不解释。
    async fn process_event(&self, event: HostSyncEvent) {
        let Some(payload) = event.to_sync_payload() else {
            // 折不成出站形状 = 事件面与载荷面漂移；`to_sync_payload` 内已 `error!`
            // 点名留痕，这里只负责**不伪造推送**（宁可少发一条）。
            // 走统一入口时 `publish` 的 `validate` 已在投递前挡下，正常不会到这一步。
            return;
        };
        self.broadcast_sync_data(payload, event.source_device()).await;
    }

    /// 广播同步数据消息
    ///
    /// 如果指定了 exclude_device，则排除该设备后广播给其他客户端；
    /// 否则广播给所有已认证客户端。
    async fn broadcast_sync_data(&self, payload: SyncPayload, exclude_device: Option<&str>) {
        let message = Message::sync_data(payload);

        if let Some(device_name) = exclude_device {
            // 排除操作者，广播给其他客户端
            if let Err(e) = self.ws_manager.broadcast_sync_to_others(device_name, &message).await {
                tracing::error!(device = %device_name, error = %e, "[SyncEventHandler] 排除源设备广播失败");
            }
        } else if let Err(e) = self.ws_manager.broadcast(&message).await {
            tracing::error!(error = %e, "[SyncEventHandler] 同步广播失败");
        }
    }
}

impl EventHandler<HostSyncEvent> for SyncEventHandler {
    fn handle(&self, event: HostSyncEvent) {
        // 广播是异步的：交给 tokio，避免在 matcher 的分发循环里串行阻塞其它处理器
        let ws_manager = self.ws_manager;

        tokio::spawn(async move {
            let handler = SyncEventHandler { ws_manager };
            handler.process_event(event).await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enums::{PluginQuestion, PluginQuestionOption, SessionSummary};
    use bedcode_plugin_api::events::SyncEvent;
    use std::sync::{Arc, Mutex};

    /// Fake 广播器：记录所有广播调用（票据 22）
    struct FakeBroadcaster {
        calls: Mutex<Vec<BroadcastCall>>,
    }

    #[derive(Debug, Clone)]
    struct BroadcastCall {
        exclude_device: Option<String>,
        payload: SyncPayload,
    }

    impl FakeBroadcaster {
        fn new() -> Self {
            Self { calls: Mutex::new(Vec::new()) }
        }

        fn take_calls(&self) -> Vec<BroadcastCall> {
            self.calls.lock().unwrap().drain(..).collect()
        }
    }

    #[async_trait::async_trait]
    impl SyncBroadcaster for FakeBroadcaster {
        async fn broadcast(&self, message: &Message) -> crate::Result<()> {
            if let Message::SyncData { payload, .. } = message {
                self.calls.lock().unwrap().push(BroadcastCall {
                    exclude_device: None,
                    payload: payload.clone(),
                });
            }
            Ok(())
        }

        async fn broadcast_sync_to_others(&self, exclude_device_name: &str, message: &Message) -> crate::Result<()> {
            if let Message::SyncData { payload, .. } = message {
                self.calls.lock().unwrap().push(BroadcastCall {
                    exclude_device: Some(exclude_device_name.to_string()),
                    payload: payload.clone(),
                });
            }
            Ok(())
        }
    }

    /// 构造 handler：Fake 泄漏为 &'static（处理器不持有任何业务登记）
    fn test_handler() -> (Arc<SyncEventHandler>, &'static FakeBroadcaster) {
        let fake_static: &'static FakeBroadcaster = Box::leak(Box::new(FakeBroadcaster::new()));
        let ws: &'static (dyn SyncBroadcaster + Send + Sync) = fake_static;
        (Arc::new(SyncEventHandler { ws_manager: ws }), fake_static)
    }

    /// 会话概要构造（插件事件自携带的载荷形状）
    fn summary(sid: &str) -> SessionSummary {
        SessionSummary {
            id: sid.to_string(),
            name: "itest-sync".to_string(),
            status: "running".to_string(),
            created_at: "2026-09-24T00:00:00Z".to_string(),
            started_at: Some("2026-09-24T00:00:00Z".to_string()),
            session_type: Some("pty".to_string()),
            config_id: Some("itest-sync".to_string()),
            task_status: None,
            task_reason: None,
        }
    }

    /// 广播一次并取回调用记录
    async fn once(event: SyncEvent) -> Vec<BroadcastCall> {
        let (handler, fake) = test_handler();
        handler.process_event(HostSyncEvent(event)).await;
        fake.take_calls()
    }

    // ==================== 会话四类：载荷透出 + 排除语义 ====================

    /// `SessionCreated` → 广播 `session_created`，载荷原样透出，源设备被排除
    #[tokio::test]
    async fn session_created_broadcasts_payload_and_excludes_source() {
        let calls = once(SyncEvent::SessionCreated {
            session: summary("s-1"),
            source_device: "d1".to_string(),
        })
        .await;

        assert_eq!(calls.len(), 1, "应广播一次: {calls:?}");
        assert_eq!(calls[0].exclude_device.as_deref(), Some("d1"), "来源设备应被排除");
        assert_eq!(
            serde_json::to_value(&calls[0].payload).unwrap(),
            serde_json::json!({
                "type": "session_created",
                "data": {
                    "session": {
                        "id": "s-1",
                        "name": "itest-sync",
                        "status": "running",
                        "created_at": "2026-09-24T00:00:00Z",
                        "started_at": "2026-09-24T00:00:00Z",
                        "session_type": "pty",
                        "config_id": "itest-sync"
                    },
                    "source_device": "d1"
                }
            }),
            "出站载荷逐字段透出（宿主不改写、不补值）"
        );
    }

    /// 桌面本地操作（空源设备）→ 全量广播，不带排除
    #[tokio::test]
    async fn local_operation_broadcasts_to_all() {
        let calls = once(SyncEvent::SessionCreated {
            session: summary("s-local"),
            source_device: String::new(),
        })
        .await;
        assert_eq!(calls.len(), 1);
        assert!(calls[0].exclude_device.is_none(), "空源设备不构成排除: {calls:?}");
    }

    /// `SessionStopped` → `session_stopped`；`source_device` 只做排除，**不出站**
    ///
    /// 逐字节锁：这是「插件事件面 = 移动端所收」这一约定的具体体现（票 02 的例外面）。
    #[tokio::test]
    async fn session_stopped_broadcasts_without_envelope_field_on_wire() {
        let calls = once(SyncEvent::SessionStopped {
            session_id: "s-2".to_string(),
            session_name: "itest-sync".to_string(),
            source_device: "d2".to_string(),
        })
        .await;

        assert_eq!(calls[0].exclude_device.as_deref(), Some("d2"));
        assert_eq!(
            serde_json::to_string(&calls[0].payload).unwrap(),
            r#"{"type":"session_stopped","data":{"session_id":"s-2","session_name":"itest-sync"}}"#,
            "出站 JSON 逐字锁（多了 source_device 或改标签都会惊动移动端）"
        );
    }

    /// `SessionRemoved` 空名照广播：P1-b 的行为锁（未知会话仍要通知各端刷新列表）
    #[tokio::test]
    async fn session_removed_with_empty_name_still_broadcasts() {
        let calls = once(SyncEvent::SessionRemoved {
            session_id: "s-3".to_string(),
            session_name: String::new(),
            source_device: "d3".to_string(),
        })
        .await;

        assert_eq!(calls.len(), 1, "幂等移除不得静默: {calls:?}");
        assert_eq!(
            serde_json::to_value(&calls[0].payload).unwrap(),
            serde_json::json!({
                "type": "session_removed",
                "data": { "session_id": "s-3", "session_name": "" }
            })
        );
        assert_eq!(calls[0].exclude_device.as_deref(), Some("d3"));
    }

    /// `SessionStatusChanged` 的状态**原样透传**插件给的 wire 字符串
    ///
    /// 替代票 02 双轨对照里的旧口径（`format!("{:?}").to_lowercase()` 把
    /// `waitingInput` 打成 `waitinginput`）。宿主不再持有 `SessionStatus` 解释面。
    #[tokio::test]
    async fn session_status_changed_passes_wire_status_through() {
        let calls = once(SyncEvent::SessionStatusChanged {
            session_id: "s-4".to_string(),
            old_status: "running".to_string(),
            new_status: "waitingInput".to_string(),
            session_name: "itest-sync".to_string(),
        })
        .await;

        assert_eq!(
            serde_json::to_value(&calls[0].payload).unwrap(),
            serde_json::json!({
                "type": "session_status_changed",
                "data": {
                    "session_id": "s-4",
                    "old_status": "running",
                    "new_status": "waitingInput",
                    "session_name": "itest-sync"
                }
            }),
            "状态字符串必须逐字透传（旧 Debug 重格式化已随票 03 退役）"
        );
        assert!(calls[0].exclude_device.is_none(), "状态变更全量广播");
    }

    // ==================== 任务与模式面 ====================

    #[tokio::test]
    async fn task_status_changed_broadcasts_to_all() {
        let calls = once(SyncEvent::TaskStatusChanged {
            session_id: "s-5".to_string(),
            task_status: "asking".to_string(),
            task_reason: Some("需要授权".to_string()),
            task_questions: Some(vec![PluginQuestion {
                question: "pick one".to_string(),
                header: "choose".to_string(),
                multi_select: false,
                options: vec![PluginQuestionOption { label: "a".to_string(), description: String::new() }],
            }]),
        })
        .await;

        assert!(calls[0].exclude_device.is_none(), "任务状态变更广播给所有客户端");
        let value = serde_json::to_value(&calls[0].payload).unwrap();
        assert_eq!(value["type"], "task_status_changed");
        assert_eq!(value["data"]["task_status"], "asking");
        assert_eq!(value["data"]["task_reason"], "需要授权");
        assert_eq!(value["data"]["task_questions"][0]["options"][0]["label"], "a");
    }

    #[tokio::test]
    async fn session_mode_changed_broadcasts() {
        let calls = once(SyncEvent::SessionModeChanged {
            session_id: "s-6".to_string(),
            auto_approve: true,
        })
        .await;
        assert_eq!(
            serde_json::to_value(&calls[0].payload).unwrap(),
            serde_json::json!({
                "type": "session_mode_changed",
                "data": { "session_id": "s-6", "auto_approve": true }
            })
        );
    }

    /// 队列变更：可选字段缺省时 wire 上不出现键（`skip_serializing_if` 形状锁）
    #[tokio::test]
    async fn task_queue_changed_omits_absent_optional_keys() {
        let calls = once(SyncEvent::TaskQueueChanged {
            session_id: "s-7".to_string(),
            queue_count: 0,
            action: "clear".to_string(),
            task_id: None,
            status: None,
        })
        .await;
        assert_eq!(
            serde_json::to_string(&calls[0].payload).unwrap(),
            r#"{"type":"task_queue_changed","data":{"session_id":"s-7","queue_count":0,"action":"clear"}}"#
        );

        let calls = once(SyncEvent::TaskQueueChanged {
            session_id: "s-7".to_string(),
            queue_count: 3,
            action: "done".to_string(),
            task_id: Some("t1".to_string()),
            status: Some("done".to_string()),
        })
        .await;
        let value = serde_json::to_value(&calls[0].payload).unwrap();
        assert_eq!(value["data"]["queue_count"], 3);
        assert_eq!(value["data"]["task_id"], "t1");
        assert_eq!(value["data"]["status"], "done");
    }

    #[tokio::test]
    async fn task_scheduled_changed_broadcasts() {
        let calls = once(SyncEvent::TaskScheduledChanged {
            job_id: "job-1".to_string(),
            status: "pending".to_string(),
            action: "create".to_string(),
        })
        .await;
        assert_eq!(
            serde_json::to_value(&calls[0].payload).unwrap(),
            serde_json::json!({
                "type": "task_scheduled_changed",
                "data": { "job_id": "job-1", "status": "pending", "action": "create" }
            })
        );
    }

    // ==================== 票 12 降级口径（wire 形状，与取值来源无关） ====================

    /// 插件未写任务字段 → 会话摘要的任务字段**在 wire 上不出现键**
    #[tokio::test]
    async fn session_created_without_task_fields_omits_them_on_wire() {
        let calls = once(SyncEvent::SessionCreated {
            session: summary("s-noslot"),
            source_device: String::new(),
        })
        .await;
        let SyncPayload::SessionCreated { session, .. } = &calls[0].payload else {
            panic!("期望 SessionCreated，实际: {:?}", calls[0].payload);
        };
        assert!(session.task_status.is_none() && session.task_reason.is_none());
        let summary_json = serde_json::to_value(&calls[0].payload).unwrap()["data"]["session"].clone();
        assert!(
            summary_json.get("taskStatus").is_none() && summary_json.get("task_reason").is_none(),
            "wire 上不得出现任务字段键: {summary_json}"
        );
    }

    /// 任务字段有值 → 逐字段透出
    #[tokio::test]
    async fn session_created_carries_task_fields() {
        let mut carried = summary("s-slot");
        carried.task_status = Some("in_progress".to_string());
        carried.task_reason = Some("AI 会话".to_string());
        let calls = once(SyncEvent::SessionCreated {
            session: carried,
            source_device: String::new(),
        })
        .await;
        let SyncPayload::SessionCreated { session, .. } = &calls[0].payload else {
            panic!("期望 SessionCreated，实际: {:?}", calls[0].payload);
        };
        assert_eq!(session.task_status.as_deref(), Some("in_progress"));
        assert_eq!(session.task_reason.as_deref(), Some("AI 会话"));
    }

    // ==================== 防回接锁 ====================

    /// 票 09 防回接锁：宿主事件模块**不得再依赖内核会话登记**。
    ///
    /// 这是「`src/session/` 整目录可删（票 11）」的最后一道前置：事件面若还读
    /// `crate::session::*`，删目录就会把它带塌。判据扫 `src/events/**` 全部
    /// Rust 源的非注释行——注释里出现是记账（说清为什么不再依赖）。
    #[test]
    fn events_module_does_not_depend_on_kernel_session_registry() {
        let events_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/events");
        let mut violations: Vec<String> = Vec::new();
        let mut stack = vec![events_dir];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                    continue;
                }
                // 本文件是锁自身，跳过（避免自匹配）
                if path.ends_with("sync_handler.rs") {
                    continue;
                }
                let Ok(content) = std::fs::read_to_string(&path) else { continue };
                for (idx, raw_line) in content.lines().enumerate() {
                    let line = raw_line.trim_start();
                    if line.starts_with("//") {
                        continue;
                    }
                    if line.contains("crate::session") || line.contains("SessionManager") {
                        violations.push(format!("{}:{}: {}", path.display(), idx + 1, line.trim()));
                    }
                }
            }
        }
        assert!(
            violations.is_empty(),
            "宿主事件模块不得依赖内核会话登记（票 09）：\n{}",
            violations.join("\n")
        );
    }

    /// 票 03 防回接锁：处理器的**实现段**不得再按会话/任务变体分支。
    ///
    /// 判据取 `#[cfg(test)]` 之前的实现部分（测试里构造各变体是必要的），扫三类
    /// 回接形态：① 再 match 事件变体；② 再构造 `SyncPayload::` 某个变体（那等于
    /// 宿主重新决定「这个变体出站放哪些字段」）；③ 再把状态 `Debug` 重格式化。
    #[test]
    fn sync_handler_does_not_interpret_session_variants() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/events/sync_handler.rs");
        let content = std::fs::read_to_string(&path).expect("读取 sync_handler.rs");
        let implementation = content
            .split("#[cfg(test)]")
            .next()
            .expect("存在实现段")
            .to_string();

        let mut violations: Vec<String> = Vec::new();
        for (idx, raw_line) in implementation.lines().enumerate() {
            let line = raw_line.trim_start();
            if line.starts_with("//") || line.starts_with("///") {
                continue;
            }
            let hits: &[&str] = &[
                "SyncPayload::",
                "match event",
                "match &event",
                "DesktopSyncEvent::",
                "SessionStatus",
                "format!(\"{:?}\"",
            ];
            for marker in hits {
                if line.contains(marker) {
                    violations.push(format!("{}:{}: {}", path.display(), idx + 1, line.trim()));
                }
            }
        }
        assert!(
            violations.is_empty(),
            "同步事件处理器实现段不得再解释会话/任务变体（票 03：折载荷归适配层，\
             必填自足归生产者）：\n{}",
            violations.join("\n")
        );
    }
}
