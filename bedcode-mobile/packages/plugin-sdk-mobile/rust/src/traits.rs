//! Plugin Traits (Mobile)
//!
//! BedcodePlugin trait — 插件核心接口
//! BedcodePluginEntry — inventory 提交类型

use crate::command::PluginCommand;
use crate::context::RustPluginContext;
use crate::terminal::TerminalHandler;
use crate::types::PluginManifest;
use std::future::Future;
use std::pin::Pin;

/// 插件核心 trait
pub trait BedcodePlugin: Send + Sync + 'static {
    const ID: &'static str;
    fn manifest() -> PluginManifest;
    fn activate(context: RustPluginContext) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>>;
    fn deactivate(_context: RustPluginContext) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>> {
        Box::pin(async { Ok(()) })
    }
    fn register_commands() -> Vec<PluginCommand> { vec![] }
    fn terminal_handlers() -> Vec<Box<dyn TerminalHandler>> { vec![] }
    fn on_startup() -> Pin<Box<dyn Future<Output = ()> + Send>> { Box::pin(async {}) }
    fn on_shutdown() -> Pin<Box<dyn Future<Output = ()> + Send>> { Box::pin(async {}) }
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

inventory::collect!(BedcodePluginEntry);

/// 提交插件注册的宏
#[macro_export]
macro_rules! submit_plugin {
    ($plugin_type:ty) => {
        inventory::submit! {
            bedcode_plugin_api_mobile::BedcodePluginEntry {
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
