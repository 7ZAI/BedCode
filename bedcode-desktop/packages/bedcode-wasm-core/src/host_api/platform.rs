//! host-platform 逻辑层 —— 通用平台能力域（ADR 0022 v2，issue 13 Phase 2）
//!
//! 系统对话框等与领域无关的平台交互。
//!
//! ## 选源对话框（`pick-*`）的双段口径（2026-09-27）
//!
//! 1. **准入门**：manifest 必须**单独声明权限 `fs:pick`**。未声明 → 显性 `Err`，
//!    且**在弹对话框之前**返回（不给「没权限也能弹框看路径列表」留缝）。
//!    选器与 `fs:read` / `fs:write` 分域：它只交付「用户亲手选中的路径」这一层。
//! 2. **结果门**：系统原生对话框（Windows IFileDialog / Linux xdg-desktop-portal）
//!    浏览面不设限（浏览由用户在系统对话框里驱动，宿主不读任何数据）；选择结束后
//!    对**选中路径**做 [`FsAuthChecker::authorize_picked`] 授权校验——命中已授权
//!    目录前缀则静默放行，未授权的弹一次授权框并按**所在目录**落账，拒绝 / 超时
//!    → `Err` 且**不回传路径**（fail-visible：不得降级成空数组）。
//!
//! 实现本体在此（host-business-decarriage 收尾）：历史上选源对话框挂在
//! `peer_engine_transfer`（对等传输域）下，经 `peer_net` 转发而来——那是
//! 「peer 命令面复用」时期的错放（ADR 0022 v2 已把 pick-* 判归 host-platform）。
//! 传输域不再承载平台对话框，插件一律走本域。

use crate::host_api::context::{AppHandleScope, FsAuthScope, PermissionScope};
use crate::permission::PERMISSION_FS_PICK;
use crate::runtime_util::block_on_async;
use crate::security::fs_auth::FsAuthChecker;
use std::sync::Arc;
use tauri_plugin_dialog::DialogExt;

/// 权限门（`fs:pick`）：未声明即拒绝，**且不弹对话框**
///
/// 文案点名缺失的权限位与补法（fail-visible）：插件拿到的错误必须能直接回答
/// 「我在 manifest 里加什么」——「permission denied」这种无信息文案在这里是
/// 把排障成本推给插件作者。
fn require_pick_permission(perm: &dyn PermissionScope, plugin_id: &str, api: &str) -> Result<(), String> {
    if super::check_permission(perm, plugin_id, PERMISSION_FS_PICK, api) {
        return Ok(());
    }
    Err(format!(
        "permission denied: manifest must declare '{PERMISSION_FS_PICK}' to use the system file picker ({api})"
    ))
}

/// 系统多文件选择器 → string[] JSON（用户取消为空数组）
pub(crate) fn platform_pick_files(
    perm: &dyn PermissionScope,
    app: &dyn AppHandleScope,
    fs: &dyn FsAuthScope,
    plugin_id: &str,
) -> Result<String, String> {
    require_pick_permission(perm, plugin_id, "host_platform_pick_files")?;
    let app = require_app(app)?;
    let picked = sync_result(block_on_async(pick_files(app)))?;
    let picked = authorize_picked(fs, plugin_id, "host_platform_pick_files", picked)?;
    serde_json::to_string(&picked).map_err(|e| format!("serialize picked files failed: {e}"))
}

/// 系统文件夹选择器 → 绝对路径；用户取消返回空串
pub(crate) fn platform_pick_folder(
    perm: &dyn PermissionScope,
    app: &dyn AppHandleScope,
    fs: &dyn FsAuthScope,
    plugin_id: &str,
) -> Result<String, String> {
    require_pick_permission(perm, plugin_id, "host_platform_pick_folder")?;
    let app = require_app(app)?;
    let picked = sync_result(block_on_async(pick_folder(app)))?;
    let picked = authorize_picked(fs, plugin_id, "host_platform_pick_folder", picked)?;
    Ok(picked.into_iter().next().unwrap_or_default())
}

/// 系统多目录选择器 → string[] JSON（用户取消为空数组）
pub(crate) fn platform_pick_folders(
    perm: &dyn PermissionScope,
    app: &dyn AppHandleScope,
    fs: &dyn FsAuthScope,
    plugin_id: &str,
) -> Result<String, String> {
    require_pick_permission(perm, plugin_id, "host_platform_pick_folders")?;
    let app = require_app(app)?;
    let picked = sync_result(block_on_async(pick_folders(app)))?;
    let picked = authorize_picked(fs, plugin_id, "host_platform_pick_folders", picked)?;
    serde_json::to_string(&picked).map_err(|e| format!("serialize picked folders failed: {e}"))
}

/// 选择结果的授权校验（`fs:pick` 契约第二段）
///
/// - **空选择**（用户取消）→ 直接放行：取消不是「拒绝授权」，不该多弹一个框；
/// - **命中已授权目录** → 静默放行（`authorize_picked` 内部按第一、二层判）；
/// - **未授权** → 弹一次框；拒绝 / 超时 / 无弹窗通道 → `Err`，**一个路径都不回传**
///   （回传空数组会让插件把「被拒」读成「用户没选文件」，静默丢数据）。
fn authorize_picked(
    fs: &dyn FsAuthScope,
    plugin_id: &str,
    api: &str,
    picked: Vec<String>,
) -> Result<Vec<String>, String> {
    if picked.is_empty() {
        return Ok(picked);
    }
    let checker: Arc<FsAuthChecker> = fs.fs_auth().clone();
    if block_on_async(checker.authorize_picked(plugin_id, &picked)) {
        return Ok(picked);
    }
    tracing::warn!(
        plugin_id = %plugin_id,
        api = %api,
        picked_count = picked.len(),
        "platform pick: selection not authorized (denied or timed out)"
    );
    Err(format!(
        "{api}: picked paths were not authorized (user denied or the request timed out); no path returned"
    ))
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

fn require_app(app: &dyn crate::host_api::context::AppHandleScope) -> Result<tauri::AppHandle, String> {
    app.app_handle()
        .map(|a| a.clone())
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
        tokio::task::spawn_blocking(crate::system::wsl::list_distributions)
            .await
            .map_err(|e| format!("wsl distro list task join failed: {e}"))?
            .map_err(|e| format!("wsl distro list failed: {e}"))
    })?;
    wsl_distro_names(distros)
}

