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
    let engine = state.host_ctx.ports.connection_engine();
    let Some((target, connected)) =
        block_on_async(&state.runtime_handle, async move { engine.primary_target().await })
            .map_err(|e| format!("connection primary-target: engine read failed: {e}"))?
    else {
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
    use crate::host_api::ports::{ConnectionEnginePort, PrimaryTarget};
    use std::sync::Arc;

    /// 无目标引擎 mock：`Ok(None)`（从未配置目标）
    struct NoTargetEngine;

    #[async_trait::async_trait]
    impl ConnectionEnginePort for NoTargetEngine {
        async fn primary_target(&self) -> crate::Result<Option<(PrimaryTarget, bool)>> {
            Ok(None)
        }
    }

    /// 已配置目标引擎 mock：`Ok(Some((target, connected)))`（wire 形状投影）
    struct ConnectedEngine;

    #[async_trait::async_trait]
    impl ConnectionEnginePort for ConnectedEngine {
        async fn primary_target(&self) -> crate::Result<Option<(PrimaryTarget, bool)>> {
            Ok(Some((PrimaryTarget { address: "192.168.1.5".into(), port: 4455 }, true)))
        }
    }

    fn state_with(engine: Arc<dyn ConnectionEnginePort>) -> (WasmPluginState, tokio::runtime::Runtime) {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let db = Arc::new(std::sync::Mutex::new(crate::db::Database::from_connection(
            rusqlite::Connection::open_in_memory().expect("open in-memory db"),
        )));
        let storage = crate::storage::PluginStorage::test_storage();
        let fs_auth = Arc::new(crate::security::fs_auth::FsAuthChecker::new(storage.clone(), None, Vec::new()));
        let status_reporter: Arc<dyn Fn(&str, &str) + Send + Sync> = Arc::new(|_, _| {});
        let state = WasmPluginState {
            plugin_id: format!("com.bedcode.conn-{}", uuid::Uuid::new_v4()),
            host_ctx: Arc::new(crate::manager::runtime::WasmHostContext::new(
                db,
                storage,
                None,
                fs_auth,
                Arc::new(crate::bus::MessageBus::new()),
                status_reporter,
                std::sync::Arc::new(
                    crate::test_support::MockPorts::new().with_connection(engine),
                ),
            )),
            runtime_handle: rt.handle().clone(),
            granted_permissions: std::collections::HashSet::new(),
            on_message_binary: None,
        };
        (state, rt)
    }

    /// 无目标（引擎读数 None）→ 显性 Err，非空对象（fail-visible 形态①，
    /// 禁「查不到就返回空」静默降级）
    #[test]
    fn no_target_fails_visibly() {
        let (state, _rt) = state_with(Arc::new(NoTargetEngine));
        let err = connection_primary_target(&state).expect_err("no target must fail visibly");
        assert!(err.contains("no primary target"), "{err}");
    }

    /// 引擎读数 → camelCase JSON 三字段（address/port/connected）wire 契约
    #[test]
    fn target_readings_project_to_wire_shape() {
        let (state, _rt) = state_with(Arc::new(ConnectedEngine));
        let json = connection_primary_target(&state).expect("target present");
        let v: serde_json::Value = serde_json::from_str(&json).expect("valid json");
        assert_eq!(v["address"], "192.168.1.5");
        assert_eq!(v["port"], 4455);
        assert_eq!(v["connected"], true);
    }
}
