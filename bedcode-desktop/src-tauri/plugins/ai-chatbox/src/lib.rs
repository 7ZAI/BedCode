//! AI Chatbox Plugin
//!
//! AI 大模型对话与终端提示词优化
//! Rust 端：提供 optimize-prompt 命令和终端输出处理器
//! 前端：侧边栏 ChatView + 终端工具栏按钮

use bedcode_plugin_api::{
    BedcodePlugin, PluginCommand, PluginManifest, PluginType,
    RustPluginContext,
};
use std::pin::Pin;

/// AI Chatbox 插件
pub struct AiChatboxPlugin;

impl BedcodePlugin for AiChatboxPlugin {
    const ID: &'static str = "com.bedcode.ai-chatbox";

    fn manifest() -> PluginManifest {
        PluginManifest {
            id: "com.bedcode.ai-chatbox".to_string(),
            name: "AI Chatbox".to_string(),
            version: "1.0.0".to_string(),
            description: "AI large model chat and terminal prompt optimization".to_string(),
            author: "BedCode".to_string(),
            main: "index.ts".to_string(),
            sandbox: "inline".to_string(),
            plugin_type: PluginType::RustTs,
            permissions: vec![
                "ui:sidebar".to_string(),
                "ui:input".to_string(),
                "storage".to_string(),
                "terminal:input".to_string(),
                "session:read".to_string(),
            ],
            contributes: bedcode_plugin_api::PluginContributes {
                views: vec![bedcode_plugin_api::ViewContribution {
                    id: "ai-chatbox.sidebar".to_string(),
                    view_type: "sidebar".to_string(),
                    title: "AI Chat".to_string(),
                    component: "ChatView".to_string(),
                }],
                configuration: Some(bedcode_plugin_api::PluginConfiguration {
                    title: "AI Chatbox Settings".to_string(),
                    properties: {
                        let mut props = std::collections::HashMap::new();
                        props.insert(
                            "apiProviders".to_string(),
                            bedcode_plugin_api::ConfigProperty {
                                prop_type: "string".to_string(),
                                title: "API Providers (JSON)".to_string(),
                                description: Some("JSON array of API provider configs".to_string()),
                                default: Some(serde_json::json!("[]")),
                                enum_values: None,
                            },
                        );
                        props.insert(
                            "activeProvider".to_string(),
                            bedcode_plugin_api::ConfigProperty {
                                prop_type: "string".to_string(),
                                title: "Active Provider Name".to_string(),
                                description: None,
                                default: Some(serde_json::json!("")),
                                enum_values: None,
                            },
                        );
                        props
                    },
                }),
                ..Default::default()
            },
        }
    }

    fn activate(_context: RustPluginContext) -> Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send>> {
        Box::pin(async {
            tracing::info!("[AiChatbox] Plugin activated (Rust)");
            Ok(())
        })
    }

    fn deactivate(_context: RustPluginContext) -> Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send>> {
        Box::pin(async {
            tracing::info!("[AiChatbox] Plugin deactivated (Rust)");
            Ok(())
        })
    }

    fn register_commands() -> Vec<PluginCommand> {
        vec![
            PluginCommand::new("optimize-prompt", |args| async move {
                // MVP: 返回传入的参数作为占位
                // 完整实现需要调用 AI API，这里提供框架
                let prompt = args.get("prompt").and_then(|v| v.as_str()).unwrap_or("");
                Ok(serde_json::json!({
                    "original": prompt,
                    "optimized": prompt, // 占位：实际优化逻辑待实现
                }))
            })
            .with_title("Optimize Prompt"),
        ]
    }
}

// 提交静态注册
bedcode_plugin_api::submit_plugin!(AiChatboxPlugin);
