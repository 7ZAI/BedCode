//! 应用域宿主实现（随包 CLI 生命周期，v8 host-app）
//!
//! WASM 插件无注册表/PATH 直接通道：安装/卸载全部由宿主侧完成
//! （`PluginServices::install_cli/uninstall_cli` 经 PluginHost 实现，
//! 见 plugin/host/app_cli.rs）。插件只声明权限并传 file_name/bin_dir。

use crate::plugin::permission::PERMISSION_APP_CLI;
use crate::plugin::wasm_runtime::{block_on_async, WasmHostContext};

/// 安装 CLI（权限 + 载荷解析 + 宿主服务执行），返回 bin 目录绝对路径
///
/// payload: `{ "file_name": "bedtask", "bin_dir": "" }` —— file_name 缺省
/// "bedtask"（Windows 自动补 .exe）；bin_dir 为空用平台默认。
pub(crate) fn install_cli(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    payload_json: &str,
) -> Result<String, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_APP_CLI, "host_app_install_cli") {
        return Err("permission denied".to_string());
    }
    let payload: serde_json::Value = serde_json::from_str(payload_json)
        .map_err(|e| format!("app error: invalid payload JSON: {}", e))?;
    let file_name = payload.get("file_name").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let bin_dir = payload.get("bin_dir").and_then(|v| v.as_str()).unwrap_or("").to_string();

    let services = block_on_async(host_ctx.services())
        .ok_or_else(|| "app error: host services unavailable".to_string())?;
    block_on_async(services.install_cli(plugin_id.to_string(), file_name, bin_dir))
}

/// 卸载 CLI（权限 + 载荷解析 + 宿主服务执行）
pub(crate) fn uninstall_cli(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    payload_json: &str,
) -> Result<(), String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_APP_CLI, "host_app_uninstall_cli") {
        return Err("permission denied".to_string());
    }
    let payload: serde_json::Value = serde_json::from_str(payload_json)
        .map_err(|e| format!("app error: invalid payload JSON: {}", e))?;
    let file_name = payload.get("file_name").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let bin_dir = payload.get("bin_dir").and_then(|v| v.as_str()).unwrap_or("").to_string();

    let services = block_on_async(host_ctx.services())
        .ok_or_else(|| "app error: host services unavailable".to_string())?;
    block_on_async(services.uninstall_cli(plugin_id.to_string(), file_name, bin_dir))
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::wasm_runtime::host_impl::tests::{build_host_ctx, grant_permissions};

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
}
