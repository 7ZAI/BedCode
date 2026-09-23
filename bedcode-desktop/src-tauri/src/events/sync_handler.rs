//! Sync Event Handler
//!
//! 同步事件处理器，将 DesktopSyncEvent 转换为 SyncData WebSocket 消息并广播

use super::matcher::EventHandler;
use crate::enums::{SessionSummary, SyncPayload};
use crate::events::DesktopSyncEvent;
use crate::server::websocket::message::Message;
use crate::server::websocket::WebSocketManager;
use crate::session::SessionManager;
use std::sync::Arc;

/// 同步事件处理器
///
/// 将 DesktopSyncEvent 转换为 SyncData WebSocket 消息并广播给客户端
pub struct SyncEventHandler {
    session_manager: Arc<SessionManager>,
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
    pub fn new(session_manager: Arc<SessionManager>, ws_manager: &'static WebSocketManager) -> Self {
        let ws_manager: &'static (dyn SyncBroadcaster + Send + Sync) = ws_manager;
        Self {
            session_manager,
            ws_manager,
        }
    }

    /// 异步处理事件
    async fn process_event(&self, event: DesktopSyncEvent) {
        tracing::info!("[SyncEventHandler] Processing event: {:?}", event);
        match event {
            DesktopSyncEvent::SessionCreated {
                session_id,
                source_device,
            } => {
                self.handle_session_created(&session_id, source_device).await;
            }
            DesktopSyncEvent::SessionStatusChanged {
                session_id,
                old_status,
                new_status,
            } => {
                self.handle_session_status_changed(&session_id, old_status, new_status)
                    .await;
            }
            DesktopSyncEvent::SessionStopped {
                session_id,
                source_device,
            } => {
                self.handle_session_stopped(&session_id, source_device).await;
            }
            DesktopSyncEvent::SessionRemoved {
                session_id,
                source_device,
            } => {
                self.handle_session_removed(&session_id, source_device).await;
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
    async fn handle_session_created(&self, session_id: &str, source_device: Option<String>) {
        // 获取会话信息
        let Some(session_info) = self.session_manager.get_session(session_id).await else {
            tracing::warn!(session_id = %session_id, "[SyncEventHandler] Session not found");
            return;
        };

        // 构建 SessionSummary（票 12：任务字段取自注解槽，不再来自会话记录）
        let annotations = self.session_manager.session_annotations(session_id).await;
        let (task_status, task_reason, _, _) = crate::session::task_fields_from_slot(&annotations);
        let session = SessionSummary {
            id: session_info.id,
            name: session_info.name,
            status: format!("{:?}", session_info.status).to_lowercase(),
            created_at: session_info.created_at.to_rfc3339(),
            started_at: session_info.started_at.map(|t| t.to_rfc3339()),
            session_type: Some(format!("{:?}", session_info.session_type).to_lowercase()),
            config_id: Some(session_info.config_id),
            task_status,
            task_reason,
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
    ) {
        // 获取会话名称
        let session_name = self
            .session_manager
            .get_session(session_id)
            .await
            .map(|s| s.name)
            .unwrap_or_default();

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
    async fn handle_session_stopped(&self, session_id: &str, source_device: Option<String>) {
        // 获取会话名称
        let session_name = self
            .session_manager
            .get_session(session_id)
            .await
            .map(|s| s.name)
            .unwrap_or_default();

        // 构建同步载荷
        let payload = SyncPayload::SessionStopped {
            session_id: session_id.to_string(),
            session_name,
        };

        // 广播消息
        self.broadcast_sync_data(payload, source_device.as_deref()).await;
    }

    /// 处理会话删除事件
    async fn handle_session_removed(&self, session_id: &str, source_device: Option<String>) {
        // 注意：此时会话可能已从 SessionManager 移除，session_name 可能为空
        // 调用方应在移除前获取名称

        // 构建同步载荷
        let payload = SyncPayload::SessionRemoved {
            session_id: session_id.to_string(),
            session_name: String::new(), // 已删除，名称不可用
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
        let session_manager = self.session_manager.clone();
        let ws_manager = self.ws_manager;

        tokio::spawn(async move {
            let handler = SyncEventHandler {
                session_manager,
                ws_manager,
            };
            handler.process_event(event).await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::DesktopSyncEvent;
    use crate::server::websocket::message::Message;
    use std::sync::Mutex;

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

    /// 构造 handler：cm 与 sm 共用同一 db（config 落库后两处可见）；Fake 泄漏为 &'static
    async fn test_handler() -> (Arc<SyncEventHandler>, &'static FakeBroadcaster) {
        let sm = Arc::new(SessionManager::new());
        let fake_static: &'static FakeBroadcaster = Box::leak(Box::new(FakeBroadcaster::new()));
        let ws: &'static (dyn SyncBroadcaster + Send + Sync) = fake_static;
        let handler = Arc::new(SyncEventHandler {
            session_manager: sm,
            ws_manager: ws,
        });
        (handler, fake_static)
    }

    /// 预置一个会话（经执行端 `create_session_from_spec(start=false)`，不 spawn PTY）
    ///
    /// 宿主侧创建编排已随 host-business-decarriage 收尾下沉插件，测试直接注入
    /// 插件会算好的 launch spec（命名 / config→launch 映射属插件决策）。
    async fn seed_session(sm: &SessionManager) -> String {
        use crate::enums::ExecutionEnvironment;
        sm.create_session_from_spec(
            crate::enums::SessionLaunchConfig {
                name: "itest-sync".to_string(),
                environment: ExecutionEnvironment::Linux,
                working_dir: "/tmp".to_string(),
                command: "bash".to_string(),
                command_args: vec!["bash".to_string()],
                env_vars: std::collections::HashMap::new(),
                cols: 120,
                rows: 40,
            },
            "itest-sync".to_string(),
            None,
            false,
            None,
            None,
        )
        .await
        .expect("create session")
    }

    /// SessionCreated → 广播 SyncPayload::SessionCreated（票据 22）
    #[tokio::test]
    async fn session_created_event_broadcasts_session_created() {
        let (handler, fake) = test_handler().await;
        let sid = seed_session(&handler.session_manager).await;
        handler
            .process_event(DesktopSyncEvent::SessionCreated {
                session_id: sid.clone(),
                source_device: Some("d1".to_string()),
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
        let sid = seed_session(&handler.session_manager).await;
        handler
            .process_event(DesktopSyncEvent::SessionStopped {
                session_id: sid.clone(),
                source_device: Some("d2".to_string()),
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
    #[tokio::test]
    async fn session_removed_event_broadcasts_session_removed() {
        let (handler, fake) = test_handler().await;
        let sid = seed_session(&handler.session_manager).await;
        handler
            .process_event(DesktopSyncEvent::SessionRemoved {
                session_id: sid.clone(),
                source_device: Some("d3".to_string()),
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
            })
            .await;
        let calls = fake.take_calls();
        assert!(matches!(calls[0].payload, SyncPayload::SessionStatusChanged { .. }));
        assert!(calls[0].exclude_device.is_none(), "状态变更应全量广播");
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

    /// 未知会话的 SessionCreated：不广播（会话不存在 → 直接返回）
    #[tokio::test]
    async fn session_created_for_unknown_session_skips_broadcast() {
        let (handler, fake) = test_handler().await;
        handler
            .process_event(DesktopSyncEvent::SessionCreated {
                session_id: "ghost".to_string(),
                source_device: None,
            })
            .await;
        assert!(fake.take_calls().is_empty(), "会话不存在不应广播");
    }

    /// 票 12 降级口径（移动端受影响清单 M2）：注解槽无人写（插件未激活 / 未写槽）→
    /// 会话摘要的任务字段缺失，**wire 上不出现该键**（与迁移前恒 `None` 的形状等价）
    #[tokio::test]
    async fn session_created_without_slot_omits_task_fields() {
        let (handler, fake) = test_handler().await;
        let sid = seed_session(&handler.session_manager).await;

        handler
            .process_event(DesktopSyncEvent::SessionCreated {
                session_id: sid.clone(),
                source_device: None,
            })
            .await;

        let calls = fake.take_calls();
        let SyncPayload::SessionCreated { session, .. } = &calls[0].payload else {
            panic!("期望 SessionCreated，实际: {:?}", calls[0].payload);
        };
        assert!(session.task_status.is_none(), "空槽 → taskStatus 缺失（M2 降级）");
        assert!(session.task_reason.is_none());
        let json = serde_json::to_value(&calls[0].payload).expect("serialize");
        let summary = &json["data"]["session"];
        assert!(
            summary.get("taskStatus").is_none() && summary.get("taskReason").is_none(),
            "wire 上不得出现任务字段键: {summary}"
        );
    }

    /// 票 12 取值口径：槽有值 → 会话摘要逐字段透出（同步事件构造点改从槽取值，
    /// 字段名与形状不变）
    #[tokio::test]
    async fn session_created_carries_task_fields_from_slot() {
        let (handler, fake) = test_handler().await;
        let sid = seed_session(&handler.session_manager).await;
        assert!(
            handler
                .session_manager
                .annotate_session(&sid, "taskStatus", "in_progress")
                .await
        );
        assert!(
            handler
                .session_manager
                .annotate_session(&sid, "taskReason", "AI 会话")
                .await
        );

        handler
            .process_event(DesktopSyncEvent::SessionCreated {
                session_id: sid.clone(),
                source_device: None,
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
