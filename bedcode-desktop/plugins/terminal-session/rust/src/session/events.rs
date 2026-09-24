//! 会话生命周期事件域（websocket 业务下沉票 05：会话事件归插件）
//!
//! ## 归属变化
//!
//! 此前会话创建/停止/移除事件经 `host-events.broadcast-sync`（`SyncEvent::Session*`）
//! 交给宿主广播面，由宿主折成 `SyncPayload` 并通过 `Message::SyncData` WS 广播——
//! 宿主持有「谁来收、排除谁、刷什么」的解释权（既有 `sync_handler` 的排除源设备
//! 语义）。票 05 起会话生命周期事件由**本插件自己**定义载荷、自己发布：
//!
//! - **桌面前端**：`host-events.emit`（Tauri 事件，事件名见 SDK
//!   `EVENT_SESSION_*` 常量，插件前端经 `context.events.on` 订阅）；
//! - **插件内部 / 跨插件消费者**：`host-bus.publish` **属主私有 topic**
//!   `com.bedcode.terminal-session::session:<kind>`（跨属主订阅被宿主总线门禁拒绝，
//!   他人既订不到也伪投递不进）；
//! - **来源设备**：`source_device` 随载荷发布（不再由宿主排除）——需要「排除发起端」
//!   语义的消费者自行按该字段过滤（原宿主 `broadcast_sync_to_others` 的职责下沉）。
//!
//! ## 载荷契约
//!
//! 载荷**自足**：不再「广播后回查会话真源补字段」（事件本身携带全部数据）；
//! 缺失数据（如移除一个已被摘录的会话）以**显式字段**表达（`session_name: ""`），
//! 不伪造半成品通知。emit 与 bus 同形（同一 `serde_json::Value`）。
//!
//! ## 失败口径
//!
//! `bus_publish` 失败**显性留痕**（`log_warn` 点名 topic），不静默吞；
//! `emit_event` 无返回值（宿主侧负责投递并记录失败，此处不伪造回执）。

// wasm 发布路径 + native 测试（事件名常量锁）均消费；其他 native 构建不需要
#[cfg(any(target_arch = "wasm32", test))]
use bedcode_plugin_api::constants::{
    EVENT_SESSION_CREATED, EVENT_SESSION_REMOVED, EVENT_SESSION_STOPPED,
};
#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::host::bus::owned_topic;
#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::host::{HostBus, HostEvents, HostLog};
#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::wasm_host::WasmHost;

/// 属主插件 id（bus 私有 topic 的属主段；与 `SessionPlugin::ID` 同值）
pub const PLUGIN_ID: &str = "com.bedcode.terminal-session";

// ==================== 载荷构造（纯函数，native 可测） ====================

/// `session:created` 载荷：`{ session, source_device }`
///
/// `session` 为 `SessionSummary` 的 JSON（snake_case 键：id/name/status/
/// created_at/started_at/session_type/config_id/task_status/task_reason——
/// 与既有 `session_list` WS 控制面同一形状）。`source_device` 空串 = 桌面本地。
pub fn created_payload(
    summary: &serde_json::Value,
    source_device: &str,
) -> serde_json::Value {
    serde_json::json!({
        "session": summary,
        "source_device": source_device,
    })
}

/// `session:stopped` 载荷：`{ session_id, session_name, source_device }`
///
/// `session_name` 来自退出前的记录快照（会话已进终态，但名字仍可用——payload
/// 自足，消费方无需再查）。`source_device` 为 kill 发起侧（自然退出为空串）。
pub fn stopped_payload(
    session_id: &str,
    session_name: &str,
    source_device: &str,
) -> serde_json::Value {
    serde_json::json!({
        "session_id": session_id,
        "session_name": session_name,
        "source_device": source_device,
    })
}

/// `session:removed` 载荷：`{ session_id, session_name, source_device }`
///
/// `session_name` 空串 = 移除一个已不在册的会话（幂等移除仍要通知各端刷新，
/// 用显式空字段表达缺失，不伪造名字）。
pub fn removed_payload(
    session_id: &str,
    session_name: &str,
    source_device: &str,
) -> serde_json::Value {
    serde_json::json!({
        "session_id": session_id,
        "session_name": session_name,
        "source_device": source_device,
    })
}

