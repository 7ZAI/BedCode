//! Plugin Types (Desktop)
//!
//! 桌面端插件类型 — 仅保留桌面端特有的内部模型
//! 共享类型（PluginManifest, PluginContributes, PluginState 等）迁移到 bedcode-plugin-api

use bedcode_plugin_api::{
    PluginContributes, PluginManifest, PluginState, PluginType,
};
use chrono::{DateTime, Utc};
use std::collections::HashSet;
use std::path::Path;

/// 已加载插件的内部表示
pub struct LoadedPlugin {
    pub manifest: PluginManifest,
    pub state: PluginState,
    pub granted_permissions: HashSet<String>,
    pub extension_path: String,
    pub activated_at: Option<DateTime<Utc>>,
    /// 插件来源：静态注册或文件扫描
    pub source: PluginSource,
}

/// 插件来源
#[derive(Debug, Clone, PartialEq)]
pub enum PluginSource {
    /// 静态注册的 Rust 插件（通过 inventory::collect）
    StaticRegistry,
    /// 文件系统扫描的 TS-only 插件
    FileScan,
    /// WASM 模块加载的 Rust+TS 插件
    Wasm,
}

impl PluginSource {
    /// 序列化为前端友好字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::StaticRegistry => "builtin",
            Self::FileScan => "scanned",
            Self::Wasm => "wasm",
        }
    }
}

/// 插件信息（返回给前端的精简版本）
///
/// 从 bedcode_plugin_api::PluginInfo 扩展，添加桌面端特有字段
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopPluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub main: String,
    pub sandbox: String,
    pub plugin_type: PluginType,
    /// WASM 模块文件名（仅 rust-ts 类型插件使用）
    pub rust_library: String,
    pub permissions: Vec<String>,
    pub state: PluginState,
    pub extension_path: String,
    pub contributes: PluginContributes,
    /// 插件图标（manifest.icon 透传，可为空）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// 插件来源
    pub source: String,
    /// 插件目录总大小（字节）
    pub size_bytes: u64,
    /// 安装时间（unix 毫秒，plugin.json mtime）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed_at: Option<i64>,
}

impl From<&LoadedPlugin> for DesktopPluginInfo {
    fn from(p: &LoadedPlugin) -> Self {
        DesktopPluginInfo {
            id: p.manifest.id.clone(),
            name: p.manifest.name.clone(),
            version: p.manifest.version.clone(),
            description: p.manifest.description.clone(),
            author: p.manifest.author.clone(),
            main: p.manifest.main.clone(),
            sandbox: p.manifest.sandbox.clone(),
            plugin_type: p.manifest.plugin_type.clone(),
            rust_library: p.manifest.rust_library.clone(),
            permissions: p.manifest.permissions.clone(),
            state: p.state.clone(),
            extension_path: p.extension_path.clone(),
            contributes: p.manifest.contributes.clone(),
            icon: p.manifest.icon.clone(),
            source: p.source.as_str().to_string(),
            size_bytes: dir_size(Path::new(&p.extension_path)),
            installed_at: manifest_installed_at(&p.extension_path),
        }
    }
}

/// 递归计算目录总大小（字节），路径不存在或不可读时返回 0
fn dir_size(path: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            match entry.file_type() {
                Ok(ft) if ft.is_file() => {
                    total += entry.metadata().map(|m| m.len()).unwrap_or(0);
                }
                Ok(ft) if ft.is_dir() => {
                    total += dir_size(&entry.path());
                }
                _ => {}
            }
        }
    }
    total
}

