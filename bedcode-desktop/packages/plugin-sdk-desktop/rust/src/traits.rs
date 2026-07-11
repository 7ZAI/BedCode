//! Plugin Traits
//!
//! BedcodePlugin trait — 插件核心接口
//! BedcodePluginEntry — inventory 提交类型
//!
//! 使用 `submit_plugin!` 宏简化插件注册

use crate::command::PluginCommand;
use crate::context::RustPluginContext;
use crate::terminal::TerminalHandler;
use crate::types::PluginManifest;
use std::future::Future;
use std::pin::Pin;

/// 插件核心 trait
///
/// 所有 Rust 插件必须实现此 trait，并通过 `submit_plugin!` 宏提交注册。
/// 主应用通过 `inventory::iter()` 收集所有静态注册的插件。
pub trait BedcodePlugin: Send + Sync + 'static {
    /// 插件唯一标识（反向域名格式，如 com.bedcode.quick-snippets）
    const ID: &'static str;

    /// 返回插件 manifest
    fn manifest() -> PluginManifest;

    /// 激活插件
    fn activate(context: RustPluginContext) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>>;

    /// 停用插件（可选，默认为空操作）
    fn deactivate(_context: RustPluginContext) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>> {
        Box::pin(async { Ok(()) })
    }

    /// 注册自定义 Tauri commands（可选）
    fn register_commands() -> Vec<PluginCommand> {
        vec![]
    }

    /// 注册终端处理器（可选）
    fn terminal_handlers() -> Vec<Box<dyn TerminalHandler>> {
        vec![]
    }

    /// 应用启动完成回调（可选，默认为空操作）
    ///
    /// 在所有核心服务初始化完成后触发，插件可在此执行启动后的初始化逻辑。
    fn on_startup() -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(async {})
    }

    /// 应用即将关闭回调（可选，默认为空操作）
    ///
    /// 在插件 deactivate 之前触发，用于执行插件特定的清理逻辑。
    /// 此时插件仍处于激活状态，权限和注册表条目仍可用。
    fn on_shutdown() -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(async {})
    }
}

/// inventory 提交类型
pub struct BedcodePluginEntry {
    pub id: &'static str,
    pub create_manifest: fn() -> PluginManifest,
    pub activate: fn(RustPluginContext) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>>,
    pub deactivate: fn(RustPluginContext) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>>,
    pub register_commands: fn() -> Vec<PluginCommand>,
    pub terminal_handlers: fn() -> Vec<Box<dyn TerminalHandler>>,
    pub on_startup: fn() -> Pin<Box<dyn Future<Output = ()> + Send>>,
    pub on_shutdown: fn() -> Pin<Box<dyn Future<Output = ()> + Send>>,
}

// inventory crate 需要的 submit! 宏目标类型
inventory::collect!(BedcodePluginEntry);

/// 提交插件注册的宏
///
/// 用法：
/// ```ignore
/// submit_plugin!(MyPlugin);
/// ```
#[macro_export]
macro_rules! submit_plugin {
    ($plugin_type:ty) => {
        inventory::submit! {
            bedcode_plugin_api::BedcodePluginEntry {
                id: <$plugin_type>::ID,
                create_manifest: <$plugin_type>::manifest,
                activate: <$plugin_type>::activate,
                deactivate: <$plugin_type>::deactivate,
                register_commands: <$plugin_type>::register_commands,
                terminal_handlers: <$plugin_type>::terminal_handlers,
                on_startup: <$plugin_type>::on_startup,
                on_shutdown: <$plugin_type>::on_shutdown,
            }
        }
    };
}
