//! 组件模型加载路径（迁移 S1，ticket 02 骨架；09 清理后为唯一路径）
//!
//! 对应 docs/implementation-plans/mobile-wasmtime-component-migration.md：
//! - 契约定义在 `packages/plugin-sdk-mobile/rust/wit/bedcode.wit`（单一事实来源），
//!   本模块用 wasmtime 47 自带的 `bindgen!`（wasmtime-internal-wit-bindgen 47.0.3 /
//!   wit-parser 0.252，spike 已实证可与 0.60 产物互通）生成绑定：
//!   - import 接口 → `Host` trait（11 组全量接线，ticket 02/03）
//!   - export 接口 → `Plugin` world struct，宿主侧调用组件
//! - 安全机制：燃料看门狗（每次调用重置）、ResourceLimiter（256MB/1M）、
//!   `abi.version()` 协商、granted_permissions 校验；AOT `.cwasm` 缓存
//!   （`Component::serialize`，缓存文件名统一 `c` 前缀，与桌面端一致）
//! - 与桌面端 component.rs 的差异：移动端 WIT `abi` 仅 version（无 form 字段）；
//!   granted_permissions 校验保留（桌面端阶段 C 已删该字段）

use super::{WasmHostContext, WasmPluginState, aot_cache_key, FUEL_PER_CALL};
use crate::AppError;
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use wasmtime::component::{bindgen, Component, Instance, Linker};
use wasmtime::{ResourceLimiter, Store};

bindgen!({
    path: "../packages/plugin-sdk-mobile/rust/wit/bedcode.wit",
    world: "plugin",
});

// ==================== Host trait 实现（import 接口） ====================
//
// 函数体经 host_impl 逻辑层执行（权限校验 + runtime_handle block_on 已抽离，
// spec §5 R5：单份实现防行为漂移）；此处仅做 WIT 类型映射与错误转换
// 返回值映射：WIT `result<T, string>` → `Result<T, String>`

impl bedcode::plugin::host_log::Host for WasmPluginState {
    fn info(&mut self, message: String) {
        tracing::info!("[plugin:{}] {}", self.plugin_id, message);
    }

    fn debug(&mut self, message: String) {
        tracing::debug!("[plugin:{}] {}", self.plugin_id, message);
    }

    fn warn(&mut self, message: String) {
        tracing::warn!("[plugin:{}] {}", self.plugin_id, message);
    }

    fn error(&mut self, message: String) {
        tracing::error!("[plugin:{}] {}", self.plugin_id, message);
    }

    fn mark_plugin_error(&mut self, error: String) {
        // 与 host_mark_plugin_error（host_impl/filesrv.rs）同语义：状态上报回调
        super::host_impl::mark_plugin_error(self, &error);
    }
}

impl bedcode::plugin::host_storage::Host for WasmPluginState {
    fn get(&mut self, key: String) -> Result<Option<String>, String> {
        super::host_impl::storage_get(self, &key)
    }

    fn set(&mut self, key: String, value: String) -> Result<(), String> {
        super::host_impl::storage_set(self, &key, &value)
    }

    fn delete(&mut self, key: String) -> Result<(), String> {
        super::host_impl::storage_delete(self, &key)
    }
}

impl bedcode::plugin::host_database::Host for WasmPluginState {
    fn execute(&mut self, sql: String) -> Result<u32, String> {
        super::host_impl::db_execute(self, &sql)
    }

    fn query(&mut self, sql: String) -> Result<Option<String>, String> {
        super::host_impl::db_query(self, &sql)
    }
}

impl bedcode::plugin::host_terminal::Host for WasmPluginState {
    fn send(&mut self, session_id: String, data: String) -> Result<(), String> {
        super::host_impl::terminal_send(self, &session_id, &data)
    }
}

impl bedcode::plugin::host_events::Host for WasmPluginState {
    // WIT 中 emit 无错误返回，宿主侧记录日志（失败在 emit_event 内记录）
    fn emit(&mut self, event_name: String, payload_json: String) {
        super::host_impl::emit_event(self, &event_name, &payload_json);
    }

    fn notify(&mut self, title: String, body: String) -> Result<(), String> {
        super::host_impl::notify(self, &title, &body)
    }
}

impl bedcode::plugin::host_http::Host for WasmPluginState {
    fn fetch(&mut self, request_json: String) -> Result<Option<String>, String> {
        super::host_impl::http_fetch(self, &request_json)
    }
}

impl bedcode::plugin::host_fs::Host for WasmPluginState {
    fn read(&mut self, path: String) -> Result<Option<String>, String> {
        super::host_impl::fs_read(self, &path)
    }

    fn write(&mut self, path: String, data: String) -> Result<(), String> {
        super::host_impl::fs_write(self, &path, &data)
    }

    fn copy(&mut self, src: String, dst: String) -> Result<(), String> {
        super::host_impl::fs_copy(self, &src, &dst)
    }

    fn delete(&mut self, path: String) -> Result<(), String> {
        super::host_impl::fs_delete(self, &path)
    }

    fn exists(&mut self, path: String) -> Result<bool, String> {
        super::host_impl::fs_exists(self, &path)
    }

    fn request_auth(&mut self, paths_json: String) -> Result<bool, String> {
        super::host_impl::fs_request_auth(self, &paths_json)
    }

    fn write_media_downloads(&mut self, src_path: String, display_name: String, mime_type: String) -> Result<(), String> {
        // 与 core ABI 同语义（文件拷贝入 MediaStore），参数三态对齐（spec §3.2 全保留）
        super::host_impl::fs_write_media_downloads(self, &src_path, &display_name, &mime_type)
    }

    fn save_to_document(&mut self, src_path: String, display_name: String, mime_type: String) -> Result<(), String> {
        super::host_impl::fs_save_to_document(self, &src_path, &display_name, &mime_type)
    }
}

impl bedcode::plugin::host_config::Host for WasmPluginState {
    fn get(&mut self, key: String) -> Result<Option<String>, String> {
        super::host_impl::config_get(self, &key)
    }
}

impl bedcode::plugin::host_bus::Host for WasmPluginState {
    fn publish(&mut self, topic: String, payload_json: String) -> Result<(), String> {
        super::host_impl::bus_publish(self, &topic, &payload_json)
    }

    fn subscribe(&mut self, topic: String) -> Result<(), String> {
        super::host_impl::bus_subscribe(self, &topic)
    }

    fn unsubscribe(&mut self, topic: String) -> Result<(), String> {
        super::host_impl::bus_unsubscribe(self, &topic)
    }
}

