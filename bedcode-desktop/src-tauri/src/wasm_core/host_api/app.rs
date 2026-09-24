//! 应用域宿主实现（随包 CLI 生命周期，v8 host-app）
//!
//! WASM 插件无注册表/PATH 直接通道：安装/卸载全部由宿主侧完成
//! （`PluginServices::install_cli/uninstall_cli` 经 PluginHost 实现，
//! 见 plugin/host/app_cli.rs）。插件只声明权限并传 file_name/bin_dir。

use crate::wasm_core::manager::runtime::WasmHostContext;
use crate::wasm_core::runtime_util::block_on_async;
use crate::wasm_core::permission::PERMISSION_APP_CLI;

/// 安装 CLI（权限 + 载荷解析 + 宿主服务执行），返回 bin 目录绝对路径
///
/// payload: `{ "file_name": "bedtask", "bin_dir": "" }` —— file_name 缺省
/// "bedtask"（Windows 自动补 .exe）；bin_dir 为空用平台默认。
pub(crate) fn install_cli(host_ctx: &WasmHostContext, plugin_id: &str, payload_json: &str) -> Result<String, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_APP_CLI, "host_app_install_cli") {
        return Err("permission denied".to_string());
    }
    let payload: serde_json::Value =
        serde_json::from_str(payload_json).map_err(|e| format!("app error: invalid payload JSON: {}", e))?;
    let file_name = payload
        .get("file_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let bin_dir = payload
        .get("bin_dir")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let services =
        block_on_async(host_ctx.services()).ok_or_else(|| "app error: host services unavailable".to_string())?;
    block_on_async(services.install_cli(plugin_id.to_string(), file_name, bin_dir))
}

/// 卸载 CLI（权限 + 载荷解析 + 宿主服务执行）
pub(crate) fn uninstall_cli(host_ctx: &WasmHostContext, plugin_id: &str, payload_json: &str) -> Result<(), String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_APP_CLI, "host_app_uninstall_cli") {
        return Err("permission denied".to_string());
    }
    let payload: serde_json::Value =
        serde_json::from_str(payload_json).map_err(|e| format!("app error: invalid payload JSON: {}", e))?;
    let file_name = payload
        .get("file_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let bin_dir = payload
        .get("bin_dir")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let services =
        block_on_async(host_ctx.services()).ok_or_else(|| "app error: host services unavailable".to_string())?;
    block_on_async(services.uninstall_cli(plugin_id.to_string(), file_name, bin_dir))
}

/// 插件自身资源目录（v25 函数级追加，**无权限门**——只返回调用方自己的安装路径）
///
/// 会话创建编排移交插件后（session-engine-downsink P1-b）宿主不再产生 `Creating`
/// 生命周期事件，本原语成为插件取自身资源目录的唯一途径（Agent 集成 hook 脚本源
/// 位于该目录）。未加载的插件 / 服务不可用 → `Err`（不静默返回空串）。
pub(crate) fn plugin_resource_dir(host_ctx: &WasmHostContext, plugin_id: &str) -> Result<String, String> {
    let services =
        block_on_async(host_ctx.services()).ok_or_else(|| "app error: host services unavailable".to_string())?;
    block_on_async(services.plugin_resource_dir(plugin_id.to_string()))
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wasm_core::host_api::tests::{build_host_ctx, grant_permissions};

    const PLUGIN: &str = "test-plugin";

    /// 无 app:cli 权限：install/uninstall 被拒绝
    #[test]
    fn cli_permission_denied() {
        let ctx = build_host_ctx();
        let err = install_cli(&ctx, PLUGIN, "{}").unwrap_err();
        assert_eq!(err, "permission denied");
        let err = uninstall_cli(&ctx, PLUGIN, "{}").unwrap_err();
        assert_eq!(err, "permission denied");
    }

    /// 有权限但载荷畸形：拒绝
    #[test]
    fn cli_invalid_payload_rejected() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_APP_CLI]);
        let err = install_cli(&ctx, PLUGIN, "not-json").unwrap_err();
        assert!(err.contains("invalid payload"), "got: {}", err);
    }

    /// 有权限、载荷合法但 services 为 None（测试上下文）：报服务不可用
    #[tokio::test]
    async fn cli_services_unavailable_in_test_ctx() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_APP_CLI]);
        let err = install_cli(&ctx, PLUGIN, r#"{"file_name":"bedtask"}"#).unwrap_err();
        assert!(err.contains("services unavailable"), "got: {}", err);
    }

    /// 资源目录**无权限门**：未授予任何权限的插件也拿不到「permission denied」
    ///
    /// 设计口径（同 `host-platform`）：本原语只返回调用方自己的安装路径、不含跨
    /// 插件信息，没有可授予的权力——加门只会造出一个恒过的死门。本用例锁住该口径：
    /// 若日后有人补上权限检查，这里会转红并迫使其回到裁剪线论证。
    #[tokio::test]
    async fn plugin_resource_dir_has_no_permission_gate() {
        let ctx = build_host_ctx();
        let err = plugin_resource_dir(&ctx, PLUGIN).unwrap_err();
        assert!(
            !err.contains("permission denied"),
            "资源目录不得设权限门，got: {err}"
        );
        assert!(err.contains("services unavailable"), "got: {}", err);
    }
}
