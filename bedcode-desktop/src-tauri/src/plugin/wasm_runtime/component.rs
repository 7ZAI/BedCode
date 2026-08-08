//! Component Model 支持（迁移阶段 C：唯一形态）
//!
//! 对应 docs/knowledge/wasmtime-component-migration.md：
//! - 契约定义在 `packages/plugin-sdk-desktop/rust/wit/bedcode.wit`
//!   （单一事实来源），本模块用 `bindgen!` 生成绑定：
//!   - import 接口 → `Host` trait，由本模块对 `WasmPluginState` 实现
//!   - export 接口 → `exports::bedcode::plugin::*::Guest`，宿主侧调用组件
//! - 已接线 13 组 import 接口（host-storage / host-log / host-config /
//!   host-terminal / host-database / host-plugin-database / host-session /
//!   host-timer / host-events / host-http / host-fs / host-bus /
//!   host-file-service / host-transfer），完整 `plugin` world 可直接实例化；
//!   接线模式见本文件 `add_to_linker` 与各 `impl ... Host` 块
//! - 宿主能力实现层在 `host_impl`（阶段 C 后仅此一层，core 胶水已删）
//!
//! ## 与 core module 路径的差异（历史，见 WIT 注释）
//!
//! - export 全部为必选：core ABI 中 on_message 等可选导出在组件契约中强制
//!   （组件 world 声明即契约，阶段 B SDK 无条件导出全部）
//! - log 不带 file/line 调用点（core ABI 经 ABI 传插件源码位置；
//!   组件形态暂无传递通道，见 wit/bedcode.wit 的 host-log 注释）
//! - 内存搬运由绑定层处理，无需 (ptr,len) 配对与 alloc/dealloc

use super::host_impl::{
    bus, config, database, events, file_service, fs, http, lifecycle, log, session, status,
    storage, terminal, timer, transfer,
};
use super::{WasmHostContext, WasmPluginState, EPOCH_GRACE_TICKS};
use crate::AppError;
use bedcode_plugin_api::abi;
use std::sync::Arc;
use wasmtime::component::{bindgen, Component, Instance, Linker};
use wasmtime::{ResourceLimiter, Store};

bindgen!({
    path: "../packages/plugin-sdk-desktop/rust/wit/bedcode.wit",
    world: "plugin",
});

// ==================== Host trait 实现（import 接口） ====================
//
// 每个接口对应 host_functions/ 中一个功能域的逻辑层函数；
// 返回值映射：WIT `result<T, string>` → `Result<T, String>`，错误内容为宿主侧
// 可读消息，跨 wasm 边界后由调用方（本模块方法）转为 AppError

impl bedcode::plugin::host_storage::Host for WasmPluginState {
    fn get(&mut self, key: String) -> Result<Option<String>, String> {
        storage::storage_get(&self.host_ctx, &self.plugin_id, &key)
            .map(|opt| opt.map(|v| v.to_string()))
    }

    fn set(&mut self, key: String, value: String) -> Result<(), String> {
        let json_value: serde_json::Value = serde_json::from_str(&value)
            .map_err(|e| format!("invalid JSON value: {}", e))?;
        storage::storage_set(&self.host_ctx, &self.plugin_id, &key, json_value)
    }

    fn delete(&mut self, key: String) -> Result<(), String> {
        storage::storage_delete(&self.host_ctx, &self.plugin_id, &key)
    }
}

impl bedcode::plugin::host_log::Host for WasmPluginState {
    fn info(&mut self, message: String) {
        log::log_info(&self.plugin_id, &message, "", 0);
    }

    fn debug(&mut self, message: String) {
        log::log_debug(&self.plugin_id, &message, "", 0);
    }

    fn warn(&mut self, message: String) {
        log::log_warn(&self.plugin_id, &message, "", 0);
    }

    fn error(&mut self, message: String) {
        log::log_error(&self.plugin_id, &message, "", 0);
    }

