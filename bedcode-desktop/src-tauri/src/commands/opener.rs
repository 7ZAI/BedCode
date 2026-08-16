//! Opener Commands
//!
//! 插件系统文件操作桥接：在系统文件管理器中打开文件所在目录并选中目标文件
//! （传输完成后「打开目录」）。命令双重校验（与 fileservice 命令同模式）：
//! 插件处于 Activated 状态 + manifest 声明 system:open 权限。

use crate::plugin::host::PluginHost;
use bedcode_plugin_api::permission::PERMISSION_SYSTEM_OPEN;
use std::sync::Arc;
use tauri::State;

/// 校验插件身份与 system:open 权限
async fn require_system_open(
    plugin_host: &PluginHost,
    plugin_id: &str,
    op: &str,
) -> crate::Result<()> {
    if !plugin_host.is_activated(plugin_id).await {
        return Err(crate::AppError::Plugin(format!(
            "{}: plugin '{}' is not activated",
            op, plugin_id
        )));
    }
    if !plugin_host
        .permission()
        .check(plugin_id, PERMISSION_SYSTEM_OPEN)
    {
        return Err(crate::AppError::Plugin(format!(
            "{}: plugin '{}' has no system:open permission",
            op, plugin_id
        )));
    }
    Ok(())
}

/// 在系统文件管理器中打开所在目录并选中目标文件/目录
///
/// - Windows：Shell COM API（`SHOpenFolderAndSelectItems`）定位选中文件——
///   不用 `explorer /select,<path>` 命令行：explorer 的参数解析器非标准
///   （逗号当分隔符、不识别 Command 序列化后的外层引号 + `\"` 转义），路径
///   解析失败会退化为打开默认位置（桌面/快速访问）并选中一个无关文件夹，
///   Win11 实测复现。COM 直接以 PIDL 操作 Shell，彻底绕开命令行解析。
///   目录输入则 `ShellExecuteExW`（explore verb）打开目录视图。
/// - macOS：`open -R <path>`（Finder 定位选中）
/// - Linux：`xdg-open` 打开所在目录（无 reveal 语义，退化为打开目录）
#[tauri::command]
pub async fn plugin_reveal_in_dir(
    plugin_id: String,
    path: String,
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<()> {
    // 诊断：点击「打开目录」时打印插件传入的原始路径（排查定位失败/打开错位置）
    tracing::info!(plugin_id = %plugin_id, path = %path, "reveal_in_dir requested");
    require_system_open(&plugin_host, &plugin_id, "plugin_reveal_in_dir").await?;

    // 兼容历史 wasm 产物：旧版插件曾用 POSIX 语义 PathBuf 拼出 `\\?\` verbatim
    // 前缀 + 混合分隔符路径（如 `\\?\D:\下载/file.mkv`），Windows 下
    // exists/canonicalize 直接报 os error 123。先剥 verbatim 前缀（纯正斜杠/
    // 混合分隔符均为宿主 API 接受，canonicalize 会还原原生形态）。
    let path = std::path::PathBuf::from(path.strip_prefix(r"\\?\").unwrap_or(&path));
    if !path.exists() {
        return Err(crate::AppError::NotFound(format!(
            "reveal: path not found: {}",
            path.display()
        )));
    }

    let result = reveal_in_dir_platform(&path);
    match result {
        Ok(()) => Ok(()),
        Err(e) => Err(crate::AppError::Internal(format!(
            "reveal: failed to open '{}': {}",
            path.display(),
            e
        ))),
    }
}

/// 平台分发：仅目标平台分支参与编译（避免未使用函数告警）
#[cfg(target_os = "windows")]
fn reveal_in_dir_platform(path: &std::path::Path) -> std::io::Result<()> {
    use windows_sys::Win32::{
        Foundation::ERROR_FILE_NOT_FOUND,
        System::Com::CoInitialize,
        UI::{
            Shell::{
                Common::ITEMIDLIST, ILCreateFromPathW, ILFree, SHOpenFolderAndSelectItems,
            },
        },
    };

    // 目录输入：直接打开目录视图（explore verb），无选中语义
    if path.is_dir() {
        return shell_execute_explore(path);
    }

    unsafe {
        // 进程级 COM 初始化（幂等；与 tauri-plugin-opener 同款，不做 CoUninitialize
        // 配对——Shell API 在 Tauri 进程生命周期内反复使用，引用计数无碍）
        let _ = CoInitialize(std::ptr::null());

        // 父目录 + 目标文件的 ITEMIDLIST（以宽字符路径直接构造，规避一切
        // 命令行转义/编码问题，中文路径原生支持）
        let dir = path.parent().unwrap_or(path);
        let dir_wide = to_wide(dir);
        let file_wide = to_wide(path);
        let dir_item = ILCreateFromPathW(dir_wide.as_ptr());
        let file_item = ILCreateFromPathW(file_wide.as_ptr());

        let hr = if dir_item.is_null() || file_item.is_null() {
            if !dir_item.is_null() {
                ILFree(dir_item);
            }
            if !file_item.is_null() {
                ILFree(file_item);
            }
            // PIDL 构造失败（非常规文件系统路径）：直接退化为打开目录
            return shell_execute_explore(dir);
        } else {
            let hr = SHOpenFolderAndSelectItems(
                dir_item,
                1,
                std::ptr::addr_of!(file_item) as *const *const ITEMIDLIST,
                0,
            );
            ILFree(dir_item);
            ILFree(file_item);
            hr
        };

        // 已知坑（tauri-plugin-opener 同款注释）：部分系统 SHOpenFolderAndSelectItems
        // 对存在文件仍报 ERROR_FILE_NOT_FOUND，此时 ShellExecuteExW 打开目录兜底
        // （能进目录但不再选中文件）。HRESULT 为 0x8007xxxx 形态，低 16 位即 Win32 码
        if (hr & 0xFFFF) as u32 == ERROR_FILE_NOT_FOUND {
            return shell_execute_explore(dir);
        }
        if hr != 0 {
            return Err(std::io::Error::from_raw_os_error(hr));
        }
        Ok(())
    }
}

/// Windows：ShellExecuteExW 打开目录视图（explore verb）
#[cfg(target_os = "windows")]
fn shell_execute_explore(dir: &std::path::Path) -> std::io::Result<()> {
    use windows_sys::Win32::UI::{
        Shell::{ShellExecuteExW, SHELLEXECUTEINFOW, SHELLEXECUTEINFOW_0},
        WindowsAndMessaging::SW_SHOWNORMAL,
    };

    let dir_wide = to_wide(dir);
    let verb: [u16; 8] = [
        0x65, 0x78, 0x70, 0x6c, 0x6f, 0x72, 0x65, 0,
    ]; // "explore\0"
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: 0,
        hwnd: std::ptr::null_mut(),
        lpVerb: verb.as_ptr(),
        lpFile: dir_wide.as_ptr(),
        lpParameters: std::ptr::null(),
        lpDirectory: std::ptr::null(),
        nShow: SW_SHOWNORMAL,
        hInstApp: std::ptr::null_mut(),
        lpIDList: std::ptr::null_mut(),
        lpClass: std::ptr::null(),
        hkeyClass: std::ptr::null_mut(),
        dwHotKey: 0,
        // hIcon / hMonitor 共用 union（windows-sys 0.61 以 Anonymous 呈现）
        Anonymous: SHELLEXECUTEINFOW_0 {
            hIcon: std::ptr::null_mut(),
        },
        hProcess: std::ptr::null_mut(),
    };
    let ok = unsafe { ShellExecuteExW(&mut info) };
    if ok == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// UTF-16 宽字符路径（NUL 结尾，供 *W Shell API 直接使用）
#[cfg(target_os = "windows")]
fn to_wide(p: &std::path::Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    p.as_os_str().encode_wide().chain(std::iter::once(0)).collect()
}

/// macOS：Finder 定位选中（`open -R`）
#[cfg(target_os = "macos")]
fn reveal_in_dir_platform(path: &std::path::Path) -> std::io::Result<()> {
    std::process::Command::new("open").arg("-R").arg(path).spawn().map(|_| ())
}

/// Linux：打开所在目录（xdg-open 无 reveal 语义）
#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn reveal_in_dir_platform(path: &std::path::Path) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or(path);
    std::process::Command::new("xdg-open").arg(dir).spawn().map(|_| ())
}
