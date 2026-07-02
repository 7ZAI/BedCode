//! Cdylib Loader
//!
//! cdylib 动态库加载器 — 通过 libloading 加载插件动态库并解析导出符号
//! 支持跨平台：Windows (.dll), macOS (.dylib), Linux (.so)

use libloading::{Library, Symbol};
use std::ffi::c_char;
use std::path::Path;

/// cdylib 插件导出的类型化函数指针
///
/// 所有 cdylib 插件必须提供这 5 个导出符号，否则加载失败
pub struct CdylibExports {
    /// 激活插件，传入 HostContext 供插件调用宿主 API
    pub activate: unsafe extern "C" fn(*const crate::plugin::host_context::HostContext) -> i32,
    /// 停用插件，释放资源
    pub deactivate: unsafe extern "C" fn() -> i32,
    /// 调用插件命令，接收命令名和 JSON 参数，返回 JSON 结果字符串
    pub invoke_command: unsafe extern "C" fn(*const c_char, *const c_char) -> *mut c_char,
    /// 终端输入回调，接收输入文本，返回处理后的文本（或原文本指针表示不修改）
    pub on_terminal_input: unsafe extern "C" fn(*const c_char) -> *mut c_char,
    /// 终端输出回调，接收输出文本，返回处理后的文本（或原文本指针表示不修改）
    pub on_terminal_output: unsafe extern "C" fn(*const c_char) -> *mut c_char,
}

/// 已加载的 cdylib 插件
///
/// 持有 Library 句柄和缓存的导出函数指针，Library 的生命周期
/// 确保函数指针在插件卸载前始终有效
pub struct LoadedCdylibPlugin {
    // Library 句柄必须持有，否则动态库被卸载后函数指针悬空
    #[allow(dead_code)]
    library: Library,
    exports: CdylibExports,
}

impl LoadedCdylibPlugin {
    /// 获取导出函数指针引用
    pub fn exports(&self) -> &CdylibExports {
        &self.exports
    }
}

/// cdylib 动态库加载器
///
/// 通过 libloading 加载插件动态库并解析所有必需的导出符号。
/// 加载前会验证文件名安全性，防止路径遍历攻击
pub struct CdylibLoader;

impl CdylibLoader {
    /// 从插件目录加载 cdylib 插件
    ///
    /// # Arguments
    /// * `plugin_dir` - 插件根目录（包含 plugin.json）
    /// * `rust_library` - plugin.json 中声明的 cdylib 文件名（如 "bedcode_plugin_ai_chatbox.dll"）
    ///
    /// # Security
    /// - rust_library 不得包含路径分隔符（防止路径遍历）
    /// - 实际文件路径相对于 plugin_dir 解析
    ///
    /// # Errors
    /// - 文件名包含路径分隔符时返回 `AppError::Plugin`
    /// - 文件不存在时返回 `AppError::Plugin`
    /// - 动态库加载失败时返回 `AppError::Plugin`
    /// - 缺少必需导出符号时返回 `AppError::Plugin`
    pub fn load(plugin_dir: &Path, rust_library: &str) -> crate::Result<LoadedCdylibPlugin> {
        // 防止路径遍历：文件名不得包含分隔符或父目录引用
        if rust_library.contains('/') || rust_library.contains('\\') || rust_library.contains("..") {
            return Err(crate::AppError::Plugin(format!(
                "Invalid rust_library name '{}': path separators and '..' are not allowed",
                rust_library
            )));
        }

        let full_path = plugin_dir.join(rust_library);

        // 验证文件存在
        if !full_path.exists() {
            return Err(crate::AppError::Plugin(format!(
                "Plugin library not found: {}",
                full_path.display()
            )));
        }

        // 加载动态库
        let library = unsafe {
            Library::new(&full_path).map_err(|e| {
                crate::AppError::Plugin(format!(
                    "Failed to load library '{}': {}",
                    full_path.display(),
                    e
                ))
            })?
        };

        // 解析所有导出符号
        let exports = unsafe { load_exports(&library)? };

        Ok(LoadedCdylibPlugin { library, exports })
    }
}

/// 从已加载的动态库中解析所有必需的导出符号
///
/// # Safety
/// 调用者必须确保 library 在返回的函数指针使用期间保持存活
unsafe fn load_exports(library: &Library) -> crate::Result<CdylibExports> {
    let activate: Symbol<'_, unsafe extern "C" fn(*const crate::plugin::host_context::HostContext) -> i32> =
        load_symbol(library, b"bedcode_plugin_activate")?;
    let deactivate: Symbol<'_, unsafe extern "C" fn() -> i32> =
        load_symbol(library, b"bedcode_plugin_deactivate")?;
    let invoke_command: Symbol<'_, unsafe extern "C" fn(*const c_char, *const c_char) -> *mut c_char> =
        load_symbol(library, b"bedcode_plugin_invoke_command")?;
    let on_terminal_input: Symbol<'_, unsafe extern "C" fn(*const c_char) -> *mut c_char> =
        load_symbol(library, b"bedcode_plugin_on_terminal_input")?;
    let on_terminal_output: Symbol<'_, unsafe extern "C" fn(*const c_char) -> *mut c_char> =
        load_symbol(library, b"bedcode_plugin_on_terminal_output")?;

    // 将 Symbol 解引用为函数指针 — Library 的生命周期由 LoadedCdylibPlugin.library 保证
    Ok(CdylibExports {
        activate: *activate,
        deactivate: *deactivate,
        invoke_command: *invoke_command,
        on_terminal_input: *on_terminal_input,
        on_terminal_output: *on_terminal_output,
    })
}

/// 从动态库中加载单个导出符号
///
/// # Safety
/// 调用者必须确保 library 在返回的函数指针使用期间保持存活
unsafe fn load_symbol<'lib, T>(
    library: &'lib Library,
    symbol_name: &[u8],
) -> crate::Result<Symbol<'lib, T>> {
    library.get(symbol_name).map_err(|e| {
        crate::AppError::Plugin(format!(
            "Missing required export symbol '{}': {}",
            String::from_utf8_lossy(symbol_name),
            e
        ))
    })
}
