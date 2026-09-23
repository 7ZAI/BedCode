//! `register` — PluginHost 职责面拆分（P2）
//!
//! 自 `host.rs` 主 impl 拆出；经 `use super::*` 继承 host 模块的
//! 全部导入与常量，方法与字段可见性规则与内联时一致。

use super::*;
impl PluginHost {
    /// 将所有已加载插件的 manifest contributes 注册到 registry
    ///
    /// 只负责取 id 清单，逐个委派给 [`Self::register_plugin_contributions`]——**单条注册
    /// 是唯一实现**（票 11 第 3 项）：启动期全量 / zip 安装单条 / 热重载三条路径共用它，
    /// 避免同一份六项注册被手抄三遍（历史上一次 contributes 演进要同改三处）。
    ///
    /// 读数与注册分离：id 清单在读锁内收集后立即释放，注册本身 `await`（还会回调宿主），
    /// 不在持锁期间做 I/O。
    pub(crate) async fn register_manifest_contributions(&self) {
        let plugin_ids: Vec<String> = {
            let plugins = self.plugins.read().await;
            plugins.keys().cloned().collect()
        };
        for id in plugin_ids {
            self.register_plugin_contributions(&id).await;
        }
    }

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

    // ==================== manifests 扩展点注册 ====================

    /// 注册单个插件的 manifest contributes 扩展点（唯一实现：启动期 / 安装 / 热重载共用）
    ///
    /// 覆盖六项：commands / views / terminal handlers / tool providers / http endpoints /
    /// file handlers。**刻意不覆盖** `configuration` / `lifecycle`（boot 期装配）与
    /// `provides` / `subscribes`（激活期走总线订阅，见 `host/activation.rs`）——在未激活时
    /// 注册这两项会造成「未激活即订阅」。
    ///
    /// 幂等：registry 侧按 plugin_id 存放，重复注册结果一致（[`Self::register_manifest_contributions`]
    /// 与热重载都会重复走到这里）。插件不在表中时静默返回（卸载竞态）。
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
