//! host-connection —— 主连接事实读取（逻辑层，ABI v15 · 票 12；票 13 复用）
//!
//! 只暴露**引擎事实**（最后配置的目标地址 / 端口 + 连接状态），零业务投影：
//! 不返回设备展示名（name 归插件自持）、不解释连接语义（B1–B6 零命中——
//! 「目标设备」是宿主连接管理器的传输事实，终端 / 会话控制等消费方各自
//! 赋予业务含义）。
//!
//! 无权限门（ADR 0022 例外先例 = host-platform）：地址是非凭据的局域网
//! 服务事实；连接行为本身已由 `ws:client` / `network:http` 等权限位门控。

use super::super::{WasmPluginState, block_on_async};

/// 逻辑层：读当前主连接目标（camelCase JSON `{ address, port, connected }`）
///
/// 从未配置过任何目标 → Err（fail-visible，禁「返回空对象」——真源读取
/// 缺链必须显性暴露，禁「查不到就返回空」静默降级形态）
pub(crate) fn connection_primary_target(state: &WasmPluginState) -> Result<String, String> {
    let conn = crate::state::get_connection_manager();
    let (target, connected) = block_on_async(&state.runtime_handle, async move {
        let t = conn.get_target().await;
        let c = conn.is_connected().await;
        (t, c)
    });
    let Some(target) = target else {
        return Err("connection: no primary target configured (device not paired/connected yet)".to_string());
    };
    serde_json::to_string(&serde_json::json!({
        "address": target.address,
        "port": target.port,
        "connected": connected,
    }))
    .map_err(|e| format!("connection primary-target: serialization failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// 未配置目标（测试进程的 ConnectionManager 单例尚未 set_target）→
    /// 显性 Err，非空对象（fail-visible 形态①）。注：单例全局——本用例
    /// 兼作「读面不 panic」的冒烟；配置过目标的断言在集成层覆盖
    #[test]
    fn no_target_fails_visibly() {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let db = Arc::new(std::sync::Mutex::new(
            rusqlite::Connection::open_in_memory().expect("open in-memory db"),
        ));
        let storage = crate::plugin::storage::PluginStorage::test_storage();
        let fs_auth = Arc::new(crate::plugin::fs_auth::FsAuthChecker::new(storage.clone(), None));
        let status_reporter: Arc<dyn Fn(&str, &str) + Send + Sync> = Arc::new(|_, _| {});
        let state = WasmPluginState {
            plugin_id: format!("com.bedcode.conn-{}", uuid::Uuid::new_v4()),
            host_ctx: Arc::new(crate::plugin::wasm_runtime::WasmHostContext::new(
                db,
                storage,
                None,
                fs_auth,
                Arc::new(crate::plugin::message_bus::MessageBus::new()),
                status_reporter,
            )),
            runtime_handle: rt.handle().clone(),
            granted_permissions: std::collections::HashSet::new(),
            on_message_binary: None,
        };
        // 单例可能在其它测试已 set_target——两种结果都合法，但形态必须二选一：
        // 合法 JSON 事实 或 显性 Err（禁第三种「成功但字段空」）
        match connection_primary_target(&state) {
            Ok(json) => {
                let v: serde_json::Value = serde_json::from_str(&json).expect("valid json");
                assert!(v.get("address").and_then(|a| a.as_str()).is_some());
                assert!(v.get("port").and_then(|p| p.as_u64()).is_some());
                assert!(v.get("connected").and_then(|c| c.as_bool()).is_some());
            }
            Err(e) => assert!(e.contains("no primary target"), "{e}"),
        }
    }
}
