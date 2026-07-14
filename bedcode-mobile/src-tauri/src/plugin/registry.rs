//! Mobile Plugin Registry
//!
//! 管理所有内置插件清单，以及 Rust 插件通过 inventory 的静态注册

use crate::plugin::types::*;

/// Rust 插件 trait — 纯 Rust 插件或 rust-ts 插件的 Rust 层需实现此 trait
pub trait MobilePlugin: Send + Sync {
    /// 插件唯一 ID
    fn id(&self) -> &str;

    /// 激活插件
    fn activate(&self, context: &PluginHostContext) -> crate::Result<()>;

    /// 停用插件
    fn deactivate(&self) -> crate::Result<()>;
}

/// 传递给 Rust 插件的宿主上下文
pub struct PluginHostContext {
    pub app_handle: tauri::AppHandle,
}

/// 获取所有内置插件的 manifest
///
/// 内置插件的元数据在编译期确定，无需从文件系统读取
pub fn builtin_manifests() -> Vec<PluginManifest> {
    vec![
        // 新增内置插件在此添加
        // 示例（暂时注释，待实际插件开发时启用）：
        // PluginManifest {
        //     id: "com.bedcode.ai-chatbox".to_string(),
        //     name: "AI Chatbox".to_string(),
        //     version: "1.0.0".to_string(),
        //     description: "AI 对话助手".to_string(),
        //     author: "BedCode".to_string(),
        //     main: "plugins/com.bedcode.ai-chatbox/index.js".to_string(),
        //     plugin_type: PluginType::TsOnly,
        //     permissions: vec!["terminal:input".to_string(), "terminal:output".to_string()],
        //     contributes: PluginContributes {
        //         views: vec![ViewContribution {
        //             id: "chatbox-toolbox".to_string(),
        //             view_type: "toolbox".to_string(),
        //             title: "AI Chat".to_string(),
        //             component: "ChatPanel".to_string(),
        //         }],
        //         terminal: Some(TerminalContribution {
        //             input_handlers: vec![],
        //             output_parsers: vec![],
        //             toolbar_items: vec![TerminalToolbarItemContribution {
        //                 id: "chatbox-toolbar".to_string(),
        //                 title: "AI Chat".to_string(),
        //                 icon: "chat".to_string(),
        //             }],
        //         }),
        //         ..Default::default()
        //     },
        // },
    ]
}
