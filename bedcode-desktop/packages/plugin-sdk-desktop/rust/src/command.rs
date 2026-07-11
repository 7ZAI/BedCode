//! Plugin Command
//!
//! 插件自定义 Tauri command 描述 — Rust 插件通过此类型注册命令处理器

use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// 命令处理函数类型
pub type CommandHandlerFn = Arc<
    dyn Fn(serde_json::Value) -> Pin<Box<dyn Future<Output = anyhow::Result<serde_json::Value>> + Send>>
        + Send
        + Sync,
>;

/// 插件自定义 command 描述
#[derive(Clone)]
pub struct PluginCommand {
    /// command 名称（不含插件 ID 前缀，运行时会自动添加 plugin_id 命名空间）
    pub name: String,
    /// 命令标题（用于 UI 展示）
    pub title: String,
    /// 异步处理函数
    pub handler: CommandHandlerFn,
}

impl PluginCommand {
    /// 创建新的 PluginCommand
    pub fn new<F, Fut>(name: impl Into<String>, handler: F) -> Self
    where
        F: Fn(serde_json::Value) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = anyhow::Result<serde_json::Value>> + Send + 'static,
    {
        Self {
            name: name.into(),
            title: String::new(),
            handler: Arc::new(move |args| Box::pin(handler(args))),
        }
    }

    /// 设置命令标题
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }
}

/// 插件 command 注册条目（运行时存储）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginCommandEntry {
    pub plugin_id: String,
    pub command_name: String,
    pub title: String,
}
