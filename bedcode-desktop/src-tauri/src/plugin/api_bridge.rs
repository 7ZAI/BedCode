//! Plugin API Bridge
//!
//! Tauri commands — 前端 PluginContext 的每个 API 调用通过 Tauri invoke 到达此桥接层
//! Rust 端做权限校验后执行操作

use crate::plugin::fs_auth::FsAuthChecker;
use crate::plugin::host::PluginHost;
use crate::plugin::types::DesktopPluginInfo;
use std::sync::Arc;
use tauri::State;

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

/// 从本地 zip 插件包安装（用户插件目录）
#[tauri::command]
pub async fn plugin_install_from_file(path: String, plugin_host: State<'_, Arc<PluginHost>>) -> crate::Result<String> {
    tracing::info!("[API] plugin_install_from_file: {}", path);
    let result = plugin_host.install_from_zip(&path).await;
    if let Err(ref e) = result {
        tracing::error!(error = %e, "[API] plugin_install_from_file failed");
    }
    result
}

/// 卸载插件（所有来源：删除插件所有数据——存储 + 激活状态 + 安装目录；
/// 要求插件未启用，运行中由前端置灰 + 后端拒绝）
#[tauri::command]
pub async fn plugin_uninstall(plugin_id: String, plugin_host: State<'_, Arc<PluginHost>>) -> crate::Result<()> {
    tracing::info!(plugin_id = %plugin_id, "[API] plugin_uninstall");
    let result = plugin_host.uninstall_plugin(&plugin_id).await;
    if let Err(ref e) = result {
        tracing::error!(plugin_id = %plugin_id, error = %e, "[API] plugin_uninstall failed");
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

/// 插件命令权限门禁（纯函数，可单测）：激活 + 权限双重校验
///
/// 门禁是桥接层独有的生产逻辑（host.rs 透传前必须拦住未激活/未授权调用），
/// 任何分支被误删/短路都会让权限绕过上线——抽取为纯函数锁定 2×2 分支语义。
fn check_plugin_gate(plugin_id: &str, activated: bool, permitted: bool, permission: &str) -> crate::Result<()> {
    if !activated {
        return Err(crate::AppError::Plugin(format!(
            "Plugin {} is not activated",
            plugin_id
        )));
    }
    if !permitted {
        return Err(crate::AppError::Plugin(format!(
            "Plugin {} has no {permission} permission",
            plugin_id
        )));
    }
    Ok(())
}

/// 插件存储：获取值
///
/// 校验调用者身份：plugin_id 对应的插件必须处于 Activated 状态
#[tauri::command]
pub async fn plugin_storage_get(
    plugin_id: String,
    key: String,
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<Option<serde_json::Value>> {
    check_plugin_gate(
        &plugin_id,
        plugin_host.is_activated(&plugin_id).await,
        plugin_host.permission().check(&plugin_id, "storage"),
        "storage",
    )?;
    plugin_host.storage().get(&plugin_id, &key).await
}

/// 插件存储：设置值
#[tauri::command]
pub async fn plugin_storage_set(
    plugin_id: String,
    key: String,
    value: serde_json::Value,
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<()> {
    check_plugin_gate(
        &plugin_id,
        plugin_host.is_activated(&plugin_id).await,
        plugin_host.permission().check(&plugin_id, "storage"),
        "storage",
    )?;
    plugin_host.storage().set(&plugin_id, &key, value).await
}

/// 插件存储：删除值
#[tauri::command]
pub async fn plugin_storage_delete(
    plugin_id: String,
    key: String,
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<()> {
    check_plugin_gate(
        &plugin_id,
        plugin_host.is_activated(&plugin_id).await,
        plugin_host.permission().check(&plugin_id, "storage"),
        "storage",
    )?;
    plugin_host.storage().delete(&plugin_id, &key).await
}

// ==================== Plugin Terminal ====================

/// 插件终端：发送输入
#[tauri::command]
pub async fn plugin_terminal_send_input(
    plugin_id: String,
    session_id: String,
    text: String,
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<()> {
    check_plugin_gate(
        &plugin_id,
        plugin_host.is_activated(&plugin_id).await,
        plugin_host.permission().check(&plugin_id, "terminal:input"),
        "terminal:input",
    )?;
    let ctx = crate::system::app_context::AppContext::global();
    ctx.session_manager().write_input(&session_id, &text).await
}

// ==================== Plugin Registry Queries ====================

/// 获取所有命令
#[tauri::command]
pub async fn plugin_list_commands(
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<Vec<crate::plugin::registry::CommandEntry>> {
    Ok(plugin_host.registry().list_commands().await)
}

/// 获取指定类型的视图
#[tauri::command]
pub async fn plugin_list_views(
    view_type: String,
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<Vec<crate::plugin::registry::ViewEntry>> {
    Ok(plugin_host.registry().get_views_by_type(&view_type).await)
}

/// 查找文件处理器
#[tauri::command]
pub async fn plugin_find_file_handler(
    extension: String,
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<Option<crate::plugin::registry::FileHandlerEntry>> {
    Ok(plugin_host.registry().find_file_handler(&extension).await)
}

// ==================== Rust Plugin Command Dispatch ====================

/// 调用 Rust 插件的自定义 command
///
/// 统一路由：前端通过 `invoke('plugin_invoke', { pluginId, command, args })` 调用
/// PluginHost 内部查找对应 handler 并执行，前端无法伪造 plugin_id
#[tauri::command]
pub async fn plugin_invoke(
    plugin_id: String,
    command: String,
    args: serde_json::Value,
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<serde_json::Value> {
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
#[tauri::command]
pub async fn plugin_fs_auth_respond(
    request_id: String,
    allowed: bool,
    remember: bool,
    fs_auth: State<'_, Arc<FsAuthChecker>>,
) -> crate::Result<()> {
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
    //!    的薄封装。
    //! 3. 门禁逻辑（`is_activated` / `permission().check`）已抽为纯函数
    //!    `check_plugin_gate`（票据 30）：激活/未激活 × 有/无权限 2×2 分支
    //!    在下方测试锁定，桥接层退化为薄封装；委托目标（`list_plugins` /
    //!    `activate_plugin` / `invoke_rust_command` / `storage()` 等）均在
    //!    `host.rs` 测试中直接覆盖。
    //!
    //! 例外：`plugin_frontend_load_report` 不依赖任何 State 参数（纯日志透传），
    //! 可直接调用测试。

    use super::*;

    /// 门禁 2×2 分支（票据 30 P0）：激活 × 权限四象限
    #[test]
    fn gate_rejects_inactive_plugin_even_with_permission() {
        let err = check_plugin_gate("com.bedcode.test", false, true, "storage").unwrap_err();
        assert!(
            err.to_string().contains("not activated"),
            "未激活必须拒绝，实际: {err}"
        );
    }

    #[test]
    fn gate_rejects_missing_permission_even_when_active() {
        let err = check_plugin_gate("com.bedcode.test", true, false, "terminal:input").unwrap_err();
        assert!(
            err.to_string().contains("no terminal:input permission"),
            "缺权限必须拒绝并点名权限，实际: {err}"
        );
    }

    #[test]
    fn gate_rejects_inactive_without_permission() {
        let err = check_plugin_gate("com.bedcode.test", false, false, "storage").unwrap_err();
        assert!(err.to_string().contains("not activated"), "实际: {err}");
    }

    #[test]
    fn gate_allows_active_with_permission() {
        assert!(check_plugin_gate("com.bedcode.test", true, true, "storage").is_ok());
        // 权限名参与错误消息（terminal:input 与 storage 消息不串）
        let err = check_plugin_gate("com.bedcode.test", true, false, "storage").unwrap_err();
        assert!(err.to_string().contains("no storage permission"), "实际: {err}");
        assert!(!err.to_string().contains("terminal:input"));
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