/// 发行版列表 → JSON 名字数组（纯函数：只取 name、保持输入顺序）
pub(crate) fn wsl_distro_names(distros: Vec<crate::system::wsl::WslDistro>) -> Result<String, String> {
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
/// 与 `pick-*` **不同口径**：定位不调起选择器、也不交付任何路径（路径本就由调用方
/// 提供），因此**不叠加权限门**（ADR 0022 裁剪线）。两者分域的判据是「本次调用
/// 有没有把未经用户确认的路径交给插件」——`pick-*` 有（所以要门 + 选择后授权
/// 校验），`reveal-in-dir` 没有。实现本体在引擎模块 [`crate::system::opener`]
/// （同一份平台分发也被宿主 `open_log_dir` 使用）；本函数只做「PathBuf 语义校验
/// + 错误转 String」的适配。
pub(crate) fn platform_reveal_in_dir(path: &str) -> Result<(), String> {
    crate::system::opener::reveal_existing_in_dir(path).map_err(|e| e.to_string())
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host_api::tests::{build_host_ctx, generated_vocabulary_know};
    use crate::host_api::grant_permissions;
    use crate::permission::PERMISSION_FS_PICK;
    use std::path::PathBuf;

    const PLUGIN: &str = "com.test.pick";

    fn distro(name: &str, is_default: bool) -> crate::system::wsl::WslDistro {
        crate::system::wsl::WslDistro {
            name: name.to_string(),
            is_default,
            state: "Running".to_string(),
            version: 2,
        }
    }

    /// canonical 后的临时目录（`matched_layer` 收的是已 canonicalize 的路径）
    fn canonical_temp_dir() -> PathBuf {
        std::fs::canonicalize(std::env::temp_dir()).expect("temp dir must be canonicalizable")
    }

    // ==================== 权限门（`fs:pick`） ====================

    /// C-101 未声明 `fs:pick` → 显性拒绝，且**在弹对话框之前**（无头上下文拿不到
    /// AppHandle，若顺序反了错误会是 headless 那条）
    #[test]
    fn pick_requires_fs_pick_permission() {
        let ctx = build_host_ctx();
        for perm in ["fs:read", "fs:write", "storage", "peer"] {
            grant_permissions(&ctx, PLUGIN, &[perm]);
            let err = platform_pick_files(ctx.as_ref(), ctx.as_ref(), ctx.as_ref(), PLUGIN)
                .expect_err("未声明 fs:pick 必须拒绝");
            assert!(
                err.contains(PERMISSION_FS_PICK),
                "错误必须点名缺失的权限位（插件作者据此知道 manifest 补什么），got: {err}"
            );
            assert!(
                !err.contains("headless"),
                "权限门必须早于对话框：拿到了 headless 报错说明顺序反了, got: {err}"
            );
        }
    }

    /// C-110 分域：`fs:read` / `fs:write` 都不隐含选择器位，反向也不成立
    #[test]
    fn pick_bit_is_independent_from_read_write() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_FS_PICK]);
        // 声明了 fs:pick → 过得了权限门（下一步才会去要 AppHandle）
        let err = platform_pick_files(ctx.as_ref(), ctx.as_ref(), ctx.as_ref(), PLUGIN)
            .expect_err("无头上下文拿不到系统对话框");
        assert!(err.contains("headless"), "声明 fs:pick 后应走到对话框环节, got: {err}");
    }

    /// 权限五同步点：新位进了 CLI 与前端两份**生成物**（漏跑 gen:permissions 即红）
    #[test]
    fn fs_pick_bit_is_in_generated_vocabulary() {
        generated_vocabulary_know("fs:pick");
    }

    // ==================== 选择结果授权校验 ====================
    //
    // 授权判据本身的用例（已授权目录静默放行 / 未授权保守拒 / 取消不弹框 /
    // 落账粒度）在 `security::fs_auth` 的测试模块——那里能读 `pending_requests`
    // 私有字段断言「没有多弹一个框」，本文件只锁宿主原语这一层的契约。

    /// C-104 在原语层的体现：声明了 `fs:pick` 且选中路径已授权 → 走到对话框环节
    /// 而不是被授权层拦下（无头上下文止步于 AppHandle，故错误必为 headless）
    #[tokio::test]
    async fn pick_passes_permission_gate_and_reaches_dialog() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_FS_PICK]);
        let err = platform_pick_files(ctx.as_ref(), ctx.as_ref(), ctx.as_ref(), PLUGIN)
            .expect_err("无头上下文没有系统对话框");
        assert!(err.contains("headless"), "got: {err}");
        assert!(
            !err.contains(PERMISSION_FS_PICK),
            "已声明 fs:pick 不得再报权限缺失, got: {err}"
        );
    }

    // ==================== 既有域（本文件原有用例） ====================

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
