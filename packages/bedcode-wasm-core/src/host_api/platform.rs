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

// 票 02 批次 06（v36 交集切片）：platform 桌面扩展四函数（pick-folds /
// wsl-distros / local-ipv4-addresses / reveal-in-dir）已拆入 `host-platform-desktop`
// 并迁宿主 `src-tauri/src/plugin/platform.rs`（路径 B：域函数 + 单测随域走宿主；
// 内核不再持有桌面平台依赖 `local_ip_address` / `system::wsl`）。

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host_api::tests::{build_host_ctx, generated_vocabulary_know};
    use crate::host_api::grant_permissions;
    use crate::permission::PERMISSION_FS_PICK;

    const PLUGIN: &str = "com.test.pick";

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
    //
    // 桌面扩展原有用例（wsl_distro_names / wsl_distros_fail_loudly /
    // filter_ipv4 / collect_local_ipv4 一族）已随域函数迁宿主
    // `src-tauri/src/plugin/platform.rs`（票 02 批次 06：域函数 + 单测随域走）。
    // 本文件只留交集 pick-* 的用例（见上）。
}
