//! Plugin module entry
//!
//! 插件会话管理模块入口

pub mod manager;
pub mod jsonl;
pub mod setup;

pub use self::manager::{PluginManager, PluginSessionState, PluginSessionStatus};
pub use self::jsonl::{ClaudeEntry, MessageContent, ContentBlock, FormattedOutput, read_new_lines};
pub use setup::PluginSetupResult;