    fn mark_plugin_error(&mut self, error: String) {
        status::mark_plugin_error(&self.host_ctx, self.plugin_id.clone(), error);
    }
}

impl bedcode::plugin::host_config::Host for WasmPluginState {
    fn get(&mut self, key: String) -> Result<Option<String>, String> {
        config::config_get(&self.plugin_id, &key)
    }
}

impl bedcode::plugin::host_terminal::Host for WasmPluginState {
    fn send(&mut self, session_id: String, data: String) -> Result<(), String> {
        terminal::terminal_send(&self.host_ctx, &self.plugin_id, &session_id, &data)
    }
}

impl bedcode::plugin::host_database::Host for WasmPluginState {
    fn execute(&mut self, sql: String) -> Result<u32, String> {
        database::db_execute(&self.host_ctx, &self.plugin_id, &sql)
    }

    fn query(&mut self, sql: String) -> Result<Option<String>, String> {
        database::db_query(&self.host_ctx, &self.plugin_id, &sql)
    }

    fn execute_params(&mut self, sql: String, params_json: String) -> Result<u32, String> {
        database::db_execute_params(&self.host_ctx, &self.plugin_id, &sql, &params_json)
    }

    fn query_params(&mut self, sql: String, params_json: String) -> Result<Option<String>, String> {
        database::db_query_params(&self.host_ctx, &self.plugin_id, &sql, &params_json)
    }
}

impl bedcode::plugin::host_plugin_database::Host for WasmPluginState {
    fn execute(&mut self, sql: String) -> Result<u32, String> {
        database::plugin_db_execute(&self.host_ctx, &self.plugin_id, &sql)
    }

    fn query(&mut self, sql: String) -> Result<Option<String>, String> {
        database::plugin_db_query(&self.host_ctx, &self.plugin_id, &sql)
    }

    fn execute_params(&mut self, sql: String, params_json: String) -> Result<u32, String> {
        database::plugin_db_execute_params(&self.host_ctx, &self.plugin_id, &sql, &params_json)
    }

    fn query_params(&mut self, sql: String, params_json: String) -> Result<Option<String>, String> {
        database::plugin_db_query_params(&self.host_ctx, &self.plugin_id, &sql, &params_json)
    }
}

impl bedcode::plugin::host_session::Host for WasmPluginState {
    fn list_sessions(&mut self) -> Result<Option<String>, String> {
        session::session_list(&self.host_ctx, &self.plugin_id)
    }

    fn get(&mut self, session_id: String) -> Result<Option<String>, String> {
        session::session_get(&self.host_ctx, &self.plugin_id, &session_id)
    }

    fn config_list(&mut self) -> Result<Option<String>, String> {
        session::session_config_list(&self.host_ctx, &self.plugin_id)
    }

    fn lifecycle_register(&mut self) -> Result<(), String> {
        lifecycle::session_lifecycle_register(&self.host_ctx, &self.plugin_id)
    }

    fn input_register(&mut self) -> Result<(), String> {
        lifecycle::session_input_register(&self.host_ctx, &self.plugin_id)
    }

    fn create(&mut self, config_id: String) -> Result<String, String> {
        session::session_create(&self.host_ctx, &self.plugin_id, &config_id)
    }
}

impl bedcode::plugin::host_timer::Host for WasmPluginState {
    fn register(&mut self, interval_secs: u64, command: String) -> Result<(), String> {
        timer::timer_register(&self.host_ctx, &self.plugin_id, interval_secs, &command)
    }
}

impl bedcode::plugin::host_events::Host for WasmPluginState {
    // WIT 中 emit/broadcast-sync 无错误返回，宿主侧记录日志（与 core 胶水一致）
    fn emit(&mut self, event_name: String, payload_json: String) {
        if let Err(e) = events::emit_event(&self.host_ctx, &event_name, &payload_json) {
            tracing::error!(error = %e, event = %event_name, "host_events.emit failed");
        }
    }

