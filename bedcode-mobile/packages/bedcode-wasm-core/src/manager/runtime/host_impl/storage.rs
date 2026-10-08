//! host_storage_* — 插件键值存储（adapter：[`WasmPluginState`] → 共享实现核）
//!
//! 机制语义（权限门 / 系统空间纵深守卫 / 键值原语）在 `bedcode-host-api-core::storage`
//! （票 18 起双端单点，机制修复一次生效）；本文件只剩「`WasmPluginState` → 共享核
//! [`StoragePorts`] 端口」的 adapter，逻辑层签名与既有调用面（`component.rs` 的
//! Host trait impl）零改动。
//!
//! **行为对齐（票 18 批次 1）**：共享核的系统空间纵深守卫（`__system__` 对插件面
//! 原语 fail-closed）自本票起对移动端同样生效——此前移动缺该守卫，是双份漂移税的
//! 实例（桌面 `host_api/storage.rs` 一直有）。权限拒绝文案逐字保留
//! （`permission denied: storage`）；存储失败经共享层统一包装 `storage error: {e}`。
//! **顺序注记**：set 的 JSON 解析从「权限门之后」移到「权限门之前」（共享层签名为
//! serde_json 规范形，与桌面 component.rs 既有形态一致）——未授权 + 非法 JSON 的
//! 边缘入参错误文本由权限文案变为解析文案；授权路径行为零变化。

use super::super::WasmPluginState;
use super::support::guarded_host_call;
use bedcode_host_api_core::storage::{PermissionGate, StoragePorts};
use bedcode_plugin_api_mobile::permission::PERMISSION_STORAGE;

/// [`WasmPluginState`] → 共享核 [`StoragePorts`] 的移动 adapter
///
/// 权限判定读 `granted_permissions` 集（manifest 授权面）；键值原语驱动异步存储：
/// `block_in_place` + `runtime_handle.block_on`，panic 经 `guarded_host_call`
/// 截获（fallback 文本与迁移前逐字一致）。
struct StateStoragePorts<'a> {
    state: &'a WasmPluginState,
}

impl StoragePorts for StateStoragePorts<'_> {
    fn check_permission(&self, _plugin_id: &str, permission: &str, _api: &str) -> bool {
        // 单调用方纪律：实现层传入的 plugin_id 恒等于 state.plugin_id（逻辑层唯一入口）
        self.state.granted_permissions.contains(permission)
    }

    fn kv_get(&self, plugin_id: &str, key: &str) -> Result<Option<serde_json::Value>, String> {
        let storage = self.state.host_ctx.storage.clone();
        let pid = plugin_id.to_string();
        let key = key.to_string();
        guarded_host_call(
            &self.state.plugin_id,
            "host_storage_get",
            Err(crate::AppError::Internal("host_storage_get panicked".to_string())),
            || tokio::task::block_in_place(|| self.state.runtime_handle.block_on(storage.get(&pid, &key))),
        )
        .map_err(|e| e.to_string())
    }

    fn kv_set(&self, plugin_id: &str, key: &str, value: serde_json::Value) -> Result<(), String> {
        let storage = self.state.host_ctx.storage.clone();
        let pid = plugin_id.to_string();
        let key = key.to_string();
        guarded_host_call(
            &self.state.plugin_id,
            "host_storage_set",
            Err(crate::AppError::Internal("host_storage_set panicked".to_string())),
            || {
                tokio::task::block_in_place(|| {
                    self.state.runtime_handle.block_on(storage.set(&pid, &key, value))
                })
            },
        )
        .map_err(|e| e.to_string())
    }

    fn kv_delete(&self, plugin_id: &str, key: &str) -> Result<(), String> {
        let storage = self.state.host_ctx.storage.clone();
        let pid = plugin_id.to_string();
        let key = key.to_string();
        guarded_host_call(
            &self.state.plugin_id,
            "host_storage_delete",
            Err(crate::AppError::Internal("host_storage_delete panicked".to_string())),
            || tokio::task::block_in_place(|| self.state.runtime_handle.block_on(storage.delete(&pid, &key))),
        )
        .map_err(|e| e.to_string())
    }
}

