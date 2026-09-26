//! WS 业务事件广播出口（移动端适配专项 `.scratch/2026-09-26-mobile-desktop-adaptation` 票 02）
//!
//! ## 为什么需要本模块
//!
//! 桌面端 WS 业务硬切（2026-09-25，ABI v28）后 `/ws/event` 端点删除，旧由宿主
//! `broadcast-sync` 广播给移动端的业务事件整条链路失源：宿主不再解释「推什么、
//! 推给谁」，而 `ws_broadcast_text` 这套服务端原语在 `wasm-apps/**` 内零调用。
//! 本模块补上这一环——**插件在既有事件收口点向声明端点 `session-control` 的
//! 全体客户端广播事件帧**，移动端据此恢复 `ws_sync_*` 事件面。
//!
//! ## 帧协议（唯一出口）
//!
//! ```json
//! {"type":"event","event":"<name>","payload":{...}}
//! ```
//!
//! - 事件名取 SDK `constants::EVENT_*`，与 bus / emit 两通道**逐字同名**
//!   （广播是第三通道，不引入第三套事件名）；
//! - 载荷取 **bus 形（snake_case）**、自足、不含凭据（移动端 `Message` 信封
//!   退役后按本帧直接映射 `MobileEvent`）。
//!
//! ## 端点反查与零客户端早退
//!
//! 端点句柄由宿主登记（插件侧不可预测），故经 `ws_list_endpoints()`
//! （`[{endpointId, path, clientCount}]`）建 path → 条目缓存：
//! - `clientCount == 0` 直接返回（**无客户端路径零宿主调用**、不计错、不打 warn）；
//! - 缓存在 `ws:client-connect` / `ws:client-disconnect` 时失效（见
//!   [`invalidate_endpoint_cache`]，接线在 `lib.rs::on_message`）——否则会出现
//!   「客户端已接入但缓存仍记 0」的永久静默。
//!
//! ## 失败口径（事件是旁路）
//!
//! 业务真源在插件私有库与 HTTP 回包，广播只是旁路通知：
//! `ws_broadcast_text` 返回成功数（**0 合法**），`Err`（端点缺失 / 权限 /
//! 已回收）只 `log_warn` 留痕，**绝不向调用方传播**——广播失败不得让会话创建
//! 或任务推进失败。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use bedcode_plugin_api::wasm_host::WasmHost;
#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::host::{HostLog, HostWebsocket};

/// 广播目标端点路径（manifest `contributes.wsEndpoints` 声明的**既有**端点，
/// 本票不新增端点）
pub const SESSION_CONTROL_PATH: &str = "session-control";

/// `ws_list_endpoints()` 条目的解析形（path → 条目）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointEntry {
    /// 宿主登记的端点句柄（`wse-` 前缀，插件不可预测）
    pub id: String,
    /// 该端点当前在线客户端数（零客户端早退判据）
    pub client_count: u64,
}

/// path → 端点条目缓存（wasm 实例级静态；native 下恒空）
static ENDPOINT_CACHE: OnceLock<Mutex<HashMap<String, EndpointEntry>>> = OnceLock::new();

// ==================== 纯函数（native 可测，无宿主调用） ====================

/// 解析 `ws_list_endpoints()` 返回串 → path → 条目
///
/// 畸形 / 非数组 / 缺字段的条目**整体跳过**（不 panic、不记半截缓存）；
/// `clientCount` 缺失按 0 处理（零客户端 = 静默，宁可不推也不瞎推）。
pub fn parse_endpoints(listed: &str) -> HashMap<String, EndpointEntry> {
    let mut map = HashMap::new();
    let entries = match serde_json::from_str::<serde_json::Value>(listed) {
        Ok(serde_json::Value::Array(entries)) => entries,
        _ => return map,
    };
    for entry in entries {
        let (Some(id), Some(path)) = (
            entry.get("endpointId").and_then(|v| v.as_str()),
            entry.get("path").and_then(|v| v.as_str()),
        ) else {
            continue;
        };
        map.insert(
            path.to_string(),
            EndpointEntry {
                id: id.to_string(),
                client_count: entry.get("clientCount").and_then(|v| v.as_u64()).unwrap_or(0),
            },
        );
    }
    map
}

/// 纯函数：从端点清单串反查 path 对应条目（无该端点 / 畸形清单 → `None`）
pub fn endpoint_entry_from_listing(listed: &str, path: &str) -> Option<EndpointEntry> {
    parse_endpoints(listed).remove(path)
}

