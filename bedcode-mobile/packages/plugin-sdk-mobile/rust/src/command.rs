//! Plugin Command (Mobile)
//!
//! 插件自定义 command 描述

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
    pub name: String,
    pub title: String,
    pub handler: CommandHandlerFn,
}

impl PluginCommand {
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

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }
}

/// 插件 command 注册条目
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginCommandEntry {
    pub plugin_id: String,
    pub command_name: String,
    pub title: String,
}