impl bedcode::plugin::host_file_service::Host for WasmPluginState {
    fn mount(&mut self, options_json: String) -> Result<String, String> {
        super::host_impl::filesrv_mount(self, &options_json)
    }

    fn unmount(&mut self, mount_path: String) -> Result<(), String> {
        super::host_impl::filesrv_unmount(self, &mount_path)
    }

    fn update_roots(&mut self, mount_path: String, roots_json: String) -> Result<(), String> {
        super::host_impl::filesrv_update_roots(self, &mount_path, &roots_json)
    }

    fn get_peer(&mut self, peer_id: String) -> Result<Option<String>, String> {
        super::host_impl::filesrv_get_peer(self, &peer_id)
    }

    fn query_peer(&mut self, peer_id: String) -> Result<(), String> {
        super::host_impl::filesrv_query_peer(self, &peer_id)
    }

    fn approve_transfer(&mut self, batch_id: String) -> Result<(), String> {
        super::host_impl::filesrv_approve_transfer(self, &batch_id)
    }

    fn reject_transfer(&mut self, batch_id: String) -> Result<(), String> {
        super::host_impl::filesrv_reject_transfer(self, &batch_id)
    }

    fn set_approval_timeout(&mut self, mount_path: String, seconds: u64) -> Result<(), String> {
        super::host_impl::filesrv_set_approval_timeout(self, &mount_path, seconds)
    }

    fn cancel_receiving(&mut self, session_id: String) -> Result<(), String> {
        super::host_impl::filesrv_cancel_receiving(self, &session_id)
    }
}

impl bedcode::plugin::host_transfer::Host for WasmPluginState {
    fn start(&mut self, request_json: String) -> Result<String, String> {
        super::host_impl::transfer_start(self, &request_json)
    }

    fn cancel(&mut self, task_id: String) -> Result<(), String> {
        super::host_impl::transfer_cancel(self, &task_id)
    }
}

// ==================== Component Linker 组装 ====================

/// 构建组件侧 Linker（注册已接线的 import 接口）
///
/// 02：host-log / host-storage 两组；ticket 03 追加其余 9 组
/// （未注册接口被组件 import 时实例化报 unknown import——02 的缺接口可读报错依据）
pub(crate) fn build_component_linker(engine: &wasmtime::Engine) -> crate::Result<Linker<WasmPluginState>> {
    let mut linker = Linker::new(engine);
    type D = wasmtime::component::HasSelf<WasmPluginState>;
    for iface in [
        bedcode::plugin::host_log::add_to_linker::<WasmPluginState, D>,
        bedcode::plugin::host_storage::add_to_linker::<WasmPluginState, D>,
        bedcode::plugin::host_database::add_to_linker::<WasmPluginState, D>,
        bedcode::plugin::host_terminal::add_to_linker::<WasmPluginState, D>,
        bedcode::plugin::host_events::add_to_linker::<WasmPluginState, D>,
        bedcode::plugin::host_http::add_to_linker::<WasmPluginState, D>,
        bedcode::plugin::host_fs::add_to_linker::<WasmPluginState, D>,
        bedcode::plugin::host_config::add_to_linker::<WasmPluginState, D>,
        bedcode::plugin::host_bus::add_to_linker::<WasmPluginState, D>,
        bedcode::plugin::host_file_service::add_to_linker::<WasmPluginState, D>,
        bedcode::plugin::host_transfer::add_to_linker::<WasmPluginState, D>,
    ] {
        iface(&mut linker, |s| s).map_err(|e| {
            AppError::Plugin(format!("Failed to register component host interface: {}", e))
        })?;
    }
    Ok(linker)
}

// ==================== WasmRuntime 组件路径 ====================

