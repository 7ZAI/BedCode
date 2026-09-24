//! Sync Event Handler
//!
//! 同步事件处理器，将 DesktopSyncEvent 转换为 SyncData WebSocket 消息并广播

use super::matcher::EventHandler;
use crate::enums::SyncPayload;
use crate::events::DesktopSyncEvent;
use crate::server::websocket::message::Message;
use crate::server::websocket::WebSocketManager;

/// 同步事件处理器
///
/// 将 DesktopSyncEvent 转换为 SyncData WebSocket 消息并广播给客户端。
///
/// **票 09：本处理器不再持有内核会话登记**。P1-b 之前，会话四类事件的载荷可以
/// 缺字段，处理器就回查 `SessionManager` 补上；真源下沉后内核里已无插件会话，
/// 那条回查恒返回空 → 补出来的是空值（**假兜底**：绿的是代码路径，不是行为）。
/// 现在载荷必须自足（插件经 `SyncEvent` 携带），缺失即 `warn` + **不广播**——
/// 宁可少发一条，也不发一条字段是编出来的推送。
pub struct SyncEventHandler {
    ws_manager: &'static (dyn SyncBroadcaster + Send + Sync),
}

/// 同步广播抽象（票据 22）：把 `WebSocketManager` 的广播能力抽为 trait，
/// 测试注入 Fake 记录广播调用，覆盖 11 个 handle_* 分支的事件→消息映射。
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
    async fn process_event(&self, event: DesktopSyncEvent) {
        tracing::info!("[SyncEventHandler] Processing event: {:?}", event);
        match event {
            DesktopSyncEvent::SessionCreated {
                session_id,
                source_device,
                session,
            } => {
                self.handle_session_created(&session_id, source_device, session).await;
            }
            DesktopSyncEvent::SessionStatusChanged {
                session_id,
                old_status,
                new_status,
                session_name,
            } => {
                self.handle_session_status_changed(&session_id, old_status, new_status, session_name)
                    .await;
            }
            DesktopSyncEvent::SessionStopped {
                session_id,
                source_device,
                session_name,
            } => {
                self.handle_session_stopped(&session_id, source_device, session_name).await;
            }
            DesktopSyncEvent::SessionRemoved {
                session_id,
                source_device,
                session_name,
            } => {
                self.handle_session_removed(&session_id, source_device, session_name).await;
            }
            DesktopSyncEvent::TaskStatusChanged {
                session_id,
                task_status,
                task_reason,
                task_questions,
            } => {
                self.handle_task_status_changed(
                    &session_id,
                    &task_status,
                    task_reason.as_deref(),
                    task_questions.as_deref(),
                )
                .await;
            }
            DesktopSyncEvent::SessionModeChanged {
                session_id,
                auto_approve,
            } => {
                self.handle_session_mode_changed(&session_id, auto_approve).await;
            }
            DesktopSyncEvent::TaskQueueChanged {
                session_id,
                queue_count,
                action,
                task_id,
                status,
            } => {
                self.handle_task_queue_changed(
                    &session_id,
                    queue_count,
                    &action,
                    task_id.as_deref(),
                    status.as_deref(),
                )
                .await;
            }
            DesktopSyncEvent::TaskScheduledChanged { job_id, status, action } => {
                self.handle_task_scheduled_changed(&job_id, &status, &action).await;
            }
        }
    }

    /// 处理会话创建事件
    async fn handle_session_created(
        &self,
        session_id: &str,
        source_device: Option<String>,
        carried: Option<crate::enums::summary::SessionSummary>,
    ) {
        // 票 09：会话概要必须随事件携带（真源在 `com.bedcode.terminal-session`，
        // 宿主已无可回查的登记）。缺失即显性留痕 + 不广播——回查空值补出来的
        // 「无名字会话」推送比不推送更坏（前端按它渲染出空条目）。
        let Some(session) = carried else {
            tracing::warn!(
                session_id = %session_id,
                "[SyncEventHandler] SessionCreated 载荷缺会话概要，拒绝广播（生产者为旧版插件 / 内核路径）"
            );
            return;
        };

        // 提取 source_device 值
        let source_device_str = source_device.clone().unwrap_or_default();

        // 构建同步载荷
        let payload = SyncPayload::SessionCreated {
            session,
            source_device: source_device_str.clone(),
        };

        // 广播消息
        self.broadcast_sync_data(payload, Some(&source_device_str)).await;
    }

    /// 处理会话状态变化事件
    async fn handle_session_status_changed(
        &self,
        session_id: &str,
        old_status: crate::enums::SessionStatus,
        new_status: crate::enums::SessionStatus,
        carried_name: Option<String>,
    ) {
        // 票 09：会话名必须随事件携带（同 `handle_session_created` 的理由）
        let Some(session_name) = carried_name else {
            tracing::warn!(
                session_id = %session_id,
                "[SyncEventHandler] SessionStatusChanged 载荷缺会话名，拒绝广播"
            );
            return;
        };

        // 构建同步载荷
        let payload = SyncPayload::SessionStatusChanged {
            session_id: session_id.to_string(),
            old_status: format!("{:?}", old_status).to_lowercase(),
            new_status: format!("{:?}", new_status).to_lowercase(),
            session_name,
        };

        // 状态变化广播给所有客户端
        self.broadcast_sync_data(payload, None).await;
    }

    /// 处理会话停止事件
    async fn handle_session_stopped(
        &self,
        session_id: &str,
        source_device: Option<String>,
        carried_name: Option<String>,
    ) {
        // 票 09：会话名必须随事件携带（同 `handle_session_created` 的理由）
        let Some(session_name) = carried_name else {
            tracing::warn!(
                session_id = %session_id,
                "[SyncEventHandler] SessionStopped 载荷缺会话名，拒绝广播"
            );
            return;
        };

        // 构建同步载荷
        let payload = SyncPayload::SessionStopped {
            session_id: session_id.to_string(),
            session_name,
        };

        // 广播消息
        self.broadcast_sync_data(payload, source_device.as_deref()).await;
    }

    /// 处理会话删除事件
    async fn handle_session_removed(
        &self,
        session_id: &str,
        source_device: Option<String>,
        carried_name: Option<String>,
    ) {
        // 票 09：本分支**不做**「缺名即不广播」——它是四类里唯一没有内核回查的分支
        // （历史上就允许空名），且 P1-b 的行为锁要求「未知会话仍广播移除」（多客户端
        // 靠这条刷新列表，幂等删除不得静默）。名字缺失只降级为无名推送。
        let session_name = carried_name.unwrap_or_default();

        // 构建同步载荷
        let payload = SyncPayload::SessionRemoved {
            session_id: session_id.to_string(),
            session_name,
        };

        // 广播消息
        self.broadcast_sync_data(payload, source_device.as_deref()).await;
    }

    /// 处理配置创建事件
    /// 处理任务状态变更事件
    async fn handle_task_status_changed(
        &self,
        session_id: &str,
        task_status: &str,
        task_reason: Option<&str>,
        task_questions: Option<&[crate::enums::PluginQuestion]>,
    ) {
        let payload = SyncPayload::TaskStatusChanged {
            session_id: session_id.to_string(),
            task_status: task_status.to_string(),
            task_reason: task_reason.map(|s| s.to_string()),
            task_questions: task_questions.map(|qs| qs.to_vec()),
        };

        // 任务状态变更广播给所有客户端
        self.broadcast_sync_data(payload, None).await;
    }

    /// 处理会话模式变更事件
    async fn handle_session_mode_changed(&self, session_id: &str, auto_approve: bool) {
        let payload = SyncPayload::SessionModeChanged {
            session_id: session_id.to_string(),
            auto_approve,
        };

        // 模式变更广播给所有客户端
        self.broadcast_sync_data(payload, None).await;
    }

    /// 处理任务队列变更事件
    async fn handle_task_queue_changed(
        &self,
        session_id: &str,
        queue_count: i64,
        action: &str,
        task_id: Option<&str>,
        status: Option<&str>,
    ) {
        let payload = SyncPayload::TaskQueueChanged {
            session_id: session_id.to_string(),
            queue_count,
            action: action.to_string(),
            task_id: task_id.map(|s| s.to_string()),
            status: status.map(|s| s.to_string()),
        };

        // 队列变更广播给所有客户端
        self.broadcast_sync_data(payload, None).await;
    }

    /// 处理定时自动任务变更事件（广播给所有客户端，供移动端刷新列表）
    async fn handle_task_scheduled_changed(&self, job_id: &str, status: &str, action: &str) {
        let payload = SyncPayload::TaskScheduledChanged {
            job_id: job_id.to_string(),
            status: status.to_string(),
            action: action.to_string(),
        };

        self.broadcast_sync_data(payload, None).await;
    }

    /// 广播同步数据消息
    ///
    /// 如果指定了 exclude_device，则排除该设备后广播给其他客户端
    /// 否则广播给所有已认证客户端
    async fn broadcast_sync_data(&self, payload: SyncPayload, exclude_device: Option<&str>) {
        let message = Message::sync_data(payload);

        if let Some(device_name) = exclude_device {
            if !device_name.is_empty() {
                // 排除操作者，广播给其他客户端
                if let Err(e) = self.ws_manager.broadcast_sync_to_others(device_name, &message).await {
                    tracing::error!("[SyncEventHandler] Failed to broadcast to others: {}", e);
                }
            } else {
                // 桌面本地操作，广播给所有客户端
                if let Err(e) = self.ws_manager.broadcast(&message).await {
                    tracing::error!("[SyncEventHandler] Failed to broadcast: {}", e);
                }
            }
        } else {
            // 状态变化等事件，广播给所有客户端
            if let Err(e) = self.ws_manager.broadcast(&message).await {
                tracing::error!("[SyncEventHandler] Failed to broadcast: {}", e);
            }
        }
    }
}