/// 以 plugin.json 的 mtime 近似安装时间（unix 毫秒）
fn manifest_installed_at(extension_path: &str) -> Option<i64> {
    let manifest_path = Path::new(extension_path).join("plugin.json");
    std::fs::metadata(manifest_path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| {
            t.duration_since(std::time::UNIX_EPOCH)
                .ok()
                .map(|d| d.as_millis() as i64)
        })
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use bedcode_plugin_api::{PluginContributes, PluginManifest, PluginState, PluginType};

    /// 构造带完整字段的测试 manifest
    fn sample_manifest() -> PluginManifest {
        PluginManifest {
            id: "com.bedcode.test".to_string(),
            name: "Test Plugin".to_string(),
            version: "1.2.3".to_string(),
            description: "descriptive text".to_string(),
            author: "tester".to_string(),
            main: "index.ts".to_string(),
            sandbox: "inline".to_string(),
            permissions: vec!["broadcast".to_string(), "storage".to_string()],
            api: vec![],
            contributes: PluginContributes::default(),
            plugin_type: PluginType::RustTs,
            rust_library: "bedcode_test.wasm".to_string(),
            icon: None,
        }
    }

    /// 构造测试 LoadedPlugin
    fn sample_loaded(source: PluginSource) -> LoadedPlugin {
        LoadedPlugin {
            manifest: sample_manifest(),
            state: PluginState::Activated,
            granted_permissions: HashSet::from(["broadcast".to_string()]),
            extension_path: "/nonexistent/plugins/com.bedcode.test".to_string(),
            activated_at: Some(Utc::now()),
            source,
        }
    }

    /// PluginSource 序列化为前端友好字符串
    #[test]
    fn test_plugin_source_as_str() {
        assert_eq!(PluginSource::StaticRegistry.as_str(), "builtin");
        assert_eq!(PluginSource::FileScan.as_str(), "scanned");
        assert_eq!(PluginSource::Wasm.as_str(), "wasm");
    }

    /// LoadedPlugin → DesktopPluginInfo 字段一一映射（期望值为手写字面量）
    #[test]
    fn test_loaded_plugin_to_desktop_info_mapping() {
        let plugin = sample_loaded(PluginSource::Wasm);
        let info = DesktopPluginInfo::from(&plugin);

        assert_eq!(info.id, "com.bedcode.test");
        assert_eq!(info.name, "Test Plugin");
        assert_eq!(info.version, "1.2.3");
        assert_eq!(info.description, "descriptive text");
        assert_eq!(info.author, "tester");
        assert_eq!(info.main, "index.ts");
        assert_eq!(info.sandbox, "inline");
        assert_eq!(info.plugin_type, PluginType::RustTs);
        assert_eq!(info.rust_library, "bedcode_test.wasm");
        assert_eq!(info.permissions, vec!["broadcast", "storage"]);
        assert_eq!(info.state, PluginState::Activated);
        assert_eq!(info.extension_path, plugin.extension_path);
        assert_eq!(info.source, "wasm");
        // 扩展路径不存在时大小与安装时间均为缺省值
        assert_eq!(info.size_bytes, 0);
        assert!(info.installed_at.is_none());
    }

    /// icon 与 source 的序列化形态：icon=None 时省略字段，source 原样输出
    #[test]
    fn test_desktop_info_serialization_omit_icon() {
        let plugin = sample_loaded(PluginSource::FileScan);
        let info = DesktopPluginInfo::from(&plugin);
        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(json["source"], "scanned");
        assert_eq!(json["pluginType"], "rust-ts", "PluginType 序列化为 kebab-case");
        assert!(json.get("icon").is_none(), "icon=None 时应省略字段");
        assert!(json.get("installedAt").is_none());
    }

    /// icon 有值时透传并在序列化中保留
    #[test]
    fn test_desktop_info_serialization_with_icon() {
        let mut plugin = sample_loaded(PluginSource::StaticRegistry);
        plugin.manifest.icon = Some("icon.svg".to_string());
        let info = DesktopPluginInfo::from(&plugin);
        assert_eq!(info.icon.as_deref(), Some("icon.svg"));
        assert_eq!(info.source, "builtin");
        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(json["icon"], "icon.svg");
    }

    /// size_bytes 递归统计目录内全部文件字节数（含嵌套目录）
    #[test]
    fn test_dir_size_counts_nested_files() {
        let dir = tempfile::tempdir().unwrap();
        // 插件根：plugin.json 3 字节；嵌套子目录：a.bin 42 字节
        std::fs::write(dir.path().join("plugin.json"), b"{}").unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub").join("a.bin"), vec![0u8; 42]).unwrap();
        assert_eq!(dir_size(dir.path()), 44);
    }

    /// 不存在的路径统计为 0（不 panic）
    #[test]
    fn test_dir_size_missing_path_is_zero() {
        let missing = std::path::Path::new("/nonexistent-bedcode-dir-xyz");
        assert_eq!(dir_size(missing), 0);
    }

    /// manifest_installed_at：扩展路径含 plugin.json 时返回其 mtime（unix 毫秒）
    #[test]
    fn test_manifest_installed_at_reads_mtime() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("plugin.json"), b"{}").unwrap();
        let millis = manifest_installed_at(&dir.path().to_string_lossy()).expect("应返回安装时间");
        assert!(millis > 0);
    }

    /// manifest_installed_at：缺少 plugin.json 时返回 None
    #[test]
    fn test_manifest_installed_at_missing_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        assert!(manifest_installed_at(&dir.path().to_string_lossy()).is_none());
    }

    /// From 转换在真实扩展目录上的端到端表现：size_bytes 与 installed_at 来自磁盘
    #[test]
    fn test_desktop_info_from_real_extension_dir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("plugin.json"), b"{}").unwrap();
        std::fs::write(dir.path().join("main.js"), vec![7u8; 100]).unwrap();

        let mut plugin = sample_loaded(PluginSource::Wasm);
        plugin.extension_path = dir.path().to_string_lossy().to_string();
        let info = DesktopPluginInfo::from(&plugin);

        assert_eq!(info.size_bytes, 102, "plugin.json(2) + main.js(100)");
        assert!(info.installed_at.is_some());
        assert_eq!(info.source, "wasm");
    }
}
