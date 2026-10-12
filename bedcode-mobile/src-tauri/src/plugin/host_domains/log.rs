//! host_log — 插件日志（移动宿主 adapter）
//!
//! 4 级日志经共享核 `bedcode-host-api-core::log`（callsite 缓存 + per-plugin
//! 级别阈值 + `[plugin:{}]` 前缀）；`mark_plugin_error` 经宿主注入的
//! `status_reporter` 回调（置 Error + 前端通知）——与 wasm-core 时代逐字同语义。

use bedcode_host_kit::WasmPluginState;

use crate::plugin::bindings::bedcode;

impl bedcode::plugin::host_log::Host for WasmPluginState {
    fn info(&mut self, message: String) {
        // 实现层在共享核（票 18 批次 4）：callsite 缓存 + per-plugin 级别阈值
        // 过滤 + `[plugin:{}]` 前缀——移动端由此获得桌面全套日志机制
        bedcode_host_api_core::log::log_info(&self.plugin_id, &message, "", 0);
    }

    fn debug(&mut self, message: String) {
        bedcode_host_api_core::log::log_debug(&self.plugin_id, &message, "", 0);
    }

    fn warn(&mut self, message: String) {
        bedcode_host_api_core::log::log_warn(&self.plugin_id, &message, "", 0);
    }

    fn error(&mut self, message: String) {
        bedcode_host_api_core::log::log_error(&self.plugin_id, &message, "", 0);
    }

    fn mark_plugin_error(&mut self, error: String) {
        // 统一 state 后宿主上下文经向下转型取（票 04）
        (super::host_ctx(self).status_reporter)(&self.plugin_id, &error);
    }
}
