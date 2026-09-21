//! host-platform 逻辑层 —— 通用平台能力域（ADR 0022 v2，issue 13 Phase 2）
//!
//! 系统对话框等与领域无关的平台交互。选源对话框本身即用户授权动作，
//! 不叠加权限门（与 host-platform 原语契约同口径）。
//!
//! 实现本体在此（host-business-decarriage 收尾）：历史上选源对话框挂在
//! `peer_engine_transfer`（对等传输域）下，经 `peer_net` 转发而来——那是
//! 「peer 命令面复用」时期的错放（ADR 0022 v2 已把 pick-* 判归 host-platform）。
//! 传输域不再承载平台对话框，插件一律走本域。

use crate::plugin::manager::wasm_runtime::{block_on_async, WasmHostContext};
use tauri_plugin_dialog::DialogExt;

/// 系统多文件选择器 → string[] JSON（用户取消为空数组）
pub(crate) fn platform_pick_files(host_ctx: &WasmHostContext) -> Result<String, String> {
    let app = require_app(host_ctx)?;
    let paths = sync_result(block_on_async(pick_files(app)))?;
    serde_json::to_string(&paths).map_err(|e| format!("serialize picked files failed: {e}"))
}

/// 系统文件夹选择器 → 绝对路径；用户取消返回空串
pub(crate) fn platform_pick_folder(host_ctx: &WasmHostContext) -> Result<String, String> {
    let app = require_app(host_ctx)?;
    let paths = sync_result(block_on_async(pick_folder(app)))?;
    Ok(paths.into_iter().next().unwrap_or_default())
}

/// 系统多目录选择器 → string[] JSON（用户取消为空数组）
pub(crate) fn platform_pick_folders(host_ctx: &WasmHostContext) -> Result<String, String> {
    let app = require_app(host_ctx)?;
    let paths = sync_result(block_on_async(pick_folders(app)))?;
    serde_json::to_string(&paths).map_err(|e| format!("serialize picked folders failed: {e}"))
}

// ==================== 选源对话框（阻塞至用户选择；取消 → 空） ====================

async fn pick_files(app_handle: tauri::AppHandle) -> crate::Result<Vec<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app_handle.dialog().file().pick_files(move |selection| {
        if tx.send(selection).is_err() {
            tracing::debug!("platform_pick_files: receiver dropped before dialog completed");
        }
    });
    match rx.await {
        Ok(Some(paths)) => paths.into_iter().map(path_to_string).collect(),
        // 用户取消选择
        Ok(None) => Ok(Vec::new()),
        Err(e) => Err(crate::AppError::Plugin(format!(
            "platform_pick_files: dialog channel closed: {e}"
        ))),
    }
}

async fn pick_folder(app_handle: tauri::AppHandle) -> crate::Result<Vec<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app_handle.dialog().file().pick_folder(move |selection| {
        if tx.send(selection).is_err() {
            tracing::debug!("platform_pick_folder: receiver dropped before dialog completed");
        }
    });
    match rx.await {
        Ok(Some(path)) => Ok(vec![path_to_string(path)?]),
        // 用户取消选择
        Ok(None) => Ok(Vec::new()),
        Err(e) => Err(crate::AppError::Plugin(format!(
            "platform_pick_folder: dialog channel closed: {e}"
        ))),
    }
}

async fn pick_folders(app_handle: tauri::AppHandle) -> crate::Result<Vec<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app_handle.dialog().file().pick_folders(move |selection| {
        if tx.send(selection).is_err() {
            tracing::debug!("platform_pick_folders: receiver dropped before dialog completed");
        }
    });
    match rx.await {
        Ok(Some(paths)) => paths.into_iter().map(path_to_string).collect(),
        // 用户取消选择
        Ok(None) => Ok(Vec::new()),
        Err(e) => Err(crate::AppError::Plugin(format!(
            "platform_pick_folders: dialog channel closed: {e}"
        ))),
    }
}

/// Dialog FilePath → UTF-8 绝对路径串（非 UTF-8 路径显式报错而非静默丢弃）
fn path_to_string(file_path: tauri_plugin_dialog::FilePath) -> crate::Result<String> {
    let path = file_path
        .into_path()
        .map_err(|e| crate::AppError::InvalidInput(format!("platform pick: failed to convert selected path: {e}")))?;
    path.to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| crate::AppError::InvalidInput("platform pick: selected path is not valid UTF-8".to_string()))
}

fn require_app(host_ctx: &WasmHostContext) -> Result<tauri::AppHandle, String> {
    host_ctx
        .app_handle
        .as_ref()
        .map(|a| (**a).clone())
        .ok_or_else(|| "platform unavailable in headless context (no app_handle)".to_string())
}

fn sync_result<T>(r: crate::Result<T>) -> Result<T, String> {
    r.map_err(|e| e.to_string())
}

/// WSL 发行版名枚举（v19 函数级追加，票 13）：`string[]` JSON，顺序保持
/// `wsl --list --verbose` 输出顺序。
///
/// 宿主无 WSL（非 Windows / 未安装 / 命令不可用）时**显性报错**而非空数组——
/// 空数组会被消费方读成「装了 0 个发行版」，与「本机没有 WSL」不可区分。
/// 枚举是阻塞进程调用，搬 `spawn_blocking` 以免占住 async store 所在 worker。
pub(crate) fn platform_wsl_distros() -> Result<String, String> {
    let distros = block_on_async(async {
        tokio::task::spawn_blocking(crate::pty::list_distributions)
            .await
            .map_err(|e| format!("wsl distro list task join failed: {e}"))?
            .map_err(|e| format!("wsl distro list failed: {e}"))
    })?;
    wsl_distro_names(distros)
}

