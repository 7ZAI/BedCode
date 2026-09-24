//! Plugin API Bridge
//!
//! Tauri commands — 前端 PluginContext 的每个 API 调用通过 Tauri invoke 到达此桥接层
//! Rust 端做权限校验后执行操作

use super::PluginHost;
use crate::wasm_core::manager::types::DesktopPluginInfo;
use crate::wasm_core::security::fs_auth::FsAuthChecker;
use std::sync::Arc;
use tauri::State;

// ==================== 前端通道身份（审计票 06 / P0-5） ====================
//
// 本桥的插件面命令**不再信任参数里的 plugin_id**：调用方必须带 `credential`，
// 身份由宿主解析（宿主面 loader 密钥 → 可操作任意目标；插件面令牌 → 目标必须是自己）。
// 机制与威胁模型见 `plugin/security/frontend_channel.rs`。

/// 宿主前端 bootstrap：取得本次页面加载的 loader 会话密钥（**首个调用者生效**）
///
/// 宿主前端在导入任何插件模块之前调用（`pluginLoader.loadAll()` 首行），插件代码开始运行时
/// 密钥已被占位。页面加载时由 `on_page_load` 钩子重置，dev 下刷新可重新取得。
#[tauri::command]
pub async fn plugin_frontend_loader_session(plugin_host: State<'_, Arc<PluginHost>>) -> crate::Result<String> {
    let session = plugin_host.frontend_channel().issue_loader_session()?;
    tracing::info!("[API] plugin_frontend_loader_session: 宿主面凭证已签发");
    Ok(session)
}