    fn broadcast_sync(&mut self, event_json: String) {
        if let Err(e) = events::broadcast_sync(&self.host_ctx, &self.plugin_id, &event_json) {
            tracing::error!(error = %e, "host_events.broadcast_sync failed");
        }
    }

    fn notify(&mut self, title: String, body: String) -> Result<(), String> {
        events::notify(&self.host_ctx, &self.plugin_id, &title, &body)
    }
}

impl bedcode::plugin::host_http::Host for WasmPluginState {
    fn fetch(&mut self, request_json: String) -> Result<Option<String>, String> {
        http::http_fetch(&self.host_ctx, &self.plugin_id, &request_json)
    }
}

impl bedcode::plugin::host_fs::Host for WasmPluginState {
    fn read(&mut self, path: String) -> Result<Option<String>, String> {
        fs::fs_read(&self.host_ctx, &self.plugin_id, &path)
    }

    fn write(&mut self, path: String, data: String) -> Result<(), String> {
        fs::fs_write(&self.host_ctx, &self.plugin_id, &path, &data)
    }

    fn copy(&mut self, src: String, dst: String) -> Result<(), String> {
        fs::fs_copy(&self.host_ctx, &self.plugin_id, &src, &dst)
    }

    fn delete(&mut self, path: String) -> Result<(), String> {
        fs::fs_delete(&self.host_ctx, &self.plugin_id, &path)
    }

    fn exists(&mut self, path: String) -> Result<bool, String> {
        fs::fs_exists(&self.host_ctx, &self.plugin_id, &path)
    }
}

impl bedcode::plugin::host_bus::Host for WasmPluginState {
    fn publish(&mut self, topic: String, payload_json: String) -> Result<(), String> {
        bus::bus_publish(&self.host_ctx, &self.plugin_id, &topic, &payload_json)
    }

    fn subscribe(&mut self, topic: String) -> Result<(), String> {
        bus::bus_subscribe(&self.host_ctx, &self.plugin_id, &topic)
    }

    fn unsubscribe(&mut self, topic: String) -> Result<(), String> {
        bus::bus_unsubscribe(&self.host_ctx, &self.plugin_id, &topic)
    }
}

impl bedcode::plugin::host_file_service::Host for WasmPluginState {
    fn mount(&mut self, options_json: String) -> Result<String, String> {
        file_service::filesrv_mount(&self.host_ctx, &self.plugin_id, &options_json)
    }

    fn unmount(&mut self, mount_path: String) -> Result<(), String> {
        file_service::filesrv_unmount(&self.host_ctx, &self.plugin_id, &mount_path)
    }

    fn update_roots(&mut self, mount_path: String, roots_json: String) -> Result<(), String> {
        file_service::filesrv_update_roots(&self.host_ctx, &self.plugin_id, &mount_path, &roots_json)
    }

    fn get_peer(&mut self, peer_id: String) -> Result<Option<String>, String> {
        file_service::filesrv_get_peer(&self.host_ctx, &self.plugin_id, &peer_id)
    }

    fn query_peer(&mut self, peer_id: String) -> Result<(), String> {
        file_service::filesrv_query_peer(&self.host_ctx, &self.plugin_id, &peer_id)
    }
}

impl bedcode::plugin::host_transfer::Host for WasmPluginState {
    fn start(&mut self, request_json: String) -> Result<String, String> {
        transfer::transfer_start(&self.host_ctx, &self.plugin_id, &request_json)
    }

    fn cancel(&mut self, task_id: String) -> Result<(), String> {
        transfer::transfer_cancel(&self.host_ctx, &self.plugin_id, &task_id)
    }
}

// ==================== Component Linker 组装 ====================