/// 逻辑层：读取值（共享实现核驱动；返回 JSON 字符串——值以 serde_json::Value
/// 存储，组件契约 WIT `option<string>` 承载 JSON 载荷）
pub(crate) fn storage_get(state: &WasmPluginState, key: &str) -> Result<Option<String>, String> {
    let value = bedcode_host_api_core::storage::storage_get(
        &StateStoragePorts { state },
        &state.plugin_id,
        key,
        &PermissionGate {
            permission: PERMISSION_STORAGE,
            api: "host_storage_get",
            deny_error: "permission denied: storage",
        },
    )?;
    value
        .map(|v| serde_json::to_string(&v).map_err(|e| format!("JSON serialize failed: {}", e)))
        .transpose()
}

/// 逻辑层：设置值（value 为 JSON 字符串，送入前解析；组件契约 WIT 与桌面同语义：
/// set(key, value: string) 先 JSON 解析）
pub(crate) fn storage_set(state: &WasmPluginState, key: &str, value: &str) -> Result<(), String> {
    let json_value: serde_json::Value =
        serde_json::from_str(value).map_err(|e| format!("invalid JSON value: {}", e))?;
    bedcode_host_api_core::storage::storage_set(
        &StateStoragePorts { state },
        &state.plugin_id,
        key,
        json_value,
        &PermissionGate {
            permission: PERMISSION_STORAGE,
            api: "host_storage_set",
            deny_error: "permission denied: storage",
        },
    )
}

/// 逻辑层：删除值
pub(crate) fn storage_delete(state: &WasmPluginState, key: &str) -> Result<(), String> {
    bedcode_host_api_core::storage::storage_delete(
        &StateStoragePorts { state },
        &state.plugin_id,
        key,
        &PermissionGate {
            permission: PERMISSION_STORAGE,
            api: "host_storage_delete",
            deny_error: "permission denied: storage",
        },
    )
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造最小 `WasmPluginState`（host_impl 是 manager::runtime 的子模块，
    /// 私有字段构造合法；宿主上下文用 test_support 的无头夹具）
    fn state_with(plugin_id: &str, granted: &[&str]) -> WasmPluginState {
        WasmPluginState {
            plugin_id: plugin_id.to_string(),
            host_ctx: crate::test_support::build_host_ctx(),
            runtime_handle: tokio::runtime::Handle::current(),
            granted_permissions: granted.iter().map(|s| s.to_string()).collect(),
            on_message_binary: None,
        }
    }

    /// 授权插件经共享核 adapter 的 set/get/delete 往返（JSON 串 wire 形）
    #[tokio::test(flavor = "multi_thread")]
    async fn storage_roundtrip_via_shared_core() {
        let state = state_with("test-plugin", &[PERMISSION_STORAGE]);
        storage_set(&state, "cfg", r#"{"count":3}"#).expect("set ok");
        assert_eq!(
            storage_get(&state, "cfg").expect("get ok"),
            Some(r#"{"count":3}"#.to_string()),
            "get 必须返回 JSON 串 wire 形"
        );
        storage_delete(&state, "cfg").expect("delete ok");
        assert_eq!(storage_get(&state, "cfg").expect("get ok"), None);
    }

    /// 未授权插件：三条原语拒绝文案逐字保留（`permission denied: storage`）
    #[tokio::test(flavor = "multi_thread")]
    async fn storage_permission_denied_text_unchanged() {
        let state = state_with("test-plugin", &[]);
        assert_eq!(storage_get(&state, "k").unwrap_err(), "permission denied: storage");
        assert_eq!(storage_set(&state, "k", "1").unwrap_err(), "permission denied: storage");
        assert_eq!(storage_delete(&state, "k").unwrap_err(), "permission denied: storage");
    }

    /// 行为对齐：共享核系统空间纵深守卫对移动端生效（`__system__` 即便被误授权
    /// 也不可达——迁移前移动缺该守卫，票 18 起与桌面同文同语义）
    #[tokio::test(flavor = "multi_thread")]
    async fn system_space_guard_now_enforced_on_mobile() {
        let state = state_with(crate::storage::SYSTEM_PLUGIN_ID, &[PERMISSION_STORAGE]);
        let err = storage_get(&state, "activation_state").unwrap_err();
        assert!(err.contains("system storage space"), "实际: {err}");
        assert!(storage_set(&state, "k", "1").is_err(), "系统空间写入必须被拒");
        assert!(storage_delete(&state, "k").is_err(), "系统空间删除必须被拒");
    }
}
