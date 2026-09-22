//! `boot` — PluginHost 职责面拆分（P2）
//!
//! 自 `host.rs` 主 impl 拆出；经 `use super::*` 继承 host 模块的
//! 全部导入与常量，方法与字段可见性规则与内联时一致。

use super::*;
impl PluginHost {
    /// 通知所有已激活的 Rust 插件应用启动完成
    pub async fn notify_startup(&self) {
        // 静态注册插件
        let static_plugins: Vec<&'static bedcode_plugin_api::BedcodePluginEntry> =
            inventory::iter::<bedcode_plugin_api::BedcodePluginEntry>
                .into_iter()
                .collect();
        for entry in &static_plugins {
            if self.is_activated(entry.id).await {
                tracing::debug!(plugin_id = %entry.id, "Notifying plugin on_startup");
                let result = tokio::time::timeout(
                    std::time::Duration::from_secs(PLUGIN_CALLBACK_TIMEOUT_SECS),
                    (entry.on_startup)(),
                )
                .await;
                match result {
                    Err(_) => tracing::error!(plugin_id = %entry.id, "Plugin on_startup timed out"),
                    Ok(Ok(())) => {}
                    // v8 契约：启动初始化失败如实记录（静态插件无 Degraded 态，
                    // 仅日志可观测；builtin 常驻语义见 ticket 03）
                    Ok(Err(e)) => tracing::error!(plugin_id = %entry.id, error = %e, "Plugin on_startup failed"),
                }
            }
        }

        // WASM 插件的 on_startup 已在 activate_plugin() 中自动调用，此处不再重复

        // TS-only 插件：通过 Tauri 事件通知
        // 无头/测试上下文无 AppContext：降级为纯日志跳过（与统一异常通道同策略），
        // 不影响上方静态插件的回调分发
        if let Some(ctx) = crate::system::app_context::AppContext::try_global() {
            if let Some(handle) = ctx.app_handle() {
                let _ = handle.emit(event::LIFECYCLE_STARTUP, serde_json::json!({}));
            }
        }

        tracing::info!("PluginHost notify_startup completed");
    }

    /// 通知所有已激活的插件应用即将关闭

    /// 通知所有已激活的插件应用即将关闭
    pub async fn notify_shutdown(&self) {
        // 静态注册插件
        let static_plugins: Vec<&'static bedcode_plugin_api::BedcodePluginEntry> =
            inventory::iter::<bedcode_plugin_api::BedcodePluginEntry>
                .into_iter()
                .collect();
        for entry in &static_plugins {
            if self.is_activated(entry.id).await {
                tracing::debug!(plugin_id = %entry.id, "Notifying plugin on_shutdown");
                let result = tokio::time::timeout(
                    std::time::Duration::from_secs(PLUGIN_CALLBACK_TIMEOUT_SECS),
                    (entry.on_shutdown)(),
                )
                .await;
                match result {
                    Err(_) => tracing::error!(plugin_id = %entry.id, "Plugin on_shutdown timed out"),
                    Ok(Ok(())) => {}
                    // 清理失败仅记录：停用流程继续，不影响状态机
                    Ok(Err(e)) => tracing::error!(plugin_id = %entry.id, error = %e, "Plugin on_shutdown failed"),
                }
            }
        }

        // WASM 插件的 on_shutdown 已在 deactivate_plugin() 中自动调用，此处不再重复

        // TS-only 插件：通过 Tauri 事件通知（无头/测试上下文降级跳过，同 notify_startup）
        if let Some(ctx) = crate::system::app_context::AppContext::try_global() {
            if let Some(handle) = ctx.app_handle() {
                let _ = handle.emit(event::LIFECYCLE_SHUTDOWN, serde_json::json!({}));
            }
        }

        tracing::info!("PluginHost notify_shutdown completed");
    }

    /// 停用所有已激活的插件（应用关闭流程）

    /// 系统组件优先激活（core-plugin-manager，内置、默认启用、只停不删）
    ///
    /// 在持久化状态自动激活之前执行：系统组件激活时将其能力导出注册进
    /// 能力注册表，后续应用插件激活的依赖检查才能命中。激活顺序按插件 ID
    /// 排序（确定性）；单个失败不阻断其余（失败组件落 Error 态，其能力
    /// 缺失由消费方激活时的依赖检查如实报错）。
    pub(crate) async fn activate_system_components(&self) {
        let mut ids: Vec<String> = {
            let plugins = self.plugins.read().await;
            plugins
                .values()
                .filter(|p| p.manifest.kind == PluginKind::System && p.source != PluginSource::StaticRegistry)
                .map(|p| p.manifest.id.clone())
                .collect()
        };
        ids.sort();
        if ids.is_empty() {
            return;
        }
        tracing::info!(
            "[PluginHost] Activating {} system component(s) before application plugins",
            ids.len()
        );
        for id in ids {
            if let Err(e) = self.activate_plugin(&id, false).await {
                tracing::error!(
                    plugin_id = %id,
                    error = %e,
                    "[PluginHost] 系统组件激活失败（能力缺失将由消费方依赖检查报错）"
                );
            }
        }
    }

    /// 根据持久化状态自动激活之前已激活的插件

