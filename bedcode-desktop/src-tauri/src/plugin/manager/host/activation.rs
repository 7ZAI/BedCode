//! `activation` — PluginHost 职责面拆分（P2）
//!
//! 自 `host.rs` 主 impl 拆出；经 `use super::*` 继承 host 模块的
//! 全部导入与常量，方法与字段可见性规则与内联时一致。

use super::*;
impl PluginHost {
    /// 检查插件是否处于激活状态（用于 API 调用的调用者身份校验）
    pub async fn is_activated(&self, plugin_id: &str) -> bool {
        let plugins = self.plugins.read().await;
        let result = plugins
            .get(plugin_id)
            .map(|p| matches!(p.state, PluginState::Activated))
            .unwrap_or(false);
        // 高频校验路径（每插件 API 调用都会经过），仅 trace 级别可见，避免刷屏
        tracing::trace!(plugin_id = %plugin_id, activated = result, "[PluginHost] is_activated");
        result
    }

    /// 停用所有已激活的插件（应用关闭流程）
    pub async fn deactivate_all(&self) -> crate::Result<()> {
        // 置关闭标志：deactivate 内的卸载动作（CLI 清理等）跳过，
        // 保留随包产物供下次启动重新激活（幂等安装）
        self.shutting_down.store(true, std::sync::atomic::Ordering::SeqCst);

        let plugin_ids: Vec<String> = {
            let plugins = self.plugins.read().await;
            plugins
                .values()
                .filter(|p| matches!(p.state, PluginState::Activated))
                .map(|p| p.manifest.id.clone())
                .collect()
        };

        for id in plugin_ids {
            if let Err(e) = self.deactivate_plugin(&id, false).await {
                tracing::error!(plugin_id = %id, error = %e, "Failed to deactivate plugin during shutdown");
            }
        }

        tracing::info!("PluginHost deactivate_all completed");
        Ok(())
    }

    /// 激活插件
    ///
    /// 预授权(启用前置):收集插件需授权路径 → 调 `fs_auth::check_batch`
    /// 单次合并弹窗。失败直接 `mark_error` + 返回 `AppError::Plugin`,
    /// 不进入 `Activating` 中间态。**必须在 `activate_plugin` 阶段1 入口
    /// (置 Activating 之前)调用,持有 plugins 锁时禁止调用**(check_batch
    /// 会发事件、可能回调宿主,持锁会死锁)。
    ///
    /// 路径来源:已注册的 `PreauthProvider` 优先;否则从 `PluginStorage`
    /// `preauth_paths` 数组读(file-transfer mount-local 同步写入);
    /// 另并入 manifest `wasiPreopenDirs` 展开后的声明目录(如 ai-chatbox
    /// 数据目录)——插件 activate 内 fs_request_auth 的弹窗晚于前端 loading
    /// 遮罩,声明目录必须提前到本阶段统一弹窗。
    /// 路径为空 → 直接放行(启用先行:file-transfer 首次启用/全部目录移除后
    /// 均可空目录激活,共享目录配置由插件设置面板引导;硬拒绝会造成
    /// 「配置需激活 → 激活需先配置」死锁)。

    /// - 静态注册插件：仅标记状态
    /// - WASM 插件：调用 __bedcode_activate 导出函数
    /// - TS-only 插件：前端模块加载在 PluginLoader 中完成
    pub async fn activate_plugin(&self, plugin_id: &str, persist: bool) -> crate::Result<()> {
        // 外壳：激活成功后接线 peer-net 节点生命周期（file-transfer 是节点唯一
        // 消费方，节点随插件启停——旧 setup 无条件自启已退役，停用即服务下线）
        let result = self.activate_plugin_inner(plugin_id, persist).await;
        if result.is_ok() && plugin_id == crate::peer_net::FILE_TRANSFER_PLUGIN_ID {
            match crate::system::app_context::AppContext::try_global() {
                Some(ctx) => {
                    if let Some(app) = ctx.app_handle() {
                        if let Err(e) = crate::peer_net::ensure_node_started(app).await {
                            tracing::error!(
                                plugin_id = %plugin_id,
                                error = %e,
                                "peer-net node start on plugin activation failed"
                            );
                        }
                    }
                }
                // boot 装配期 AppContext 未注册：静默跳过，由 boot 末尾的状态
                // 对账（sync_node_with_plugin_state）兜底
                None => {
                    tracing::debug!("peer-net node start skipped: AppContext not ready (boot assembly)");
                }
            }
        }
        result
    }