/// 纯函数：广播目标裁决（零客户端早退判据）
///
/// `None` = 不广播（无端点 / 零客户端），调用方**静默返回**；
/// `Some(id)` = 向该端点广播。
pub fn broadcast_target(entry: Option<&EndpointEntry>) -> Option<&str> {
    match entry {
        Some(entry) if entry.client_count > 0 => Some(entry.id.as_str()),
        _ => None,
    }
}

/// 事件帧文本（唯一帧壳出处）
///
/// `{"type":"event","event":"<name>","payload":{...}}`——payload 恒为对象
/// （缺数据也以显式字段表达，不 flatten 到顶层，避免移动端与动作回包混淆）。
pub fn event_frame(event_name: &str, payload: &serde_json::Value) -> String {
    serde_json::json!({
        "type": "event",
        "event": event_name,
        "payload": payload,
    })
    .to_string()
}

/// 纯函数：广播结果 → 留痕文本（`None` = 静默）
///
/// `Ok(_)`（含 `Ok(0)`：无客户端 / 队列满由宿主按计数语义表达，**不计错**）
/// 一律静默；`Err` 只产留痕文本，由调用方 `log_warn` 后继续——事件是旁路，
/// 失败不向业务路径传播。
pub fn delivery_note(result: &Result<u32, String>, event_name: &str) -> Option<String> {
    match result {
        Ok(_) => None,
        Err(e) => Some(format!(
            "ws event broadcast failed (event={event_name}): {e}"
        )),
    }
}

/// `task:status-changed` 载荷：bus 形 + 可选 reason / questions（单点收口）
///
/// 可选字段**缺席即不出现键**（不伪造空值 / 不写 null）；空串 reason 同样缺席
/// （与注解槽 `publish_task_slots` 的过滤口径一致）。
pub fn task_status_payload(
    session_id: &str,
    status: &str,
    reason: Option<&str>,
    questions: Option<&serde_json::Value>,
) -> serde_json::Value {
    let mut payload = serde_json::json!({
        "session_id": session_id,
        "task_status": status,
    });
    if let Some(reason) = reason.filter(|r| !r.is_empty()) {
        payload["task_reason"] = serde_json::json!(reason);
    }
    if let Some(questions) = questions.filter(|q| !q.is_null()) {
        payload["task_questions"] = questions.clone();
    }
    payload
}

/// `session:mode-changed` 载荷：bus 形（两个开关，无 camelCase 兼容键）
pub fn session_mode_payload(session_id: &str, auto_approve: bool, auto_execute: bool) -> serde_json::Value {
    serde_json::json!({
        "session_id": session_id,
        "auto_approve": auto_approve,
        "auto_execute": auto_execute,
    })
}

/// `task:queue-changed` 载荷：与 bus / emit 逐字同形（可选字段为 `null`）
pub fn queue_changed_payload(
    session_id: &str,
    queue_count: i64,
    action: &str,
    task_id: Option<&str>,
    status: Option<&str>,
) -> serde_json::Value {
    serde_json::json!({
        "session_id": session_id,
        "queue_count": queue_count,
        "action": action,
        "task_id": task_id,
        "status": status,
    })
}

/// `task:scheduled-changed` 载荷：与 bus / emit 逐字同形
pub fn scheduled_changed_payload(job_id: &str, status: &str, action: &str) -> serde_json::Value {
    serde_json::json!({
        "job_id": job_id,
        "status": status,
        "action": action,
    })
}

// ==================== wasm：宿主调用（端点反查 + 广播） ====================

/// 端点反查：path → endpointId（带缓存与零客户端早退）
///
/// `None` = 无该端点 / 零客户端 / 宿主原语不可用（调用方静默返回）。
#[cfg(target_arch = "wasm32")]
pub fn endpoint_id_for_path(host: &WasmHost, path: &str) -> Option<String> {
    let cache = ENDPOINT_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let cached = cache
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(path)
        .cloned();
    let entry = match cached {
        Some(entry) => Some(entry),
        None => {
            let listed = host.ws_list_endpoints().ok()?;
            let mut guard = cache.lock().unwrap_or_else(|e| e.into_inner());
            *guard = parse_endpoints(&listed);
            guard.get(path).cloned()
        }
    };
    broadcast_target(entry.as_ref()).map(|id| id.to_string())
}

/// native：宿主原语不可用（wit import 在 native 无实现）——恒 `None`，
/// 由 [`broadcast_event`] 的 native 空实现兜底，测试不得依赖真实调用
#[cfg(not(target_arch = "wasm32"))]
pub fn endpoint_id_for_path(_host: &WasmHost, _path: &str) -> Option<String> {
    None
}

