//! 插件事件帧路由（移动端适配专项票 03：`session-control` 常驻事件通道）
//!
//! ## 为什么是独立模块（替代 `handler/sync.rs`）
//!
//! 桌面端 WS 业务硬切（ABI v28）后 `/ws/event` 端点删除，业务事件改由
//! `com.bedcode.terminal-session` 在声明端点 `session-control` 上广播，帧壳
//! `{"type":"event","event":"<name>","payload":{...}}`（票 02 定稿）。旧链路
//! （`Message::SyncData` 信封 → `SyncHandler`）随之失源——本模块把插件事件帧
//! 翻译成既有 [`MobileEvent`] 变体，**前端 `ws_sync_*` 事件名与 store 收敛
//! 逻辑零改动**（转发层 `router/event.rs` 照常 emit）。
//!
//! ## 契约（spec §6.2）
//!
//! - **只读**：本连接只收事件，不发任何动作帧（动作走 HTTP，票 04）；
//! - **认证前的帧一律丢弃**（防御畸形服务端）：认证标记由
//!   [`PluginEventRouter::mark_authenticated`] 在 auth 首帧发出后置位；
//! - 未知事件名 / 缺字段 / 畸形载荷 → `debug` 留痕丢弃，**不 panic、不断连**
//!   （前进式演进：老端忽略未知字段）；
//! - **不做发送端过滤**：`source_device` 照收，本机回声由消费端幂等吸收
//!   （列表按 id 去重 / 状态收敛，见 `useMobileConnection` 的同步回调）；
//! - **事件不重放**：断连期间的事件永久丢失，重连后由前端对账补齐（Rust 侧
//!   在通道就绪时 emit `ws_event_channel_ready`，不对账即「永远差一点」）。

use std::sync::atomic::{AtomicBool, Ordering};

use serde::Deserialize;
use serde_json::Value;
use tokio::sync::broadcast;

use crate::enums::plugin::PluginQuestion;
use crate::enums::SessionSummary;
use crate::router::MobileEvent;

// ==================== 帧壳与事件名（桌面 wire，逐字不可漂移） ====================

/// 事件帧 `type`（票 02 帧壳：`{"type":"event",...}`）
pub const FRAME_TYPE_EVENT: &str = "event";

/// `session:created`
pub const EVENT_SESSION_CREATED: &str = "session:created";
/// `session:stopped`
pub const EVENT_SESSION_STOPPED: &str = "session:stopped";
/// `session:removed`
pub const EVENT_SESSION_REMOVED: &str = "session:removed";
/// `task:status-changed`
pub const EVENT_TASK_STATUS_CHANGED: &str = "task:status-changed";
/// `session:mode-changed`
pub const EVENT_SESSION_MODE_CHANGED: &str = "session:mode-changed";
/// `task:queue-changed`
pub const EVENT_TASK_QUEUE_CHANGED: &str = "task:queue-changed";
/// `task:scheduled-changed`
pub const EVENT_TASK_SCHEDULED_CHANGED: &str = "task:scheduled-changed";

// ==================== 载荷形状（snake_case；未知字段忽略，缺字段丢弃） ====================

#[derive(Deserialize)]
struct CreatedPayload {
    session: SessionSummary,
    #[serde(default)]
    source_device: String,
}

#[derive(Deserialize)]
struct SessionIdNamePayload {
    session_id: String,
    #[serde(default)]
    session_name: String,
}

#[derive(Deserialize)]
struct TaskStatusPayload {
    session_id: String,
    task_status: String,
    #[serde(default)]
    task_reason: Option<String>,
    #[serde(default)]
    task_questions: Option<Vec<PluginQuestion>>,
}

#[derive(Deserialize)]
struct ModePayload {
    session_id: String,
    #[serde(default)]
    auto_approve: bool,
}

#[derive(Deserialize)]
struct QueuePayload {
    session_id: String,
    queue_count: i64,
    action: String,
    #[serde(default)]
    task_id: Option<String>,
    #[serde(default)]
    status: Option<String>,
}

#[derive(Deserialize)]
struct ScheduledPayload {
    job_id: String,
    status: String,
    action: String,
}