    pub(crate) async fn activate_plugin_inner(&self, plugin_id: &str, persist: bool) -> crate::Result<()> {
        tracing::info!(plugin_id = %plugin_id, persist, "[PluginHost] activate_plugin");

        // 阶段 0(无锁):预授权 — 必须在持 plugins 锁之前完成,失败直接
        // 返回 Err,前端 catch 后回退 toggle。loading 遮罩由前端 toggle
        // 推迟到此调用之后才显示,确保授权弹窗与 loading 不会同时出现
        self.preauthorize_plugin(plugin_id).await?;

        // 阶段 1（短写锁）：读取状态与 manifest 字段、重新授权后立即释放锁。
        // 禁止持 plugins 锁执行 WASM activate：activate 内可能回调宿主
        // （如 scheduler 的 cli_install 经 services.install_cli 读 plugins map），
        // 持写锁回调会死锁（锁约定见 PluginManager::activate，双侧一致）
        struct ActivatePlan {
            source: PluginSource,
            api: Vec<String>,
            subscribes: Vec<String>,
            declared_preopen_dirs: Vec<String>,
            kind: PluginKind,
            dependencies: Vec<String>,
        }
        let plan = {
            let mut plugins = self.plugins.write().await;
            let loaded = plugins.get_mut(plugin_id).ok_or_else(|| {
                tracing::error!(plugin_id = %plugin_id, "[PluginHost] activate_plugin: plugin not found in plugins map");
                crate::AppError::Plugin(format!("Plugin not found: {}", plugin_id))
            })?;

            match &loaded.state {
                PluginState::Activated => {
                    tracing::debug!(plugin_id = %plugin_id, "[PluginHost] Plugin already activated, skipping");
                    return Ok(());
                }
                PluginState::Error(e) => {
                    tracing::warn!(
                        plugin_id = %plugin_id,
                        error = %e,
                        "[PluginHost] Plugin in error state, attempting re-activation"
                    );
                }
                // Degraded 重试激活：启动初始化上次失败，本次重新走完整流程
                PluginState::Degraded(e) => {
                    tracing::info!(
                        plugin_id = %plugin_id,
                        error = %e,
                        "[PluginHost] Plugin in degraded state, attempting re-activation"
                    );
                }
                _ => {
                    tracing::debug!(
                        plugin_id = %plugin_id,
                        state = ?loaded.state,
                        "[PluginHost] Plugin current state, proceeding with activation"
                    );
                }
            }

            // 重新授权：deactivate 会 revoke_all，再次激活时必须重新授予
            let permissions = loaded.manifest.permissions.clone();
            let granted = self.permission.grant_permissions(plugin_id, &permissions);
            // 词汇表外的声明会在授权时被过滤（= 没声明）。票 01 已把生产 manifest 的
            // 装饰词汇清零、票 02 又取消了 storage 的默认授予，因此「声明了却没生效」
            // 必须可见——否则第三方插件作者只能靠运行时 permission denied 反推。
            let dropped: Vec<&String> = permissions.iter().filter(|p| !granted.contains(p.as_str())).collect();
            if !dropped.is_empty() {
                tracing::warn!(
                    plugin_id = %plugin_id,
                    dropped = ?dropped,
                    "manifest 声明的权限不在 SDK 词汇表内，授权时被过滤"
                );
            }
            loaded.granted_permissions = granted;

            // 置中间态后再释放锁执行 WASM activate：列表查询在激活期间
            // 可见 Activating（瞬时态，终态由下方 phase 2/3 写入）
            loaded.state = PluginState::Activating;

            ActivatePlan {
                source: loaded.source.clone(),
                api: loaded.manifest.api.clone(),
                subscribes: loaded.manifest.contributes.subscribes.clone(),
                declared_preopen_dirs: loaded.manifest.wasi_preopen_dirs.clone(),
                kind: loaded.manifest.kind,
                dependencies: loaded.manifest.dependencies.clone(),
            }
        };

        // 阶段 1.5（无锁）：能力依赖装配检查（core-plugin-manager）——manifest
        // `dependencies` 声明的每个能力必须已有提供者（宿主原语或已激活的
        // 系统组件实例）；缺失即激活失败并指明能力名。系统组件先于应用插件
        // 激活（见 PluginHost::new 的 activate_system_components），故此处的
        // 注册表快照对应用插件而言已含全部系统组件提供者。
        if !plan.dependencies.is_empty() {
            let missing = self.wasm_host_ctx.capabilities().missing(&plan.dependencies);
            if !missing.is_empty() {
                let msg = format!(
                    "plugin dependencies not satisfied, missing capabilities: {}",
                    missing.join(", ")
                );
                tracing::error!(
                    plugin_id = %plugin_id,
                    missing = ?missing,
                    "[PluginHost] 能力依赖未装配，激活失败"
                );
                self.mark_error(plugin_id, msg.clone()).await;
                return Err(crate::AppError::Plugin(format!(
                    "Plugin {} activation failed: {}",
                    plugin_id, msg
                )));
            }
        }

        // 阶段 2（无 map 锁）：执行 WASM activate + on_startup
        // 仅持单插件实例锁（避免重入死锁），失败置 Error 状态
        // on_startup 的 guest 自报失败记录于此，phase 3 据此写 Degraded 终态
        let mut startup_failure: Option<String> = None;
        if plan.source == PluginSource::Wasm {
            // WASI 预打开目录漂移检测：声明了预打开目录的插件，激活前先核对当前
            // 实例是否已覆盖「现在已授权」的目录。首次启用时授权经 activate() 内
            // fs_request_auth 弹窗才落库（早于实例化），实例预打开为空；重试激活
            // （停用再启用）时授权已持久化，若实例未覆盖则重建——否则 /data 永远
            // 挂不上，激活自检必失败（Bug B 死循环）。无声明的插件跳过重建（零开销）。
            if !plan.declared_preopen_dirs.is_empty() {
                let resolved = crate::plugin::manager::wasm_runtime::resolve_preopen_dirs(
                    &self.wasm_host_ctx,
                    plugin_id,
                    &plan.declared_preopen_dirs,
                );
                let missing = {
                    let wasm_plugins = self.wasm_plugins.read().await;
                    match wasm_plugins.get(plugin_id).cloned() {
                        Some(inst) => {
                            // 锁序纪律：先释放 map 读锁再锁实例（与 deactivate 一致），
                            // 避免「持 map 锁 + 实例锁」的组合与未来热重载写锁交叉
                            drop(wasm_plugins);
                            let preopened = inst.lock().await.preopened_dirs().to_vec();
                            !resolved.iter().all(|d| preopened.iter().any(|p| p == d))
                        }
                        // 实例缺失：走重建路径补建（与 reload 语义一致）
                        None => true,
                    }
                };
                if missing {
                    tracing::info!(
                        plugin_id = %plugin_id,
                        declared = ?plan.declared_preopen_dirs,
                        resolved = ?resolved,
                        "Rebuilding WASM instance before activation: preopen dirs changed after instance creation"
                    );
                    self.rebuild_wasm_instance(plugin_id).await?;
                }
            }

            let wasm_plugin = {
                let wasm_plugins = self.wasm_plugins.read().await;
                wasm_plugins.get(plugin_id).cloned()
            };
            let Some(wasm_plugin) = wasm_plugin else {
                tracing::error!(plugin_id = %plugin_id, "WASM plugin not found in wasm_plugins map");
                // phase 1 已置 Activating 中间态：失败路径必须落终态，不留悬挂
                self.mark_error(plugin_id, "WASM module not loaded".to_string()).await;
                return Err(crate::AppError::Plugin(format!(
                    "Plugin {} WASM module not loaded",
                    plugin_id
                )));
            };

            // WASI 需要无 handle 线程执行 guest 导出（见 run_guest_call）
            match self.run_guest_call(wasm_plugin.clone(), |p| p.activate()).await {
                Ok(Ok(0)) => {
                    tracing::info!(plugin_id = %plugin_id, "[PluginHost] Plugin activated");
                }
                Ok(Ok(code)) => {
                    tracing::error!(
                        plugin_id = %plugin_id,
                        code = %code,
                        "[PluginHost] Plugin activate() returned error code"
                    );
                    self.mark_error(plugin_id, format!("activate() returned error code {}", code))
                        .await;
                    return Err(crate::AppError::Plugin(format!(
                        "Plugin {} activate() returned error code {}",
                        plugin_id, code
                    )));
                }
                Ok(Err(e)) => {
                    tracing::error!(plugin_id = %plugin_id, error = %e, "[PluginHost] Plugin activate() failed");
                    self.mark_error(plugin_id, format!("activate() failed: {}", e)).await;
                    return Err(crate::AppError::Plugin(format!(
                        "Plugin {} activate() failed: {}",
                        plugin_id, e
                    )));
                }
                Err(panic) => {
                    let msg = crate::plugin::manager::wasm_runtime::panic_payload_to_string(&panic);
                    self.mark_error(plugin_id, format!("activate() panicked: {}", msg))
                        .await;
                    return Err(crate::AppError::Plugin(format!(
                        "Plugin {} activate() panicked: {}",
                        plugin_id, msg
                    )));
                }
            }

            // 激活成功后自动调用 on_startup（启动初始化；结果决定 Activated / Degraded）
            // v8 契约：guest 自报失败不再静默吞掉——Degraded 终态如实反映
            // 「实例可用、扩展点已注册，但启动初始化未完成」
            tracing::info!(plugin_id = %plugin_id, "[PluginHost] Calling on_startup");
            match self.run_guest_call(wasm_plugin, |p| p.on_startup()).await {
                Ok(Ok(Ok(()))) => {
                    tracing::info!(plugin_id = %plugin_id, "[PluginHost] Plugin on_startup completed");
                }
                Ok(Ok(Err(e))) => {
                    tracing::error!(
                        plugin_id = %plugin_id,
                        error = %e,
                        "[PluginHost] Plugin on_startup reported failure"
                    );
                    startup_failure = Some(e);
                }
                Ok(Err(e)) => {
                    // 调用层错误（非 trap）：导出不可达 / 燃料异常等，启动初始化同样未完成
                    tracing::error!(
                        plugin_id = %plugin_id,
                        error = %e,
                        "[PluginHost] Plugin on_startup call failed"
                    );
                    startup_failure = Some(e.to_string());
                }
                Err(panic) => {
                    let msg = crate::plugin::manager::wasm_runtime::panic_payload_to_string(&panic);
                    tracing::error!(
                        plugin_id = %plugin_id,
                        error = %msg,
                        "[PluginHost] Plugin on_startup panicked"
                    );
                    // panic 已污染 Store，后续调用必然失败：按故障态处理（区别于可用的
                    // 降级），恢复依赖既有 trap 自动重载机制
                    self.mark_error(plugin_id, format!("on_startup panicked: {}", msg))
                        .await;
                    return Err(crate::AppError::Plugin(format!(
                        "Plugin {} on_startup panicked: {}",
                        plugin_id, msg
                    )));
                }
            }
        }

        // 阶段 3（短写锁）：写入终态（Activated 或 Degraded），然后释放锁执行订阅登记。
        // Degraded 同样完成订阅/api 登记：WASM 实例本身是活的，
        // 与「启动初始化部分失败」正交
        {
            let mut plugins = self.plugins.write().await;
            if let Some(loaded) = plugins.get_mut(plugin_id) {
                loaded.state = match &startup_failure {
                    Some(reason) => PluginState::Degraded(reason.clone()),
                    None => PluginState::Activated,
                };
                loaded.activated_at = Some(Utc::now());
            }
        }
        // core-monitor：启动初始化部分失败（Degraded 终态）埋点
        if startup_failure.is_some() {
            self.wasm_runtime
                .monitor()
                .plugin(plugin_id)
                .record_lifecycle(crate::plugin::monitor::LifecycleEvent::Degraded);
        }

        // 登记互调 api 清单（ADR-0017）：激活后 `bedcode.api.*` 请求可路由到本插件。
        // 未声明 api 的插件登记空清单，幂等无操作
        self.wasm_host_ctx.api_registry().register(plugin_id, &plan.api);

        // core-plugin-manager：系统组件激活后装配能力注册表——实例化时探测到的
        // 可路由能力导出（host-* 同形接口）注册为系统组件提供者，应用插件的
        // 对应 import 自此经 Linker 路由转发到本组件实例（host-side 转发）
        if plan.kind == PluginKind::System {
            self.register_system_capabilities(plugin_id).await;
        }

        // 注册 manifest 中声明的 topic 订阅
        if !plan.subscribes.is_empty() {
            let plugin_id_owned = plugin_id.to_string();
            for topic in &plan.subscribes {
                self.message_bus.subscribe_wasm(&plugin_id_owned, topic).await;
            }
            tracing::info!(
                plugin_id = %plugin_id_owned,
                topic_count = plan.subscribes.len(),
                "[PluginHost] Plugin subscribed to topic(s): {:?}",
                plan.subscribes
            );
        }

        // 终态日志：成功与降级分别如实呈现（汇总日志在 PluginHost::new 尾部）
        match &startup_failure {
            Some(reason) => tracing::info!(
                plugin_id = %plugin_id,
                persist,
                "[PluginHost] Plugin activated with degradation: {}",
                reason
            ),
            None => {
                tracing::info!(plugin_id = %plugin_id, persist, "[PluginHost] Plugin activated successfully");
            }
        }

        if persist {
            tracing::debug!(plugin_id = %plugin_id, "[PluginHost] Persisting activation state after activating");
            self.persist_activation_state().await;
        }

        Ok(())
    }

