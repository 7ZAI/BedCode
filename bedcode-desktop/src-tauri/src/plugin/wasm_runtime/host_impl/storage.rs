//! 存储域宿主实现（插件键值存储，按 plugin_id 隔离）
//!
//! `storage_get/set/delete`（权限校验 + 服务调用）供 Component Model 绑定
//! （`wasm_runtime::component`）调用。

use crate::plugin::permission::PERMISSION_STORAGE;
use crate::plugin::wasm_runtime::{block_on_async, WasmHostContext};

/// 获取值（权限校验 + 服务调用）
pub(crate) fn storage_get(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    key: &str,
) -> Result<Option<serde_json::Value>, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_STORAGE, "host_storage_get") {
        return Err("permission denied".to_string());
    }
    let storage = host_ctx.storage.clone();
    block_on_async(storage.get(plugin_id, key))
        .map_err(|e| format!("storage error: {}", e))
}

/// 设置值（权限校验 + 服务调用）
pub(crate) fn storage_set(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    key: &str,
    value: serde_json::Value,
) -> Result<(), String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_STORAGE, "host_storage_set") {
        return Err("permission denied".to_string());
    }
    let storage = host_ctx.storage.clone();
    block_on_async(storage.set(plugin_id, key, value))
        .map_err(|e| format!("storage error: {}", e))
}

/// 删除值（权限校验 + 服务调用）
pub(crate) fn storage_delete(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    key: &str,
) -> Result<(), String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_STORAGE, "host_storage_delete") {
        return Err("permission denied".to_string());
    }
    let storage = host_ctx.storage.clone();
    block_on_async(storage.delete(plugin_id, key))
        .map_err(|e| format!("storage error: {}", e))
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::wasm_runtime::host_impl::tests::{build_host_ctx, grant_permissions};

    const PLUGIN: &str = "test-plugin";

    /// 未授权插件（从未 grant）：storage 三个操作均被权限门禁拒绝
    #[test]
    fn storage_ops_permission_denied() {
        let ctx = build_host_ctx();
        assert_eq!(storage_get(&ctx, PLUGIN, "k").unwrap_err(), "permission denied");
        assert_eq!(
            storage_set(&ctx, PLUGIN, "k", serde_json::json!(1)).unwrap_err(),
            "permission denied"
        );
        assert_eq!(storage_delete(&ctx, PLUGIN, "k").unwrap_err(), "permission denied");
    }

    /// 授权后 set/get/delete 往返 + 插件间隔离 + 缺失 key 返回 None
    #[tokio::test]
    async fn storage_set_get_delete_roundtrip() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_STORAGE]);
        let value = serde_json::json!({ "count": 3, "tags": ["a", "b"] });

        storage_set(&ctx, PLUGIN, "cfg", value.clone()).expect("set ok");
        assert_eq!(
            storage_get(&ctx, PLUGIN, "cfg").expect("get ok").expect("value"),
            value
        );
        // 插件间隔离：另一个插件读不到（key 按 plugin_id 分区）——
        // 需先授权该插件，否则在权限门禁处就被拒绝，无法触达存储层语义
        grant_permissions(&ctx, "other-plugin", &[PERMISSION_STORAGE]);
        assert!(storage_get(&ctx, "other-plugin", "cfg").expect("get ok").is_none());
        // 未设置的 key 返回 None
        assert!(storage_get(&ctx, PLUGIN, "missing").expect("get ok").is_none());

        storage_delete(&ctx, PLUGIN, "cfg").expect("delete ok");
        assert!(storage_get(&ctx, PLUGIN, "cfg").expect("get ok").is_none());
        // 删除不存在的 key 幂等
        storage_delete(&ctx, PLUGIN, "cfg").expect("delete again ok");
    }
}
