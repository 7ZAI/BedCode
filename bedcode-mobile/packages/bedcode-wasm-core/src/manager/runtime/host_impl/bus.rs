//! host_bus_* — 消息总线（adapter：[`WasmPluginState`] → 共享实现核）
//!
//! 门禁语义（topic 形态机制 / 命名空间门 / 订阅面门 / 互调门 / 发布判定链）在
//! `bedcode-host-api-core::bus`（票 18 批次 2 起双端单点，机制修复一次生效）；本文件
//! 只剩「`WasmPluginState` → 共享核 [`BusPorts`]」的 adapter 与投递机制
//! （`block_in_place` 驱动；队列与订阅簿是本 crate `crate::bus` 的形状，票 18 §2
//! 「抽语义，队列留各端」）。
//!
//! **行为对齐（票 18 批次 2）**——此前移动无任何总线门禁（双份漂移税实例），自本批
//! 起与桌面同文同语义：
//! 1. **命名空间门**：`<owner>::<name>` 形态只有属主（与宿主）可发布/订阅；
//! 2. **订阅面门**：回复道 `bedcode.api.reply.*` 对 WASM 关闭 + legacy 定向形态
//!    显式拒绝并回带新形态；退订只过命名空间门（清理动作幂等可用）；
//! 3. **互调门**：移动 WIT 无 host-api-call 域（ADR 0018），适配器恒放行 = 既有行为；
//! 4. **JSON 严格解析**：发布载荷非法 JSON 由「降级为原始串 + warn」改为显性拒绝
//!    （fail-visible：降级会让 guest 以为已投递、订阅方收到形状不同的载荷。SDK
//!    `bus_publish` 收 `serde_json::Value`，序列化恒为合法 JSON，既有插件不受影响
//!    ——实测 file-transfer 是唯一 bus 消费面且走公开道）。
//!
//! 权限拒绝文案逐字保留（`permission denied: bus`）。

use super::super::WasmPluginState;
use super::support::guarded_host_call;
use bedcode_host_api_core::bus as core_bus;
use bedcode_host_api_core::bus::BusPorts;
use bedcode_host_api_core::gate::PermissionGate;
use bedcode_plugin_api_mobile::permission::PERMISSION_BUS;

/// [`WasmPluginState`] → 共享核 [`BusPorts`] 的移动 adapter
struct StateBusPorts<'a> {
    state: &'a WasmPluginState,
}

impl BusPorts for StateBusPorts<'_> {
    fn check_permission(&self, _plugin_id: &str, permission: &str, _api: &str) -> bool {
        // 单调用方纪律：实现层传入的 plugin_id 恒等于 state.plugin_id（逻辑层唯一入口）
        self.state.granted_permissions.contains(permission)
    }

    fn authorize_api_call(&self, _plugin_id: &str, _api: &str) -> bool {
        // 移动 WIT 无 host-api-call 域（ADR 0018）——api 道免互调门 = 既有行为
        true
    }

    fn publish_json(&self, topic: &str, sender: &str, payload: serde_json::Value) {
        self.state.host_ctx.message_bus.publish(topic, sender, payload);
    }

    fn publish_binary(&self, topic: &str, sender: &str, payload: Vec<u8>) {
        self.state.host_ctx.message_bus.publish_binary(topic, sender, payload);
    }
}