    ///
    /// 注册系统组件的能力提供者（core-plugin-manager）
    ///
    /// 读取实例化时探测到的可路由能力导出，逐项注册进能力注册表；
    /// 导出缺失/不可路由的能力跳过并告警（不阻断激活——组件仍可提供
    /// 其余能力，缺失能力由依赖检查在消费方激活时报错）。
    pub(crate) async fn register_system_capabilities(&self, plugin_id: &str) {
        let instance = {
            let wasm_plugins = self.wasm_plugins.read().await;
            wasm_plugins.get(plugin_id).cloned()
        };
        let Some(instance) = instance else {
            tracing::warn!(
                plugin_id = %plugin_id,
                "[PluginHost] 系统组件无 WASM 实例，跳过能力注册（非 WASM 来源？）"
            );
            return;
        };
        let exported = instance.lock().await.exported_capabilities().to_vec();
        if exported.is_empty() {
            tracing::warn!(
                plugin_id = %plugin_id,
                "[PluginHost] 系统组件未导出任何可路由能力接口（manifest type=system 但无能力导出）"
            );
        }
        for capability in exported {
            if let Err(e) =
                self.wasm_host_ctx
                    .capabilities()
                    .register_system_component(&capability, plugin_id, instance.clone())
            {
                tracing::error!(
                    plugin_id = %plugin_id,
                    capability = %capability,
                    error = %e,
                    "[PluginHost] 系统组件能力注册失败"
                );
            }
        }
    }