/// 缓存失效（客户端接入 / 断开时调用）
///
/// 缓存里的 `clientCount` 是快照：客户端数变化后必须失效，否则「缓存记 0
/// 但客户端已连上」会让后续事件永久静默（断链不可自愈）。
pub fn invalidate_endpoint_cache() {
    if let Some(cache) = ENDPOINT_CACHE.get() {
        cache.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }
}

/// 向 `session-control` 端点全体客户端广播业务事件（**唯一广播出口**）
///
/// 失败只留痕（[`delivery_note`]），**不改变调用方的返回值**——事件是旁路，
/// 业务真源在插件私有库与 HTTP 回包。
#[cfg(target_arch = "wasm32")]
pub fn broadcast_event(host: &WasmHost, event_name: &str, payload: &serde_json::Value) {
    let Some(endpoint_id) = endpoint_id_for_path(host, SESSION_CONTROL_PATH) else {
        return;
    };
    let outcome = host
        .ws_broadcast_text(&endpoint_id, &event_frame(event_name, payload))
        .map_err(|e| e.message);
    if let Some(note) = delivery_note(&outcome, event_name) {
        host.log_warn(&note);
    }
}

/// native 空实现：广播只在 wasm 运行时有意义（宿主原语在 native 无实现）
#[cfg(not(target_arch = "wasm32"))]
pub fn broadcast_event(_host: &WasmHost, _event_name: &str, _payload: &serde_json::Value) {}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    /// 端点清单样本（宿主 `ws_list_endpoints()` 的形状：`[{endpointId, path, clientCount}]`）
    fn listing() -> &'static str {
        r#"[
            {"endpointId":"wse-1","path":"session-control","clientCount":2},
            {"endpointId":"wse-2","path":"terminal","clientCount":0}
        ]"#
    }

    fn keys_of(value: &serde_json::Value) -> Vec<String> {
        value
            .as_object()
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default()
    }

    // ---------- 帧壳 ----------

    /// 帧壳逐字段对齐移动端事件通道契约（`{"type":"event",...}`）
    #[test]
    fn event_frame_is_typed_event_envelope() {
        let frame: serde_json::Value =
            serde_json::from_str(&event_frame("session:created", &serde_json::json!({"session_id": "s1"})))
                .expect("帧必须是合法 JSON");
        assert_eq!(frame["type"], "event");
        assert_eq!(frame["event"], "session:created");
        assert_eq!(frame["payload"]["session_id"], "s1");
    }

    /// 反例：载荷不得 flatten 到顶层（移动端按 `payload` 键取载荷）
    #[test]
    fn event_frame_keeps_payload_nested() {
        let frame: serde_json::Value =
            serde_json::from_str(&event_frame("task:queue-changed", &serde_json::json!({"queue_count": 3})))
                .expect("帧必须是合法 JSON");
        assert!(frame.get("queue_count").is_none(), "载荷字段不得出现在帧顶层");
        assert_eq!(keys_of(&frame).len(), 3, "帧壳恒三键：type / event / payload");
    }

    // ---------- 端点反查 ----------

    /// 正例：清单含目标 path → 命中端点句柄与客户端数
    #[test]
    fn endpoint_entry_from_listing_finds_session_control() {
        let entry = endpoint_entry_from_listing(listing(), SESSION_CONTROL_PATH)
            .expect("清单含 session-control");
        assert_eq!(entry.id, "wse-1");
        assert_eq!(entry.client_count, 2);
    }

    /// 反例：清单无目标 path → `None`（不回退到别的端点，避免推错通道）
    #[test]
    fn endpoint_entry_from_listing_returns_none_when_absent() {
        assert!(endpoint_entry_from_listing(listing(), "ghost").is_none());
        assert!(endpoint_entry_from_listing("[]", SESSION_CONTROL_PATH).is_none());
    }

    /// 异常：畸形清单 / 缺字段条目 → `None`，**不 panic**（宿主串不可信）
    #[test]
    fn endpoint_entry_from_listing_survives_malformed_listing() {
        assert!(endpoint_entry_from_listing("not json", SESSION_CONTROL_PATH).is_none());
        assert!(endpoint_entry_from_listing("{}", SESSION_CONTROL_PATH).is_none());
        // 缺 endpointId / path 的条目整体跳过（不记半截缓存）
        let partial = r#"[{"path":"session-control"},{"endpointId":"wse-9"}]"#;
        assert!(endpoint_entry_from_listing(partial, SESSION_CONTROL_PATH).is_none());
    }

    // ---------- 零客户端早退 ----------

    /// 零客户端 → 无广播目标（无客户端路径零宿主调用）
    #[test]
    fn broadcast_target_skips_endpoint_without_clients() {
        let empty = EndpointEntry {
            id: "wse-1".to_string(),
            client_count: 0,
        };
        assert_eq!(broadcast_target(Some(&empty)), None, "clientCount=0 必须早退");
    }

    /// 正例：有客户端 → 返回端点句柄
    #[test]
    fn broadcast_target_returns_endpoint_with_clients() {
        let live = EndpointEntry {
            id: "wse-1".to_string(),
            client_count: 1,
        };
        assert_eq!(broadcast_target(Some(&live)), Some("wse-1"));
        // 反例：无端点（清单里没有 / 宿主原语不可用）→ 无目标
        assert_eq!(broadcast_target(None), None);
    }

    // ---------- 载荷形状（7 事件） ----------

    /// `task:status-changed` 最小形：仅两键，可选字段**缺席不出现**
    #[test]
    fn task_status_payload_without_optional_fields_omits_keys() {
        let payload = task_status_payload("s1", "in_progress", None, None);
        assert_eq!(payload["session_id"], "s1");
        assert_eq!(payload["task_status"], "in_progress");
        assert!(
            payload.get("task_reason").is_none(),
            "无 reason 不得出现 task_reason 键（不伪造）"
        );
        assert!(payload.get("task_questions").is_none());
    }

    /// `task:status-changed` 全字段：reason + questions 原样透传
    #[test]
    fn task_status_payload_carries_reason_and_questions() {
        let questions = serde_json::json!([{ "id": "q1", "text": "continue?" }]);
        let payload = task_status_payload("s1", "waiting_input", Some("needs approval"), Some(&questions));
        assert_eq!(payload["task_status"], "waiting_input");
        assert_eq!(payload["task_reason"], "needs approval");
        assert_eq!(payload["task_questions"], questions);
    }

    /// 边界：空串 reason / `null` questions 同样缺席（与注解槽过滤口径一致）
    #[test]
    fn task_status_payload_treats_empty_reason_and_null_questions_as_absent() {
        let payload = task_status_payload("s1", "idle", Some(""), Some(&serde_json::Value::Null));
        assert!(payload.get("task_reason").is_none(), "空串 reason 不落键");
        assert!(payload.get("task_questions").is_none(), "null questions 不落键");
    }

    /// `session:mode-changed`：bus 形三键（无 camelCase 兼容键）
    #[test]
    fn session_mode_payload_is_snake_case_only() {
        let payload = session_mode_payload("s1", true, false);
        assert_eq!(payload, serde_json::json!({
            "session_id": "s1",
            "auto_approve": true,
            "auto_execute": false,
        }));
    }

    /// `task:queue-changed`：与 bus / emit 逐字同形（可选字段为 null）
    #[test]
    fn queue_changed_payload_matches_bus_shape() {
        let payload = queue_changed_payload("s1", 3, "add", None, None);
        assert_eq!(payload, serde_json::json!({
            "session_id": "s1",
            "queue_count": 3,
            "action": "add",
            "task_id": null,
            "status": null,
        }));
        // done 广播携带关联信息（移动端预设任务完成匹配）
        let done = queue_changed_payload("s1", 0, "done", Some("t1"), Some("done"));
        assert_eq!(done["task_id"], "t1");
        assert_eq!(done["status"], "done");
    }

    /// `task:scheduled-changed`：三键同形
    #[test]
    fn scheduled_changed_payload_matches_bus_shape() {
        assert_eq!(
            scheduled_changed_payload("j1", "executed", "trigger"),
            serde_json::json!({ "job_id": "j1", "status": "executed", "action": "trigger" })
        );
    }

    /// 载荷键锁：全部 snake_case（拒绝 camelCase 混入形成第三套键名）
    #[test]
    fn payload_keys_are_snake_case() {
        let payloads = [
            task_status_payload("s1", "completed", Some("done"), None),
            session_mode_payload("s1", true, true),
            queue_changed_payload("s1", 1, "add", Some("t1"), Some("pending")),
            scheduled_changed_payload("j1", "pending", "create"),
            // session 三事件的载荷（session/events.rs 构造，此处按同形断言）
            serde_json::json!({ "session": { "id": "s1", "created_at": "" }, "source_device": "" }),
            serde_json::json!({ "session_id": "s1", "session_name": "dev", "source_device": "" }),
        ];
        for payload in payloads {
            for key in keys_of(&payload) {
                assert!(
                    key.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                    "广播载荷键必须 snake_case，got: {key}"
                );
            }
            if let Some(session) = payload.get("session") {
                for key in keys_of(session) {
                    assert!(
                        key.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                        "内嵌 session 键必须 snake_case，got: {key}"
                    );
                }
            }
        }
    }

    // ---------- 失败口径 ----------

    /// 成功（含 0）静默：0 是合法结果（无客户端 / 宿主按计数语义表达）
    #[test]
    fn delivery_note_is_silent_on_success() {
        assert_eq!(delivery_note(&Ok(0), "session:created"), None);
        assert_eq!(delivery_note(&Ok(2), "session:created"), None);
    }

    /// 失败只产留痕文本（不 panic、不向调用方传播），消息带事件名与宿主原因
    #[test]
    fn delivery_note_reports_failure_with_context() {
        let note = delivery_note(&Err("endpoint not owned".to_string()), "task:queue-changed")
            .expect("失败必须留痕");
        assert!(note.contains("task:queue-changed"), "留痕需带事件名, got: {note}");
        assert!(note.contains("endpoint not owned"), "留痕需带宿主原因, got: {note}");
    }

    // ---------- 结构锁 ----------

    /// 结构锁一：广播出口唯一——`ws_broadcast_text(` 在实现段只出现在 ws_events.rs
    ///
    /// 旁路新增广播点必须走 [`broadcast_event`]（端点反查 + 零客户端早退 +
    /// 失败留痕三件事不可复制），散落调用会逐个漏掉这些不变量。
    #[test]
    fn broadcast_outlet_is_single() {
        let root = env!("CARGO_MANIFEST_DIR");
        let files = [
            "src/lib.rs",
            "src/session/events.rs",
            "src/task/state.rs",
            "src/task/queue.rs",
            "src/task/scheduled.rs",
            "src/task/preset.rs",
            "src/ws_control.rs",
            "src/ws_terminal.rs",
        ];
        let mut violations = Vec::new();
        for file in files {
            let path = format!("{root}/{file}");
            let src = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
            for (idx, raw) in src.lines().enumerate() {
                let line = raw.trim_start();
                if line.starts_with("//") {
                    continue;
                }
                if line.contains("ws_broadcast_text(") || line.contains("ws_broadcast_binary(") {
                    violations.push(format!("{file}:{}: {}", idx + 1, line.trim()));
                }
            }
        }
        assert!(
            violations.is_empty(),
            "广播出口必须唯一（ws_events.rs），越界调用：\n{}",
            violations.join("\n")
        );

        // 本模块实现段恰好一处（多了 = 第二出口，少了 = 出口被旁路）
        let own = std::fs::read_to_string(format!("{root}/src/ws_events.rs")).expect("read ws_events.rs");
        let impl_part = own.split("\n#[cfg(test)]").next().unwrap_or(&own);
        let calls = impl_part
            .lines()
            .filter(|l| !l.trim_start().starts_with("//") && l.contains("ws_broadcast_text("))
            .count();
        assert_eq!(calls, 1, "ws_events.rs 实现段必须恰好一处广播调用");
    }

    /// 结构锁二：广播调用点数钉死（session 1 / queue 1 / scheduled 2 / state 4）
    ///
    /// 少一点 = 某条推进路径的事件静默失声（移动端收不到）；多一点必须来
    /// 交代新增事件语义（本票只接既有收口点）。
    #[test]
    fn broadcast_call_points_are_pinned() {
        let root = env!("CARGO_MANIFEST_DIR");
        let count = |file: &str, marker: &str| -> usize {
            let path = format!("{root}/src/{file}");
            let src = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
            src.lines()
                .filter(|l| !l.trim_start().starts_with("//") && l.contains(marker))
                .count()
        };
        // state.rs：`broadcast_task_status` 1 定义 + 3 调用点（interrupted /
        // dispatched / hook 状态）；`broadcast_event` 1（helper 内）+ 1（mode-changed）
        assert_eq!(
            count("task/state.rs", "broadcast_task_status("),
            4,
            "task:status-changed 三个 emit 点 + 单点收口函数定义"
        );
        assert_eq!(
            count("task/state.rs", "ws_events::broadcast_event("),
            2,
            "state.rs 广播出口调用：helper 内 1 + session:mode-changed 1"
        );
        assert_eq!(
            count("session/events.rs", "ws_events::broadcast_event("),
            1,
            "session 三事件经 publish() 单点广播"
        );
        assert_eq!(
            count("task/queue.rs", "ws_events::broadcast_event("),
            1,
            "task:queue-changed 经 broadcast_queue_changed 单点广播"
        );
        assert_eq!(
            count("task/scheduled.rs", "ws_events::broadcast_event("),
            2,
            "scheduled.rs：task:scheduled-changed 1 + session:mode-changed 1"
        );
    }
}