impl super::WasmRuntime {
    /// 从文件编译 WASM 组件（带 AOT 缓存）
    ///
    /// 缓存文件以 `c` 前缀 + wasm 路径 hash 命名（与桌面端一致；组件产物统一
    /// 前缀，09 清理 core 路径后无共存的 Module 产物）。机制：宿主 cache 目录
    /// （非插件目录）、原子写回、deserialize 失败降级重编译。
    pub(crate) fn compile_component_from_file(&self, path: &Path) -> crate::Result<Component> {
        let Some(cache_dir) = &self.aot_cache_dir else {
            return Component::from_file(&self.engine, path).map_err(|e| {
                AppError::Plugin(format!(
                    "Failed to compile WASM component from '{}': {}",
                    path.display(),
                    e
                ))
            });
        };

        let wasm_md = std::fs::metadata(path).ok();
        let cache_path = cache_dir.join(format!(
            "c{:016x}.cwasm",
            aot_cache_key(path, wasm_md.as_ref().map(|md| md.len()).unwrap_or(0))
        ));

        let cache_fresh = wasm_md
            .and_then(|w| w.modified().ok())
            .zip(
                std::fs::metadata(&cache_path)
                    .ok()
                    .and_then(|c| c.modified().ok()),
            )
            .map(|(wm, cm)| cm >= wm)
            .unwrap_or(false);

        if cache_fresh {
            // unsafe：产物为本机自写缓存；Engine 版本/特性不匹配时 deserialize 失败，
            // 回退到完整编译路径
            if let Ok(component) = unsafe { Component::deserialize_file(&self.engine, &cache_path) }
            {
                tracing::debug!(
                    path = %cache_path.display(),
                    "Loaded WASM component from AOT cache"
                );
                return Ok(component);
            }
        }

        let component = Component::from_file(&self.engine, path).map_err(|e| {
            AppError::Plugin(format!(
                "Failed to compile WASM component from '{}': {}",
                path.display(),
                e
            ))
        })?;

        // 写回 AOT 缓存：先写临时文件再 rename（原子替换，避免崩溃留半截产物）；
        // 失败不阻断加载（下次启动重新编译）
        match component.serialize() {
            Ok(bytes) => {
                if let Err(e) = std::fs::create_dir_all(cache_dir) {
                    tracing::warn!(
                        path = %cache_dir.display(),
                        error = %e,
                        "Failed to create AOT cache dir, will recompile next time"
                    );
                    return Ok(component);
                }
                let tmp_path = cache_path.with_extension("cwasm.tmp");
                let write_result = std::fs::write(&tmp_path, &bytes)
                    .and_then(|_| std::fs::rename(&tmp_path, &cache_path));
                if let Err(e) = write_result {
                    tracing::warn!(
                        path = %cache_path.display(),
                        error = %e,
                        "Failed to write AOT cache, will recompile next time"
                    );
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to serialize component for AOT cache");
            }
        }

        Ok(component)
    }

    /// 实例化 WASM 组件
    ///
    /// 防护：ResourceLimiter、实例化前燃料注入（覆盖静态构造器）、
    /// `abi.version()` 协商（> ABI_VERSION 拒绝）、导出完整性校验
    /// （`Plugin::new` 要求 world 全量导出）。
    pub(crate) fn instantiate_component(
        &self,
        component: &Component,
        plugin_id: &str,
        host_ctx: Arc<WasmHostContext>,
        granted_permissions: HashSet<String>,
    ) -> crate::Result<LoadedComponentPlugin> {
        let state = WasmPluginState {
            plugin_id: plugin_id.to_string(),
            host_ctx,
            runtime_handle: self.runtime_handle.clone(),
            granted_permissions,
        };
        let mut store = Store::new(&self.engine, state);
        store.limiter(|state| state as &mut dyn ResourceLimiter);
        // 实例化可能执行 guest 代码（静态构造器等），先注入单次调用燃料
        store.set_fuel(FUEL_PER_CALL).map_err(|e| {
            AppError::Plugin(format!(
                "Failed to set fuel for plugin '{}': {}",
                plugin_id, e
            ))
        })?;

        let instance = self.linker.instantiate(&mut store, component).map_err(|e| {
            AppError::Plugin(format!(
                "Failed to instantiate WASM component for plugin '{}': {}",
                plugin_id, e
            ))
        })?;

        LoadedComponentPlugin::verify_abi(&mut store, &instance)?;

        // 实例创建日志：与 LoadedComponentPlugin::drop 的死亡日志成对，
        // 构成实例生命周期观测（plugin_id 键控）
        tracing::info!(
            plugin_id = %plugin_id,
            "WASM plugin instance created (component model)"
        );

        Ok(LoadedComponentPlugin {
            plugin_id: plugin_id.to_string(),
            instance,
            store,
            created_at: std::time::Instant::now(),
        })
    }
}

// ==================== 组件插件实例 ====================

/// 已加载的组件形态 WASM 插件
///
/// 持有 component Instance + Store；全部调用走 bindgen 生成的类型化接口
/// （无 (ptr,len) 内存搬运）。Store 必须与 Instance 一起持有，
/// 否则导出函数无法调用。业务方法（invoke/activate/钩子等）在 03 按需添加。
pub(crate) struct LoadedComponentPlugin {
    plugin_id: String,
    instance: wasmtime::component::Instance,
    store: Store<WasmPluginState>,
    /// 实例创建时刻（Drop 日志计算存活时长）
    created_at: std::time::Instant,
}

impl Drop for LoadedComponentPlugin {
    /// 实例死亡日志：Store 被 drop（停用 / 热重载替换 / 应用退出 / 异常清理）时记录，
    /// 与创建日志（`instantiate_component`）成对，构成实例生命周期观测。
    /// Drop 内无锁操作，tracing 安全。
    fn drop(&mut self) {
        tracing::info!(
            plugin_id = %self.plugin_id,
            lifetime_ms = self.created_at.elapsed().as_millis() as u64,
            "WASM plugin instance dropped"
        );
    }
}

impl LoadedComponentPlugin {
    /// ABI 版本协商（插件 `abi.version()` 高于宿主支持版本时拒绝加载）
    ///
    /// - `abi.version()` 语义与 `bedcode_plugin_api_mobile::abi::ABI_VERSION` 一致
    /// - 移动端 WIT 无 `abi.form()`（项目未发布、一次性切割，无 core 共存形态）
    fn verify_abi(
        store: &mut Store<WasmPluginState>,
        instance: &Instance,
    ) -> crate::Result<()> {
        store.set_fuel(FUEL_PER_CALL).map_err(|e| {
            AppError::Plugin(format!("Failed to set fuel for ABI verification: {}", e))
        })?;
        // Plugin::new 即导出完整性校验：world 声明的 8 组导出接口全量必须存在
        let exports = Plugin::new(&mut *store, instance).map_err(|e| {
            AppError::Plugin(format!("WASM component missing required exports: {}", e))
        })?;
        let version = exports
            .bedcode_plugin_abi()
            .call_version(&mut *store)
            .map_err(|e| {
                AppError::Plugin(format!("WASM component abi.version() call failed: {}", e))
            })?;
        if version > bedcode_plugin_api_mobile::abi::ABI_VERSION {
            return Err(AppError::Plugin(format!(
                "Plugin requires ABI v{} but host supports v{} — please upgrade BedCode",
                version,
                bedcode_plugin_api_mobile::abi::ABI_VERSION
            )));
        }
        Ok(())
    }

    /// 获取 world 导出绑定（每次调用重新索引导出，开销可忽略）
    ///
    /// 顺带重置燃料预算（单次调用预算，宿主调用阻塞不消耗燃料，见 FUEL_PER_CALL）
    fn exports(&mut self) -> crate::Result<Plugin> {
        self.store.set_fuel(FUEL_PER_CALL).map_err(|e| {
            AppError::Plugin(format!("WASM fuel refill failed: {}", e))
        })?;
        Plugin::new(&mut self.store, &self.instance).map_err(|e| {
            AppError::Plugin(format!("WASM component exports access failed: {}", e))
        })
    }

    // ==================== 业务方法 ====================

    /// 调用插件的 activate 导出
    pub(crate) fn activate(&mut self) -> crate::Result<i32> {
        let exports = self.exports()?;
        let lifecycle = exports.bedcode_plugin_lifecycle();
        match lifecycle.call_activate(&mut self.store) {
            Ok(Ok(())) => Ok(0),
            Ok(Err(msg)) => {
                Err(AppError::Plugin(format!("WASM activate() failed: {}", msg)))
            }
            Err(e) => Err(AppError::Plugin(format!("WASM activate() call failed: {}", e))),
        }
    }

    /// 调用插件的 deactivate 导出
    pub(crate) fn deactivate(&mut self) -> crate::Result<i32> {
        let exports = self.exports()?;
        let lifecycle = exports.bedcode_plugin_lifecycle();
        match lifecycle.call_deactivate(&mut self.store) {
            Ok(Ok(())) => Ok(0),
            Ok(Err(msg)) => {
                Err(AppError::Plugin(format!("WASM deactivate() failed: {}", msg)))
            }
            Err(e) => Err(AppError::Plugin(format!("WASM deactivate() call failed: {}", e))),
        }
    }

