//! `errors` — PluginHost 职责面拆分（P2）
//!
//! 自 `host.rs` 主 impl 拆出；经 `use super::*` 继承 host 模块的
//! 全部导入与常量，方法与字段可见性规则与内联时一致。

use super::*;
impl PluginHost {
    /// 标记插件为错误状态
    pub async fn mark_error(&self, plugin_id: &str, error: String) {
        let mut plugins = self.plugins.write().await;
        if let Some(loaded) = plugins.get_mut(plugin_id) {
            loaded.state = PluginState::Error(error);
        }
    }

    /// 插件运行时异常统一上报前端（全局异常通道，`PLUGIN_RUNTIME_ERROR`）
    ///
    /// 宿主检测到插件异常（非插件主动上报）时调用，覆盖三类场景：
    /// - `panic`：宿主函数 panic 穿透 wasmtime（catch_unwind 兜底，Store 已污染）
    /// - `trap`：wasm trap / 导出绑定失败 / store 中毒（已调度自动重载）
    /// - `recovery_failed`：自动重载失败，插件进入 Error 态
    ///
    /// 语义：日志**始终**记录全量错误（重载循环期间不丢现场）；前端 toast
    /// 按插件节流（见 [`PLUGIN_RUNTIME_ERROR_NOTIFY_INTERVAL_SECS`]），连发
    /// 异常只弹一次，避免 trap 重载风暴刷屏。无 AppContext（测试/无头）时
    /// 降级为纯日志，不 panic。

    /// 插件运行时异常统一上报前端（全局异常通道，`PLUGIN_RUNTIME_ERROR`）
    ///
    /// 宿主检测到插件异常（非插件主动上报）时调用，覆盖三类场景：
    /// - `panic`：宿主函数 panic 穿透 wasmtime（catch_unwind 兜底，Store 已污染）
    /// - `trap`：wasm trap / 导出绑定失败 / store 中毒（已调度自动重载）
    /// - `recovery_failed`：自动重载失败，插件进入 Error 态
    ///
    /// 语义：日志**始终**记录全量错误（重载循环期间不丢现场）；前端 toast
    /// 按插件节流（见 [`PLUGIN_RUNTIME_ERROR_NOTIFY_INTERVAL_SECS`]），连发
    /// 异常只弹一次，避免 trap 重载风暴刷屏。无 AppContext（测试/无头）时
    /// 降级为纯日志，不 panic。
    pub async fn notify_plugin_runtime_error(&self, plugin_id: &str, kind: &str, error: &str) {
        // 日志始终记录（调用方也各自记日志，此处为统一通道的兜底记录）
        tracing::error!(
            plugin_id = %plugin_id,
            kind = %kind,
            error = %error,
            "Plugin runtime error (unified channel)"
        );

        // 节流：同一插件窗口内已提示过则跳过 toast（日志不受影响）；
        // recovery_failed 不节流——它每次重载失败只发一次（重载循环 30s 间隔），
        // 若落在 trap 通知的 15s 窗口内会被吞，用户看到「已恢复」实际进入 Error 态
        if kind != "recovery_failed" {
            let mut throttle = self
                .runtime_error_notify_throttle
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if let Some(last) = throttle.get(plugin_id) {
                if last.elapsed() < std::time::Duration::from_secs(PLUGIN_RUNTIME_ERROR_NOTIFY_INTERVAL_SECS) {
                    tracing::debug!(
                        plugin_id = %plugin_id,
                        kind = %kind,
                        "plugin runtime error toast throttled (recent notification)"
                    );
                    return;
                }
            }
            throttle.insert(plugin_id.to_string(), std::time::Instant::now());
        }

        // 插件展示名（manifest.name），查不到时退回插件 ID
        let plugin_name = {
            let plugins = self.plugins.read().await;
            plugins
                .get(plugin_id)
                .map(|p| p.manifest.name.clone())
                .unwrap_or_else(|| plugin_id.to_string())
        };

        // 无头/测试上下文无 AppHandle：降级为纯日志
        let Some(ctx) = crate::system::app_context::AppContext::try_global() else {
            return;
        };
        let Some(handle) = ctx.app_handle() else {
            return;
        };
        if let Err(e) = handle.emit(
            crate::system::constants::event::PLUGIN_RUNTIME_ERROR,
            serde_json::json!({
                "plugin_id": plugin_id,
                "plugin_name": plugin_name,
                "kind": kind,
                "error": error,
            }),
        ) {
            // 前端事件派发失败不致命：日志已全量记录，仅提示通道中断
            tracing::warn!(
                plugin_id = %plugin_id,
                "Failed to emit PLUGIN_RUNTIME_ERROR to frontend: {}",
                e
            );
        }
    }
}
