//! PluginHost 的 trait 实现（PluginServices / MessageDispatcher / Clone）
//! 与会话事件分发
//!
//! 从 `host.rs` 拆出：WASM host 函数回路的服务侧实现、定时器管理、
//! 会话生命周期/输入事件分发。

use std::sync::Arc;

use tauri::Emitter;

use super::listeners::{PluginInputListener, PluginLifecycleListener};
use super::PluginHost;
use bedcode_plugin_api::PluginState;
use crate::plugin::wasm_runtime::PluginServices;
use crate::session::SessionManager;

impl PluginHost {
    /// 将会话生命周期事件分发给指定插件的 on_session_lifecycle 回调
    pub fn dispatch_lifecycle_to_plugin(&self, plugin_id: &str, payload: &serde_json::Value) {
        if !self.is_activated_block(plugin_id) {
            return;
        }

        let host = self.clone();
        let plugin_id = plugin_id.to_string();
        let payload = payload.clone();
        crate::plugin::wasm_runtime::block_on_async(async move {
            if let Err(e) = host
                .with_wasm_plugin_call(&plugin_id, |plugin| plugin.on_session_lifecycle(&payload))
                .await
            {
                tracing::error!(
                    "SessionLifecycle: dispatch to plugin '{}' failed: {}",
                    plugin_id, e
                );
            }
        });
    }

    /// 将提交输入行事件分发给指定插件的 on_input_submitted 回调（见 ADR 0001）
    ///
    /// 由 PluginInputListener 在 SessionManager spawn 的错误隔离任务中调用；
    /// 分发失败仅记录日志，不影响输入本身
    pub fn dispatch_input_to_plugin(&self, plugin_id: &str, payload: &serde_json::Value) {
        if !self.is_activated_block(plugin_id) {
            // 插件未处于 Activated 状态（Loaded/Deactivated/Error）：事件被此门禁静默丢弃，
            // 是输入分发链路上唯一无日志的断点，记录 debug 便于定位
            tracing::debug!(
                "InputSubmitted: drop event for plugin '{}': plugin not in Activated state",
                plugin_id
            );
            return;
        }

        tracing::debug!(
            "InputSubmitted: dispatch to plugin '{}', payload={}",
            plugin_id,
            payload
        );

        let host = self.clone();
        let plugin_id = plugin_id.to_string();
        let payload = payload.clone();
        crate::plugin::wasm_runtime::block_on_async(async move {
            if let Err(e) = host
                .with_wasm_plugin_call(&plugin_id, |plugin| plugin.on_input_submitted(&payload))
                .await
            {
                tracing::error!(
                    "InputSubmitted: dispatch to plugin '{}' failed: {}",
                    plugin_id, e
                );
            }
        });
    }

    /// 阻塞式检查插件是否已激活（用于同步分发场景）
    fn is_activated_block(&self, plugin_id: &str) -> bool {
        let plugins = self.plugins.clone();
        crate::plugin::wasm_runtime::block_on_async(async move {
            let plugins = plugins.read().await;
            plugins.get(plugin_id)
                .map(|p| matches!(p.state, PluginState::Activated))
                .unwrap_or(false)
        })
    }
}


// ==================== PluginServices Implementation ====================
// ==================== PluginServices Implementation ====================

impl PluginServices for PluginHost {
    fn register_session_lifecycle_listener(
        &self,
        plugin_id: String,
        session_manager: Arc<SessionManager>,
    ) {
        // host function 处于同步上下文，通过 block_on_async 完成异步注册
        let listener = PluginLifecycleListener::new(plugin_id, self.clone());
        crate::plugin::wasm_runtime::block_on_async(
            session_manager.register_lifecycle_listener(Arc::new(listener)),
        );
    }

    fn register_session_input_listener(
        &self,
        plugin_id: String,
        session_manager: Arc<SessionManager>,
    ) {
        // host function 处于同步上下文，通过 block_on_async 完成异步注册
        let listener = PluginInputListener::new(plugin_id, self.clone());
        crate::plugin::wasm_runtime::block_on_async(
            session_manager.register_input_listener(Arc::new(listener)),
        );
    }