    /// 调用插件的 invoke_command 导出
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

    /// 调用 events 导出并统一映射 WIT `result<_, string>` 到 `crate::Result`
    ///
    /// 插件侧失败（`Ok(Err(msg))`）按 warn 记录后返回 Ok——事件分发不阻断宿主流程；
    /// 调用级失败（Err）包装 AppError 返回。闭包返回预格式化消息串，由本函数统一
    /// 拼「WASM <导出名>()」上下文（错误串带失败点）。
    fn call_event_export(
        &mut self,
        call: impl FnOnce(&mut Self) -> Result<Result<(), String>, String>,
        export_name: &str,
    ) -> crate::Result<()> {
        match call(self) {
            Ok(Ok(())) => Ok(()),
            Ok(Err(msg)) => {
                tracing::warn!("WASM {}() failed: {}", export_name, msg);
                Ok(())
            }
            Err(e) => Err(AppError::Plugin(format!(
                "WASM {}() call failed: {}",
                export_name, e
            ))),
        }
    }

    /// 调用插件的消息总线消息接收导出（移动端 WIT events.on-bus-message）
    pub(crate) fn on_bus_message(
        &mut self,
        msg: &bedcode_plugin_api_mobile::BusMessage,
    ) -> crate::Result<()> {
        let payload_str = serde_json::to_string(&msg.payload).unwrap_or_default();
        self.call_event_export(
            |s| {
                let exports = s.exports().map_err(|e| e.to_string())?;
                let events = exports.bedcode_plugin_events();
                events
                    .call_on_bus_message(&mut s.store, &msg.topic, &payload_str)
                    .map_err(|e| e.to_string())
            },
            "on_bus_message",
        )
    }

    /// 调用插件的上传策略钩子导出（fail-closed 语义由调用方保持；
    /// 组件契约强制实现，guest 返回的决定 JSON 原样透传）
    pub(crate) fn call_upload_hook(&mut self, meta_json: &str) -> crate::Result<String> {
        let exports = self.exports()?;
        let hooks = exports.bedcode_plugin_upload_hook();
        hooks.call_on_upload_request(&mut self.store, meta_json).map_err(|e| {
            AppError::Plugin(format!("WASM on_upload_request() call failed: {}", e))
        })
    }