impl EventHandler<DesktopSyncEvent> for SyncEventHandler {
    fn handle(&self, event: DesktopSyncEvent) {
        // 克隆必要的数据用于异步任务
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
    use crate::events::DesktopSyncEvent;
    use crate::server::websocket::message::Message;
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
            Self {
                calls: Mutex::new(Vec::new()),
            }
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

    /// 构造 handler：Fake 泄漏为 &'static（处理器本体已不持有会话登记，票 09）
    async fn test_handler() -> (Arc<SyncEventHandler>, &'static FakeBroadcaster) {
        let fake_static: &'static FakeBroadcaster = Box::leak(Box::new(FakeBroadcaster::new()));
        let ws: &'static (dyn SyncBroadcaster + Send + Sync) = fake_static;
        let handler = Arc::new(SyncEventHandler { ws_manager: ws });
        (handler, fake_static)
    }

    /// 会话概要构造（插件事件自携带的载荷形态）
    fn summary(sid: &str) -> crate::enums::SessionSummary {
        crate::enums::SessionSummary {
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

    /// SessionCreated → 广播 SyncPayload::SessionCreated（票据 22）
    ///
    /// 票 09：载荷**自携带**会话概要（插件真源），不再依赖处理器回查内核。
    #[tokio::test]
    async fn session_created_event_broadcasts_session_created() {
        let (handler, fake) = test_handler().await;
        handler
            .process_event(DesktopSyncEvent::SessionCreated {
                session_id: "s-1".to_string(),
                source_device: Some("d1".to_string()),
                session: Some(summary("s-1")),
            })
            .await;
        let calls = fake.take_calls();
        assert_eq!(calls.len(), 1, "应广播一次: {calls:?}");
        assert!(
            matches!(calls[0].payload, SyncPayload::SessionCreated { .. }),
            "实际: {:?}",
            calls[0].payload
        );
        assert_eq!(calls[0].exclude_device.as_deref(), Some("d1"), "来源设备应被排除");
    }

    /// SessionStopped → 广播 SyncPayload::SessionStopped（排除来源设备）
    #[tokio::test]
    async fn session_stopped_event_broadcasts_session_stopped() {
        let (handler, fake) = test_handler().await;
        handler
            .process_event(DesktopSyncEvent::SessionStopped {
                session_id: "s-2".to_string(),
                source_device: Some("d2".to_string()),
                session_name: Some("itest-sync".to_string()),
            })
            .await;
        let calls = fake.take_calls();
        assert!(
            matches!(calls[0].payload, SyncPayload::SessionStopped { .. }),
            "实际: {:?}",
            calls[0].payload
        );
        assert_eq!(calls[0].exclude_device.as_deref(), Some("d2"));
    }

    /// SessionRemoved → 广播 SessionRemoved（排除来源设备）
    ///
    /// 空名也照广播：P1-b 的「未知会话仍广播移除」锁（多客户端刷新依赖它）。
    #[tokio::test]
    async fn session_removed_event_broadcasts_session_removed() {
        let (handler, fake) = test_handler().await;
        handler
            .process_event(DesktopSyncEvent::SessionRemoved {
                session_id: "s-3".to_string(),
                source_device: Some("d3".to_string()),
                session_name: None, // 本分支不做「缺名即不广播」（见实现注释）
            })
            .await;
        let calls = fake.take_calls();
        assert!(
            matches!(calls[0].payload, SyncPayload::SessionRemoved { .. }),
            "实际: {:?}",
            calls[0].payload
        );
        assert_eq!(calls[0].exclude_device.as_deref(), Some("d3"));
    }

    /// SessionStatusChanged → 广播（无来源设备 → 全量广播）
    #[tokio::test]
    async fn session_status_changed_broadcasts_to_all() {
        let (handler, fake) = test_handler().await;
        handler
            .process_event(DesktopSyncEvent::SessionStatusChanged {
                session_id: "s-any".to_string(),
                old_status: crate::enums::SessionStatus::Running,
                new_status: crate::enums::SessionStatus::Stopped,
                session_name: Some("itest-sync".to_string()),
            })
            .await;
        let calls = fake.take_calls();
        assert!(matches!(calls[0].payload, SyncPayload::SessionStatusChanged { .. }));
        assert!(calls[0].exclude_device.is_none(), "状态变更应全量广播");
    }

    /// 票 09：三类事件载荷缺字段时**不广播**（旧行为是回查内核补空值）
    ///
    /// 反向锁：谁把内核回查（或 `.unwrap_or_default()`）加回来，本用例转红——
    /// 判据不是「有没有回查代码」，而是**行为**：缺载荷时广播数必须为 0。
    #[tokio::test]
    async fn incomplete_session_payloads_are_not_broadcast() {
        let (handler, fake) = test_handler().await;

        handler
            .process_event(DesktopSyncEvent::SessionCreated {
                session_id: "s-c".to_string(),
                source_device: None,
                session: None,
            })
            .await;
        handler
            .process_event(DesktopSyncEvent::SessionStatusChanged {
                session_id: "s-s".to_string(),
                old_status: crate::enums::SessionStatus::Running,
                new_status: crate::enums::SessionStatus::Stopped,
                session_name: None,
            })
            .await;
        handler
            .process_event(DesktopSyncEvent::SessionStopped {
                session_id: "s-t".to_string(),
                source_device: None,
                session_name: None,
            })
            .await;

        let calls = fake.take_calls();
        assert!(
            calls.is_empty(),
            "载荷缺失必须 warn + 不广播（不得回查内核补空值），实际广播: {calls:?}"
        );
    }

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

    /// SessionModeChanged → 广播 SessionModeChanged
    #[tokio::test]
    async fn session_mode_changed_broadcasts() {
        let (handler, fake) = test_handler().await;
        handler
            .process_event(DesktopSyncEvent::SessionModeChanged {
                session_id: "s-1".to_string(),
                auto_approve: true,
            })
            .await;
        let calls = fake.take_calls();
        assert!(matches!(
            calls[0].payload,
            SyncPayload::SessionModeChanged { auto_approve: true, .. }
        ));
        assert!(calls[0].exclude_device.is_none());
    }

    /// TaskQueueChanged → 广播 TaskQueueChanged
    #[tokio::test]
    async fn task_queue_changed_broadcasts() {
        let (handler, fake) = test_handler().await;
        handler
            .process_event(DesktopSyncEvent::TaskQueueChanged {
                session_id: "s-1".to_string(),
                queue_count: 3,
                action: "add".to_string(),
                task_id: Some("t1".to_string()),
                status: Some("pending".to_string()),
            })
            .await;
        let calls = fake.take_calls();
        assert!(matches!(
            calls[0].payload,
            SyncPayload::TaskQueueChanged { queue_count: 3, .. }
        ));
    }

    /// TaskScheduledChanged → 广播 TaskScheduledChanged
    #[tokio::test]
    async fn task_scheduled_changed_broadcasts() {
        let (handler, fake) = test_handler().await;
        handler
            .process_event(DesktopSyncEvent::TaskScheduledChanged {
                job_id: "job-1".to_string(),
                status: "pending".to_string(),
                action: "create".to_string(),
            })
            .await;
        let calls = fake.take_calls();
        assert!(matches!(&calls[0].payload, SyncPayload::TaskScheduledChanged { job_id, .. } if job_id == "job-1"));
    }

    /// 票 12 降级口径（移动端受影响清单 M2）：插件未写任务字段 →
    /// 会话摘要的任务字段缺失，**wire 上不出现该键**（与迁移前恒 `None` 的形状等价）
    ///
    /// 票 09：任务字段随事件自携带（`summary()` 不带任务字段即此场景），
    /// 不再经内核注解槽取值——本用例守的是 **wire 形状**，与取值来源无关。
    #[tokio::test]
    async fn session_created_without_task_fields_omits_them_on_wire() {
        let (handler, fake) = test_handler().await;
        handler
            .process_event(DesktopSyncEvent::SessionCreated {
                session_id: "s-noslot".to_string(),
                source_device: None,
                session: Some(summary("s-noslot")),
            })
            .await;

        let calls = fake.take_calls();
        let SyncPayload::SessionCreated { session, .. } = &calls[0].payload else {
            panic!("期望 SessionCreated，实际: {:?}", calls[0].payload);
        };
        assert!(session.task_status.is_none(), "无任务字段 → taskStatus 缺失（M2 降级）");
        assert!(session.task_reason.is_none());
        let json = serde_json::to_value(&calls[0].payload).expect("serialize");
        let summary = &json["data"]["session"];
        assert!(
            summary.get("taskStatus").is_none() && summary.get("taskReason").is_none(),
            "wire 上不得出现任务字段键: {summary}"
        );
    }

    /// 票 12 取值口径：任务字段有值 → 会话摘要逐字段透出（来源=插件载荷，
    /// 字段名与形状不变）
    #[tokio::test]
    async fn session_created_carries_task_fields() {
        let (handler, fake) = test_handler().await;
        let mut carried = summary("s-slot");
        carried.task_status = Some("in_progress".to_string());
        carried.task_reason = Some("AI 会话".to_string());

        handler
            .process_event(DesktopSyncEvent::SessionCreated {
                session_id: "s-slot".to_string(),
                source_device: None,
                session: Some(carried),
            })
            .await;

        let calls = fake.take_calls();
        let SyncPayload::SessionCreated { session, .. } = &calls[0].payload else {
            panic!("期望 SessionCreated，实际: {:?}", calls[0].payload);
        };
        assert_eq!(session.task_status.as_deref(), Some("in_progress"));
        assert_eq!(session.task_reason.as_deref(), Some("AI 会话"));
    }
}