    pub(crate) fn abort_plugin_timer(&self, plugin_id: &str) {
        let mut timers = self.plugin_timers.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(handle) = timers.remove(plugin_id) {
            handle.abort();
            tracing::info!(plugin_id = %plugin_id, "[PluginHost] Timer aborted");
        }
    }

    pub async fn deactivate_plugin(&self, plugin_id: &str, persist: bool) -> crate::Result<()> {
        // 外壳：停用成功后接线 peer-net 节点生命周期（file-transfer 停用即
        // 服务下线，对端即时感知；幂等）
        let result = self.deactivate_plugin_inner(plugin_id, persist).await;
        if result.is_ok() && plugin_id == crate::peer_net::FILE_TRANSFER_PLUGIN_ID {
            match crate::system::app_context::AppContext::try_global() {
                Some(ctx) => {
                    if let Some(app) = ctx.app_handle() {
                        if let Err(e) = crate::peer_net::stop_node_for_plugin(app).await {
                            tracing::error!(
                                plugin_id = %plugin_id,
                                error = %e,
                                "peer-net node stop on plugin deactivation failed"
                            );
                        }
                    }
                }
                // boot 装配期 AppContext 未注册：由 boot 末尾的状态对账兜底
                None => {
                    tracing::debug!("peer-net node stop skipped: AppContext not ready (boot assembly)");
                }
            }
        }
        result
    }