/// 纯函数：事件名 + 载荷 → `MobileEvent`（未知事件 / 畸形载荷 → `None`）
///
/// 映射表逐条对应 spec §3.2（7 类事件 → 既有变体，前端事件名不变）。
pub fn to_mobile_event(event_name: &str, payload: &Value) -> Option<MobileEvent> {
    match event_name {
        EVENT_SESSION_CREATED => {
            let p: CreatedPayload = serde_json::from_value(payload.clone()).ok()?;
            Some(MobileEvent::SyncSessionCreated {
                session: p.session,
                source_device: p.source_device,
            })
        }
        EVENT_SESSION_STOPPED => {
            let p: SessionIdNamePayload = serde_json::from_value(payload.clone()).ok()?;
            Some(MobileEvent::SyncSessionStopped {
                session_id: p.session_id,
                session_name: p.session_name,
            })
        }
        EVENT_SESSION_REMOVED => {
            let p: SessionIdNamePayload = serde_json::from_value(payload.clone()).ok()?;
            Some(MobileEvent::SyncSessionRemoved {
                session_id: p.session_id,
                session_name: p.session_name,
            })
        }
        EVENT_TASK_STATUS_CHANGED => {
            let p: TaskStatusPayload = serde_json::from_value(payload.clone()).ok()?;
            Some(MobileEvent::SyncTaskStatusChanged {
                session_id: p.session_id,
                task_status: p.task_status,
                task_reason: p.task_reason,
                task_questions: p.task_questions,
            })
        }
        EVENT_SESSION_MODE_CHANGED => {
            let p: ModePayload = serde_json::from_value(payload.clone()).ok()?;
            Some(MobileEvent::SyncSessionModeChanged {
                session_id: p.session_id,
                auto_approve: p.auto_approve,
            })
        }
        EVENT_TASK_QUEUE_CHANGED => {
            let p: QueuePayload = serde_json::from_value(payload.clone()).ok()?;
            Some(MobileEvent::SyncTaskQueueChanged {
                session_id: p.session_id,
                queue_count: p.queue_count,
                action: p.action,
                task_id: p.task_id,
                status: p.status,
            })
        }
        EVENT_TASK_SCHEDULED_CHANGED => {
            let p: ScheduledPayload = serde_json::from_value(payload.clone()).ok()?;
            Some(MobileEvent::SyncTaskScheduledChanged {
                job_id: p.job_id,
                status: p.status,
                action: p.action,
            })
        }
        _ => None,
    }
}

// ==================== 事件路由（连接级状态：认证门） ====================

/// 插件事件帧路由（每连接一个实例）
///
/// 挂在 WS 收帧链路上（见 `connection/default_handler.rs`）：事件帧在此闭环，
/// 非事件帧返回 `false` 交回旧信封链路（票 04 退役该链路）。
pub struct PluginEventRouter {
    event_tx: broadcast::Sender<MobileEvent>,
    /// 认证门：auth 首帧发出前到达的帧一律丢弃（防御畸形服务端）
    authenticated: AtomicBool,
}