    /// 存量兼容（ADR 0020 迁移）：升级前已启用的用户插件首启自动批准一次
    ///
    /// 审批门禁上线前，用户安装的插件按 manifest 全量授权即可运行；直接按新门禁拒绝
    /// 会把用户现有功能打死（票 03 裁决 1）。因此对「持久化状态为已启用 且 尚无批准
    /// 记录」的用户安装插件补一条批准记录并 `warn` 留痕——用户仍可在插件详情页看到
    /// 完整权限清单。从未启用过的用户插件不在此列，照常走人工审批。
    ///
    /// 不做「权限清单变更时重弹」——本轮不加（裁决 1，留作后续可选项）。
    async fn auto_approve_legacy_user_plugin(&self, plugin_id: &str) {
        use crate::plugin::security::approval;

        let (is_user_installed, extension_path, version, requested) = {
            let plugins = self.plugins.read().await;
            match plugins.get(plugin_id) {
                Some(loaded) => (
                    loaded.source == PluginSource::UserInstalled,
                    loaded.extension_path.clone(),
                    loaded.manifest.version.clone(),
                    loaded.manifest.permissions.clone(),
                ),
                // 列表对账已过滤过期 id，此处兜底静默跳过
                None => return,
            }
        };
        if !is_user_installed || extension_path.is_empty() {
            return;
        }

        let store = approval::PluginApprovalStore::new(self.storage.clone());
        match store.get(plugin_id).await {
            // 已有批准记录（含哈希不匹配的失效记录）：交给审批门禁按哈希裁决，不越权补批
            Ok(Some(_)) => return,
            Ok(None) => {}
            Err(e) => {
                tracing::warn!(
                    plugin_id = %plugin_id,
                    error = %e,
                    "[PluginHost] 读取审批记录失败，跳过存量用户插件自动批准"
                );
                return;
            }
        }

        let approved = approval::known_permissions(&requested);
        let dir = PathBuf::from(&extension_path);
        let content_hash = match tokio::task::spawn_blocking(move || approval::compute_dir_hash(&dir)).await {
            Ok(Ok(hash)) => hash,
            Ok(Err(e)) => {
                tracing::warn!(
                    plugin_id = %plugin_id,
                    error = %e,
                    "[PluginHost] 计算插件目录哈希失败，跳过存量用户插件自动批准"
                );
                return;
            }
            Err(e) => {
                tracing::warn!(
                    plugin_id = %plugin_id,
                    error = %e,
                    "[PluginHost] 哈希任务失败，跳过存量用户插件自动批准"
                );
                return;
            }
        };

        match store.approve(plugin_id, &approved, &content_hash, &version).await {
            Ok(()) => tracing::warn!(
                plugin_id = %plugin_id,
                permission_count = approved.len(),
                "[PluginHost] 存量用户安装插件首启自动批准一次（审批门禁上线前的遗留启用状态）"
            ),
            Err(e) => tracing::warn!(
                plugin_id = %plugin_id,
                error = %e,
                "[PluginHost] 存量用户插件自动批准写入失败"
            ),
        }
    }

    /// 根据持久化状态自动激活之前已激活的插件
    pub(crate) async fn auto_activate_from_persisted_state(&self) {
        let activated_map = match self.storage.load_activated_plugins().await {
            Ok(map) => {
                tracing::info!(
                    "[PluginHost] Loaded persisted activation state: {} entry/entries",
                    map.len()
                );
                for (id, active) in &map {
                    tracing::debug!(plugin_id = %id, persist = active, "[PluginHost]   Persisted");
                }
                map
            }
            Err(e) => {
                tracing::warn!(
                    "[PluginHost] Failed to load persisted activation state, skipping auto-activation: {}",
                    e
                );
                return;
            }
        };

        if activated_map.is_empty() {
            tracing::info!("[PluginHost] No persisted activation state, skipping auto-activation");
            return;
        }

        let to_activate: Vec<String> = {
            let plugins = self.plugins.read().await;
            activated_map
                .iter()
                .filter(|(id, &is_active)| {
                    if !is_active {
                        return false;
                    }
                    plugins
                        .get(*id)
                        .map(|p| p.source != PluginSource::StaticRegistry)
                        .unwrap_or(false)
                })
                .map(|(id, _)| id.clone())
                .collect()
        };

        tracing::info!(
            "[PluginHost] Auto-activating {} plugin(s) from persisted state",
            to_activate.len()
        );

        for plugin_id in &to_activate {
            tracing::info!(plugin_id = %plugin_id, "[PluginHost] Auto-activating plugin");
            // 存量兼容（ADR 0020）：审批门禁上线前就已启用的用户插件，首启补一次自动批准
            self.auto_approve_legacy_user_plugin(plugin_id).await;
            if let Err(e) = self.activate_plugin(plugin_id, false).await {
                tracing::error!(plugin_id = %plugin_id, error = %e, "[PluginHost] Failed to auto-activate plugin");
            }
        }

        // 清理已不存在的插件 ID
        let current_ids: HashSet<String> = self.plugins.read().await.keys().cloned().collect();
        let original_len = activated_map.len();
        let mut cleaned_map = activated_map;
        cleaned_map.retain(|id, _| current_ids.contains(id));
        if cleaned_map.len() != original_len {
            tracing::info!(
                "[PluginHost] Cleaning {} stale plugin ID(s) from persisted state",
                original_len - cleaned_map.len()
            );
            if let Err(e) = self.storage.save_activated_plugins(&cleaned_map).await {
                tracing::warn!("[PluginHost] Failed to clean up stale activation entries: {}", e);
            }
        }

        if !to_activate.is_empty() {
            tracing::info!(
                "[PluginHost] Auto-activated {} plugin(s) from persisted state",
                to_activate.len()
            );
        }
    }
}
