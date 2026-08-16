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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::block_on;

    #[test]
    fn test_new_and_with_title() {
        let cmd = PluginCommand::new("echo", |_args: serde_json::Value| async move {
            Ok(serde_json::json!({ "ok": true }))
        });
        assert_eq!(cmd.name, "echo");
        // new 的 title 默认为空，with_title 链式覆盖
        assert_eq!(cmd.title, "");
        let titled = cmd.with_title("Echo Command");
        assert_eq!(titled.title, "Echo Command");
        assert_eq!(titled.name, "echo");
    }

    #[test]
    fn test_handler_invocation_round_trip() {
        // 处理器是 Arc<dyn Fn>，可被宿主跨线程调用；验证参数透传与返回值
        let cmd = PluginCommand::new("echo", |args: serde_json::Value| async move {
            Ok(serde_json::json!({ "echo": args }))
        });
        let out = block_on((cmd.handler)(serde_json::json!({ "v": 1 }))).unwrap();
        assert_eq!(out, serde_json::json!({ "echo": { "v": 1 } }));
    }

    #[test]
    fn test_handler_error_propagates() {
        let cmd = PluginCommand::new("fail", |_args: serde_json::Value| async move {
            anyhow::bail!("boom")
        });
        let err = block_on((cmd.handler)(serde_json::Value::Null)).unwrap_err();
        assert_eq!(err.to_string(), "boom");
    }

    #[test]
    fn test_entry_serde_camel_case() {
        // 运行时条目走 JSON 序列化（Tauri 命令返回前端），字段名 camelCase 是线协议
        let entry = PluginCommandEntry {
            plugin_id: "com.bedcode.demo".to_string(),
            command_name: "run".to_string(),
            title: "Run".to_string(),
        };
        let json = serde_json::to_value(&entry).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "pluginId": "com.bedcode.demo",
                "commandName": "run",
                "title": "Run"
            })
        );
        // 前端按 camelCase 构造回传时应能反序列化
        let back: PluginCommandEntry = serde_json::from_value(json).unwrap();
        assert_eq!(back.plugin_id, "com.bedcode.demo");
        assert_eq!(back.command_name, "run");
    }
}