/// 将已接线的 import 接口注册到 component linker
///
/// 每个接口一个 `add_to_linker`（`HasSelf<T>` 让 getter 返回 `&mut T`）。
/// 新增接线：实现对应 `Host` trait 后在此追加一行。
pub(crate) fn add_to_linker(linker: &mut Linker<WasmPluginState>) -> crate::Result<()> {
    type D = wasmtime::component::HasSelf<WasmPluginState>;
    for iface in [
        bedcode::plugin::host_storage::add_to_linker::<WasmPluginState, D>,
        bedcode::plugin::host_log::add_to_linker::<WasmPluginState, D>,
        bedcode::plugin::host_config::add_to_linker::<WasmPluginState, D>,
        bedcode::plugin::host_terminal::add_to_linker::<WasmPluginState, D>,
        bedcode::plugin::host_database::add_to_linker::<WasmPluginState, D>,
        bedcode::plugin::host_plugin_database::add_to_linker::<WasmPluginState, D>,
        bedcode::plugin::host_session::add_to_linker::<WasmPluginState, D>,
        bedcode::plugin::host_timer::add_to_linker::<WasmPluginState, D>,
        bedcode::plugin::host_events::add_to_linker::<WasmPluginState, D>,
        bedcode::plugin::host_http::add_to_linker::<WasmPluginState, D>,
        bedcode::plugin::host_fs::add_to_linker::<WasmPluginState, D>,
        bedcode::plugin::host_bus::add_to_linker::<WasmPluginState, D>,
        bedcode::plugin::host_file_service::add_to_linker::<WasmPluginState, D>,
        bedcode::plugin::host_transfer::add_to_linker::<WasmPluginState, D>,
    ] {
        iface(linker, |s| s).map_err(|e| {
            AppError::Plugin(format!("Failed to register component host interface: {}", e))
        })?;
    }
    Ok(())
}

// ==================== 组件插件实例 ====================

/// 已加载的组件形态 WASM 插件（阶段 C 后唯一形态）
///
/// 持有 component Instance + Store，全部调用走 bindgen 生成的类型化接口
/// （无 (ptr,len) 内存搬运）。Store 必须与 Instance 一起持有，
/// 否则导出函数无法调用。
pub struct LoadedWasmPlugin {
    instance: Instance,
    store: Store<WasmPluginState>,
}

impl LoadedWasmPlugin {
    /// 实例化组件
    ///
    /// 与 core 路径相同的防护：资源限制、epoch 中断、ABI 版本协商
    /// （组件必须声明 form=1，版本号语义不变）
    pub(crate) fn new(
        engine: &wasmtime::Engine,
        component_linker: &Linker<WasmPluginState>,
        component: &Component,
        plugin_id: &str,
        host_ctx: Arc<WasmHostContext>,
    ) -> crate::Result<Self> {
        let state = WasmPluginState {
            plugin_id: plugin_id.to_string(),
            host_ctx,
        };
        let mut store = Store::new(engine, state);
        store.limiter(|state| state as &mut dyn ResourceLimiter);
        store.epoch_deadline_trap();
        store.set_epoch_deadline(EPOCH_GRACE_TICKS);

        let instance = component_linker.instantiate(&mut store, component).map_err(|e| {
            AppError::Plugin(format!(
                "Failed to instantiate WASM component for plugin '{}': {}",
                plugin_id, e
            ))
        })?;

        Self::verify_abi(&mut store, &instance)?;

        Ok(Self { instance, store })
    }

    /// ABI 版本协商（对应 core 路径的 `__bedcode_abi_version` 校验）
    ///
    /// - `abi.version()` 语义与 `abi::ABI_VERSION` 完全一致
    /// - `abi.form()` 必须为 1（component 形态）；0 是 core 形态的自研 ABI
    fn verify_abi(
        store: &mut Store<WasmPluginState>,
        instance: &Instance,
    ) -> crate::Result<()> {
        let exports = Plugin::new(&mut *store, instance).map_err(|e| {
            AppError::Plugin(format!("WASM component missing required exports: {}", e))
        })?;
        let abi_guest = exports.bedcode_plugin_abi();

        let version = abi_guest.call_version(&mut *store).map_err(|e| {
            AppError::Plugin(format!("WASM component abi.version() call failed: {}", e))
        })?;
        let form = abi_guest.call_form(&mut *store).map_err(|e| {
            AppError::Plugin(format!("WASM component abi.form() call failed: {}", e))
        })?;

        if form != abi::FORM_COMPONENT {
            return Err(AppError::Plugin(format!(
                "WASM component for plugin declares abi form {} (expected {})",
                form,
                abi::FORM_COMPONENT
            )));
        }
        if version > abi::ABI_VERSION {
            return Err(AppError::Plugin(format!(
                "Plugin requires ABI v{} but host supports v{} — please upgrade BedCode",
                version, abi::ABI_VERSION
            )));
        }
        Ok(())
    }