    fn mark_plugin_error(&self, plugin_id: String, error: String) {
        crate::plugin::wasm_runtime::block_on_async(async move {
            // 仅通知前端弹窗提示：不置 Error、不持久化，插件保持激活，会话照常运行。
            // hooks 安装失败等自检错误属可恢复/局部问题，不应因此禁用整个插件。
            tracing::error!("[PluginHost] Plugin {} self-check failed: {}", plugin_id, error);

            let _ = crate::system::app_context::AppContext::global()
                .app_handle()
                .emit(
                    crate::system::constants::event::PLUGIN_ERROR,
                    serde_json::json!({
                        "plugin_id": plugin_id,
                        "error": error,
                    }),
                );
        });
    }

    fn register_plugin_timer(&self, plugin_id: String, interval_secs: u64, command: String) {
        // 重复注册替换旧定时器：先中止旧任务再插入新句柄，
        // 同一插件仅保留一个定时器实例
        let mut timers = self
            .plugin_timers
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(old) = timers.remove(&plugin_id) {
            old.abort();
        }

        let host = self.clone();
        let pid = plugin_id.clone();
        let cmd = command.clone();
        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
            // 首个 tick 立即触发：跳过，从下一个周期开始（避免注册瞬间就回调）
            interval.tick().await;
            loop {
                interval.tick().await;

                let now = chrono::Utc::now();
                let args = serde_json::json!({
                    "now_ms": now.timestamp_millis(),
                    // 与 SQLite datetime('now') 同格式（UTC，无时区后缀），
                    // 便于插件在 SQL 中直接字符串比较到期时间
                    "now_utc": now.format("%Y-%m-%d %H:%M:%S").to_string(),
                });

                // 到点调用插件 command；插件未激活/已卸载时返回 Err，
                // 属预期内路径（定时器中止前的空窗期），仅记 debug 日志
                match host.invoke_rust_command(&pid, &cmd, args).await {
                    Ok(_) => {}
                    Err(e) => {
                        tracing::debug!(
                            plugin_id = %pid,
                            command = %cmd,
                            error = %e,
                            "[PluginHost] timer tick skipped"
                        );
                    }
                }
            }
        });

        timers.insert(plugin_id.clone(), handle);
        drop(timers);

        tracing::info!(
            "[PluginHost] Timer started for '{}': interval={}s command={}",
            plugin_id, interval_secs, command
        );
    }
}

// 通过 Arc 共享内部状态实现 Clone
impl Clone for PluginHost {
    fn clone(&self) -> Self {
        Self {
            plugins: self.plugins.clone(),
            registry: self.registry.clone(),
            permission: self.permission.clone(),
            storage: self.storage.clone(),
            rust_command_handlers: self.rust_command_handlers.clone(),
            rust_terminal_handlers: self.rust_terminal_handlers.clone(),
            wasm_runtime: self.wasm_runtime.clone(),
            wasm_plugins: self.wasm_plugins.clone(),
            wasm_host_ctx: self.wasm_host_ctx.clone(),
            message_bus: self.message_bus.clone(),
            file_service: self.file_service.clone(),
            plugin_timers: self.plugin_timers.clone(),
            wasm_reload_throttle: self.wasm_reload_throttle.clone(),
        }
    }
}

// ==================== MessageDispatcher Implementation ====================

impl crate::plugin::message_bus::MessageDispatcher for PluginHost {
    fn dispatch_to_wasm(&self, plugin_id: &str, msg: &bedcode_plugin_api::BusMessage) -> anyhow::Result<()> {
        let host = self.clone();
        let plugin_id = plugin_id.to_string();
        let msg = msg.clone();
        crate::plugin::wasm_runtime::block_on_async(async move {
            // 调用失败（trap/store 中毒）时自动重载恢复，见 with_wasm_plugin_call
            host.with_wasm_plugin_call(&plugin_id, |plugin| {
                plugin.on_message(&msg.topic, &msg.sender, &msg.payload)
            })
            .await
            .map_err(|e| anyhow::Error::from(e))
        })
    }

    fn is_activated(&self, plugin_id: &str) -> bool {
        let plugins = self.plugins.clone();
        crate::plugin::wasm_runtime::block_on_async(async move {
            let plugins = plugins.read().await;
            plugins.get(plugin_id)
                .map(|p| matches!(p.state, PluginState::Activated))
                .unwrap_or(false)
        })
    }
}

// ==================== Tests ====================

