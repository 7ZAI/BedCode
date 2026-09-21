//! `register` — PluginHost 职责面拆分（P2）
//!
//! 自 `host.rs` 主 impl 拆出；经 `use super::*` 继承 host 模块的
//! 全部导入与常量，方法与字段可见性规则与内联时一致。

use super::*;
impl PluginHost {
    /// 将所有已加载插件的 manifest contributes 注册到 registry
    pub(crate) async fn register_manifest_contributions(&self) {
        let plugins = self.plugins.read().await;
        for loaded in plugins.values() {
            let m = &loaded.manifest;
            self.registry.register_commands(&m.id, &m.contributes.commands).await;
            self.registry.register_views(&m.id, &m.contributes.views).await;
            if let Some(ref term) = m.contributes.terminal {
                self.registry
                    .register_terminal_handlers(&m.id, &term.input_handlers, &term.output_parsers)
                    .await;
            }
            self.registry
                .register_tool_providers(&m.id, &m.contributes.tool_providers)
                .await;
            self.registry
                .register_http_endpoints(&m.id, &m.contributes.http_endpoints)
                .await;
            self.registry
                .register_file_handlers(&m.id, &m.contributes.file_handlers)
                .await;
        }
    }

    /// 注册 Rust 插件的 command handlers 到运行时注册表（inventory 静态注册）

    /// 注册 Rust 插件的 command handlers 到运行时注册表（inventory 静态注册）
    pub(crate) async fn register_rust_command_handlers(&self) {
        let static_plugins: Vec<&'static bedcode_plugin_api::BedcodePluginEntry> =
            inventory::iter::<bedcode_plugin_api::BedcodePluginEntry>
                .into_iter()
                .collect();

        let mut handlers = self.rust_command_handlers.write().await;
        for entry in static_plugins {
            let commands = (entry.register_commands)();
            let plugin_id = entry.id;
            for cmd in commands {
                let full_name = format!("{}::{}", plugin_id, cmd.name);
                tracing::info!("Registered Rust command: {}", full_name);
                handlers.insert(full_name, cmd);
            }
        }
    }

    /// 注册 Rust 插件的 terminal handlers 到运行时注册表（inventory 静态注册）

    /// 注册 Rust 插件的 terminal handlers 到运行时注册表（inventory 静态注册）
    pub(crate) async fn register_rust_terminal_handlers(&self) {
        let static_plugins: Vec<&'static bedcode_plugin_api::BedcodePluginEntry> =
            inventory::iter::<bedcode_plugin_api::BedcodePluginEntry>
                .into_iter()
                .collect();

        let mut handlers = self.rust_terminal_handlers.write().await;
        for entry in static_plugins {
            let plugin_handlers = (entry.terminal_handlers)();
            for handler in plugin_handlers {
                tracing::info!(plugin_id = %entry.id, "Registered Rust terminal handler");
                handlers.push(handler);
            }
        }
    }

    // ==================== Accessors ====================

    /// 获取 WASM 宿主上下文引用

    /// 注册单个插件的 manifest contributes 扩展点（install_from_zip 用，dev 合入）
    pub(crate) async fn register_plugin_contributions(&self, plugin_id: &str) {
        let loaded = self.plugins.read().await.get(plugin_id).cloned();
        let Some(loaded) = loaded else {
            return;
        };
        let m = &loaded.manifest;
        self.registry.register_commands(&m.id, &m.contributes.commands).await;
        self.registry.register_views(&m.id, &m.contributes.views).await;
        if let Some(ref term) = m.contributes.terminal {
            self.registry
                .register_terminal_handlers(&m.id, &term.input_handlers, &term.output_parsers)
                .await;
        }
        self.registry
            .register_tool_providers(&m.id, &m.contributes.tool_providers)
            .await;
        self.registry
            .register_http_endpoints(&m.id, &m.contributes.http_endpoints)
            .await;
        self.registry
            .register_file_handlers(&m.id, &m.contributes.file_handlers)
            .await;
    }
}