// ==================== wasm 发布（emit + bus 双通道同形） ====================

/// 双通道发布：属主私有 bus topic + 前端 emit（同形载荷）
///
/// bus 失败显性 `log_warn`（点名 topic）；emit 无返回值（宿主投递）。
#[cfg(target_arch = "wasm32")]
fn publish(event_name: &str, payload: &serde_json::Value) {
    let topic = owned_topic(PLUGIN_ID, event_name);
    if let Err(e) = WasmHost.bus_publish(&topic, payload) {
        WasmHost.log_warn(&format!(
            "session lifecycle event bus publish failed (topic={}): {}",
            topic, e.message
        ));
    }
    WasmHost.emit_event(event_name, payload);
}

/// 会话创建事件（创建编排 `launch::spawn_session` 唯一发布点）
#[cfg(target_arch = "wasm32")]
pub fn publish_created(summary: &serde_json::Value, source_device: &str) {
    publish(EVENT_SESSION_CREATED, &created_payload(summary, source_device));
}

/// 会话停止事件（`session::on_pty_exit` 唯一发布点——终态单一发布者不变量）
#[cfg(target_arch = "wasm32")]
pub fn publish_stopped(session_id: &str, session_name: &str, source_device: &str) {
    publish(
        EVENT_SESSION_STOPPED,
        &stopped_payload(session_id, session_name, source_device),
    );
}

/// 会话移除事件（移除 / 重启编排的旧会话摘除）
#[cfg(target_arch = "wasm32")]
pub fn publish_removed(session_id: &str, session_name: &str, source_device: &str) {
    publish(
        EVENT_SESSION_REMOVED,
        &removed_payload(session_id, session_name, source_device),
    );
}