/// 插件前端：为指定插件签发通道令牌（需 loader 会话密钥 + 插件处于运行态）
///
/// 令牌随停用回收；同一插件重新签发会作废旧令牌。
#[tauri::command]
pub async fn plugin_channel_token(
    plugin_id: String,
    loader_session: String,
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<String> {
    if !plugin_host.frontend_channel().verify_loader_session(&loader_session) {
        tracing::warn!(plugin_id = %plugin_id, "[API] plugin_channel_token: loader 会话凭证无效");
        return Err(crate::AppError::Plugin(
            "invalid frontend loader session credential".to_string(),
        ));
    }
    // 运行态（Activated / Degraded）才签发：未激活插件没有 granted 集，令牌无意义
    if !plugin_host.is_running(&plugin_id).await {
        tracing::warn!(plugin_id = %plugin_id, "[API] plugin_channel_token: 插件未运行，拒绝签发");
        return Err(crate::AppError::Plugin(format!(
            "Plugin {} is not running, channel token refused",
            plugin_id
        )));
    }
    let token = plugin_host
        .frontend_channel()
        .issue_token(&loader_session, &plugin_id)?;
    tracing::info!(plugin_id = %plugin_id, "[API] plugin_channel_token: 插件面令牌已签发");
    Ok(token)
}

/// 插件面命令的统一身份校验（fail-closed 语义与裁决规则见 `frontend_channel::authorize`）
fn authorize_plugin_call(plugin_host: &PluginHost, plugin_id: &str, credential: &str) -> crate::Result<()> {
    plugin_host.frontend_channel().authorize(plugin_id, credential)
}

// ==================== Plugin Lifecycle ====================

/// 获取所有已加载插件列表
#[tauri::command]
pub async fn plugin_list_loaded(plugin_host: State<'_, Arc<PluginHost>>) -> crate::Result<Vec<DesktopPluginInfo>> {
    tracing::debug!("[API] plugin_list_loaded called");
    let result = plugin_host.list_plugins().await;
    tracing::debug!("[API] plugin_list_loaded returning {} plugin(s)", result.len());
    Ok(result)
}

/// 获取单个插件信息
#[tauri::command]
pub async fn plugin_get_info(
    plugin_id: String,
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<Option<DesktopPluginInfo>> {
    tracing::debug!(plugin_id = %plugin_id, "[API] plugin_get_info");
    Ok(plugin_host.get_plugin(&plugin_id).await)
}

/// 预授权（启用前置，独立命令供前端先行调用）
///
/// 前端 toggle 时序：先调本命令（此阶段**不显示** loading 遮罩，授权弹窗
/// 可正常交互）→ 通过后再显示遮罩调 `plugin_activate`；拒绝则直接失败，
/// 不进入激活流程。`plugin_activate` 内部的 preauthorize 保留为兜底
/// （启动 auto-activate 无头场景 + 已授权路径短路无二次弹窗）。
#[tauri::command]
pub async fn plugin_preauthorize(plugin_id: String, plugin_host: State<'_, Arc<PluginHost>>) -> crate::Result<()> {
    tracing::info!(plugin_id = %plugin_id, "[API] plugin_preauthorize");
    let result = plugin_host.preauthorize_plugin(&plugin_id).await;
    if let Err(ref e) = result {
        tracing::error!(plugin_id = %plugin_id, error = %e, "[API] plugin_preauthorize failed");
    }
    result
}

/// 激活插件（用户操作，持久化状态）
#[tauri::command]
pub async fn plugin_activate(plugin_id: String, plugin_host: State<'_, Arc<PluginHost>>) -> crate::Result<()> {
    tracing::info!(plugin_id = %plugin_id, "[API] plugin_activate");
    let result = plugin_host.activate_plugin(&plugin_id, true).await;
    if let Err(ref e) = result {
        tracing::error!(plugin_id = %plugin_id, error = %e, "[API] plugin_activate failed");
    }
    result
}

/// 停用插件（用户操作，持久化状态）
#[tauri::command]
pub async fn plugin_deactivate(plugin_id: String, plugin_host: State<'_, Arc<PluginHost>>) -> crate::Result<()> {
    tracing::info!(plugin_id = %plugin_id, "[API] plugin_deactivate");
    let result = plugin_host.deactivate_plugin(&plugin_id, true).await;
    if let Err(ref e) = result {
        tracing::error!(plugin_id = %plugin_id, error = %e, "[API] plugin_deactivate failed");
    }
    result
}

/// 批准用户安装插件的权限清单（ADR 0020 审批门禁的放行操作）
///
/// 生效权限 = 批准 ∩ manifest 请求；批准与插件目录内容哈希绑定，
/// 目录内容在批准后变化 → 批准自动撤销并回到 NeedsApproval。
#[tauri::command]
pub async fn plugin_approve(plugin_id: String, plugin_host: State<'_, Arc<PluginHost>>) -> crate::Result<Vec<String>> {
    tracing::info!(plugin_id = %plugin_id, "[API] plugin_approve");
    let result = plugin_host.approve_plugin(&plugin_id).await;
    if let Err(ref e) = result {
        tracing::error!(plugin_id = %plugin_id, error = %e, "[API] plugin_approve failed");
    }
    result
}

/// 标记插件错误
#[tauri::command]
pub async fn plugin_mark_error(
    plugin_id: String,
    error: String,
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<()> {
    plugin_host.mark_error(&plugin_id, error).await;
    Ok(())
}

/// 获取插件激活状态映射（plugin_id → is_activated）
#[tauri::command]
pub async fn plugin_get_activated_state(
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<std::collections::HashMap<String, bool>> {
    Ok(plugin_host.get_activated_state().await)
}

// ==================== Frontend Load Diagnostics ====================

/// 插件前端模块加载诊断上报（宿主内部诊断通道，spec §3.7 / issue 04）
///
/// 前端 PluginLoader 在 TS 模块导入/激活成败时调用，把结果写入 tracing 落盘日志
/// （runtime.*.log），使日志能看到插件加载「后端半程之外」的前端半程。仅写日志：
/// 不改插件状态机；也不做存在性/权限校验——诊断通道自身失败会丢日志行，
/// 门禁拒绝只会再丢一次。
///
/// 注意：这是宿主自身的启动期诊断命令，不是插件协议，不受「不加特殊上报 ABI」约束。
#[tauri::command]
pub async fn plugin_frontend_load_report(
    plugin_id: String,
    stage: String,
    ok: bool,
    detail: Option<String>,
) -> crate::Result<()> {
    if ok {
        tracing::info!(
            plugin_id = %plugin_id,
            stage = %stage,
            "[PluginLoader] frontend module load ok"
        );
    } else {
        tracing::error!(
            plugin_id = %plugin_id,
            stage = %stage,
            detail = detail.as_deref().unwrap_or("(no detail)"),
            "[PluginLoader] frontend module load FAILED"
        );
    }
    Ok(())
}

// ==================== Plugin Storage ====================

/// 插件存储：获取值
///
/// 身份：`credential` 决定调用方（宿主面 / 该插件自己）；`plugin_id` 只作目标。
/// 校验通过后再查激活态与 `storage` 权限（查的是**目标插件**的 granted 集）。
#[tauri::command]
pub async fn plugin_storage_get(
    plugin_id: String,
    key: String,
    credential: String,
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<Option<serde_json::Value>> {
    authorize_plugin_call(&plugin_host, &plugin_id, &credential)?;
    if !plugin_host.is_activated(&plugin_id).await {
        return Err(crate::AppError::Plugin(format!(
            "Plugin {} is not activated",
            plugin_id
        )));
    }
    if !plugin_host.permission().check(&plugin_id, "storage") {
        return Err(crate::AppError::Plugin(format!(
            "Plugin {} has no storage permission",
            plugin_id
        )));
    }
    plugin_host.storage().get(&plugin_id, &key).await
}

/// 插件存储：设置值
#[tauri::command]
pub async fn plugin_storage_set(
    plugin_id: String,
    key: String,
    value: serde_json::Value,
    credential: String,
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<()> {
    authorize_plugin_call(&plugin_host, &plugin_id, &credential)?;
    if !plugin_host.is_activated(&plugin_id).await {
        return Err(crate::AppError::Plugin(format!(
            "Plugin {} is not activated",
            plugin_id
        )));
    }
    if !plugin_host.permission().check(&plugin_id, "storage") {
        return Err(crate::AppError::Plugin(format!(
            "Plugin {} has no storage permission",
            plugin_id
        )));
    }
    plugin_host.storage().set(&plugin_id, &key, value).await
}

/// 插件存储：删除值
#[tauri::command]
pub async fn plugin_storage_delete(
    plugin_id: String,
    key: String,
    credential: String,
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<()> {
    authorize_plugin_call(&plugin_host, &plugin_id, &credential)?;
    if !plugin_host.is_activated(&plugin_id).await {
        return Err(crate::AppError::Plugin(format!(
            "Plugin {} is not activated",
            plugin_id
        )));
    }
    if !plugin_host.permission().check(&plugin_id, "storage") {
        return Err(crate::AppError::Plugin(format!(
            "Plugin {} has no storage permission",
            plugin_id
        )));
    }
    plugin_host.storage().delete(&plugin_id, &key).await
}

// ==================== Plugin Terminal（票 08 已注销） ====================

// `plugin_terminal_send_input` 随票 08 删除：它是「宿主替插件把终端输入导流到 PTY」
// 的桥，会话命令面注销后插件前端改走自家命令通道（`session.input` → 本插件
// `session::input_via_pty`），宿主不再有这条替插件写输入的路径。
//
// 门禁不是被放宽而是被**换掉载体**：插件命令通道（`plugin_invoke`）自带
// 「身份令牌 + 激活」两段，写入侧 `host-pty.write` 的 `pty:io` 权限门仍在
// WIT 层仲裁（插件的 manifest 声明面）。故本条删除不影响输入权限的有效性。

// ==================== Plugin Registry Queries ====================

/// 获取所有命令
#[tauri::command]
pub async fn plugin_list_commands(
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<Vec<crate::wasm_core::manager::registry::CommandEntry>> {
    Ok(plugin_host.registry().list_commands().await)
}

/// 获取指定类型的视图
#[tauri::command]
pub async fn plugin_list_views(
    view_type: String,
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<Vec<crate::wasm_core::manager::registry::ViewEntry>> {
    Ok(plugin_host.registry().get_views_by_type(&view_type).await)
}

/// 查找文件处理器
#[tauri::command]
pub async fn plugin_find_file_handler(
    extension: String,
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<Option<crate::wasm_core::manager::registry::FileHandlerEntry>> {
    Ok(plugin_host.registry().find_file_handler(&extension).await)
}

// ==================== Rust Plugin Command Dispatch ====================

/// 调用 Rust 插件的自定义 command
///
/// 统一路由：前端通过 `invoke('plugin_invoke', { pluginId, command, args, credential })` 调用。
/// **身份由 `credential` 绑定**（审计票 06）：插件只能驱动自己的 command（目标必须等于令牌
/// 身份），宿主前端持 loader 会话密钥可驱动任意插件的 command（宿主职权）。
/// 此前该命令只查 `is_activated`，任何插件前端都能以他人 plugin_id 触发其命令副作用。
#[tauri::command]
pub async fn plugin_invoke(
    plugin_id: String,
    command: String,
    args: serde_json::Value,
    credential: String,
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<serde_json::Value> {
    authorize_plugin_call(&plugin_host, &plugin_id, &credential)?;
    plugin_host.invoke_rust_command(&plugin_id, &command, args).await
}

/// 获取所有 Rust 插件的 command 列表
#[tauri::command]
pub async fn plugin_list_rust_commands(
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<Vec<bedcode_plugin_api::PluginCommandEntry>> {
    Ok(plugin_host.list_rust_commands().await)
}

// ==================== Dev Mode ====================

/// 热重载 WASM 插件（仅开发模式可用）
///
/// 执行完整的卸载-重载-激活循环，用于开发期间快速迭代。
/// 生产构建中调用此命令返回错误
#[tauri::command]
pub async fn plugin_dev_reload(plugin_id: String, plugin_host: State<'_, Arc<PluginHost>>) -> crate::Result<()> {
    #[cfg(debug_assertions)]
    {
        plugin_host.reload_wasm_plugin(&plugin_id).await
    }
    #[cfg(not(debug_assertions))]
    {
        let _ = (plugin_host, plugin_id);
        Err(crate::AppError::Plugin(
            "Hot reload only available in dev mode".to_string(),
        ))
    }
}

// ==================== File System Auth ====================

/// 回复文件系统授权请求（由前端弹窗调用）
///
/// **只接受宿主面凭证**（loader 会话密钥，审计票 06）：授权请求事件是广播的，插件前端也
/// 能 `listen` 到，若该命令不绑身份，插件就能替用户「同意」自己的文件访问请求——
/// 那是把授权弹窗变成摆设。宿主弹窗 `FsAuthDialog.vue` 持宿主面凭证，插件拿不到。
#[tauri::command]
pub async fn plugin_fs_auth_respond(
    request_id: String,
    allowed: bool,
    remember: bool,
    credential: String,
    plugin_host: State<'_, Arc<PluginHost>>,
    fs_auth: State<'_, Arc<FsAuthChecker>>,
) -> crate::Result<()> {
    use crate::wasm_core::security::frontend_channel::ChannelIdentity;
    if plugin_host.frontend_channel().resolve(&credential) != Some(ChannelIdentity::Host) {
        tracing::warn!(
            request_id = %request_id,
            "[API] plugin_fs_auth_respond: 非宿主面凭证，拒绝替代用户决策"
        );
        return Err(crate::AppError::Plugin(
            "file system authorization must be answered by the host frontend".to_string(),
        ));
    }
    tracing::info!(
        "[API] plugin_fs_auth_respond: request_id={}, allowed={}, remember={}",
        request_id,
        allowed,
        remember
    );
    fs_auth.respond(&request_id, allowed, remember).await;
    Ok(())
}

// ==================== Tests ====================

/// 从本地 zip 分发包安装插件（dev 合入）
///
/// 仅接收已下载的本地文件路径：解压与身份/路径校验在 Rust 端完成
#[tauri::command]
pub async fn plugin_install_from_file(path: String, plugin_host: State<'_, Arc<PluginHost>>) -> crate::Result<String> {
    tracing::info!("[API] plugin_install_from_file: {}", path);
    let result = plugin_host.install_from_zip(&path).await;
    if let Err(ref e) = result {
        tracing::error!(error = %e, "[API] plugin_install_from_file failed");
    }
    result
}

/// 卸载插件（所有来源；要求插件未启用，dev 合入）
#[tauri::command]
pub async fn plugin_uninstall(plugin_id: String, plugin_host: State<'_, Arc<PluginHost>>) -> crate::Result<()> {
    tracing::info!(plugin_id = %plugin_id, "[API] plugin_uninstall");
    let result = plugin_host.uninstall_plugin(&plugin_id).await;
    if let Err(ref e) = result {
        tracing::error!(plugin_id = %plugin_id, error = %e, "[API] plugin_uninstall failed");
    }
    result
}

#[cfg(test)]
mod tests {
    //! 本模块（Tauri commands 桥）大部分不可单测的原因：
    //!
    //! 1. 所有 command 函数的第一个/最后一个参数均为
    //!    `State<'_, Arc<PluginHost>>`（或 `State<'_, Arc<FsAuthChecker>>`），
    //!    Tauri 的 `State` 不实现 `From<T>`，且其 `CommandArg` 实现需要
    //!    Tauri 运行时上下文（`StateManager`）才能构造 —— 单元测试无法
    //!    直接调用这些函数。
    //! 2. 启用 `tauri` 的 `test` feature（`tauri::test::mock_builder`）可
    //!    模拟运行时，但需要修改 Cargo.toml（本任务约束：只加测试模块），
    //!    且桥接函数体全部是「权限门禁 + 委托给 PluginHost / FsAuthChecker」
    //!    的薄封装，无独立纯逻辑可提取。
    //! 3. 门禁逻辑（`is_activated` / `permission().check`）与委托目标
    //!    （`list_plugins` / `activate_plugin` / `invoke_rust_command` /
    //!    `storage()` 等）均已在 `host.rs` 测试中直接覆盖（含错误分支的
    //!    错误字符串断言），桥接层只是透传。
    //!
    //! 结论：不硬造测试；桥接层行为由 host.rs 的宿主测试 + 前端集成测试
    //! 覆盖。`plugin_terminal_send_input` 的写入委托目标（会话窄转发层
    //! `session_gateway::input` → 插件 `session-input`）由 `session_e2e` 闭环用例
    //! 覆盖，桥接层自身只剩门禁三段。
    //!
    //! 若未来启用 tauri test feature，可在此处为 `plugin_storage_*` /
    //! `plugin_terminal_send_input` 的门禁错误分支补测试。
    //!
    //! 例外：`plugin_frontend_load_report` 不依赖任何 State 参数（纯日志透传），
    //! 可直接调用测试。

    /// 票 08 防回接锁：宿主会话命令面与插件终端输入通道**不得再出现**。
    ///
    /// 注销的是「宿主-会话」这一族命令（`list_sessions` / `get_session` /
    /// `resize_session` / `write_to_session` / `send_special_key`）与
    /// `plugin_terminal_send_input`。判据是**两侧同时干净**（票 08 验收原文）：
    ///
    /// - Rust 侧：`generate_handler!` 注册表与 `commands::` 路径都不再出现；
    /// - 前端侧：没有任何 `invoke('…')` 还能调到已注销的命令名。
    ///
    /// 只扫非注释行——各模块的「为什么删」说明段落里出现这些名字是**记账**。
    /// 注释豁免的代价是「把回接写进注释不算违规」，这是有意的：注释不参与运行。
    #[test]
    fn retired_session_command_surface_is_not_reintroduced() {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));

        // 三个源码面：宿主 Rust、宿主前端（含 .vue）、插件前端
        let scan_roots: Vec<std::path::PathBuf> = vec![
            manifest_dir.join("src"),
            manifest_dir.join("../src"),
            manifest_dir.join("../plugins"),
        ];

        let rust_needles = [
            "commands::list_sessions",
            "commands::get_session",
            "commands::resize_session",
            "commands::write_to_session",
            "commands::send_special_key",
            "plugin_terminal_send_input",
        ];
        // 前端**不带引号**匹配：`invoke('list_sessions')` / `invoke("list_sessions")` /
        // 模板串三种写法都要拦。这些是 snake_case 命令名，前端源码里除 invoke 串之外
        // 不该出现（首轮写成带单引号的针，变异自检用双引号接回时**漏判**，故收窄到名字）。
        let frontend_needles = [
            "list_sessions",
            "get_session",
            "resize_session",
            "write_to_session",
            "send_special_key",
            "plugin_terminal_send_input",
        ];

        let mut violations: Vec<String> = Vec::new();
        for root in &scan_roots {
            let mut stack = vec![root.clone()];
            while let Some(dir) = stack.pop() {
                let Ok(entries) = std::fs::read_dir(&dir) else { continue };
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        // node_modules / dist / target 不进扫描（构建产物不是源码面）
                        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                        if matches!(name, "node_modules" | "dist" | "target" | ".git") {
                            continue;
                        }
                        stack.push(path);
                        continue;
                    }
                    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                    let needles: &[&str] = match ext {
                        "rs" => &rust_needles,
                        "ts" | "vue" => &frontend_needles,
                        _ => continue,
                    };
                    // 本文件是锁自身，跳过（避免自匹配）
                    if path.ends_with("api_bridge.rs") {
                        continue;
                    }
                    let Ok(content) = std::fs::read_to_string(&path) else { continue };
                    for (idx, raw_line) in content.lines().enumerate() {
                        let line = raw_line.trim_start();
                        if line.starts_with("//") || line.starts_with('*') || line.starts_with("/*") {
                            continue;
                        }
                        for needle in needles {
                            if line.contains(needle) {
                                violations.push(format!(
                                    "{}:{}: {}",
                                    path.display(),
                                    idx + 1,
                                    line.trim()
                                ));
                            }
                        }
                    }
                }
            }
        }

        assert!(
            violations.is_empty(),
            "已注销的会话命令面出现回接痕迹（票 08）：\n{}",
            violations.join("\n")
        );
    }

    /// 诊断上报命令：ok/error 两条路径都只写 tracing，恒返回 Ok（issue 04）
    #[tokio::test]
    async fn frontend_load_report_always_ok() {
        let ok_report =
            super::plugin_frontend_load_report("com.bedcode.demo".into(), "import".into(), true, None).await;
        let fail_report = super::plugin_frontend_load_report(
            "com.bedcode.demo".into(),
            "activate".into(),
            false,
            Some("import timeout".into()),
        )
        .await;
        assert!(ok_report.is_ok());
        assert!(fail_report.is_ok());
    }
}
