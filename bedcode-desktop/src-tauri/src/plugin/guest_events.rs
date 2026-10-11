//! 桌面 guest 事件派发器（票 03 / ADR 0022）
//!
//! **机制留内核、契约面上移**：`events` 导出双端词汇不同（桌面 `on-message` /
//! `on-process-done`；移动是 `on-bus-message` / `on-auth-success` / …），且带
//! `result` 返回无法用原生 `TypedFunc` 表达（`Result` 不实现
//! `ComponentNamedList`）。故事件循环机制留 wasm-core，具体「调哪个导出、什么
//! 签名」由本文件用桌面自有绑定 [`super::bindings`] 实现并注入。
//!
//! 生产装配：`PluginHost::new` → `WasmRuntime::set_guest_events`（与 monitor /
//! peer_ctx_provider 同范式）。内核测试二进制不链宿主 lib，由 wasm-core 侧
//! `TestGuestEvents` 替身承担（逐字同语义）。

use bedcode_wasm_core::manager::runtime::guest_events::GuestEventDispatcher;
use bedcode_wasm_core::manager::runtime::WasmPluginState;
use wasmtime::Store;

use super::bindings;

/// 桌面 `events` 导出派发实现
#[derive(Debug, Default, Clone, Copy)]
pub struct DesktopGuestEvents;

/// 观察型回调的统一收口：guest 自报失败记 warn 后返回 Ok（不拖垮整条事件分发链），
/// trap 上抛（那是宿主侧问题）。口径与内核 `TestGuestEvents` 逐字一致。
fn settle(outcome: std::result::Result<std::result::Result<(), String>, wasmtime::Error>, export: &str) -> bedcode_wasm_core::Result<()> {
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

impl GuestEventDispatcher for DesktopGuestEvents {
    fn on_message(
        &self,
        store: &mut Store<WasmPluginState>,
        instance: wasmtime::component::Instance,
        topic: &str,
        sender: &str,
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
            bedcode_wasm_core::runtime_util::block_on_async(
                events.call_on_message(&mut *store, topic, sender, payload_json),
            ),
            "events.on-message",
        )
    }

    fn on_process_done(
        &self,
        store: &mut Store<WasmPluginState>,
        instance: wasmtime::component::Instance,
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
            bedcode_wasm_core::runtime_util::block_on_async(
                events.call_on_process_done(&mut *store, payload_json),
            ),
            "events.on-process-done",
        )
    }
}
