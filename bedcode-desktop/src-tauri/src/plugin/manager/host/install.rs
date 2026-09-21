//! `install` — PluginHost 职责面拆分（P2）
//!
//! 自 `host.rs` 主 impl 拆出；经 `use super::*` 继承 host 模块的
//! 全部导入与常量，方法与字段可见性规则与内联时一致。

use super::*;
impl PluginHost {
    /// 停用插件
    /// 中止指定插件的定时器（停用时调用，v6 ADR 0003）
    /// 从 zip 分发包安装插件（dev 合入）
    ///
    /// 解压校验（manifest/身份/路径安全）→ 重新扫描用户目录 → WASM 实例化 →
    /// 注册 manifest 扩展点。同 id 已安装时回滚安装目录并报错（需先卸载）。
    pub async fn install_from_zip(&self, zip_path: &str) -> crate::Result<String> {
        let plugin_id =
            crate::plugin::downloader::PluginDownloader::install_from_file(zip_path, &self.user_plugins_dir)?;

        if self.plugins.read().await.contains_key(&plugin_id) {
            let dir = self.user_plugins_dir.join(&plugin_id);
            if dir.exists() {
                std::fs::remove_dir_all(&dir)
                    .map_err(|e| crate::AppError::Plugin(format!("Failed to rollback plugin dir: {}", e)))?;
            }
            return Err(crate::AppError::Plugin(format!(
                "Plugin '{}' is already installed. Uninstall it first to install a new version.",
                plugin_id
            )));
        }

        // 重新扫描用户目录，加载新插件（含 WASM 实例化，与 new() 初始化路径一致）
        let loaded_map = PluginLoader::load_all(
            &self.user_plugins_dir,
            &self.permission,
            Some(PluginSource::UserInstalled),
        );
        let Some(loaded) = loaded_map.get(&plugin_id).cloned() else {
            return Err(crate::AppError::Plugin(format!(
                "Plugin installed but failed to load: {}",
                plugin_id
            )));
        };

        let wasm_plugin = if !loaded.manifest.rust_library.is_empty() {
            let wasm_path = Path::new(&loaded.extension_path).join(format!(
                "{}{}",
                loaded.manifest.rust_library,
                crate::system::constants::plugin::WASM_FILE_EXT
            ));
            match self.wasm_runtime.load_plugin_from_file(
                &wasm_path,
                &plugin_id,
                self.wasm_host_ctx.clone(),
                &loaded.manifest.wasi_preopen_dirs,
                loaded.manifest.resource_overrides.as_ref(),
            ) {
                Ok(wasm_plugin) => Some(Arc::new(Mutex::new(wasm_plugin))),
                Err(e) => {
                    tracing::error!(
                        plugin_id = %plugin_id,
                        error = %e,
                        "[PluginHost] Failed to load WASM for newly installed plugin"
                    );
                    None
                }
            }
        } else {
            None
        };

        // 写入 plugins / wasm_plugins map（WASM 实例化失败时插件以 Error 态入表，
        // 保证列表可见可诊断，与 new() 初始化语义一致）
        {
            let mut plugins = self.plugins.write().await;
            plugins.insert(
                plugin_id.clone(),
                if wasm_plugin.is_some() || loaded.manifest.rust_library.is_empty() {
                    loaded.clone()
                } else {
                    LoadedPlugin {
                        state: PluginState::Error(format!("WASM load failed: {}", loaded.extension_path)),
                        ..loaded.clone()
                    }
                },
            );
        }
        if let Some(wp) = wasm_plugin {
            self.wasm_plugins.write().await.insert(plugin_id.clone(), wp);
        }
        // 注册 manifest contributes 扩展点（commands/views/fileHandlers 等）
        self.register_plugin_contributions(&plugin_id).await;

        tracing::info!(
            plugin_id = %plugin_id,
            "[PluginHost] Plugin installed from zip and registered"
        );
        Ok(plugin_id)
    }

    /// 卸载插件：删除插件所有数据（存储 + 激活状态 + 安装目录 + 私有数据库，dev 合入）
    ///
    /// 适用范围：**所有来源的插件**（内置随包 / 用户 zip 安装 / 文件扫描 / 静态注册）。
    /// 前置条件：插件存在且**未启用**（Activated/Activating/Degraded 拒绝卸载）。
    /// 执行：停用（hooks 清理/总线退订/扩展点注销）→ 丢弃私有数据库连接 →
    /// 删除安装目录 → 移除运行时实例与记录 → 清空插件存储 → 撤销持久化审批
    /// 记录 → 清理持久化激活状态与限频簿记。