    pub(crate) async fn deactivate_plugin_inner(&self, plugin_id: &str, persist: bool) -> crate::Result<()> {
        tracing::info!(plugin_id = %plugin_id, persist, "[PluginHost] deactivate_plugin");

        // mDNS 基础能力服务（spec v2 §5.1）：插件停用即回收其全部浏览 + 广播
        // 句柄（host-mdns v2 生命周期随属主；只碰本人，宿主/它插件登记不受影响）
        crate::plugin::manager::wasm_runtime::host_impl::mdns::purge_for_plugin(plugin_id);

        // WS 基础能力服务（ABI v14，spec §2.3）：插件停用即回收其全部出站连接
        // （只碰本人；服务端端点双表回收随票 05 一并接入）
        crate::plugin::manager::wasm_runtime::host_impl::ws::purge_for_plugin(plugin_id);

        // PTY 基础能力服务（ABI v16，spec D2）：插件停用即 kill 并摘除其全部私有
        // PTY，逐条补发 `pty:exit.<owner>`（reason=killed）——孤儿进程不随插件消失
        // 而悬挂。必须在下方 `remove_all_subscriptions` 之前，否则补发的事件无人可投。
        crate::plugin::manager::wasm_runtime::host_impl::pty::purge_for_plugin(plugin_id, &self.message_bus);

        // 并发任务域（ABI v20）：插件停用即 cancel 其全部在册任务 + 清回调队列
        // （只碰本人；运行中单元协作式跑完或超时，未开始单元 skipped）
        crate::plugin::manager::task::purge_for_plugin(plugin_id);

        // WASM 插件：调用 on_shutdown + __bedcode_deactivate
        {
            let plugins = self.plugins.read().await;
            if let Some(loaded) = plugins.get(plugin_id) {
                if loaded.source == PluginSource::Wasm {
                    let wasm_plugins = self.wasm_plugins.read().await;
                    if let Some(wasm_plugin) = wasm_plugins.get(plugin_id).cloned() {
                        drop(wasm_plugins);
                        // 停用前先调用 on_shutdown（WASI 需无 handle 线程，见 run_guest_call）
                        tracing::info!(plugin_id = %plugin_id, "[PluginHost] Calling on_shutdown");
                        // v8 契约：guest 自报的清理失败单独记录，不与调用故障混淆；
                        // 停用流程继续，不影响状态机
                        match self.run_guest_call(wasm_plugin.clone(), |p| p.on_shutdown()).await {
                            Ok(Ok(Ok(()))) => {
                                tracing::info!(plugin_id = %plugin_id, "[PluginHost] Plugin on_shutdown completed");
                            }
                            Ok(Ok(Err(e))) => {
                                tracing::warn!(
                                    plugin_id = %plugin_id,
                                    error = %e,
                                    "[PluginHost] Plugin on_shutdown reported failure"
                                );
                            }
                            Ok(Err(e)) => {
                                tracing::warn!(
                                    plugin_id = %plugin_id,
                                    error = %e,
                                    "[PluginHost] Plugin on_shutdown call failed"
                                );
                            }
                            Err(panic) => {
                                let msg = crate::plugin::manager::wasm_runtime::panic_payload_to_string(&panic);
                                tracing::warn!(
                                    plugin_id = %plugin_id,
                                    error = %msg,
                                    "[PluginHost] Plugin on_shutdown panicked"
                                );
                            }
                        }

                        match self.run_guest_call(wasm_plugin, |p| p.deactivate()).await {
                            Ok(Ok(0)) => {
                                tracing::info!(plugin_id = %plugin_id, "[PluginHost] Plugin deactivated");
                            }
                            Ok(Ok(code)) => {
                                tracing::warn!(
                                    plugin_id = %plugin_id,
                                    code = %code,
                                    "[PluginHost] Plugin deactivate() returned error code"
                                );
                            }
                            Ok(Err(e)) => {
                                tracing::error!(plugin_id = %plugin_id, error = %e, "[PluginHost] Plugin deactivate() failed");
                            }
                            Err(panic) => {
                                let msg = crate::plugin::manager::wasm_runtime::panic_payload_to_string(&panic);
                                tracing::error!(plugin_id = %plugin_id, error = %msg, "[PluginHost] Plugin deactivate() panicked");
                            }
                        }
                    }
                }
            }
        }

        // 统一清理：取消注册和撤销权限
        self.registry.unregister_plugin(plugin_id).await;
        self.permission.revoke_all(plugin_id);
        // 注销互调 api 清单（ADR-0017）：停用后目标调用被门禁拒绝
        self.wasm_host_ctx.api_registry().unregister(plugin_id);
        // core-plugin-manager：系统组件停用即撤销其能力提供，能力回落宿主原语
        // （二选一装配：注册表对消费方恒可用；条件回落防误撤重建后的新注册）
        self.wasm_host_ctx.capabilities().revert_all_from(plugin_id);

        // 中止插件定时器（若有）：停用后不再到点回调
        self.abort_plugin_timer(plugin_id);

        // 清理消息总线订阅
        self.message_bus.remove_all_subscriptions(plugin_id).await;

        // 移除该插件的会话生命周期监听器与输入监听器
        {
            let session_manager = self.wasm_host_ctx().session_manager_arc();
            session_manager.remove_lifecycle_listener(plugin_id).await;
            session_manager.remove_input_listener(plugin_id).await;
        }

        let mut plugins = self.plugins.write().await;
        let loaded = plugins
            .get_mut(plugin_id)
            .ok_or_else(|| crate::AppError::Plugin(format!("Plugin not found: {}", plugin_id)))?;

        loaded.state = PluginState::Deactivated;
        loaded.activated_at = None;
        tracing::info!(plugin_id = %plugin_id, persist, "[PluginHost] Plugin deactivated successfully");

        // 释放写锁后再持久化
        drop(plugins);

        if persist {
            tracing::debug!(plugin_id = %plugin_id, "[PluginHost] Persisting activation state after deactivating");
            self.persist_activation_state().await;
        }

        Ok(())
    }