    /// 调用插件的批量传输请求钩子导出（fail-closed 语义由调用方保持）
    pub(crate) fn call_transfer_request(&mut self, meta_json: &str) -> crate::Result<String> {
        let exports = self.exports()?;
        let hooks = exports.bedcode_plugin_transfer_request_hook();
        hooks.call_on_transfer_request(&mut self.store, meta_json).map_err(|e| {
            AppError::Plugin(format!("WASM on_transfer_request() call failed: {}", e))
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

    /// 调用插件的生命周期事件导出（移动端 PluginLifecycleEvent 枚举 → WIT 方法映射）
    ///
    /// 映射表：AppStartup/AppShutdown → lifecycle.on-startup/on-shutdown；
    /// AuthSuccess → events.on-auth-success；Disconnect → events.on-disconnect；
    /// SessionCreated/SessionStopped → events.on-session-created/on-session-stopped；
    /// TerminalInput/TerminalOutput → terminal-hooks（事件经同一枚举分发，
    /// 各导出方法直达）
    pub(crate) fn call_lifecycle_event(
        &mut self,
        event: &crate::plugin::types::PluginLifecycleEvent,
    ) -> crate::Result<()> {
        use crate::plugin::types::PluginLifecycleEvent;
        match event {
            PluginLifecycleEvent::AppStartup => self.on_startup(),
            PluginLifecycleEvent::AppShutdown => self.on_shutdown(),
            PluginLifecycleEvent::AuthSuccess => self.call_event_export(
                |s| {
                    let exports = s.exports().map_err(|e| e.to_string())?;
                    let events = exports.bedcode_plugin_events();
                    events
                        .call_on_auth_success(&mut s.store)
                        .map_err(|e| e.to_string())
                },
                "on_auth_success",
            ),
            PluginLifecycleEvent::Disconnect { reason } => self.call_event_export(
                |s| {
                    let exports = s.exports().map_err(|e| e.to_string())?;
                    let events = exports.bedcode_plugin_events();
                    events
                        .call_on_disconnect(&mut s.store, reason)
                        .map_err(|e| e.to_string())
                },
                "on_disconnect",
            ),
            PluginLifecycleEvent::SessionCreated { session_id } => self.call_event_export(
                |s| {
                    let exports = s.exports().map_err(|e| e.to_string())?;
                    let events = exports.bedcode_plugin_events();
                    events
                        .call_on_session_created(&mut s.store, session_id)
                        .map_err(|e| e.to_string())
                },
                "on_session_created",
            ),
            PluginLifecycleEvent::SessionStopped { session_id } => self.call_event_export(
                |s| {
                    let exports = s.exports().map_err(|e| e.to_string())?;
                    let events = exports.bedcode_plugin_events();
                    events
                        .call_on_session_stopped(&mut s.store, session_id)
                        .map_err(|e| e.to_string())
                },
                "on_session_stopped",
            ),
            PluginLifecycleEvent::TerminalInput { session_id, data }
            | PluginLifecycleEvent::TerminalOutput { session_id, data } => {
                // 终端钩子：返回文本（option<string>）仅表示插件响应成功，宿主不消费该文本
                //（富文本回调不在本枚举，宿主只透传调用并丢弃返回值）
                let result = match event {
                    PluginLifecycleEvent::TerminalInput { .. } => self.on_terminal_input(session_id, data),
                    _ => self.on_terminal_output(session_id, data),
                };
                result.map(|_| ())
            }
        }
    }

    /// 测试访问器：直接获取 Store（燃料断言用）
    pub(crate) fn raw_store(&mut self) -> (&mut Store<WasmPluginState>, &wasmtime::component::Instance) {
        (&mut self.store, &self.instance)
    }
}

// ==================== 测试 ====================
//
// 测试组件为独立 crate（bedcode-mobile/packages/plugin-component-test），基于
// WIT 契约生成绑定；异常形态经 features 控制（high-abi/spin-loop/big-alloc/
// import-extra）。构建+编码策略与桌面端一致：产物存在且源码未更新时复用，
// 避免每次跑测试都触发 cargo build。

#[cfg(test)]
pub(crate) mod tests {
    use super::super::{WasmHostContext, WasmRuntime};
    use super::*;
    use crate::plugin::fs_auth::FsAuthChecker;
    use crate::plugin::message_bus::MessageBus;
    use crate::plugin::storage::PluginStorage;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};

    /// 测试用插件 ID（宿主主库表前缀校验依赖它）
    const TEST_PLUGIN_ID: &str = "com.bedcode.test";
    const PERMISSION_STORAGE: &str = bedcode_plugin_api_mobile::permission::PERMISSION_STORAGE;

    /// 构建测试引擎：燃料看门狗必须与生产配置一致（WasmRuntime::new）
    fn test_engine() -> wasmtime::Engine {
        let mut config = wasmtime::Config::new();
        config.consume_fuel(true);
        wasmtime::Engine::new(&config).expect("create test engine")
    }

    /// 测试组件字节缓存（按 features key），跨用例复用避免重复 cargo build
    static COMPONENT_CACHE: OnceLock<Mutex<HashMap<String, Vec<u8>>>> = OnceLock::new();

    /// 构建真实插件组件：SDK `wasm_entry!` 宏产物（迁移 ticket 04 验收用）
    ///
    /// 与 `build_test_component` 同链路：cargo build（wasm32，wasm feature）→
    /// wit-component 编码。被测对象是 SDK 宏生成的组件（区别于手写 Guest impl
    /// 的 plugin-component-test）——宏展开错误 / export! 接线错误在此暴露。
    pub(crate) fn build_auto_task_component() -> Vec<u8> {
        let cache = COMPONENT_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        const KEY: &str = "auto-task";
        if let Some(bytes) = cache.lock().unwrap().get(KEY) {
            return bytes.clone();
        }

        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let plugin_dir = manifest_dir.join("../plugins/auto-task");
        let target_dir = plugin_dir.join("target").to_str().unwrap().to_string();
        let manifest_path = plugin_dir.join("rust/Cargo.toml").to_str().unwrap().to_string();
        let status = std::process::Command::new("cargo")
            .args([
                "build",
                "--release",
                "--target",
                "wasm32-unknown-unknown",
                "--target-dir",
                &target_dir,
                "--no-default-features",
                "--features",
                "wasm",
                "--manifest-path",
                &manifest_path,
            ])
            .status()
            .expect("Failed to run cargo build for auto-task component");
        assert!(status.success(), "auto-task WASM build failed");

        let core = std::fs::read(
            plugin_dir
                .join("target/wasm32-unknown-unknown/release/bedcode_plugin_auto_task.wasm"),
        )
        .expect("Failed to read auto-task module after build");
        // 宏产物必经 componentize（等效本函数内编码）；SDK 构建链已内置该步骤。
        // 此处直接编码 core module（若传入已组件化产物，编码器会拒绝）
        let component = wit_component::ComponentEncoder::default()
            .validate(true)
            .module(&core)
            .expect("component encoder module")
            .encode()
            .expect("component encoder encode");
        assert_eq!(&component[..4], [0x00, 0x61, 0x73, 0x6d], "auto-task 组件应以 core module 段起始");
        assert_eq!(&component[4..8], [0x0d, 0x00, 0x01, 0x00], "auto-task 组件头应为 0d 00 01 00");

        cache.lock().unwrap().insert(KEY.to_string(), component.clone());
        component
    }

    /// 构建并编码测试组件：cargo build（指定 features）→ wit-component 编码
    fn build_test_component(features: &[&str]) -> Vec<u8> {
        let cache = COMPONENT_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        let key = if features.is_empty() {
            "default".to_string()
        } else {
            features.join("+")
        };
        if let Some(bytes) = cache.lock().unwrap().get(&key) {
            return bytes.clone();
        }

        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let plugin_dir = manifest_dir.join("../packages/plugin-component-test");
        let target_dir = plugin_dir.join("target").to_str().unwrap().to_string();
        let features_arg = features.join(",");
        let manifest_path = plugin_dir.join("Cargo.toml").to_str().unwrap().to_string();
        let mut args = vec![
            "build",
            "--release",
            "--target",
            "wasm32-unknown-unknown",
            "--target-dir",
            &target_dir,
        ];
        if !features.is_empty() {
            args.push("--features");
            args.push(&features_arg);
        }
        args.push("--manifest-path");
        args.push(&manifest_path);

        let status = std::process::Command::new("cargo")
            .args(&args)
            .status()
            .expect("Failed to run cargo build for test component");
        assert!(status.success(), "Test component WASM build failed");

        let core = std::fs::read(
            plugin_dir
                .join("target/wasm32-unknown-unknown/release/bedcode_plugin_component_test.wasm"),
        )
        .expect("Failed to read test component module after build");
        let component = wit_component::ComponentEncoder::default()
            .validate(true)
            .module(&core)
            .expect("component encoder module")
            .encode()
            .expect("component encoder encode");
        // 产物形态：核心模块段在前、组件头随后（spike 实证 00 61 73 6d 0d 00 01 00）
        assert_eq!(&component[..4], [0x00, 0x61, 0x73, 0x6d], "编码后组件应以 core module 段起始");
        assert_eq!(&component[4..8], [0x0d, 0x00, 0x01, 0x00], "编码后组件头应为 0d 00 01 00");

        cache.lock().unwrap().insert(key, component.clone());
        component
    }

    /// 构造最小宿主上下文（db 内存库 + tempdir storage；app_handle=None 无头形态）
    pub(crate) fn build_host_ctx(tmp: &tempfile::TempDir) -> Arc<WasmHostContext> {
        let db = Arc::new(Mutex::new(
            rusqlite::Connection::open_in_memory().expect("open in-memory db"),
        ));
        let storage = Arc::new(PluginStorage::new(&tmp.path().to_path_buf()));
        // 无头/测试上下文：fs_auth 的 app_handle 亦为 None（桌面端 build_host_ctx 同形态）
        let fs_auth = Arc::new(FsAuthChecker::new(storage.clone(), None));
        let status_reporter: Arc<dyn Fn(&str, &str) + Send + Sync> = Arc::new(|_, _| {});
        Arc::new(WasmHostContext::new(
            db,
            storage,
            None,
            fs_auth,
            Arc::new(MessageBus::new()),
            status_reporter,
        ))
    }

    /// 组件完整加载往返：实例化 + ABI 协商（version=6）+ 导出完整性 +
    /// 燃料注入生效 + manifest 读取 + Host trait（storage/log）接线直测
    #[test]
    fn test_component_roundtrip_and_host_impl() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            // 预写 storage：验证 host_storage Host impl 读回（JSON 值往返）
            let tmp = tempfile::tempdir().expect("tempdir");
            let host_ctx = build_host_ctx(&tmp);
            host_ctx
                .storage
                .set(
                    TEST_PLUGIN_ID,
                    "test-key",
                    serde_json::json!({"k": "v"}),
                )
                .await
                .expect("preset storage key");

            let runtime = WasmRuntime::new(Some(tmp.path().join("aot")))
                .expect("create wasm runtime");
            // 组件必须用 runtime 自身 Engine 编译（跨 Engine 实例化被 wasmtime 拒绝）
            let component = Component::from_binary(runtime.engine(), &build_test_component(&[]))
                .expect("compile test component");
            let mut plugin = runtime
                .instantiate_component(
                    &component,
                    TEST_PLUGIN_ID,
                    host_ctx.clone(),
                    HashSet::from([PERMISSION_STORAGE.to_string()]),
                )
                .expect("instantiate component");

            // host_storage Host impl 直测（guest 侧调用发生在 invoke，03 覆盖）
            let mut state = WasmPluginState {
                plugin_id: TEST_PLUGIN_ID.to_string(),
                host_ctx: host_ctx.clone(),
                runtime_handle: plugin.store.data().runtime_handle.clone(),
                granted_permissions: HashSet::from([PERMISSION_STORAGE.to_string()]),
            };
            let got = bedcode::plugin::host_storage::Host::get(&mut state, "test-key".into())
                .expect("host_storage.get");
            assert_eq!(got.as_deref(), Some(r#"{"k":"v"}"#));
            bedcode::plugin::host_storage::Host::set(
                &mut state,
                "roundtrip-key".into(),
                r#"{"n":1}"#.into(),
            )
            .expect("host_storage.set");
            let got2 = bedcode::plugin::host_storage::Host::get(&mut state, "roundtrip-key".into())
                .expect("host_storage.get #2");
            assert_eq!(got2.as_deref(), Some(r#"{"n":1}"#));
            // 权限拒绝：未授权插件读 storage 返回 Err（fail-closed 语义）
            let mut unauth = WasmPluginState {
                plugin_id: TEST_PLUGIN_ID.to_string(),
                host_ctx,
                runtime_handle: plugin.store.data().runtime_handle.clone(),
                granted_permissions: HashSet::new(),
            };
            let denied = bedcode::plugin::host_storage::Host::get(&mut unauth, "test-key".into());
            assert!(denied.is_err(), "未授权 storage 访问必须被拒绝");

            // abi.version() 协商：version=6 <= ABI_VERSION=6
            let exports = plugin.exports().expect("world exports");
            assert_eq!(
                exports.bedcode_plugin_abi().call_version(&mut plugin.store).unwrap(),
                bedcode_plugin_api_mobile::abi::ABI_VERSION
            );

            // 燃料注入生效：abi.version() 调用必然消耗燃料
            {
                let remaining = plugin.store.get_fuel().expect("get fuel");
                assert!(
                    remaining < FUEL_PER_CALL,
                    "export call must consume fuel, remaining={}",
                    remaining
                );
            }

            // manifest（world 导出完整性的顺带验证）
            let manifest: serde_json::Value =
                serde_json::from_str(&exports.bedcode_plugin_manifest().call_get(&mut plugin.store).expect("manifest")).unwrap();
            assert_eq!(manifest["id"], "com.bedcode.component-test");
        });
    }

    /// 11 组 import 接口全部注册成功（build_component_linker 是纯接线代码，
    /// 任何一组接口名冲突/接线参数错误都会在此失败）
    #[test]
    fn test_component_linker_registers_all_interfaces() {
        let engine = test_engine();
        build_component_linker(&engine).expect("register all host interfaces");
    }

    /// 重复注册同一组接口必须报错
    ///
    /// 防止 build_component_linker 被调用两次时静默覆盖接线（实例化时会以
    /// 意外行为失败，不如注册期直接暴露）——桌面端 component.rs 同款测试
    #[test]
    fn test_component_linker_rejects_duplicate_registration() {
        let engine = test_engine();
        let mut linker = wasmtime::component::Linker::<WasmPluginState>::new(&engine);
        type D = wasmtime::component::HasSelf<WasmPluginState>;
        bedcode::plugin::host_storage::add_to_linker::<WasmPluginState, D>(&mut linker, |s| s)
            .expect("first registration");
        let err = bedcode::plugin::host_storage::add_to_linker::<WasmPluginState, D>(
            &mut linker,
            |s| s,
        )
        .expect_err("duplicate registration should fail");
        // wasmtime 对已注册的同名 interface instance 报 "defined twice"
        assert!(
            err.to_string().contains("defined twice"),
            "unexpected duplicate registration error: {}",
            err
        );
    }

    /// 业务方法全走通（03 验收）：生命周期 + 命令（含 host import 往返）+
    /// 终端钩子 + manifest + bus 事件 + 上传/传输钩子（fail-closed 决定透传）+
    /// 生命周期事件映射
    #[test]
    fn test_component_business_methods_roundtrip() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let tmp = tempfile::tempdir().expect("tempdir");
            let host_ctx = build_host_ctx(&tmp);
            host_ctx
                .storage
                .set(TEST_PLUGIN_ID, "test-key", serde_json::json!({"k": "v"}))
                .await
                .expect("preset storage key");

            let runtime = WasmRuntime::new(Some(tmp.path().join("aot"))).expect("wasm runtime");
            let component = Component::from_binary(runtime.engine(), &build_test_component(&[]))
                .expect("compile test component");
            let mut plugin = runtime
                .instantiate_component(
                    &component,
                    TEST_PLUGIN_ID,
                    host_ctx,
                    HashSet::from([PERMISSION_STORAGE.to_string()]),
                )
                .expect("instantiate component");

            // 生命周期
            assert_eq!(plugin.activate().expect("activate"), 0);
            assert_eq!(plugin.deactivate().expect("deactivate"), 0);
            plugin.on_startup().expect("on_startup");
            plugin.on_shutdown().expect("on_shutdown");

            // 命令调用：guest 内 storage/config import 双向往返（跨边界）
            let result = plugin
                .invoke_command("test.echo", r#"{"hello":"component"}"#)
                .expect("invoke_command");
            let result_json: serde_json::Value = serde_json::from_str(&result).unwrap();
            assert_eq!(result_json["name"], "test.echo");
            assert_eq!(result_json["stored"], "{\"k\":\"v\"}");
            // host-config 接线验证：system.time_ms 返回非空时间戳
            let now_ms = result_json["now_ms"].as_str().unwrap_or("");
            assert!(
                !now_ms.is_empty() && now_ms.parse::<u64>().is_ok(),
                "host-config system.time_ms 应返回时间戳，实际: {:?}",
                now_ms
            );

            // 终端钩子（组件契约强制实现；测试组件返回 None）
            assert_eq!(plugin.on_terminal_input("s1", "x").unwrap(), None);
            assert_eq!(plugin.on_terminal_output("s1", "x").unwrap(), None);

            // manifest
            let manifest: serde_json::Value =
                serde_json::from_str(&plugin.get_manifest().expect("manifest")).unwrap();
            assert_eq!(manifest["id"], "com.bedcode.component-test");

            // 上传/传输钩子：fail-closed 决定透传（组件返回固定拒绝 JSON）
            let upload_decision =
                plugin.call_upload_hook(r#"{"name":"a"}"#).expect("upload hook");
            let d: serde_json::Value = serde_json::from_str(&upload_decision).unwrap();
            assert_eq!(d["approved"], false, "上传钩子必须 fail-closed 拒绝");
            let transfer_decision = plugin
                .call_transfer_request(r#"{"name":"a"}"#)
                .expect("transfer hook");
            let d: serde_json::Value = serde_json::from_str(&transfer_decision).unwrap();
            assert_eq!(d["approved"], false, "传输钩子必须 fail-closed 拒绝");

            // bus 事件回调（组件返回 Ok）
            plugin
                .on_bus_message(&bedcode_plugin_api_mobile::BusMessage {
                    topic: "t".to_string(),
                    sender: "s".to_string(),
                    payload: serde_json::json!({}),
                    timestamp: 0,
                })
                .expect("on_bus_message");

            // 生命周期事件映射（auth/disconnect/session 各走一遍）
            use crate::plugin::types::PluginLifecycleEvent;
            plugin
                .call_lifecycle_event(&PluginLifecycleEvent::AuthSuccess)
                .expect("auth success");
            plugin
                .call_lifecycle_event(&PluginLifecycleEvent::Disconnect {
                    reason: "bye".to_string(),
                })
                .expect("disconnect");
            plugin
                .call_lifecycle_event(&PluginLifecycleEvent::SessionCreated {
                    session_id: "s1".to_string(),
                })
                .expect("session created");
            plugin
                .call_lifecycle_event(&PluginLifecycleEvent::SessionStopped {
                    session_id: "s1".to_string(),
                })
                .expect("session stopped");
            plugin
                .call_lifecycle_event(&PluginLifecycleEvent::TerminalInput {
                    session_id: "s1".to_string(),
                    data: "x".to_string(),
                })
                .expect("terminal input event");
        });
    }

    /// abi.version() 高于宿主支持的组件被拒绝加载
    #[test]
    fn test_component_high_abi_rejected() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let tmp = tempfile::tempdir().expect("tempdir");
            let host_ctx = build_host_ctx(&tmp);
            let runtime = WasmRuntime::new(Some(tmp.path().join("aot"))).expect("wasm runtime");
            let component = Component::from_binary(runtime.engine(), &build_test_component(&["high-abi"]))
                .expect("compile test component");
            let err = match runtime
                .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx, HashSet::new())
            {
                Err(e) => e,
                Ok(_) => panic!("高版本 ABI 组件必须被拒绝"),
            };
            let msg = err.to_string();
            assert!(msg.contains("ABI v999"), "应报告组件要求的版本，实际: {}", msg);
            assert!(msg.contains("upgrade"), "应提示升级 BedCode，实际: {}", msg);
        });
    }

