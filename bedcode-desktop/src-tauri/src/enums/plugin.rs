//! Plugin Types
//!
//! 插件相关共享类型 — 定义已迁移到 SDK `bedcode-plugin-api`（单一事实来源），
//! 此处 re-export 保持宿主侧导入路径不变

pub use bedcode_plugin_api::events::{PluginQuestion, PluginQuestionOption};
