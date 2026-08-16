//! Mobile Plugin Types (Host)
//!
//! 从 bedcode-plugin-api-mobile re-export 共享类型
//! 仅保留宿主运行时内部类型

pub use bedcode_plugin_api_mobile::types::*;
pub use bedcode_plugin_api_mobile::types::PluginManifest;

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// 插件来源
#[derive(Debug, Clone, PartialEq)]
pub enum PluginSource {
    /// APK assets 内置插件
    ApkAsset,
    /// 远程下载插件
    RemoteDownload,
    /// 本地文件安装插件
    FileInstall,
    /// 仅前端注册（无 WASM 模块）
    FrontendOnly,
}

/// 已加载插件的内部表示
pub struct LoadedPlugin {
    pub manifest: PluginManifest,
    pub state: bedcode_plugin_api_mobile::types::PluginState,
    pub granted_permissions: HashSet<String>,
    /// 插件来源
    pub source: PluginSource,
    /// 插件目录路径（包含 plugin.json 的目录）
    pub extension_path: String,
}

/// 插件生命周期事件
///
/// 统一的事件枚举，供 PluginManager::dispatch_lifecycle_event() 使用。
/// 每个变体携带触发点传入的上下文数据。
#[derive(Debug, Clone)]
pub enum PluginLifecycleEvent {
    AppStartup,
    AppShutdown,
    AuthSuccess,
    Disconnect { reason: String },
    SessionCreated { session_id: String },
    SessionStopped { session_id: String },
    TerminalInput { session_id: String, data: String },
    TerminalOutput { session_id: String, data: String },
}

impl PluginLifecycleEvent {
    /// 返回声明字段名（用于 is_declared 检查）
    pub fn name(&self) -> &'static str {
        match self {
            Self::AppStartup => "onStartup",
            Self::AppShutdown => "onShutdown",
            Self::AuthSuccess => "onAuthSuccess",
            Self::Disconnect { .. } => "onDisconnect",
            Self::SessionCreated { .. } => "onSessionCreated",
            Self::SessionStopped { .. } => "onSessionStopped",
            Self::TerminalInput { .. } => "onTerminalInput",
            Self::TerminalOutput { .. } => "onTerminalOutput",
        }
    }

    /// 返回前端 Tauri 事件名（不含 plugin:lifecycle: 前缀）
    pub fn tauri_event_name(&self) -> &'static str {
        match self {
            Self::AppStartup => "appStartup",
            Self::AppShutdown => "appShutdown",
            Self::AuthSuccess => "authSuccess",
            Self::Disconnect { .. } => "disconnect",
            Self::SessionCreated { .. } => "sessionCreated",
            Self::SessionStopped { .. } => "sessionStopped",
            Self::TerminalInput { .. } => "terminalInput",
            Self::TerminalOutput { .. } => "terminalOutput",
        }
    }

    /// 转为前端 Tauri 事件 payload
    pub fn to_payload(&self) -> serde_json::Value {
        match self {
            Self::AppStartup | Self::AppShutdown | Self::AuthSuccess => {
                serde_json::json!({})
            }
            Self::Disconnect { reason } => {
                serde_json::json!({ "reason": reason })
            }
            Self::SessionCreated { session_id } | Self::SessionStopped { session_id } => {
                serde_json::json!({ "sessionId": session_id })
            }
            Self::TerminalInput { session_id, data } | Self::TerminalOutput { session_id, data } => {
                serde_json::json!({ "sessionId": session_id, "data": data })
            }
        }
    }
}

/// 返回给前端的插件信息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MobilePluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub main: String,
    pub plugin_type: bedcode_plugin_api_mobile::types::PluginType,
    pub permissions: Vec<String>,
    pub state: bedcode_plugin_api_mobile::types::PluginState,
    pub contributes: bedcode_plugin_api_mobile::types::PluginContributes,
    /// 插件来源
    pub source: String,
    /// 插件目录路径（含 plugin.json 的目录），前端经 asset protocol 加载前端模块
    pub extension_path: String,
    /// 插件图标：emoji 或相对插件目录的图片路径，缺省时前端生成字母头像回退
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// 插件目录总大小（字节），目录不存在时为 0
    pub size_bytes: u64,
    /// 安装时间（unix 毫秒），取 plugin.json 的 mtime；无法获取时为 null
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed_at: Option<i64>,
}

/// 递归统计目录总大小（字节），路径不存在或不可读时返回已累计部分
fn dir_size(path: &std::path::Path) -> u64 {
    let mut total = 0u64;
    let entries = match std::fs::read_dir(path) {
        Ok(entries) => entries,
        Err(_) => return 0,
    };
    for entry in entries.flatten() {
        let file_type = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        if file_type.is_file() {
            total += entry.metadata().map(|m| m.len()).unwrap_or(0);
        } else if file_type.is_dir() {
            total += dir_size(&entry.path());
        }
    }
    total
}

/// 以 plugin.json 的 mtime 近似安装时间（unix 毫秒）
fn manifest_installed_at(extension_path: &str) -> Option<i64> {
    let manifest = std::path::Path::new(extension_path).join("plugin.json");
    let mtime = std::fs::metadata(&manifest).ok()?.modified().ok()?;
    let since_epoch = mtime.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some(since_epoch.as_millis() as i64)
}

impl From<&LoadedPlugin> for MobilePluginInfo {
    fn from(p: &LoadedPlugin) -> Self {
        let source_str = match &p.source {
            PluginSource::ApkAsset => "apk-asset",
            PluginSource::RemoteDownload => "remote-download",
            PluginSource::FileInstall => "file-install",
            PluginSource::FrontendOnly => "frontend-only",
        };
        MobilePluginInfo {
            id: p.manifest.id.clone(),
            name: p.manifest.name.clone(),
            version: p.manifest.version.clone(),
            description: p.manifest.description.clone(),
            author: p.manifest.author.clone(),
            main: p.manifest.main.clone(),
            plugin_type: p.manifest.plugin_type.clone(),
            permissions: p.manifest.permissions.clone(),
            state: p.state.clone(),
            contributes: p.manifest.contributes.clone(),
            source: source_str.to_string(),
            extension_path: p.extension_path.clone(),
            icon: p.manifest.icon.clone(),
            size_bytes: dir_size(std::path::Path::new(&p.extension_path)),
            installed_at: manifest_installed_at(&p.extension_path),
        }
    }
}
