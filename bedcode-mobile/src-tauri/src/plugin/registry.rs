//! Mobile Plugin Registry
//!
//! 内置插件清单（已迁移到 APK assets 动态加载，此模块保留空实现）

use crate::plugin::types::*;

/// 获取所有内置插件的 manifest
///
/// 内置插件已迁移到 APK assets 动态加载，此函数返回空列表
/// 保留函数签名以兼容现有调用点
pub fn builtin_manifests() -> Vec<PluginManifest> {
    vec![]
}
