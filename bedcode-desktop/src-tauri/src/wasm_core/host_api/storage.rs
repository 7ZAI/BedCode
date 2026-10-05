//! `host-storage` 能力域实现（插件键值存储，3 条原语，按 `plugin_id` 隔离）
//!
//! 每条原语三段式：
//!
//! 1. **权限门** `storage`（经端口问宿主结果——判定与落日志只在宿主一处）；
//! 2. **系统空间纵深守卫**（[`SYSTEM_PLUGIN_ID`] fail-closed，R-02 纵深防御）；
//! 3. **能力路由**（core-plugin-manager：该能力由系统组件提供时，按**调用方**命名空间
//!    转发到它的同形导出）→ 未命中才走宿主原语（[`SqlitePorts::storage_get`] 等）。
//!
//! 真源仍是数据库（`plugin_storage` 表，schema 在 `crate::db`），本域**不另立真源**。
//!
//! ## 为什么这段实现**留在 wasm 核心内**（ADR 0036）
//!
//! 与 [`super::database`] 同理：`host-storage` 的属主分区与系统空间守卫是
//! wasm_core 自己的插件机制（宿主激活状态、审批记录都落在这张表里），机制实现与
//! 机制真源不分家。
//!
//! ## 零业务代码红线（AGENTS §5.1）
//!
//! 本域只做「按属主分区的键值原语 + 路由」，不解释任何键的含义（激活状态、审批记录、
//! 预授权路径 …全归宿主/插件各自解释）。

use crate::wasm_core::host_api::sqlite_ports::SqlitePorts;
use crate::wasm_core::permission::PERMISSION_STORAGE;

/// 系统级 `plugin_id`：非插件私有的全局数据命名空间（`plugin_storage` 表内按
/// `plugin_id` 分区，故系统空间与插件空间天然隔离）
///
/// **真源是 [`crate::wasm_core::storage::SYSTEM_PLUGIN_ID`]**（插件激活状态与系统级
/// 数据的写入方在那里）；本域是它的 fail-closed 消费方（[`ensure_not_system_space`]），
/// 经再导出取同一个值——两处都指同一份常量，不存在同值副本。
pub(crate) use crate::wasm_core::storage::SYSTEM_PLUGIN_ID;

/// 插件面存储原语的系统空间守卫（R-02 纵深防御）
///
/// 插件实例的 `plugin_id` 由运行时从已认证身份派生（`component.rs` 内
/// `&self.plugin_id`），guest 无法伪造；但若真出现 `__system__`（实施者 bug /
/// 未来某条宽松入径），必须 fail-closed——系统空间是宿主激活状态/审批记录等的
/// 真源，任何插件写它就是越权。
fn ensure_not_system_space(plugin_id: &str) -> Result<(), String> {
    if plugin_id == SYSTEM_PLUGIN_ID {
        return Err("storage: plugin may not access system storage space".to_string());
    }
    Ok(())
}

/// 获取值（权限校验 + 服务调用）
pub fn storage_get(ports: &dyn SqlitePorts, plugin_id: &str, key: &str) -> Result<Option<serde_json::Value>, String> {
    if !ports.check_permission(plugin_id, PERMISSION_STORAGE, "host_storage_get") {
        return Err("permission denied".to_string());
    }
    ensure_not_system_space(plugin_id)?;
    // 能力路由：系统组件提供者命中时转发（组件间不共享内存，WIT 边界序列化）
    if let Some(result) = ports.forward_storage_get(plugin_id, key) {
        return result.map(|opt| opt.map(|s| serde_json::from_str(&s).unwrap_or(serde_json::Value::String(s))));
    }
    ports
        .storage_get(plugin_id, key)
        .map_err(|e| format!("storage error: {}", e))
}

/// 设置值（权限校验 + 服务调用）
pub fn storage_set(
    ports: &dyn SqlitePorts,
    plugin_id: &str,
    key: &str,
    value: serde_json::Value,
) -> Result<(), String> {
    if !ports.check_permission(plugin_id, PERMISSION_STORAGE, "host_storage_set") {
        return Err("permission denied".to_string());
    }
    ensure_not_system_space(plugin_id)?;
    if let Some(result) = ports.forward_storage_set(plugin_id, key, &value.to_string()) {
        return result;
    }
    ports
        .storage_set(plugin_id, key, value)
        .map_err(|e| format!("storage error: {}", e))
}

/// 删除值（权限校验 + 服务调用）
pub fn storage_delete(ports: &dyn SqlitePorts, plugin_id: &str, key: &str) -> Result<(), String> {
    if !ports.check_permission(plugin_id, PERMISSION_STORAGE, "host_storage_delete") {
        return Err("permission denied".to_string());
    }
    ensure_not_system_space(plugin_id)?;
    if let Some(result) = ports.forward_storage_delete(plugin_id, key) {
        return result;
    }
    ports
        .storage_delete(plugin_id, key)
        .map_err(|e| format!("storage error: {}", e))
}

#[cfg(test)]
mod tests;