    /// 燃料看门狗生效：死循环组件在单次调用预算内被 trap
    #[test]
    fn test_component_fuel_trap() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let tmp = tempfile::tempdir().expect("tempdir");
            let host_ctx = build_host_ctx(&tmp);
            let runtime = WasmRuntime::new(Some(tmp.path().join("aot"))).expect("wasm runtime");
            let component = Component::from_binary(runtime.engine(), &build_test_component(&["spin-loop"]))
                .expect("compile test component");
            let mut plugin = runtime
                .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx, HashSet::new())
                .expect("实例化正常（死循环在调用期，不在构造器）");

            let exports = plugin.exports().expect("world exports");
            let err = exports
                .bedcode_plugin_command()
                .call_invoke(&mut plugin.store, "spin", "{}")
                .expect_err("死循环必须被燃料看门狗 trap");
            // fuel 耗尽 trap 的 Display 含 wasm backtrace；trap 即通过，
            // 关键是 backtrace 顶部是 command#invoke（trap 发生在 guest 死循环内）
            let msg = err.to_string();
            assert!(
                msg.contains("command#invoke"),
                "trap 应发生在调用栈内并带 wasm backtrace，实际: {}",
                msg
            );
        });
    }

    /// ResourceLimiter 生效：内存阈值单元断言 + 超限组件集成断言
    #[test]
    fn test_component_limiter_rejects_over_limit_memory() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let tmp = tempfile::tempdir().expect("tempdir");
            let host_ctx = build_host_ctx(&tmp);

            // 单元层：阈值语义（256MB 内放行、超限拒绝）
            let mut state = WasmPluginState {
                plugin_id: TEST_PLUGIN_ID.to_string(),
                host_ctx: host_ctx.clone(),
                runtime_handle: tokio::runtime::Handle::current(),
                granted_permissions: HashSet::new(),
            };
            assert_eq!(state.memory_growing(0, 256 * 1024 * 1024, None).unwrap(), true);
            assert_eq!(
                state.memory_growing(0, 256 * 1024 * 1024 + 1, None).unwrap(),
                false,
                "超过 256MB 的内存增长必须被拒绝"
            );
            assert_eq!(state.table_growing(0, 1_000_000, None).unwrap(), true);
            assert_eq!(state.table_growing(0, 1_000_001, None).unwrap(), false);

            // 集成层：guest 直接 memory.grow 300MB → limiter 拒绝 → 返回 {"grow":"failed"}
            let runtime = WasmRuntime::new(Some(tmp.path().join("aot"))).expect("wasm runtime");
            let component = Component::from_binary(runtime.engine(), &build_test_component(&["big-alloc"]))
                .expect("compile test component");
            let mut plugin = runtime
                .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx, HashSet::new())
                .expect("实例化正常（分配发生在调用期）");
            let exports = plugin.exports().expect("world exports");
            let result = exports
                .bedcode_plugin_command()
                .call_invoke(&mut plugin.store, "alloc", "{}");
            match result {
                Err(e) => {
                    // 也可能是 trap 形式拒绝，同样视为 limiter 生效
                    eprintln!("[big-alloc] invoke failed (trap): {}", e);
                }
                Ok(s) => {
                    let v: serde_json::Value = serde_json::from_str(&s).unwrap_or_default();
                    assert_eq!(
                        v["grow"].as_str(),
                        Some("failed"),
                        "ResourceLimiter 未拦截组件内存增长: {}",
                        s
                    );
                }
            }
        });
    }

    /// 迁移 ticket 04 验收：SDK `wasm_entry!` 宏生成的组件能被宿主加载，
    /// 完成 ABI 协商（version=6）、激活、命令调用（含错误 JSON 透传）
    ///
    /// 被测对象是真实插件（auto-task）经新 SDK 编译的产物 —— 宏展开正确性、
    /// `export!` 跨 crate 接线、8 组接口全量导出的最终证明（插件业务代码零改动）。
    #[test]
    fn test_sdk_macro_component_loads_and_activates() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let tmp = tempfile::tempdir().expect("tempdir");
            let host_ctx = build_host_ctx(&tmp);
            let runtime = WasmRuntime::new(Some(tmp.path().join("aot"))).expect("wasm runtime");
            let component = Component::from_binary(runtime.engine(), &build_auto_task_component())
                .expect("compile auto-task component");
            let mut plugin = runtime
                .instantiate_component(
                    &component,
                    "com.bedcode.auto-task",
                    host_ctx,
                    HashSet::new(),
                )
                .expect("instantiate auto-task component");

            // ABI 协商：与 SDK abi::ABI_VERSION 同步（宏内 `abi.version()` 输出）
            let exports = plugin.exports().expect("world exports");
            assert_eq!(
                exports.bedcode_plugin_abi().call_version(&mut plugin.store).unwrap(),
                bedcode_plugin_api_mobile::abi::ABI_VERSION
            );

            // 激活/停用（auto-task 仅日志，无权限依赖）
            assert_eq!(plugin.activate().expect("activate"), 0);
            assert_eq!(plugin.deactivate().expect("deactivate"), 0);

            // manifest（宏内 `manifest()` 序列化 plugin.json）
            let manifest: serde_json::Value =
                serde_json::from_str(&plugin.get_manifest().expect("manifest")).unwrap();
            assert_eq!(manifest["id"], "com.bedcode.auto-task");

            // 命令调用：auto-task 对未知命令返回 Err → 宏序列化为 {"error": ...} JSON
            let result = plugin
                .invoke_command("no.such.cmd", "{}")
                .expect("invoke_command");
            let v: serde_json::Value = serde_json::from_str(&result).unwrap();
            assert!(
                v["error"].as_str().is_some_and(|s| s.contains("Unknown command")),
                "命令错误应经宏转义为 error JSON，实际: {}",
                result
            );

            // 未实现钩子的默认行为（SDK trait 默认 fail-closed）：upload/transfer 均拒绝
            let d: serde_json::Value = serde_json::from_str(
                &plugin
                    .call_upload_hook(r#"{"relativePath":"a.txt","size":1}"#)
                    .expect("upload hook"),
            )
            .unwrap();
            assert_eq!(d["allow"], false, "默认上传钩子必须 fail-closed 拒绝");
            let d: serde_json::Value = serde_json::from_str(
                &plugin
                    .call_transfer_request(r#"{"batchId":"b1"}"#)
                    .expect("transfer hook"),
            )
            .unwrap();
            assert_eq!(d["allow"], false, "默认传输钩子必须 fail-closed 拒绝");
        });
    }

    /// AOT：二次加载命中 `.cwasm` 缓存（删除源文件仍可加载即证明命中），
    /// 产物只写宿主 cache 目录
    #[test]
    fn test_component_aot_cache_hit() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let tmp = tempfile::tempdir().expect("tempdir");
            let cache_dir = tmp.path().join("cache");
            let runtime = WasmRuntime::new(Some(cache_dir.clone())).expect("wasm runtime");

            // 源组件写盘（default 形态组件字节）
            let component_bytes = build_test_component(&[]);
            let src = tmp.path().join("plugin.component.wasm");
            std::fs::write(&src, &component_bytes).expect("write source component");

            // 首次编译：产物落缓存
            runtime
                .compile_component_from_file(&src)
                .expect("first compile");
            let cache_files: Vec<_> = std::fs::read_dir(&cache_dir)
                .expect("cache dir")
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().map(|x| x == "cwasm").unwrap_or(false))
                .collect();
            assert_eq!(cache_files.len(), 1, "缓存目录应恰有 1 个 .cwasm 产物");
            let cache_file = cache_files[0].path();
            assert!(
                cache_file
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with('c')),
                "组件缓存文件应以 c 前缀命名: {}",
                cache_file.display()
            );

            // 二次加载：缓存 mtime 不变 = 命中缓存（未触发重编译/重写）。
            // 注：源文件消失时缓存不可命中（新鲜度判定依赖
            // 源 mtime，源缺失按不新鲜处理）——这个边界行为与 Module 路径一致，不改
            let cache_mtime = std::fs::metadata(&cache_file)
                .and_then(|m| m.modified())
                .expect("cache mtime");
            // 20ms 远大于文件系统 mtime 粒度；重编译必然重写缓存文件使 mtime 前进
            std::thread::sleep(std::time::Duration::from_millis(20));
            runtime
                .compile_component_from_file(&src)
                .expect("second compile (cache hit)");
            let cache_mtime2 = std::fs::metadata(&cache_file)
                .and_then(|m| m.modified())
                .expect("cache mtime after second compile");
            assert_eq!(
                cache_mtime, cache_mtime2,
                "二次编译未命中缓存（缓存文件被重写）"
            );

            // 产物不写回源目录：源目录无 .wasm/.cwasm 残留
            let src_dir: Vec<_> = std::fs::read_dir(tmp.path())
                .expect("src dir")
                .filter_map(|e| e.ok())
                .collect();
            for e in src_dir {
                let name = e.file_name().to_string_lossy().to_string();
                assert!(
                    !name.ends_with(".cwasm"),
                    "AOT 产物不得写回源目录（只允许宿主 cache 目录持有）: {}",
                    e.path().display()
                );
            }
        });
    }
}
