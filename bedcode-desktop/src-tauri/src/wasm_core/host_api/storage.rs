//! 存储域宿主实现（插件键值存储，按 plugin_id 隔离）
//!
//! `storage_get/set/delete`（权限校验 + 服务调用）供 Component Model 绑定
//! （`wasm_runtime::component`）调用。
//!
//! 能力路由（core-plugin-manager）：`host-storage` 能力当前由系统组件提供时，
//! 权限校验通过后转发到系统组件实例的同形导出（host-side 转发）；否则走
//! 宿主原语（本文件现状路径）。转发结果与宿主原语共用同一返回形状
//!（WIT `result<option<string>, string>` 载荷为 JSON 文本）。

use crate::wasm_core::manager::capability;
use crate::wasm_core::runtime_util::block_on_async;
use crate::wasm_core::permission::PERMISSION_STORAGE;
use crate::wasm_core::storage::SYSTEM_PLUGIN_ID;

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
pub(crate) fn storage_get(
    storage: &dyn crate::wasm_core::host_api::context::StorageScope,
    cap: &dyn crate::wasm_core::host_api::context::CapabilityScope,
    perm: &dyn crate::wasm_core::host_api::context::PermissionScope,
    plugin_id: &str,
    key: &str,
) -> Result<Option<serde_json::Value>, String> {
    if !super::check_permission(perm, plugin_id, PERMISSION_STORAGE, "host_storage_get") {
        return Err("permission denied".to_string());
    }
    ensure_not_system_space(plugin_id)?;
    // 能力路由：系统组件提供者命中时转发（组件间不共享内存，WIT 边界序列化）
    if let Some(result) = capability::forward_storage_get(cap, plugin_id, key) {
        return result.map(|opt| opt.map(|s| serde_json::from_str(&s).unwrap_or(serde_json::Value::String(s))));
    }
    let storage = storage.storage().clone();
    block_on_async(storage.get(plugin_id, key)).map_err(|e| format!("storage error: {}", e))
}

/// 设置值（权限校验 + 服务调用）
pub(crate) fn storage_set(
    storage: &dyn crate::wasm_core::host_api::context::StorageScope,
    cap: &dyn crate::wasm_core::host_api::context::CapabilityScope,
    perm: &dyn crate::wasm_core::host_api::context::PermissionScope,
    plugin_id: &str,
    key: &str,
    value: serde_json::Value,
) -> Result<(), String> {
    if !super::check_permission(perm, plugin_id, PERMISSION_STORAGE, "host_storage_set") {
        return Err("permission denied".to_string());
    }
    ensure_not_system_space(plugin_id)?;
    if let Some(result) = capability::forward_storage_set(cap, plugin_id, key, &value.to_string()) {
        return result;
    }
    let storage = storage.storage().clone();
    block_on_async(storage.set(plugin_id, key, value)).map_err(|e| format!("storage error: {}", e))
}

