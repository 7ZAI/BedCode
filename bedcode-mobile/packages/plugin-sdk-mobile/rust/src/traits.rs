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

#[cfg(test)]
mod tests {
    // 宏展开引用 `bedcode_plugin_api_mobile::`（插件 crate 视角的外部依赖名），
    // SDK 自身测试用 self 别名让该路径指向本 crate
    extern crate self as bedcode_plugin_api_mobile;

    use super::*;
    use crate::test_utils::{block_on, MockBusAccess, MockEventEmitter, MockFileAccess, MockSessionQuery, MockStorage};
    use crate::types::{PluginContributes, PluginType};
    use crate::PermissionManager;
    use std::sync::Arc;

    /// 最小测试插件：走 submit_plugin! 宏注册，验证 inventory 收集链路
    struct TestPlugin;

    impl BedcodePlugin for TestPlugin {
        const ID: &'static str = "com.bedcode.test-plugin";

        fn manifest() -> PluginManifest {
            PluginManifest {
                id: Self::ID.to_string(),
                name: "Test Plugin".to_string(),
                version: "0.1.0".to_string(),
                description: String::new(),
                author: String::new(),
                main: String::new(),
                plugin_type: PluginType::Rust,
                permissions: vec!["storage".to_string()],
                contributes: PluginContributes::default(),
                icon: None,
                wasm_hash: String::new(),
                rust_library: String::new(),
            }
        }

        fn activate(_context: RustPluginContext) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>> {
            Box::pin(async { Ok(()) })
        }
    }

    // 宏注册发生在模块初始化期（inventory::submit!），测试体内即可见
    submit_plugin!(TestPlugin);

    #[test]
    fn test_inventory_collects_registered_plugin() {
        // 宿主通过 inventory::iter 收集全部静态注册插件 —— 注册即契约
        let ids: Vec<&str> =
            inventory::iter::<BedcodePluginEntry>().map(|e| e.id).collect();
        assert!(
            ids.contains(&"com.bedcode.test-plugin"),
            "registered plugin not found in inventory: {:?}",
            ids
        );
    }

    #[test]
    fn test_entry_function_pointers_and_defaults() {
        let entry = inventory::iter::<BedcodePluginEntry>()
            .find(|e| e.id == "com.bedcode.test-plugin")
            .expect("entry missing");
        // manifest 经函数指针重建，字段与插件定义一致
        let manifest = (entry.create_manifest)();
        assert_eq!(manifest.id, "com.bedcode.test-plugin");
        assert_eq!(manifest.plugin_type, PluginType::Rust);
        // 未覆盖的扩展点走 trait 默认实现：空命令表/空终端处理器
        assert!((entry.register_commands)().is_empty());
        assert!((entry.terminal_handlers)().is_empty());
        // 默认启动/关闭回调可直接执行（async 无等待点）
        block_on((entry.on_startup)());
        block_on((entry.on_shutdown)());
    }

    #[test]
    fn test_default_deactivate_succeeds() {
        // 未覆盖 deactivate 时默认返回 Ok(()) —— 停用不得报错中断
        let entry = inventory::iter::<BedcodePluginEntry>()
            .find(|e| e.id == "com.bedcode.test-plugin")
            .expect("entry missing");
        let pm = Arc::new(PermissionManager::new());
        let ctx = RustPluginContext::new(
            "com.bedcode.test-plugin".into(),
            Arc::new(MockStorage::default()),
            Arc::new(MockSessionQuery),
            Arc::new(MockEventEmitter::default()),
            pm,
            std::collections::HashSet::new(),
            Arc::new(MockFileAccess::default()),
            Arc::new(MockBusAccess::default()),
        );
        block_on((entry.deactivate)(ctx)).unwrap();
    }
}