    /// 获取 world 导出绑定（每次调用重新索引导出，开销可忽略）
    ///
    /// 所有导出调用都经过此处：顺带刷新 epoch 超时窗口
    /// （core 路径在 `get_export_func` 中做同样的事），
    /// 防止长驻插件在实例化后累计超时被误 trap
    fn exports(&mut self) -> crate::Result<Plugin> {
        self.store.set_epoch_deadline(EPOCH_GRACE_TICKS);
        Plugin::new(&mut self.store, &self.instance).map_err(|e| {
            AppError::Plugin(format!("WASM component exports access failed: {}", e))
        })
    }

    /// 调用插件的 activate 导出
    pub(crate) fn activate(&mut self) -> crate::Result<i32> {
        let exports = self.exports()?;
        let lifecycle = exports.bedcode_plugin_lifecycle();
        match lifecycle.call_activate(&mut self.store) {
            Ok(Ok(())) => Ok(0),
            Ok(Err(msg)) => Err(AppError::Plugin(format!("WASM activate() failed: {}", msg))),
            Err(e) => Err(AppError::Plugin(format!("WASM activate() call failed: {}", e))),
        }
    }

    /// 调用插件的 deactivate 导出
    pub(crate) fn deactivate(&mut self) -> crate::Result<i32> {
        let exports = self.exports()?;
        let lifecycle = exports.bedcode_plugin_lifecycle();
        match lifecycle.call_deactivate(&mut self.store) {
            Ok(Ok(())) => Ok(0),
            Ok(Err(msg)) => Err(AppError::Plugin(format!("WASM deactivate() failed: {}", msg))),
            Err(e) => Err(AppError::Plugin(format!("WASM deactivate() call failed: {}", e))),
        }
    }

    /// 调用插件的 invoke_command 导出（JSON 载荷保留，语义与 core 路径 1:1）
    pub(crate) fn invoke_command(
        &mut self,
        command_name: &str,
        args_json: &str,
    ) -> crate::Result<String> {
        let exports = self.exports()?;
        let cmd = exports.bedcode_plugin_command();
        cmd.call_invoke(&mut self.store, command_name, args_json).map_err(|e| {
            AppError::Plugin(format!("WASM invoke_command() call failed: {}", e))
        })
    }

    /// 调用插件的 on_terminal_input 导出
    pub(crate) fn on_terminal_input(
        &mut self,
        session_id: &str,
        text: &str,
    ) -> crate::Result<Option<String>> {
        let exports = self.exports()?;
        let hooks = exports.bedcode_plugin_terminal_hooks();
        hooks.call_on_terminal_input(&mut self.store, session_id, text).map_err(|e| {
            AppError::Plugin(format!("WASM on_terminal_input() call failed: {}", e))
        })
    }

    /// 调用插件的 on_terminal_output 导出
    pub(crate) fn on_terminal_output(
        &mut self,
        session_id: &str,
        data: &str,
    ) -> crate::Result<Option<String>> {
        let exports = self.exports()?;
        let hooks = exports.bedcode_plugin_terminal_hooks();
        hooks.call_on_terminal_output(&mut self.store, session_id, data).map_err(|e| {
            AppError::Plugin(format!("WASM on_terminal_output() call failed: {}", e))
        })
    }