/// 发行版列表 → JSON 名字数组（纯函数：只取 name、保持输入顺序）
pub(crate) fn wsl_distro_names(distros: Vec<crate::pty::WslDistro>) -> Result<String, String> {
    let names: Vec<String> = distros.into_iter().map(|d| d.name).collect();
    serde_json::to_string(&names).map_err(|e| format!("serialize wsl distros failed: {e}"))
}

/// 本机可访问 IPv4 地址列表（v19 函数级追加，票 14）→ `string[]` JSON
///
/// 与宿主命令面 `get_local_ip_addresses` 同口径（只要 IPv4、排除回环与链路本地，
/// 与会话中心设备页迁移前的行为逐字一致）；**无可用地址时返回空数组**——
/// 「没有可用地址」是合法状态（前端渲染「未找到可用的 IPv4 地址」占位），
/// 这与 `wsl-distros` 的显性报错口径不同（那里空列表与「未安装 WSL」不可区分）。
/// 网卡枚举是阻塞调用，搬 `spawn_blocking` 以免占住 async store 所在 worker。
pub(crate) fn platform_local_ipv4_addresses() -> Result<String, String> {
    let addresses = block_on_async(async {
        tokio::task::spawn_blocking(collect_local_ipv4)
            .await
            .map_err(|e| format!("local ipv4 list task join failed: {e}"))
    })?;
    serde_json::to_string(&addresses).map_err(|e| format!("serialize local ipv4 addresses failed: {e}"))
}

/// 真实网卡枚举（含过滤）
fn collect_local_ipv4() -> Vec<String> {
    let interfaces = local_ip_address::list_afinet_netifas().unwrap_or_default();
    filter_ipv4(interfaces)
}

/// 网卡条目过滤（纯函数，native 单测覆盖）：只保留 IPv4 且非回环 / 非链路本地，
/// 输出顺序即输入顺序（与宿主命令面同口径）
pub(crate) fn filter_ipv4(interfaces: Vec<(String, std::net::IpAddr)>) -> Vec<String> {
    interfaces
        .into_iter()
        .filter(|(_, ip)| match ip {
            std::net::IpAddr::V4(v4) => !v4.is_loopback() && !v4.is_link_local(),
            std::net::IpAddr::V6(_) => false,
        })
        .map(|(_, ip)| ip.to_string())
        .collect()
}

// ==================== 系统文件管理器定位（v22 函数级追加） ====================

/// 在系统文件管理器中定位并选中文件/目录（v22 函数级追加）：`()`
///
/// 与 `pick-*` 同口径——定位是平台交互动作、不读取任何数据（路径本就由调用方
/// 提供），故**不叠加权限门**（ADR 0022 裁剪线）。实现本体在引擎模块
/// [`crate::system::opener`]（同一份平台分发也被宿主 `open_log_dir` 使用）；
/// 本函数只做「PathBuf 语义校验 + 错误转 String」的适配。
pub(crate) fn platform_reveal_in_dir(path: &str) -> Result<(), String> {
    crate::system::opener::reveal_existing_in_dir(path).map_err(|e| e.to_string())
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    fn distro(name: &str, is_default: bool) -> crate::pty::WslDistro {
        crate::pty::WslDistro {
            name: name.to_string(),
            is_default,
            state: "Running".to_string(),
            version: 2,
        }
    }

    /// 契约：只回发行版名（无 state / version / is_default 等派生信息），顺序保持
    #[test]
    fn wsl_distro_names_returns_names_in_order() {
        let json = wsl_distro_names(vec![distro("Ubuntu", true), distro("Debian", false)]).expect("serialize");
        assert_eq!(json, r#"["Ubuntu","Debian"]"#);
    }

    /// 空列表可用（安装了 WSL 但没有任何发行版 → 空数组；与「无 WSL」的区分
    /// 由错误通道承担，见下一用例）
    #[test]
    fn wsl_distro_names_empty_list() {
        assert_eq!(wsl_distro_names(Vec::new()).expect("serialize"), "[]");
    }

    /// 无 WSL 的宿主（非 Windows）显性报错，不静默回空数组
    #[cfg(not(target_os = "windows"))]
    #[test]
    fn wsl_distros_fail_loudly_without_windows() {
        let err = platform_wsl_distros().expect_err("非 Windows 宿主必须显性报错");
        assert!(err.contains("wsl distro list"), "got: {err}");
    }

    /// IPv4 过滤契约（票 14）：只留 IPv4、排除回环与链路本地、IPv6 一律丢弃、
    /// 输出顺序与输入一致——与宿主命令面 `get_local_ip_addresses` 同口径
    #[test]
    fn filter_ipv4_excludes_loopback_link_local_and_ipv6() {
        use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
        let interfaces = vec![
            ("lo".to_string(), IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))),
            ("eth0".to_string(), IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10))),
            ("eth0".to_string(), IpAddr::V4(Ipv4Addr::new(169, 254, 1, 1))),
            ("eth1".to_string(), IpAddr::V6(Ipv6Addr::LOCALHOST)),
            ("wlan0".to_string(), IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5))),
        ];
        assert_eq!(filter_ipv4(interfaces), vec!["192.168.1.10", "10.0.0.5"]);
    }

    /// 空输入 → 空数组（无可用网卡是合法状态，不是错误）
    #[test]
    fn filter_ipv4_empty_input_yields_empty_list() {
        assert!(filter_ipv4(Vec::new()).is_empty());
    }

    /// 真实网卡枚举不得出口回环地址（把过滤逻辑与真实调用串起来的最小断言；
    /// 无网卡环境下为空数组同样成立）
    #[test]
    fn collect_local_ipv4_never_returns_loopback() {
        for ip in collect_local_ipv4() {
            assert!(!ip.starts_with("127."), "回环地址不得出口: {ip}");
        }
    }
}