    /// 热重载 WASM 插件（开发模式）
    ///
    /// 执行完整的卸载-重载-激活循环：
    /// 1. 停用插件
    /// 2. 重新编译并实例化 WASM 模块
    /// 3. 重新激活插件

    /// 获取当前所有非 StaticRegistry 插件的激活状态映射
    ///
    /// 持久化语义为用户意图：Activated 与 Degraded 均记 true——降级是健康
    /// 快照而非启停意图，下次启动仍按 persisted=true 重试激活；
    /// Error/Deactivated 等记 false（与既有行为一致）
    pub async fn get_activated_state(&self) -> HashMap<String, bool> {
        let plugins = self.plugins.read().await;
        let mut map = HashMap::new();
        for (id, loaded) in plugins.iter() {
            if loaded.source == PluginSource::StaticRegistry {
                continue;
            }
            // core-plugin-manager：系统组件默认启用、启动时无条件激活
            // （activate_system_components），其启停不持久化——持久化真源是
            // 「内置」而非用户状态，停用仅对当前会话生效
            if loaded.manifest.kind == PluginKind::System {
                continue;
            }
            let is_active = matches!(loaded.state, PluginState::Activated | PluginState::Degraded(_));
            map.insert(id.clone(), is_active);
        }
        tracing::debug!(
            "[PluginHost] get_activated_state() returning {} entry/entries",
            map.len()
        );
        map
    }

