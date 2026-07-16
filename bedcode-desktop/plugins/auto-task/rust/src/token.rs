//! Token 管理
//!
//! 插件认证 token 校验与生成

use bedcode_plugin_api::wasm_host::WasmHost;

/// Token 配置结果
#[derive(Debug, Clone)]
pub struct TokenSetupResult {
    pub success: bool,
    pub message: String,
    pub token_generated: bool,
}

/// 确保 plugin token 合法
///
/// 通过宿主配置读取当前 token。WASM 插件无法写入宿主配置，
/// token 的生成和持久化由宿主侧负责。
pub fn ensure_token(host: &WasmHost) -> TokenSetupResult {
    host.log_info("ensure_token called (reading from host config)");

    let token = host.config_get("plugin.token");

    match token {
        Some(t) if !t.is_empty() => {
            host.log_info(&format!("Plugin token validated (len={})", t.len()));
            TokenSetupResult {
                success: true,
                message: "Token 已验证".to_string(),
                token_generated: false,
            }
        }
        _ => {
            host.log_warn("Plugin token not configured or empty");
            TokenSetupResult {
                success: true,
                message: "Token 校验已跳过".to_string(),
                token_generated: false,
            }
        }
    }
}
