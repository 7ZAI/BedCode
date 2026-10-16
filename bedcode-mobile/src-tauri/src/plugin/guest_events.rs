//! 移动 guest 事件派发器（票 05 绑定归位 / ADR 0022「机制留内核、契约面上移」）
//!
//! 内核对 `events` 导出**零绑定**（票 03 起）：事件循环机制（队列、属主化、背压、
//! 燃料续费、trap 记账）在 wasm-core，而「调哪个导出、什么签名」是各端 WIT 契约
//! 事实——移动词汇 = `on-bus-message` / `on-auth-success` / `on-disconnect` /
//! `on-session-created` / `on-session-stopped`（桌面是 `on-message` /
//! `on-process-done`）。本文件用移动宿主自有绑定 [`super::bindings`] 实现该契约面
//! 并注入运行时（`WasmRuntime::set_guest_events`，在 `PluginManager::
//! init_wasm_runtime_with` 装配，与 message_bus dispatcher 同批）。
//!
//! `events.*` 返回 WIT `result<_, string>`，原生 `TypedFunc` 表达不了（`Result`
//! 不实现 `ComponentNamedList`）——这是契约面必须由绑定实现、而不是内核按名探测的
//! 根本原因。
//!
//! **失败语义**（与桌面实现逐字同款）：guest 自报失败记 warn 后返回 `Ok`（事件投递
//! fire-and-forget，单个处理失败不该拖垮分发链）；调用级失败（trap / 导出缺失）
//! 上抛，由内核侧记 trap 并按机制处理。

use bedcode_wasm_core::manager::runtime::guest_events::GuestEventDispatcher;
use bedcode_wasm_core::manager::runtime::WasmPluginState;
use wasmtime::Store;

use super::bindings;

/// 移动 `events` 导出派发实现
#[derive(Debug, Default, Clone, Copy)]
pub struct MobileGuestEvents;

/// 观察型回调的统一收口：guest 自报失败记 warn 后返回 Ok，trap 上抛
fn settle(
    outcome: std::result::Result<std::result::Result<(), String>, wasmtime::Error>,
    export: &str,
) -> bedcode_wasm_core::Result<()> {
    match outcome {
        Ok(Ok(())) => Ok(()),
        Ok(Err(msg)) => {
            tracing::warn!(export, "WASM export reported failure: {msg}");
            Ok(())
        }
        Err(e) => Err(bedcode_wasm_core::AppError::Plugin(format!(
            "WASM {}() call failed: {}",
            export, e
        ))),
    }
}

impl GuestEventDispatcher for MobileGuestEvents {
    /// 总线消息投递（移动 `events.on-bus-message`）
    ///
    /// 移动 WIT 的签名是 `(topic, payload-json)`，**没有 sender**——入参 `sender`
    /// 仍按内核统一签名接收（内核不知道两端的 events 词汇差异），此处不消费。
    fn on_message(
        &self,
        store: &mut Store<WasmPluginState>,
        instance: wasmtime::component::Instance,
        topic: &str,
        _sender: &str,
        payload_json: &str,
    ) -> bedcode_wasm_core::Result<()> {
        // 导出视图打开失败即 fail-visible（不是「拿不到就跳过投递」）
        let exports = bindings::Plugin::new(&mut *store, &instance).map_err(|e| {
            bedcode_wasm_core::AppError::Plugin(format!(
                "WASM component exports access failed: {}",
                e
            ))
        })?;
        let events = exports.bedcode_plugin_events();
        settle(
            bedcode_wasm_core::runtime_util::block_on_async(events.call_on_bus_message(
                &mut *store,
                topic,
                payload_json,
            )),
            "events.on-bus-message",
        )
    }

    /// 进程执行完成（桌面 `events.on-process-done`）——移动 world 无此导出
    ///
    /// 移动端没有 host-process 域，该回调在移动宿主**永不触发**；显性报错
    /// （而非静默 Ok）是为了让「误接桌面事件词汇」当场暴露，而不是被吞掉。
    fn on_process_done(
        &self,
        _store: &mut Store<WasmPluginState>,
        _instance: wasmtime::component::Instance,
        _payload_json: &str,
    ) -> bedcode_wasm_core::Result<()> {
        Err(bedcode_wasm_core::AppError::Plugin(
            "mobile guest events: events.on-process-done does not exist in the mobile world".to_string(),
        ))
    }

    fn on_auth_success(
        &self,
        store: &mut Store<WasmPluginState>,
        instance: wasmtime::component::Instance,
        ) -> bedcode_wasm_core::Result<()> {
        let exports = bindings::Plugin::new(&mut *store, &instance).map_err(|e| {
            bedcode_wasm_core::AppError::Plugin(format!(
                "WASM component exports access failed: {}",
                e
            ))
        })?;
        let events = exports.bedcode_plugin_events();
        settle(
            bedcode_wasm_core::runtime_util::block_on_async(events.call_on_auth_success(&mut *store)),
            "events.on-auth-success",
        )
    }

    fn on_disconnect(
        &self,
        store: &mut Store<WasmPluginState>,
        instance: wasmtime::component::Instance,
        reason: &str,
    ) -> bedcode_wasm_core::Result<()> {
        let exports = bindings::Plugin::new(&mut *store, &instance).map_err(|e| {
            bedcode_wasm_core::AppError::Plugin(format!(
                "WASM component exports access failed: {}",
                e
            ))
        })?;
        let events = exports.bedcode_plugin_events();
        settle(
            bedcode_wasm_core::runtime_util::block_on_async(
                events.call_on_disconnect(&mut *store, reason),
            ),
            "events.on-disconnect",
        )
    }

    fn on_session_created(
        &self,
        store: &mut Store<WasmPluginState>,
        instance: wasmtime::component::Instance,
        session_id: &str,
    ) -> bedcode_wasm_core::Result<()> {
        let exports = bindings::Plugin::new(&mut *store, &instance).map_err(|e| {
            bedcode_wasm_core::AppError::Plugin(format!(
                "WASM component exports access failed: {}",
                e
            ))
        })?;
        let events = exports.bedcode_plugin_events();
        settle(
            bedcode_wasm_core::runtime_util::block_on_async(
                events.call_on_session_created(&mut *store, session_id),
            ),
            "events.on-session-created",
        )
    }

    fn on_session_stopped(
        &self,
        store: &mut Store<WasmPluginState>,
        instance: wasmtime::component::Instance,
        session_id: &str,
    ) -> bedcode_wasm_core::Result<()> {
        let exports = bindings::Plugin::new(&mut *store, &instance).map_err(|e| {
            bedcode_wasm_core::AppError::Plugin(format!(
                "WASM component exports access failed: {}",
                e
            ))
        })?;
        let events = exports.bedcode_plugin_events();
        settle(
            bedcode_wasm_core::runtime_util::block_on_async(
                events.call_on_session_stopped(&mut *store, session_id),
            ),
            "events.on-session-stopped",
        )
    }
}