    /// 持久化当前激活状态到 SQLite

    /// 持久化当前激活状态到 SQLite
    pub(crate) async fn persist_activation_state(&self) {
        let activated_map = self.get_activated_state().await;
        tracing::debug!(
            "[PluginHost] Persisting activation state: {} plugin(s)",
            activated_map.len()
        );
        for (id, active) in &activated_map {
            tracing::debug!(plugin_id = %id, persist = active, "[PluginHost]   Persist");
        }
        if let Err(e) = self.storage.save_activated_plugins(&activated_map).await {
            tracing::error!("[PluginHost] Failed to persist plugin activation state: {}", e);
        }
    }

    /// 系统组件优先激活（core-plugin-manager，内置、默认启用、只停不删）
    ///
    /// 在持久化状态自动激活之前执行：系统组件激活时将其能力导出注册进
    /// 能力注册表，后续应用插件激活的依赖检查才能命中。激活顺序按插件 ID
    /// 排序（确定性）；单个失败不阻断其余（失败组件落 Error 态，其能力
    /// 缺失由消费方激活时的依赖检查如实报错）。

    /// 判断插件是否应该按需激活
    pub async fn should_lazy_activate(&self, plugin_id: &str) -> bool {
        let plugins = self.plugins.read().await;
        if let Some(loaded) = plugins.get(plugin_id) {
            if loaded.source == PluginSource::StaticRegistry {
                return false;
            }
            if !matches!(loaded.state, PluginState::Loaded) {
                return false;
            }
            let c = &loaded.manifest.contributes;
            !c.commands.is_empty() || c.terminal.is_some() || !c.views.is_empty()
        } else {
            false
        }
    }
}