#[cfg(not(target_arch = "wasm32"))]
const _: () = ();

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    fn summary() -> serde_json::Value {
        serde_json::json!({
            "id": "s1",
            "name": "dev",
            "status": "running",
            "created_at": "2026-09-25T00:00:00Z",
            "config_id": "cfg-1",
        })
    }

    /// created 载荷：自足（session 完整内嵌）+ 来源设备透传
    #[test]
    fn created_payload_embeds_full_summary_and_source() {
        let payload = created_payload(&summary(), "device-A");
        assert_eq!(payload["session"]["id"], "s1");
        assert_eq!(payload["session"]["name"], "dev");
        assert_eq!(payload["source_device"], "device-A");
        // 载荷中的 session 键名与既有 session_list 形状一致（snake_case）
        assert!(payload["session"].get("created_at").is_some(), "session 字段键名与凭证形状一致");
        assert!(payload["session"].get("createdAt").is_none(), "不得引入第二套键名");
    }

    /// 桌面本地创建：source_device 空串（消费方按空=本地处理）
    #[test]
    fn created_payload_local_empty_source() {
        let payload = created_payload(&summary(), "");
        assert_eq!(payload["source_device"], "");
    }

    /// stopped 载荷：三字段齐全，进程停止后不依赖回查
    #[test]
    fn stopped_payload_is_self_sufficient() {
        let payload = stopped_payload("s1", "dev", "device-A");
        assert_eq!(payload["session_id"], "s1");
        assert_eq!(payload["session_name"], "dev");
        assert_eq!(payload["source_device"], "device-A");
    }

    /// removed 载荷：幂等移除已不在册会话时 session_name 显式空串（不伪造）
    #[test]
    fn removed_payload_missing_name_is_explicit_empty() {
        let payload = removed_payload("s-gone", "", "");
        assert_eq!(payload["session_id"], "s-gone");
        assert_eq!(payload["session_name"], "");
        assert_eq!(payload["source_device"], "");
    }

    /// removed 载荷正例：名字 + 来源设备透传
    #[test]
    fn removed_payload_carries_name_and_source() {
        let payload = removed_payload("s2", "itest", "device-B");
        assert_eq!(payload["session_name"], "itest");
        assert_eq!(payload["source_device"], "device-B");
    }

    /// 事件名 = 前端 events.on 的 key + bus topic 后缀（常量锁）
    #[test]
    fn event_names_match_bus_suffixes() {
        assert_eq!(EVENT_SESSION_CREATED, "session:created");
        assert_eq!(EVENT_SESSION_STOPPED, "session:stopped");
        assert_eq!(EVENT_SESSION_REMOVED, "session:removed");
        // 属主私有 topic 形状：<owner>::session:<kind>
        #[cfg(target_arch = "wasm32")]
        {
            assert_eq!(owned_topic(PLUGIN_ID, EVENT_SESSION_CREATED), "com.bedcode.terminal-session::session:created");
        }
    }

    /// 票 05 结构锁一：会话事件生产路径**不再调用宿主同步广播产品接口**
    ///
    /// 扫 launch.rs / actions.rs / session/mod.rs（跳过注释行），`broadcast_sync(`
    /// 与 `SyncEvent::` 必须为零——宿主不再解释/决定会话事件的同步广播
    /// （events.rs 自身是发布封装，不含生产调用；锁代码里的模式串会自匹配，
    /// 故不进扫描清单）。
    #[test]
    fn session_event_producers_do_not_use_host_sync_broadcast() {
        let root = env!("CARGO_MANIFEST_DIR");
        let files = [
            ("launch.rs", format!("{root}/src/launch.rs")),
            ("actions.rs", format!("{root}/src/actions.rs")),
            ("session/mod.rs", format!("{root}/src/session/mod.rs")),
        ];
        let mut violations: Vec<String> = Vec::new();
        for (name, path) in files {
            let src = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("read {path}: {e}"));
            for (idx, raw) in src.lines().enumerate() {
                let line = raw.trim_start();
                if line.starts_with("//") || line.starts_with("///") || line.starts_with("//!") {
                    continue;
                }
                if line.contains("broadcast_sync(") || line.contains("SyncEvent::") {
                    violations.push(format!("{name}:{}: {}", idx + 1, line.trim()));
                }
            }
        }
        assert!(
            violations.is_empty(),
            "会话事件生产路径不得再调宿主同步广播（票 05）：\n{}",
            violations.join("\n")
        );
    }

    /// 票 05 结构锁二：会话生命周期事件的**发布点点数**钉死
    ///
    /// 终态单一发布者不变量（SessionStopped 只在 on_pty_exit 一处发）之外，
    /// created / removed 的触发路径也逐一钉点数——少一点即红（某条推进路径
    /// 被删时事件静默失声），多一点必须来交代为什么。
    #[test]
    fn session_event_publish_points_are_pinned() {
        let root = env!("CARGO_MANIFEST_DIR");
        let files = [
            ("launch.rs", format!("{root}/src/launch.rs")),
            ("actions.rs", format!("{root}/src/actions.rs")),
            ("session/mod.rs", format!("{root}/src/session/mod.rs")),
        ];
        let count = |marker: &str| -> usize {
            files
                .iter()
                .map(|(_, path)| {
                    let src = std::fs::read_to_string(path).unwrap_or_default();
                    // 全文件计数（跳过注释）：这些文件的测试模块不引用发布函数，
                    // 且中点中可能前置 #[cfg(test)] 辅助项，按实现段切分会误截
                    src.lines()
                        .filter(|l| l.contains(marker) && !l.trim_start().starts_with("//"))
                        .count()
                })
                .sum()
        };
        // 发布点点数：created 1（spawn_session）/ stopped 1（on_pty_exit）/
        // removed 3（重启编排 + 移除两分支：ghost 幂等 + 正常摘除）
        assert_eq!(count("::publish_created("), 1, "SessionCreated 唯一发布点");
        assert_eq!(count("::publish_stopped("), 1, "SessionStopped 唯一发布点（终态单一发布者）");
        assert_eq!(count("::publish_removed("), 3, "SessionRemoved 三触发路径");
    }
}