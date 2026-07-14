//! Mobile Plugin Host
//!
//! 调用 Rust 插件的 trait 实现（通过 inventory 静态注册）

use crate::plugin::registry::{MobilePlugin, PluginHostContext};
use crate::Result;

/// Rust 插件宿主
pub struct PluginHost {
    plugins: Vec<Box<dyn MobilePlugin>>,
}

impl PluginHost {
    /// 创建空宿主（无 Rust 插件时使用）
    pub fn empty() -> Self {
        Self { plugins: vec![] }
    }

    /// 激活指定 Rust 插件
    pub fn activate(&self, plugin_id: &str, ctx: &PluginHostContext) -> Result<()> {
        for plugin in &self.plugins {
            if plugin.id() == plugin_id {
                return plugin.activate(ctx);
            }
        }
        // 非 Rust 插件或未找到，不算错误
        Ok(())
    }

    /// 停用指定 Rust 插件
    pub fn deactivate(&self, plugin_id: &str) -> Result<()> {
        for plugin in &self.plugins {
            if plugin.id() == plugin_id {
                return plugin.deactivate();
            }
        }
        Ok(())
    }

    /// 获取所有 Rust 插件 ID
    pub fn plugin_ids(&self) -> Vec<String> {
        self.plugins.iter().map(|p| p.id().to_string()).collect()
    }
}