    /// 卸载插件：删除插件所有数据（存储 + 激活状态 + 安装目录 + 私有数据库，dev 合入）
    ///
    /// 适用范围：**所有来源的插件**（内置随包 / 用户 zip 安装 / 文件扫描 / 静态注册）。
    /// 前置条件：插件存在且**未启用**（Activated/Activating/Degraded 拒绝卸载）。
    /// 执行：停用（hooks 清理/总线退订/扩展点注销）→ 丢弃私有数据库连接 →
    /// 删除安装目录 → 移除运行时实例与记录 → 清空插件存储 → 撤销持久化审批
    /// 记录 → 清理持久化激活状态与限频簿记。
    pub async fn uninstall_plugin(&self, plugin_id: &str) -> crate::Result<()> {
        // 安装目录取自插件自身的 extension_path：内置插件在资源目录、用户插件在
        // app_data_dir/plugins，二者都是「插件自有的安装目录」，删除语义一致
        let plugin_dir = {
            let plugins = self.plugins.read().await;
            let loaded = plugins
                .get(plugin_id)
                .ok_or_else(|| crate::AppError::Plugin(format!("Plugin not found: {}", plugin_id)))?;
            // 规则：仅未启用插件可卸载（Activating 是激活进行中的瞬时态，同样拒绝）
            if matches!(
                loaded.state,
                PluginState::Activated | PluginState::Activating | PluginState::Degraded(_)
            ) {
                return Err(crate::AppError::Plugin(format!(
                    "Plugin is running, deactivate it before uninstalling: {}",
                    plugin_id
                )));
            }
            std::path::PathBuf::from(&loaded.extension_path)
        };

        // 停用（防御性：前置条件已保证未运行，此处只做残留资源清理，幂等）
        self.deactivate_plugin(plugin_id, true).await?;

        // 丢弃私有数据库缓存连接：删除目录前释放文件句柄
        self.wasm_host_ctx.drop_plugin_db(plugin_id).await;

        // 删除插件安装目录（插件文件与私有 plugin.db 同目录）。目录名须与插件 id
        // 一致才删，防止 manifest 的异常 extension_path 指向非插件目录；静态注册
        // 插件（随二进制分发，无独立目录）extension_path 为空 → 跳过
        if plugin_dir.as_os_str().is_empty() {
            tracing::debug!(plugin_id = %plugin_id, "[PluginHost] No plugin dir to remove (static/builtin)");
        } else if plugin_dir.file_name().and_then(|n| n.to_str()) != Some(plugin_id) {
            tracing::warn!(
                plugin_id = %plugin_id,
                path = %plugin_dir.display(),
                "[PluginHost] Skip plugin dir removal: extension path does not match plugin id"
            );
        } else if plugin_dir.exists() {
            std::fs::remove_dir_all(&plugin_dir).map_err(|e| {
                crate::AppError::Plugin(format!("Failed to remove plugin dir '{}': {}", plugin_dir.display(), e))
            })?;
        }

        // 移除 WASM 实例与插件记录
        self.wasm_plugins.write().await.remove(plugin_id);
        self.plugins.write().await.remove(plugin_id);

        // 顺带清理用户插件目录下同 id 的孤儿残留（无 plugin.json 的数据目录）：
        // 这类目录不参与加载、UI 不可见，但会卡住同 id 重装（install 的磁盘查重
        // 会误判为已安装）。含 plugin.json 时视为另一来源的有效安装，保留不删。
        let orphan_dir = self.user_plugins_dir.join(plugin_id);
        if orphan_dir != plugin_dir && orphan_dir.exists() && !orphan_dir.join(PLUGIN_MANIFEST_FILE).exists() {
            match std::fs::remove_dir_all(&orphan_dir) {
                Ok(()) => {
                    tracing::info!(
                        plugin_id = %plugin_id,
                        dir = %orphan_dir.display(),
                        "[PluginHost] Removed orphan residue dir on uninstall"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        plugin_id = %plugin_id,
                        error = %e,
                        dir = %orphan_dir.display(),
                        "[PluginHost] Failed to remove orphan residue dir on uninstall"
                    );
                }
            }
        }

        // 清理插件存储（插件私有数据 + fs 授权 / 预授权记录）
        if let Err(e) = self.storage.clear_all(plugin_id).await {
            tracing::warn!(plugin_id = %plugin_id, error = %e, "Failed to clear plugin storage on uninstall");
        }
        // 撤销持久化审批记录：审批 map 位于 `__system__` 空间，clear_all 清不到
        if let Err(e) = crate::plugin::security::approval::PluginApprovalStore::new(self.storage.clone())
            .revoke(plugin_id)
            .await
        {
            tracing::warn!(
                plugin_id = %plugin_id,
                error = %e,
                "Failed to revoke plugin approval on uninstall"
            );
        }
        self.persist_activation_state().await;

        // 清理限频簿记：避免重装同 id 插件沿用旧记录
        self.wasm_reload_throttle
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(plugin_id);
        self.runtime_error_notify_throttle
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(plugin_id);

        tracing::info!(plugin_id = %plugin_id, "[PluginHost] Plugin uninstalled");
        Ok(())
    }
}