/// 删除值（权限校验 + 服务调用）
pub(crate) fn storage_delete(
    storage: &dyn crate::wasm_core::host_api::context::StorageScope,
    cap: &dyn crate::wasm_core::host_api::context::CapabilityScope,
    perm: &dyn crate::wasm_core::host_api::context::PermissionScope, plugin_id: &str, key: &str) -> Result<(), String> {
    if !super::check_permission(perm, plugin_id, PERMISSION_STORAGE, "host_storage_delete") {
        return Err("permission denied".to_string());
    }
    ensure_not_system_space(plugin_id)?;
    if let Some(result) = capability::forward_storage_delete(cap, plugin_id, key) {
        return result;
    }
    let storage = storage.storage().clone();
    block_on_async(storage.delete(plugin_id, key)).map_err(|e| format!("storage error: {}", e))
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wasm_core::host_api::tests::{build_host_ctx, grant_permissions};

    const PLUGIN: &str = "test-plugin";

    /// 未授权插件（从未 grant）：storage 三个操作均被权限门禁拒绝
    #[test]
    fn storage_ops_permission_denied() {
        let ctx = build_host_ctx();
        assert_eq!(storage_get(ctx.as_ref(), ctx.as_ref(), ctx.as_ref(), PLUGIN, "k").unwrap_err(), "permission denied");
        assert_eq!(
            storage_set(ctx.as_ref(), ctx.as_ref(), ctx.as_ref(), PLUGIN, "k", serde_json::json!(1)).unwrap_err(),
            "permission denied"
        );
        assert_eq!(storage_delete(ctx.as_ref(), ctx.as_ref(), ctx.as_ref(), PLUGIN, "k").unwrap_err(), "permission denied");
    }

    /// 授权后 set/get/delete 往返 + 插件间隔离 + 缺失 key 返回 None
    #[tokio::test]
    async fn storage_set_get_delete_roundtrip() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_STORAGE]);
        let value = serde_json::json!({ "count": 3, "tags": ["a", "b"] });

        storage_set(ctx.as_ref(), ctx.as_ref(), ctx.as_ref(), PLUGIN, "cfg", value.clone()).expect("set ok");
        assert_eq!(storage_get(ctx.as_ref(), ctx.as_ref(), ctx.as_ref(), PLUGIN, "cfg").expect("get ok").expect("value"), value);
        // 插件间隔离：另一个插件读不到（key 按 plugin_id 分区）——
        // 需先授权该插件，否则在权限门禁处就被拒绝，无法触达存储层语义
        grant_permissions(&ctx, "other-plugin", &[PERMISSION_STORAGE]);
        assert!(storage_get(ctx.as_ref(), ctx.as_ref(), ctx.as_ref(), "other-plugin", "cfg").expect("get ok").is_none());
        // 未设置的 key 返回 None
        assert!(storage_get(ctx.as_ref(), ctx.as_ref(), ctx.as_ref(), PLUGIN, "missing").expect("get ok").is_none());

        storage_delete(ctx.as_ref(), ctx.as_ref(), ctx.as_ref(), PLUGIN, "cfg").expect("delete ok");
        assert!(storage_get(ctx.as_ref(), ctx.as_ref(), ctx.as_ref(), PLUGIN, "cfg").expect("get ok").is_none());
        // 删除不存在的 key 幂等
        storage_delete(ctx.as_ref(), ctx.as_ref(), ctx.as_ref(), PLUGIN, "cfg").expect("delete again ok");
    }

    /// R-02 纵深防御：系统空间（`__system__`）对插件面原语不可达——即便权限
    /// 已授予，读/写/删系统空间一律拒绝（激活状态、审批记录等宿主真源不可触碰）
    #[test]
    fn system_space_rejected_on_plugin_primitives() {
        let ctx = build_host_ctx();
        // 正常插件 + 模拟“系统 id 被误授权限”（buggy 路径）：权限门过了
        // 也必须在纵深守卫处被拒
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_STORAGE]);
        grant_permissions(&ctx, SYSTEM_PLUGIN_ID, &[PERMISSION_STORAGE]);

        let err = storage_get(ctx.as_ref(), ctx.as_ref(), ctx.as_ref(), SYSTEM_PLUGIN_ID, "activation_state")
            .expect_err("系统空间读取必须被拒");
        assert!(err.contains("system storage space"), "实际: {err}");
        assert!(
            storage_set(ctx.as_ref(), ctx.as_ref(), ctx.as_ref(), SYSTEM_PLUGIN_ID, "k", serde_json::json!(1))
                .is_err(),
            "系统空间写入必须被拒"
        );
        assert!(
            storage_delete(ctx.as_ref(), ctx.as_ref(), ctx.as_ref(), SYSTEM_PLUGIN_ID, "activation_state").is_err(),
            "系统空间删除必须被拒"
        );
        // 正常插件空间不受影响
        assert!(storage_get(ctx.as_ref(), ctx.as_ref(), ctx.as_ref(), PLUGIN, "k").expect("get ok").is_none());
    }
}
