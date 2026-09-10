//! 插件 Rust 命令分发与终端 handler 管道
//!
//! 从 `host.rs` 拆出的 `impl PluginHost` 块：Rust command 路由（WASM /
//! 静态注册）、trap 自动重载、TerminalHandler 输入/输出管道。

use std::sync::Arc;

use tokio::sync::Mutex;

use bedcode_plugin_api::PluginCommandEntry;

use super::{LoadedWasmPlugin, PluginHost, PLUGIN_AUTO_RELOAD_MIN_INTERVAL_SECS};
use crate::plugin::types::PluginSource;

impl PluginHost {
    // ==================== Rust Command Dispatch ====================

    /// 执行 Rust 插件的 command handler
    ///
    /// 路由逻辑：
    /// - WASM 插件：通过 WASM 导出函数调用 invoke_command
    /// - 静态注册插件：通过运行时注册表查找 handler
    pub async fn invoke_rust_command(
        &self,
        plugin_id: &str,
        command_name: &str,
        args: serde_json::Value,
    ) -> crate::Result<serde_json::Value> {
        if !self.is_activated(plugin_id).await {
            return Err(crate::AppError::Plugin(format!(
                "Plugin {} is not activated",
                plugin_id
            )));
        }

        let source = {
            let plugins = self.plugins.read().await;
            plugins
                .get(plugin_id)
                .map(|p| p.source.clone())
                .ok_or_else(|| crate::AppError::Plugin(format!("Plugin not found: {}", plugin_id)))?
        };

        match source {
            PluginSource::Wasm => self.invoke_wasm_command(plugin_id, command_name, args).await,
            PluginSource::StaticRegistry => self.invoke_static_command(plugin_id, command_name, args).await,
            PluginSource::FileScan => Err(crate::AppError::Plugin(format!(
                "Plugin {} is TS-only, cannot invoke Rust command",
                plugin_id
            ))),
        }
    }

    /// 获取 WASM 插件实例句柄（map 读锁仅在取 Arc 期间持有，随即释放，
    /// 实例串行化由各插件自己的 Mutex 承担，插件间互不阻塞）
    pub(super) async fn get_wasm_plugin(&self, plugin_id: &str) -> Option<Arc<Mutex<LoadedWasmPlugin>>> {
        let wasm_plugins = self.wasm_plugins.read().await;
        wasm_plugins.get(plugin_id).cloned()
    }

    /// WASM 插件调用失败（trap / store 中毒）后的自动恢复
    ///
    /// wasmtime 同步引擎下任何一次 trap 都会 `set_trapped()` 污染 Store，
    /// 之后该实例所有调用持续报 `CannotEnterComponent`，唯一恢复途径是整体重载
    /// （deactivate → 重新实例化 → activate，即 [`reload_wasm_plugin`]）。
    /// 本方法只做：限频（防重载风暴）+ 后台调度 + 失败时置 Error 态。
    ///
    /// 同步上下文可调用（内部 spawn 不阻塞）；调用方须先释放插件实例锁。
    pub fn schedule_plugin_reload_after_trap(&self, plugin_id: &str) {
        let plugin_id = plugin_id.to_string();

        // 限频：距上次自动重载不足最小间隔则跳过（已在上次恢复或仍属持久性故障）
        {
            let mut throttle = self.wasm_reload_throttle.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(last) = throttle.get(&plugin_id) {
                if last.elapsed() < std::time::Duration::from_secs(PLUGIN_AUTO_RELOAD_MIN_INTERVAL_SECS) {
                    tracing::warn!(
                        plugin_id = %plugin_id,
                        "plugin trap recovery throttled (recent reload), keeping error state"
                    );
                    return;
                }
            }
            throttle.insert(plugin_id.clone(), std::time::Instant::now());
        }

        let host = self.clone();
        tracing::warn!(
            plugin_id = %plugin_id,
            "plugin WASM trap detected, scheduling auto reload"
        );
        tokio::spawn(async move {
            // 恢复窗口内用户已停用（或正在停用）时不擅自重载
            if !host.is_activated(&plugin_id).await {
                tracing::info!(
                    plugin_id = %plugin_id,
                    "plugin no longer activated, skip auto reload"
                );
                return;
            }
            match host.reload_wasm_plugin(&plugin_id).await {
                Ok(()) => {
                    tracing::info!(plugin_id = %plugin_id, "plugin auto reloaded after trap");
                }
                Err(e) => {
                    tracing::error!(
                        plugin_id = %plugin_id,
                        error = %e,
                        "plugin auto reload after trap failed"
                    );
                    // 统一异常通道：自动恢复失败，插件进入 Error 态（前端提示）
                    host.notify_plugin_runtime_error(&plugin_id, "recovery_failed", &e.to_string())
                        .await;
                    // 置 Error 态：UI 可见原因，且 is_activated 门禁停止后续分发
                    host.mark_error(&plugin_id, format!("auto reload after trap failed: {}", e))
                        .await;
                }
            }
        });
    }