    /// 调用插件的 on_startup 导出
    pub(crate) fn on_startup(&mut self) -> crate::Result<()> {
        let exports = self.exports()?;
        let lifecycle = exports.bedcode_plugin_lifecycle();
        lifecycle.call_on_startup(&mut self.store).map_err(|e| {
            AppError::Plugin(format!("WASM on_startup() call failed: {}", e))
        })
    }

    /// 调用插件的 on_shutdown 导出
    pub(crate) fn on_shutdown(&mut self) -> crate::Result<()> {
        let exports = self.exports()?;
        let lifecycle = exports.bedcode_plugin_lifecycle();
        lifecycle.call_on_shutdown(&mut self.store).map_err(|e| {
            AppError::Plugin(format!("WASM on_shutdown() call failed: {}", e))
        })
    }

    /// 调用插件的消息总线消息接收导出
    pub(crate) fn on_message(
        &mut self,
        topic: &str,
        sender: &str,
        payload: &serde_json::Value,
    ) -> crate::Result<()> {
        let payload_str = serde_json::to_string(payload).unwrap_or_default();
        let exports = self.exports()?;
        let events = exports.bedcode_plugin_events();
        match events.call_on_message(&mut self.store, topic, sender, &payload_str) {
            Ok(Ok(())) => Ok(()),
            Ok(Err(msg)) => {
                tracing::warn!("WASM on_message() failed: {}", msg);
                Ok(())
            }
            Err(e) => Err(AppError::Plugin(format!("WASM on_message() call failed: {}", e))),
        }
    }

    /// 调用插件的会话生命周期事件导出
    pub(crate) fn on_session_lifecycle(
        &mut self,
        payload: &serde_json::Value,
    ) -> crate::Result<()> {
        let payload_str = serde_json::to_string(payload).unwrap_or_default();
        let exports = self.exports()?;
        let events = exports.bedcode_plugin_events();
        match events.call_on_session_lifecycle(&mut self.store, &payload_str) {
            Ok(Ok(())) => Ok(()),
            Ok(Err(msg)) => {
                tracing::warn!("WASM on_session_lifecycle() failed: {}", msg);
                Ok(())
            }
            Err(e) => {
                Err(AppError::Plugin(format!("WASM on_session_lifecycle() call failed: {}", e)))
            }
        }
    }

    /// 调用插件的提交输入行事件导出（纯观察通知，失败仅记录日志）
    pub(crate) fn on_input_submitted(
        &mut self,
        payload: &serde_json::Value,
    ) -> crate::Result<()> {
        let payload_str = serde_json::to_string(payload).unwrap_or_default();
        let exports = self.exports()?;
        let events = exports.bedcode_plugin_events();
        match events.call_on_input_submitted(&mut self.store, &payload_str) {
            Ok(Ok(())) => Ok(()),
            Ok(Err(msg)) => {
                tracing::warn!("WASM on_input_submitted() failed: {}", msg);
                Ok(())
            }
            Err(e) => {
                Err(AppError::Plugin(format!("WASM on_input_submitted() call failed: {}", e)))
            }
        }
    }

    /// 调用插件的上传策略钩子导出（fail-closed 语义由调用方保持）
    pub(crate) fn on_upload_request(&mut self, meta_json: &str) -> crate::Result<String> {
        let exports = self.exports()?;
        let hooks = exports.bedcode_plugin_upload_hook();
        hooks.call_on_upload_request(&mut self.store, meta_json).map_err(|e| {
            AppError::Plugin(format!("WASM on_upload_request() call failed: {}", e))
        })
    }

    /// 获取插件的 manifest JSON
    pub(crate) fn get_manifest(&mut self) -> crate::Result<String> {
        let exports = self.exports()?;
        let manifest = exports.bedcode_plugin_manifest();
        manifest.call_get(&mut self.store).map_err(|e| {
            AppError::Plugin(format!("WASM manifest() call failed: {}", e))
        })
    }
}

// ==================== 产物形态检测 ====================
//
// 阶段 C 已删除：产物仅剩组件形态，无需按魔法字节分派加载路径。