impl PluginEventRouter {
    /// 建路由（事件经此 `event_tx` 广播，由 `router/event.rs` 转发前端）
    pub fn new(event_tx: broadcast::Sender<MobileEvent>) -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self {
            event_tx,
            authenticated: AtomicBool::new(false),
        })
    }

    /// 置位认证门（auth 首帧发出成功后调用）
    pub fn mark_authenticated(&self) {
        self.authenticated.store(true, Ordering::SeqCst);
    }

    /// 处理一条文本帧
    ///
    /// 返回 `true` = 该帧是事件帧（已路由或已按契约丢弃），调用方**不得**再交给
    /// 旧信封链路；返回 `false` = 非事件帧（非 JSON / 其它 type），交回旧链路。
    pub fn route_text(&self, text: &str) -> bool {
        let Ok(frame) = serde_json::from_str::<Value>(text) else {
            return false;
        };
        if frame.get("type").and_then(|v| v.as_str()) != Some(FRAME_TYPE_EVENT) {
            return false;
        }
        if !self.authenticated.load(Ordering::SeqCst) {
            tracing::debug!("[PluginEvent] drop event frame received before auth");
            return true;
        }
        let Some(event_name) = frame.get("event").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) else {
            tracing::debug!("[PluginEvent] drop event frame without 'event' name");
            return true;
        };
        let payload = frame.get("payload").cloned().unwrap_or(Value::Null);
        match to_mobile_event(event_name, &payload) {
            Some(event) => self.emit(event_name, event),
            None => tracing::debug!(
                "[PluginEvent] drop unknown/malformed event: name={} (forward-compatible)",
                event_name
            ),
        }
        true
    }

    fn emit(&self, event_name: &str, event: MobileEvent) {
        tracing::info!("[PluginEvent] routed event: {}", event_name);
        if let Err(e) = self.event_tx.send(event) {
            // 无订阅者（转发层未启动）：不是致命错误，留痕即可——事件是旁路，
            // 真源在 HTTP 回包与对账（spec §6.2）
            tracing::debug!("[PluginEvent] no receiver for event {}: {}", event_name, e);
        }
    }
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::constants::connection::BROADCAST_CHANNEL_CAPACITY;

    fn router() -> (std::sync::Arc<PluginEventRouter>, broadcast::Receiver<MobileEvent>) {
        let (tx, rx) = broadcast::channel(BROADCAST_CHANNEL_CAPACITY);
        (PluginEventRouter::new(tx), rx)
    }

    /// 事件帧（与票 01 夹具 `event_frame` 同形：`{"type":"event","event","payload"}`）
    fn frame(event: &str, payload: Value) -> String {
        serde_json::json!({ "type": FRAME_TYPE_EVENT, "event": event, "payload": payload }).to_string()
    }

    fn summary() -> Value {
        serde_json::json!({
            "id": "s1",
            "name": "dev",
            "status": "running",
            "created_at": "2026-09-26T00:00:00Z",
            "config_id": "cfg-1",
        })
    }

    async fn routed(rx: &mut broadcast::Receiver<MobileEvent>) -> MobileEvent {
        match tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv()).await {
            Ok(Ok(ev)) => ev,
            Ok(Err(e)) => panic!("event channel error: {e}"),
            Err(_) => panic!("timed out waiting for MobileEvent"),
        }
    }

    // ---------- 7 事件映射 ----------

    #[tokio::test]
    async fn session_created_maps_to_sync_variant() {
        let (r, mut rx) = router();
        r.mark_authenticated();
        assert!(r.route_text(&frame(
            EVENT_SESSION_CREATED,
            serde_json::json!({ "session": summary(), "source_device": "phone" })
        )));
        match routed(&mut rx).await {
            MobileEvent::SyncSessionCreated { session, source_device } => {
                assert_eq!(session.id, "s1");
                assert_eq!(session.status, "running");
                assert_eq!(source_device, "phone");
            }
            other => panic!("expected SyncSessionCreated, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn session_stopped_and_removed_map_to_sync_variants() {
        let (r, mut rx) = router();
        r.mark_authenticated();
        assert!(r.route_text(&frame(
            EVENT_SESSION_STOPPED,
            serde_json::json!({ "session_id": "s1", "session_name": "dev" })
        )));
        match routed(&mut rx).await {
            MobileEvent::SyncSessionStopped {
                session_id,
                session_name,
            } => {
                assert_eq!(session_id, "s1");
                assert_eq!(session_name, "dev");
            }
            other => panic!("expected SyncSessionStopped, got {other:?}"),
        }

        assert!(r.route_text(&frame(
            EVENT_SESSION_REMOVED,
            serde_json::json!({ "session_id": "s2", "session_name": "itest" })
        )));
        match routed(&mut rx).await {
            MobileEvent::SyncSessionRemoved {
                session_id,
                session_name,
            } => {
                assert_eq!(session_id, "s2");
                assert_eq!(session_name, "itest");
            }
            other => panic!("expected SyncSessionRemoved, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn task_status_changed_carries_reason_and_questions() {
        let (r, mut rx) = router();
        r.mark_authenticated();
        let questions = serde_json::json!([
            { "header": "Confirm", "question": "continue?", "multi_select": false,
              "options": [{ "label": "yes", "description": "" }] }
        ]);
        assert!(r.route_text(&frame(
            EVENT_TASK_STATUS_CHANGED,
            serde_json::json!({
                "session_id": "s1",
                "task_status": "waiting_input",
                "task_reason": "needs approval",
                "task_questions": questions,
            })
        )));
        match routed(&mut rx).await {
            MobileEvent::SyncTaskStatusChanged {
                session_id,
                task_status,
                task_reason,
                task_questions,
            } => {
                assert_eq!(session_id, "s1");
                assert_eq!(task_status, "waiting_input");
                assert_eq!(task_reason.as_deref(), Some("needs approval"));
                assert_eq!(task_questions.map(|q| q.len()), Some(1));
            }
            other => panic!("expected SyncTaskStatusChanged, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn task_status_changed_without_optional_fields_is_accepted() {
        let (r, mut rx) = router();
        r.mark_authenticated();
        // 票 02 载荷纪律：可选字段缺席即不出现键（不伪造 null）
        assert!(r.route_text(&frame(
            EVENT_TASK_STATUS_CHANGED,
            serde_json::json!({ "session_id": "s1", "task_status": "completed" })
        )));
        match routed(&mut rx).await {
            MobileEvent::SyncTaskStatusChanged {
                task_reason,
                task_questions,
                ..
            } => {
                assert!(task_reason.is_none());
                assert!(task_questions.is_none());
            }
            other => panic!("expected SyncTaskStatusChanged, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn mode_queue_scheduled_map_to_sync_variants() {
        let (r, mut rx) = router();
        r.mark_authenticated();

        assert!(r.route_text(&frame(
            EVENT_SESSION_MODE_CHANGED,
            serde_json::json!({ "session_id": "s1", "auto_approve": true, "auto_execute": false })
        )));
        match routed(&mut rx).await {
            MobileEvent::SyncSessionModeChanged {
                session_id,
                auto_approve,
            } => {
                assert_eq!(session_id, "s1");
                assert!(auto_approve, "auto_approve 取桌面 auto_approve 字段");
            }
            other => panic!("expected SyncSessionModeChanged, got {other:?}"),
        }

        assert!(r.route_text(&frame(
            EVENT_TASK_QUEUE_CHANGED,
            serde_json::json!({
                "session_id": "s1", "queue_count": 3, "action": "add",
                "task_id": null, "status": null,
            })
        )));
        match routed(&mut rx).await {
            MobileEvent::SyncTaskQueueChanged {
                session_id,
                queue_count,
                action,
                task_id,
                status,
            } => {
                assert_eq!(session_id, "s1");
                assert_eq!(queue_count, 3);
                assert_eq!(action, "add");
                assert!(task_id.is_none() && status.is_none());
            }
            other => panic!("expected SyncTaskQueueChanged, got {other:?}"),
        }

        assert!(r.route_text(&frame(
            EVENT_TASK_SCHEDULED_CHANGED,
            serde_json::json!({ "job_id": "j1", "status": "executed", "action": "trigger" })
        )));
        match routed(&mut rx).await {
            MobileEvent::SyncTaskScheduledChanged { job_id, status, action } => {
                assert_eq!(job_id, "j1");
                assert_eq!(status, "executed");
                assert_eq!(action, "trigger");
            }
            other => panic!("expected SyncTaskScheduledChanged, got {other:?}"),
        }
    }

    // ---------- 反例 / 边界 / 异常 ----------

    #[tokio::test]
    async fn unknown_event_is_dropped_without_disconnect() {
        let (r, mut rx) = router();
        r.mark_authenticated();
        // 未知事件名：按契约丢弃（前进式演进），但帧已按事件帧处理（true）
        assert!(r.route_text(&frame("device:connected", serde_json::json!({}))));
        // 超时未收到事件 = 已丢弃
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), rx.recv())
                .await
                .is_err(),
            "未知事件不得产出 MobileEvent"
        );
    }

    #[tokio::test]
    async fn malformed_payload_is_dropped_not_panicking() {
        let (r, mut rx) = router();
        r.mark_authenticated();
        // 缺必填字段（session_id / job_id）→ 丢弃
        assert!(r.route_text(&frame(
            EVENT_SESSION_STOPPED,
            serde_json::json!({ "session_name": "x" })
        )));
        assert!(r.route_text(&frame(
            EVENT_TASK_SCHEDULED_CHANGED,
            serde_json::json!({ "job_id": "j1" })
        )));
        // 载荷类型错误（对象给了数组）→ 丢弃，不 panic
        assert!(r.route_text(&frame(EVENT_TASK_QUEUE_CHANGED, serde_json::json!([]))));
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), rx.recv())
                .await
                .is_err(),
            "畸形载荷不得产出 MobileEvent"
        );
    }

    #[tokio::test]
    async fn frame_before_auth_is_dropped() {
        let (r, mut rx) = router();
        // 未置位认证门：事件帧照旧被识别（true）但不路由
        assert!(r.route_text(&frame(
            EVENT_SESSION_CREATED,
            serde_json::json!({ "session": summary(), "source_device": "" })
        )));
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), rx.recv())
                .await
                .is_err(),
            "认证前的帧必须丢弃（防御畸形服务端）"
        );

        // 置位后同帧即可路由（门是一次性置位，不是每帧判定）
        r.mark_authenticated();
        assert!(r.route_text(&frame(
            EVENT_SESSION_CREATED,
            serde_json::json!({ "session": summary(), "source_device": "" })
        )));
        assert!(matches!(routed(&mut rx).await, MobileEvent::SyncSessionCreated { .. }));
    }

    #[tokio::test]
    async fn non_event_frames_fall_through_to_legacy_path() {
        let (r, _rx) = router();
        r.mark_authenticated();
        // 非 JSON / 其它 type：交回旧信封链路（票 04 退役前仍存在）
        assert!(!r.route_text("not json at all"));
        assert!(!r.route_text("{\"type\":\"session_list\"}"));
        assert!(!r.route_text("{\"type\":\"error\",\"message\":\"boom\"}"));
    }

    #[tokio::test]
    async fn repeated_event_is_emitted_and_left_to_consumer_idempotence() {
        // 幂等语义在消费端（列表按 id 去重 / 状态收敛）：传输层不吞重复帧——
        // 「同一事件重复到达」是重连对账与回声的常态，吞帧会让状态收敛失去输入
        let (r, mut rx) = router();
        r.mark_authenticated();
        let payload = serde_json::json!({ "session": summary(), "source_device": "phone" });
        assert!(r.route_text(&frame(EVENT_SESSION_CREATED, payload.clone())));
        assert!(r.route_text(&frame(EVENT_SESSION_CREATED, payload)));
        assert!(matches!(routed(&mut rx).await, MobileEvent::SyncSessionCreated { .. }));
        assert!(matches!(routed(&mut rx).await, MobileEvent::SyncSessionCreated { .. }));
    }

    /// 结构锁：事件路由实现段不得出现 `Message::`（信封在 WS 生产路径零使用）
    ///
    /// 票 03 起事件帧走本模块；若有人把事件塞回 `Message` 枚举，等于又把
    /// 已退役的信封协议接回生产路径（票 04 正是要删掉它）。
    #[test]
    fn event_routing_does_not_use_legacy_message_enum() {
        let root = env!("CARGO_MANIFEST_DIR");
        for file in ["src/handler/plugin_event.rs", "src/connection/event_ws.rs"] {
            let path = format!("{root}/{file}");
            let src = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
            // 只扫实现段（测试段的结构锁字面量会自匹配）
            let impl_part = src.split("\n#[cfg(test)]").next().unwrap_or(&src);
            for (idx, raw) in impl_part.lines().enumerate() {
                let line = raw.trim_start();
                if line.starts_with("//") {
                    continue;
                }
                assert!(
                    !line.contains("Message::"),
                    "{file}:{} 事件路由实现段不得出现 `Message::`（信封已退役）: {}",
                    idx + 1,
                    line.trim()
                );
            }
        }
    }

    /// 结构锁：7 事件名与 spec §3.1 表逐字一致（含桌面插件 `ws_events.rs` 同源）
    #[test]
    fn event_names_match_desktop_wire() {
        assert_eq!(EVENT_SESSION_CREATED, "session:created");
        assert_eq!(EVENT_SESSION_STOPPED, "session:stopped");
        assert_eq!(EVENT_SESSION_REMOVED, "session:removed");
        assert_eq!(EVENT_TASK_STATUS_CHANGED, "task:status-changed");
        assert_eq!(EVENT_SESSION_MODE_CHANGED, "session:mode-changed");
        assert_eq!(EVENT_TASK_QUEUE_CHANGED, "task:queue-changed");
        assert_eq!(EVENT_TASK_SCHEDULED_CHANGED, "task:scheduled-changed");
        assert_eq!(FRAME_TYPE_EVENT, "event");
    }
}
