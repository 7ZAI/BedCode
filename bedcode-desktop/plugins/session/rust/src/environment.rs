//! 执行环境平台事实（票 13）：WSL 发行版枚举
//!
//! 会话配置表单的 `environment = wsl2` 分支需要宿主可用的 WSL 发行版列表——
//! 这是平台事实（`host-platform.wsl-distros` 原语，无业务语义，ADR 0022 裁剪线），
//! 插件不自建枚举逻辑（`wsl --list --verbose` 的调用与 UTF-16LE 解码回退归宿主）。
//!
//! 宿主无 WSL（非 Windows / 未安装 / 命令不可用）时原语**显性报错**而非返回空数组
//! ——空数组会被表单读成「装了 0 个发行版」，与「本机没有 WSL」不可区分。本模块
//! 原样上抛该错误，前端据此渲染「未检测到 WSL」提示（与宿主设置页同一口径）。

/// WSL 发行版名列表 → `{distros: string[]}`
#[cfg(target_arch = "wasm32")]
pub fn wsl_distros_via_host() -> Result<serde_json::Value, String> {
    use bedcode_plugin_api::host::HostPlatform;
    use bedcode_plugin_api::wasm_host::WasmHost;
    let distros = WasmHost
        .platform_wsl_distros()
        .map_err(|e| format!("host wsl-distros failed: {}", e.message))?;
    Ok(serde_json::json!({ "distros": distros }))
}

/// native（cargo test）：宿主原语不可用 → 显性失败（不静默返回空列表）
#[cfg(not(target_arch = "wasm32"))]
pub fn wsl_distros_via_host() -> Result<serde_json::Value, String> {
    Err("wsl distros unavailable outside wasm runtime".to_string())
}
