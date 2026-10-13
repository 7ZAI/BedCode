//! 桌面平台选择器（`host-platform.pick-files` / `pick-folders` 的桌面实现）——端差异落宿主
//!
//! 来源：ADR 0052 D1「差异应该在各自的宿主侧」+ 单形态收口（2026-10-13）的后果修正。
//! 内核 `host_api/platform.rs` 只持**共享机制**（`fs:pick` 准入门 + 选择结果
//! `authorize_picked` 授权校验 + JSON 形状），选择器本体是端差异：
//!
//! - 桌面（本模块）：`tauri-plugin-dialog` 原生对话框（Windows IFileDialog /
//!   Linux xdg-desktop-portal / macOS NSOpenPanel）；
//! - 移动：SAF 选择器——宿主 `plugin/host_ports.rs` 的引擎端口经内核
//!   `EnginePortsPlatformPicker` 适配（`WasmHostContext::new_with_ports` 自动接线）。
//!
//! 为什么必须端口化：`FileDialogBuilder::pick_folder` / `pick_folders` 是
//! tauri-plugin-dialog 的 desktop-only API（`#[cfg(desktop)]`），Android 上没有
//! 这两个方法——实现留在内核会让 `aarch64-linux-android` 交叉编译直接失败
//! （真机 build 实测，2026-10-13）。内核零端分支（`#[cfg(feature = ...)]` 也不行：
//! 用户裁决「不一致的有宿主组装实现差异，而不是在 wasm-core 中」）。
//!
//! 装配点：`lib.rs` 构造 `PluginHost::new` 时作为 `platform_picker` 参数注入——
//! 与 `host_facts` 同批（实例化 / 激活**之前**，插件激活期若调 `pick-*` 也必须
//! 拿到宿主实现）。无头 / 测试传 `None`（内核缺省 fail-visible 选择器）。

use bedcode_wasm_core::host_api::ports::PlatformPickerPort;
use tauri_plugin_dialog::DialogExt;

/// 桌面平台选择器（捕获应用句柄；对话框调用需要它）
pub struct DesktopPlatformPicker {
    /// 与 `PluginHost::new` 的 `app_handle` 同源（lib 装配时克隆传入）
    app_handle: tauri::AppHandle,
}

impl DesktopPlatformPicker {
    /// 以应用句柄构造（无头上下文没有句柄，调用点也不会构造本类型）
    pub fn new(app_handle: tauri::AppHandle) -> Self {
        Self { app_handle }
    }
}

#[async_trait::async_trait]
impl PlatformPickerPort for DesktopPlatformPicker {
    async fn pick_files(&self) -> bedcode_wasm_core::Result<Vec<String>> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.app_handle.dialog().file().pick_files(move |selection| {
            if tx.send(selection).is_err() {
                tracing::debug!("platform_pick_files: receiver dropped before dialog completed");
            }
        });
        match rx.await {
            Ok(Some(paths)) => paths.into_iter().map(path_to_string).collect(),
            // 用户取消选择（与「被拒」可区分：空数组，不回错误）
            Ok(None) => Ok(Vec::new()),
            Err(e) => Err(bedcode_wasm_core::AppError::Plugin(format!(
                "platform_pick_files: dialog channel closed: {e}"
            ))),
        }
    }

    async fn pick_folders(&self) -> bedcode_wasm_core::Result<Vec<String>> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.app_handle.dialog().file().pick_folders(move |selection| {
            if tx.send(selection).is_err() {
                tracing::debug!("platform_pick_folders: receiver dropped before dialog completed");
            }
        });
        match rx.await {
            Ok(Some(paths)) => paths.into_iter().map(path_to_string).collect(),
            Ok(None) => Ok(Vec::new()),
            Err(e) => Err(bedcode_wasm_core::AppError::Plugin(format!(
                "platform_pick_folders: dialog channel closed: {e}"
            ))),
        }
    }
}

/// Dialog FilePath → UTF-8 绝对路径串（非 UTF-8 路径显式报错而非静默丢弃）
fn path_to_string(file_path: tauri_plugin_dialog::FilePath) -> bedcode_wasm_core::Result<String> {
    let path = file_path.into_path().map_err(|e| {
        bedcode_wasm_core::AppError::InvalidInput(format!(
            "platform pick: failed to convert selected path: {e}"
        ))
    })?;
    path.to_str().map(|s| s.to_string()).ok_or_else(|| {
        bedcode_wasm_core::AppError::InvalidInput(
            "platform pick: selected path is not valid UTF-8".to_string(),
        )
    })
}
