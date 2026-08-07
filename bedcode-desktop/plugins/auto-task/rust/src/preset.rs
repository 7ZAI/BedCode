//! 预设任务管理（一次性消耗）
//!
//! 预设任务是「无运行中会话 / 未选会话时创建的待投递任务」：
//! 创建时不绑定会话，加入某会话队列后即从预设中删除（one-shot 语义），
//! 删除由 `add-preset-to-queue` 原子完成（先删后插，单消费者）。
//!
//! 仅桌面端功能：事件只走 emit_event + bus 前端通道，不广播移动端
//! 同步通道（broadcast_sync），与 SyncEvent 线协议解耦。
//!
//! 生命周期：create → 存 preset_tasks → add-preset-to-queue 复制进
//! task_queue（source='queue'，预存入队后归为自动任务）并删除预设。

use bedcode_plugin_api::constants::EVENT_TASK_PRESET_CHANGED;
use bedcode_plugin_api::host::{HostBus, HostEvents, HostLog, HostPluginDatabase};
use bedcode_plugin_api::sql_params;
use bedcode_plugin_api::wasm_host::WasmHost;
use serde_json::Value;

/// 预设任务表建表 SQL（按语句拆分，宿主 plugin_db_execute 单语句执行）
pub const PRESET_TASKS_SCHEMA: &[&str] = &[
    r#"
CREATE TABLE IF NOT EXISTS preset_tasks (
    id         TEXT PRIMARY KEY,
    prompt     TEXT NOT NULL,
    created_at TEXT NOT NULL
)"#,
    "CREATE INDEX IF NOT EXISTS idx_preset_tasks_created ON preset_tasks(created_at)",
];

// ==================== Preset Operations ====================

/// 查询全部预设任务（创建时间倒序，最新在上）
pub fn list_presets(host: &WasmHost) -> Vec<Value> {
    host.plugin_db_query(
        "SELECT id, prompt, created_at FROM preset_tasks ORDER BY created_at DESC, id DESC",
    )
    .ok()
    .flatten()
    .and_then(|v| v.as_array().cloned())
    .unwrap_or_default()
}

/// 创建预设任务，返回 preset_id
pub fn create_preset(host: &WasmHost, prompt: &str) -> String {
    let id = new_id(host);
    let _ = host.plugin_db_execute_params(
        "INSERT INTO preset_tasks (id, prompt, created_at) VALUES (?1, ?2, datetime('now'))",
        &sql_params![id, prompt],
    );

    host.log_info(&format!(
        "Preset task created: id={} prompt={:?}",
        id,
        prompt.chars().take(64).collect::<String>()
    ));

    id
}

/// 删除预设任务；返回是否实际删除（已被消耗/不存在时为 false）
pub fn delete_preset(host: &WasmHost, preset_id: &str) -> bool {
    let affected = host
        .plugin_db_execute_params(
            "DELETE FROM preset_tasks WHERE id = ?1",
            &sql_params![preset_id],
        )
        .unwrap_or(-1);

    if affected > 0 {
        host.log_info(&format!("Preset task deleted: id={}", preset_id));
        true
    } else {
        false
    }
}

/// 更新预设任务内容；返回是否实际更新（不存在/已被消耗时为 false）
pub fn update_preset(host: &WasmHost, preset_id: &str, prompt: &str) -> bool {
    let affected = host
        .plugin_db_execute_params(
            "UPDATE preset_tasks SET prompt = ?1 WHERE id = ?2",
            &sql_params![prompt, preset_id],
        )
        .unwrap_or(-1);

    if affected > 0 {
        host.log_info(&format!("Preset task updated: id={}", preset_id));
        true
    } else {
        false
    }
}

/// 把预设任务加入指定会话队列（一次性消耗，原子语义）
///
/// 顺序保证单消费者：先删除预设（affected=1 才继续），再复制 prompt 进
/// task_queue 末尾（source='queue'）。并发场景下只有一个调用方成功删除，
/// 其余收到「已消耗」错误，不会重复入队。
///
/// 返回 (task_id, position)
pub fn add_preset_to_queue(
    host: &WasmHost,
    session_id: &str,
    preset_id: &str,
) -> Result<(String, i64), String> {
    // 1. 读取预设 prompt（先读后删：prompt 用于入队）
    let prompt = host
        .plugin_db_query_params(
            "SELECT prompt FROM preset_tasks WHERE id = ?1",
            &sql_params![preset_id],
        )
        .ok()
        .flatten()
        .and_then(|v| v.as_array().and_then(|a| a.first().cloned()))
        .and_then(|row| {
            row.get("prompt")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
        })
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("preset task not found or already consumed: {}", preset_id))?;

    // 2. 删除预设（先删后插：删除成功才继续，保证单次消耗）
    if !delete_preset(host, preset_id) {
        return Err(format!("preset task already consumed: {}", preset_id));
    }

    // 3. 复制进队列（source='queue'：预存被消费后即归入自动任务，
    //    来源只区分 手动输入/自动任务/定时任务 三种）
    let (task_id, position) =
        crate::queue::add_task_with_source(host, session_id, &prompt, "queue");

    host.log_info(&format!(
        "Preset task enqueued: preset_id={} task_id={} session_id={} position={}",
        preset_id, task_id, session_id, position
    ));

    Ok((task_id, position))
}

/// 广播预设任务变更事件（前端 UI 刷新通道；仅桌面端，不广播移动端）
pub fn broadcast_preset_changed(host: &WasmHost, preset_id: &str, action: &str) {
    let _ = host.bus_publish(
        EVENT_TASK_PRESET_CHANGED,
        &serde_json::json!({ "preset_id": preset_id, "action": action }),
    );
    host.emit_event(
        EVENT_TASK_PRESET_CHANGED,
        &serde_json::json!({ "preset_id": preset_id, "action": action }),
    );
}

// ==================== Helpers ====================

/// 生成预设任务 ID（与 queue.rs 同法：randomblob hex）
///
/// 回退仅作编译期/极端兜底（randomblob 查询几乎不会失败），
/// 用 preset 表行数近似唯一，避免依赖 wasm 不可用的系统 API
fn new_id(host: &WasmHost) -> String {
    host.plugin_db_query("SELECT lower(hex(randomblob(16))) AS id")
        .ok()
        .flatten()
        .and_then(|v| v.as_array().and_then(|a| a.first().cloned()))
        .and_then(|row| {
            row.get("id")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
        })
        .unwrap_or_else(|| {
            let count = host
                .plugin_db_query("SELECT count(*) AS c FROM preset_tasks")
                .ok()
                .flatten()
                .and_then(|v| v.as_array().and_then(|a| a.first().cloned()))
                .and_then(|row| row.get("c").and_then(|v| v.as_i64()))
                .unwrap_or(0);
            format!("preset-{}", count + 1)
        })
}