/// bus 域权限门（移动端有权限位；桌面同端口恒放行——两端策略分叉由端口承载，
/// 见共享核 bus 模块文档）
fn bus_gate(api: &'static str) -> PermissionGate<'static> {
    PermissionGate { permission: PERMISSION_BUS, api, deny_error: "permission denied: bus" }
}

/// 逻辑层：发布消息（共享核门禁链：权限位 → 严格 JSON → 命名空间 → 互调门 → 投递）
pub(crate) fn bus_publish(state: &WasmPluginState, topic: &str, payload_str: &str) -> Result<(), String> {
    core_bus::bus_publish(
        &StateBusPorts { state },
        &state.plugin_id,
        topic,
        payload_str,
        Some(&bus_gate("host_bus_publish")),
    )
}

/// 逻辑层：发布二进制消息（v9）——字节列原样透传（零 JSON 编解码，
/// 可传非 UTF-8 与大载荷）；门禁语义与 JSON 发布一致
pub(crate) fn bus_publish_binary(state: &WasmPluginState, topic: &str, payload: Vec<u8>) -> Result<(), String> {
    core_bus::bus_publish_binary(
        &StateBusPorts { state },
        &state.plugin_id,
        topic,
        payload,
        Some(&bus_gate("host_bus_publish_binary")),
    )
}

/// 逻辑层：订阅 topic（门禁在投递之前同步判定：错误必须回给 guest，
/// 不能退化成「返回 Ok 但没订阅上」；block_in_place 驱动订阅簿写入）
pub(crate) fn bus_subscribe(state: &WasmPluginState, topic: &str) -> Result<(), String> {
    let ports = StateBusPorts { state };
    core_bus::check_gate(&ports, &state.plugin_id, Some(&bus_gate("host_bus_subscribe")))?;
    core_bus::check_subscribe_access(&state.plugin_id, topic)?;
    guarded_host_call(&state.plugin_id, "host_bus_subscribe", (), || {
        tokio::task::block_in_place(|| {
            state
                .runtime_handle
                .block_on(state.host_ctx.message_bus.subscribe_wasm(&state.plugin_id, topic))
        })
    });
    Ok(())
}

/// 逻辑层：以二进制格式偏好订阅（v9）——只接收 publish-binary 投递
pub(crate) fn bus_subscribe_binary(state: &WasmPluginState, topic: &str) -> Result<(), String> {
    let ports = StateBusPorts { state };
    core_bus::check_gate(&ports, &state.plugin_id, Some(&bus_gate("host_bus_subscribe_binary")))?;
    core_bus::check_subscribe_access(&state.plugin_id, topic)?;
    guarded_host_call(&state.plugin_id, "host_bus_subscribe_binary", (), || {
        tokio::task::block_in_place(|| {
            state.runtime_handle.block_on(
                state
                    .host_ctx
                    .message_bus
                    .subscribe_wasm_binary(&state.plugin_id, topic),
            )
        })
    });
    Ok(())
}

/// 逻辑层：取消订阅（只过权限位与命名空间门——legacy/回复道不拦，
/// 退订是清理动作，必须幂等可用）
pub(crate) fn bus_unsubscribe(state: &WasmPluginState, topic: &str) -> Result<(), String> {
    let ports = StateBusPorts { state };
    core_bus::check_gate(&ports, &state.plugin_id, Some(&bus_gate("host_bus_unsubscribe")))?;
    core_bus::check_namespace(&state.plugin_id, topic, "unsubscribe", "that plugin (and the host)")?;
    guarded_host_call(&state.plugin_id, "host_bus_unsubscribe", (), || {
        tokio::task::block_in_place(|| {
            state
                .runtime_handle
                .block_on(state.host_ctx.message_bus.unsubscribe(&state.plugin_id, topic))
        })
    });
    Ok(())
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

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

    /// 无操作 dispatcher：publish 要求 dispatcher 已注入，静态订阅不实际使用它
    struct NoopDispatcher;

    #[async_trait::async_trait]
    impl crate::bus::MessageDispatcher for NoopDispatcher {
        async fn dispatch_to_wasm(
            &self,
            _plugin_id: &str,
            _msg: &bedcode_plugin_api_mobile::BusMessage,
        ) -> anyhow::Result<()> {
            Ok(())
        }

        async fn is_activated(&self, _plugin_id: &str) -> bool {
            true
        }
    }

    /// 静态订阅者：把收到的消息转发到 mpsc 通道供断言
    struct ChannelHandler(tokio::sync::mpsc::UnboundedSender<bedcode_plugin_api_mobile::BusMessage>);

    impl crate::bus::BusMessageHandler for ChannelHandler {
        fn on_message(&self, msg: &bedcode_plugin_api_mobile::BusMessage) -> anyhow::Result<()> {
            let _ = self.0.send(msg.clone());
            Ok(())
        }
    }

    /// 授权插件发布公开 topic：门禁放行 + 静态订阅者收到完整消息
    /// （topic/sender/payload 原样——行为对齐后既有公开道零回归）
    #[tokio::test(flavor = "multi_thread")]
    async fn bus_publish_public_topic_delivers_to_subscriber() {
        let state = state_with("plugin-a", &[PERMISSION_BUS]);
        state.host_ctx.message_bus.set_dispatcher(Arc::new(NoopDispatcher)).await;
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        state
            .host_ctx
            .message_bus
            .subscribe_static("plugin-b", "greeting", Box::new(ChannelHandler(tx)))
            .await;

        bus_publish(&state, "greeting", r#"{"hello":"world"}"#).expect("publish ok");

        let msg = rx.recv().await.expect("subscriber must receive message");
        assert_eq!(msg.topic, "greeting");
        assert_eq!(msg.sender, "plugin-a");
        assert_eq!(msg.payload, serde_json::json!({ "hello": "world" }));
    }

    /// 行为对齐①：他人命名空间伪发布被命名空间门拒绝（此前移动无此门）
    #[tokio::test(flavor = "multi_thread")]
    async fn bus_publish_rejects_other_namespace() {
        let state = state_with("com.evil", &[PERMISSION_BUS]);
        let err = bus_publish(&state, "com.victim::pty:exit", "{}").unwrap_err();
        assert!(err.contains("namespace") && err.contains("com.victim"), "got: {err}");
    }

    /// 行为对齐②：非法 JSON 发布显性拒绝（此前降级为原始串 + warn）
    #[tokio::test(flavor = "multi_thread")]
    async fn bus_publish_rejects_invalid_json() {
        let state = state_with("plugin-a", &[PERMISSION_BUS]);
        let err = bus_publish(&state, "t", "not-json").unwrap_err();
        assert!(
            err.starts_with("bus error: invalid JSON payload:") && err.contains("expected"),
            "got: {err}"
        );
    }

    /// 权限拒绝文案逐字保留
    #[tokio::test(flavor = "multi_thread")]
    async fn bus_permission_denied_text_unchanged() {
        let state = state_with("plugin-a", &[]);
        assert_eq!(bus_publish(&state, "t", "{}").unwrap_err(), "permission denied: bus");
        assert_eq!(bus_subscribe(&state, "t").unwrap_err(), "permission denied: bus");
        assert_eq!(bus_unsubscribe(&state, "t").unwrap_err(), "permission denied: bus");
    }

    /// 行为对齐③：订阅面门生效——他人命名空间 / 回复道 / legacy 拒绝；
    /// 自有命名空间、请求道、公开 topic 放行；legacy 退订放行（清理幂等）
    #[tokio::test(flavor = "multi_thread")]
    async fn bus_subscribe_gates_now_enforced_on_mobile() {
        let state = state_with("com.victim", &[PERMISSION_BUS]);
        let err = bus_subscribe(&state, "com.other::inbox").unwrap_err();
        assert!(err.contains("namespace"), "got: {err}");
        let err = bus_subscribe(&state, "bedcode.api.reply.com.victim.req-1").unwrap_err();
        assert!(err.contains("reply"), "got: {err}");
        let err = bus_subscribe(&state, "pty:exit.com.victim").unwrap_err();
        assert!(err.contains("legacy") && err.contains("com.victim::pty:exit"), "got: {err}");
        bus_subscribe(&state, "com.victim::pty:exit").expect("own ns ok");
        bus_subscribe(&state, "bedcode.api.com.victim.echo").expect("request lane ok");
        bus_subscribe(&state, "peer:consent").expect("public ok");
        bus_unsubscribe(&state, "pty:exit.com.victim").expect("legacy 退订放行");
    }
}