    /// 在无当前 runtime handle 的阻塞线程上执行 guest 导出调用
    ///
    /// WASI 预打开模式下，wasi 同步绑定（`in_tokio`）要求调用线程没有进入
    /// 任何 tokio runtime——否则其内部 `handle.block_on` 会 panic
    /// （"Cannot start a runtime from within a runtime"）。故所有 guest 调用
    /// 统一搬到 `spawn_blocking` 阻塞线程执行：该线程无当前 handle，wasi 走
    /// 其自身 ambient runtime；宿主函数经 [`block_on_async`] 的 ambient 兜底
    /// 同样可阻塞执行。非 WASI 插件不受影响（未见 wasi 导入就不触发）。
    ///
    /// 返回 `Result<crate::Result<T>, panic 载荷>`：guest 的 `crate::Result<T>`
    /// 保留在内层（`Ok(Ok(v))`=值 / `Ok(Err(e))`=guest 错误），panic 走外层
    /// `Err(panic)`（不跨 spawn_blocking 传播为 JoinError）。
    pub(super) async fn run_guest_call<T>(
        &self,
        wasm_plugin: Arc<Mutex<LoadedWasmPlugin>>,
        call: impl FnOnce(&mut LoadedWasmPlugin) -> crate::Result<T> + Send + 'static,
    ) -> std::result::Result<crate::Result<T>, Box<dyn std::any::Any + Send>>
    where
        T: Send + 'static,
    {
        tokio::task::spawn_blocking(move || {
            // 阻塞线程无当前 tokio handle，符合 wasi 同步绑定（in_tokio）要求；
            // 在 ambient runtime 上驱动 tokio 锁（借用闭包内 Arc，见 block_on_ambient），
            // guest 调用在无 handle 线程执行，锁在 catch_unwind 后由 drop 释放
            let mut guard = crate::plugin::wasm_runtime::block_on_ambient(wasm_plugin.lock());
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| call(&mut guard)))
        })
        .await
        // spawn_blocking 任务自身 panic（理论上 guard move 前的一切异常）→
        // 视为 panic 载荷，与 catch_unwind 语义对齐（外层 Result 的 Err 槽位
        // = panic 载荷），调用方统一走重载恢复
        .unwrap_or_else(|join| Err(Box::new(join.to_string()) as Box<dyn std::any::Any + Send>))
    }

    /// 持锁调用 WASM 插件导出并统一处理失败恢复
    ///
    /// 调用失败（trap / 导出绑定失败 / store 中毒）或 panic（宿主函数内
    /// 嵌套 block_in_place 等）时：先释放实例锁（unwind 自动释放 / 显式释放），
    /// 再调度自动重载（见 [`schedule_plugin_reload_after_trap`]），最后返回 Err 给调用方。
    /// panic 不捕获会穿透污染 wasmtime Store 且不触发重载——插件永久不可用
    /// （任务卡 transferring、hook 全部超时），故必须 catch_unwind。
    pub(super) async fn with_wasm_plugin_call<T>(
        &self,
        plugin_id: &str,
        call: impl FnOnce(&mut LoadedWasmPlugin) -> crate::Result<T> + Send + 'static,
    ) -> crate::Result<T>
    where
        T: Send + 'static,
    {
        let Some(wasm_plugin) = self.get_wasm_plugin(plugin_id).await else {
            return Err(crate::AppError::Plugin(format!(
                "WASM plugin {} not found in loaded instances",
                plugin_id
            )));
        };
        // 在无 handle 阻塞线程上执行（WASI 需要），结果 catch_unwind 已在此
        let result = self.run_guest_call(wasm_plugin, call).await;
        match result {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(e)) => {
                // 实例已不可用 → 自动重载恢复（锁已释放，无死锁）；
                // 统一异常通道通知前端（trap 连发由节流合并）
                self.notify_plugin_runtime_error(plugin_id, "trap", &e.to_string())
                    .await;
                self.schedule_plugin_reload_after_trap(plugin_id);
                Err(e)
            }
            Err(panic) => {
                // panic：unwind 已释放实例锁，但 wasmtime Store 被污染，
                // 必须重载才能恢复插件
                let msg = crate::plugin::wasm_runtime::panic_payload_to_string(&panic);
                tracing::error!(
                    plugin_id = %plugin_id,
                    panic = %msg,
                    "WASM plugin call panicked, scheduling reload"
                );
                // 统一异常通道通知前端：插件发生未知错误（用户可见的业务提示）
                self.notify_plugin_runtime_error(plugin_id, "panic", &msg).await;
                self.schedule_plugin_reload_after_trap(plugin_id);
                Err(crate::AppError::Plugin(format!(
                    "WASM plugin {} call panicked: {}",
                    plugin_id, msg
                )))
            }
        }
    }

    /// 调用 WASM 插件的 command
    pub(super) async fn invoke_wasm_command(
        &self,
        plugin_id: &str,
        command_name: &str,
        args: serde_json::Value,
    ) -> crate::Result<serde_json::Value> {
        // 为需要 resource_dir 的命令自动注入插件 extension_path
        // （剥离 verbatim 前缀，保证插件侧正斜杠拼接可用，见 loader.rs strip_verbatim_prefix）
        let mut enriched_args = args;
        if enriched_args.get("resource_dir").is_none() {
            let plugins = self.plugins.read().await;
            if let Some(loaded) = plugins.get(plugin_id) {
                enriched_args.as_object_mut().map(|obj| {
                    obj.insert(
                        "resource_dir".to_string(),
                        serde_json::Value::String(crate::plugin::loader::strip_verbatim_prefix(&loaded.extension_path)),
                    );
                });
            }
        }

        let args_str = serde_json::to_string(&enriched_args)
            .map_err(|e| crate::AppError::Plugin(format!("Failed to serialize command args: {}", e)))?;

        // 调用失败（trap/store 中毒）时自动重载恢复，见 with_wasm_plugin_call
        let command_name = command_name.to_string();
        let result_str = self
            .with_wasm_plugin_call(plugin_id, move |plugin| plugin.invoke_command(&command_name, &args_str))
            .await?;

        let value: serde_json::Value = serde_json::from_str(&result_str).map_err(|e| {
            crate::AppError::Plugin(format!(
                "WASM plugin {} invoke_command() returned invalid JSON: {}",
                plugin_id, e
            ))
        })?;

        // 插件 invoke_command 的 Err 经 SDK 宏序列化为 {"error": "..."} 的**成功**
        // JSON（非 WIT Err），此处还原为真正错误——否则前端把失败当成功
        // （真机实证：dial-peer 被拒仍 markConnected，桌面显示「已连接」）
        if let Some(err) = value.get("error").and_then(|v| v.as_str()) {
            if !err.is_empty() {
                return Err(crate::AppError::Plugin(err.to_string()));
            }
        }

        Ok(value)
    }

    /// 调用静态注册插件的 command handler
    pub(super) async fn invoke_static_command(
        &self,
        plugin_id: &str,
        command_name: &str,
        args: serde_json::Value,
    ) -> crate::Result<serde_json::Value> {
        let handlers = self.rust_command_handlers.read().await;
        let full_name = format!("{}::{}", plugin_id, command_name);
        let cmd = handlers
            .get(&full_name)
            .ok_or_else(|| crate::AppError::Plugin(format!("Command not found: {}", full_name)))?;

        let result = (cmd.handler)(args)
            .await
            .map_err(|e| crate::AppError::Plugin(format!("Command execution error: {}", e)))?;

        Ok(result)
    }

    /// 获取所有 Rust 插件的 command 列表
    pub async fn list_rust_commands(&self) -> Vec<PluginCommandEntry> {
        let handlers = self.rust_command_handlers.read().await;
        handlers
            .iter()
            .map(|(full_name, cmd)| {
                let parts: Vec<&str> = full_name.splitn(2, "::").collect();
                let plugin_id = parts.first().map(|s| s.to_string()).unwrap_or_default();
                let command_name = parts.get(1).map(|s| s.to_string()).unwrap_or_default();
                PluginCommandEntry {
                    plugin_id,
                    command_name,
                    title: cmd.title.clone(),
                }
            })
            .collect()
    }

    // ==================== Terminal Handler Pipeline ====================

    /// 是否有已注册的 Rust terminal handler
    pub async fn has_terminal_handlers(&self) -> bool {
        !self.rust_terminal_handlers.read().await.is_empty()
    }

    /// 通过插件 TerminalHandler 管道处理终端输入
    pub async fn process_terminal_input(&self, session_id: &str, text: &str) -> String {
        let handlers = self.rust_terminal_handlers.read().await;
        let mut result = text.to_string();
        for handler in handlers.iter() {
            if let Some(modified) = handler.on_input(session_id, &result) {
                tracing::debug!(
                    "Terminal input modified by plugin handler: session_id={}, original_len={}, modified_len={}",
                    session_id,
                    result.len(),
                    modified.len()
                );
                result = modified;
            }
        }
        result
    }

    /// 通过插件 TerminalHandler 管道处理终端输出
    pub async fn process_terminal_output(&self, session_id: &str, data: &str) -> String {
        let handlers = self.rust_terminal_handlers.read().await;
        let mut result = data.to_string();
        for handler in handlers.iter() {
            if let Some(modified) = handler.on_output(session_id, &result) {
                tracing::debug!(
                    "Terminal output modified by plugin handler: session_id={}, original_len={}, modified_len={}",
                    session_id,
                    result.len(),
                    modified.len()
                );
                result = modified;
            }
        }
        result
    }

    /// 将提交输入行分发给 Rust 插件的 TerminalHandler 观察回调（见 ADR 0001）
    ///
    /// 与 `process_terminal_input`（逐块同步修改）互补：纯观察、不修改、
    /// 由 SessionManager 在异步错误隔离任务中调用
    pub async fn process_input_submitted(&self, session_id: &str, text: &str) {
        let handlers = self.rust_terminal_handlers.read().await;
        tracing::debug!(
            "process_input_submitted session_id={}, text_len={}, rust_handlers={}",
            session_id,
            text.len(),
            handlers.len()
        );
        for handler in handlers.iter() {
            handler.on_input_submitted(session_id, text);
        }
    }
}
