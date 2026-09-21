//! `wasm` — PluginHost 职责面拆分（P2）
//!
//! 自 `host.rs` 主 impl 拆出；经 `use super::*` 继承 host 模块的
//! 全部导入与常量，方法与字段可见性规则与内联时一致。

use super::*;
impl PluginHost {
    /// 重建 WASM 插件实例（load_plugin_from_file + 替换 map 条目）
    ///
    /// 激活路径：声明 WASI 预打开目录的插件首次授权后实例未覆盖新授权目录时
    /// 重建，使 /data 挂载与授权一致；热重载路径同样复用（停用 → 重建 → 重注册）。
    /// 重建 WASM 插件实例（load_plugin_from_file + 替换 map 条目）
    ///
    /// 激活路径：声明 WASI 预打开目录的插件首次授权后实例未覆盖新授权目录时
    /// 重建，使 /data 挂载与授权一致；热重载路径同样复用（停用 → 重建 → 重注册）。
    /// 失败上抛（原实例保留，激活流程走既有错误分支置 Error 态）。
    pub(crate) async fn rebuild_wasm_instance(&self, plugin_id: &str) -> crate::Result<()> {
        let (rust_library, extension_path, declared_preopen_dirs, resource_overrides) = {
            let plugins = self.plugins.read().await;
            let loaded = plugins
                .get(plugin_id)
                .ok_or_else(|| crate::AppError::Plugin(format!("Plugin not found: {}", plugin_id)))?;
            (
                loaded.manifest.rust_library.clone(),
                loaded.extension_path.clone(),
                loaded.manifest.wasi_preopen_dirs.clone(),
                loaded.manifest.resource_overrides,
            )
        };

        let plugin_dir = Path::new(&extension_path);
        let wasm_filename = format!("{}.wasm", rust_library);
        let wasm_path = plugin_dir.join(&wasm_filename);

        let new_wasm_plugin = self.wasm_runtime.load_plugin_from_file(
            &wasm_path,
            plugin_id,
            self.wasm_host_ctx.clone(),
            &declared_preopen_dirs,
            resource_overrides.as_ref(),
        )?;
        self.wasm_plugins
            .write()
            .await
            .insert(plugin_id.to_string(), Arc::new(Mutex::new(new_wasm_plugin)));

        // core-plugin-manager：系统组件实例重建后，能力注册表中的旧实例句柄
        // 已失效，按新实例重新装配（trap 自愈回落宿主原语的场景亦在此恢复）
        let is_system = {
            let plugins = self.plugins.read().await;
            plugins
                .get(plugin_id)
                .map(|p| p.manifest.kind == PluginKind::System)
                .unwrap_or(false)
        };
        if is_system {
            self.wasm_host_ctx.capabilities().revert_all_from(plugin_id);
            self.register_system_capabilities(plugin_id).await;
        }
        Ok(())
    }

    /// 停用插件
    /// 中止指定插件的定时器（停用时调用，v6 ADR 0003）
    /// 从 zip 分发包安装插件（dev 合入）
    ///
    /// 解压校验（manifest/身份/路径安全）→ 重新扫描用户目录 → WASM 实例化 →
    /// 注册 manifest 扩展点。同 id 已安装时回滚安装目录并报错（需先卸载）。

    /// 热重载 WASM 插件（开发模式）
    ///
    /// 执行完整的卸载-重载-激活循环：
    /// 1. 停用插件
    /// 2. 重新编译并实例化 WASM 模块
    /// 3. 重新激活插件
    pub async fn reload_wasm_plugin(&self, plugin_id: &str) -> crate::Result<()> {
        {
            let plugins = self.plugins.read().await;
            let loaded = plugins
                .get(plugin_id)
                .ok_or_else(|| crate::AppError::Plugin(format!("Plugin not found: {}", plugin_id)))?;
            if loaded.source != PluginSource::Wasm {
                return Err(crate::AppError::Plugin(format!(
                    "Plugin {} is not a WASM plugin, cannot hot-reload",
                    plugin_id
                )));
            }
        }

        tracing::info!(plugin_id = %plugin_id, "Hot-reloading WASM plugin");

        // 1. 停用插件（不持久化）
        self.deactivate_plugin(plugin_id, false).await?;

        // 2. 重新编译并实例化 WASM 模块（替换 wasm_plugins map 条目）
        self.rebuild_wasm_instance(plugin_id).await?;

        // 3. 重新注册 manifest contributes
        let m = {
            let plugins = self.plugins.read().await;
            let loaded = plugins
                .get(plugin_id)
                .ok_or_else(|| crate::AppError::Plugin(format!("Plugin not found after reload: {}", plugin_id)))?;
            loaded.manifest.clone()
        };
        self.registry.register_commands(&m.id, &m.contributes.commands).await;
        self.registry.register_views(&m.id, &m.contributes.views).await;
        if let Some(ref term) = m.contributes.terminal {
            self.registry
                .register_terminal_handlers(&m.id, &term.input_handlers, &term.output_parsers)
                .await;
        }
        self.registry
            .register_tool_providers(&m.id, &m.contributes.tool_providers)
            .await;
        self.registry
            .register_http_endpoints(&m.id, &m.contributes.http_endpoints)
            .await;
        self.registry
            .register_file_handlers(&m.id, &m.contributes.file_handlers)
            .await;

        // 4. 重新激活
        self.activate_plugin(plugin_id, false).await?;

        tracing::info!(plugin_id = %plugin_id, "WASM plugin hot-reloaded successfully");
        Ok(())
    }
}
