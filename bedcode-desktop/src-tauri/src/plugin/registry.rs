//! Plugin Registry
//!
//! 扩展点注册表 — 管理 commands/views/terminal/http/file_handlers 的注册与查询
//! 前端 PluginContext 的注册调用通过 Tauri invoke 到达此注册表

use bedcode_plugin_api::{
    CommandContribution, FileHandlerContribution, ToolProviderContribution, ViewContribution,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 已注册的命令条目
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandEntry {
    pub plugin_id: String,
    pub command_id: String,
    pub title: String,
    pub icon: Option<String>,
}

/// 已注册的视图条目
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewEntry {
    pub plugin_id: String,
    pub view_id: String,
    pub view_type: String,
    pub title: String,
    pub component: String,
}

/// 已注册的终端处理器
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalHandlers {
    pub plugin_id: String,
    pub input_handlers: Vec<String>,
    pub output_parsers: Vec<String>,
}

/// 已注册的 HTTP 端点
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpEndpointEntry {
    pub plugin_id: String,
    pub path: String,
}

/// 已注册的文件处理器
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileHandlerEntry {
    pub plugin_id: String,
    pub handler_id: String,
    pub extensions: Vec<String>,
    pub viewer: String,
    pub icon: Option<String>,
}

/// 扩展点注册表
pub struct PluginRegistry {
    commands: Arc<RwLock<HashMap<String, CommandEntry>>>,
    views: Arc<RwLock<HashMap<String, ViewEntry>>>,
    terminal_handlers: Arc<RwLock<HashMap<String, TerminalHandlers>>>,
    http_endpoints: Arc<RwLock<HashMap<String, HttpEndpointEntry>>>,
    file_handlers: Arc<RwLock<HashMap<String, FileHandlerEntry>>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            commands: Arc::new(RwLock::new(HashMap::new())),
            views: Arc::new(RwLock::new(HashMap::new())),
            terminal_handlers: Arc::new(RwLock::new(HashMap::new())),
            http_endpoints: Arc::new(RwLock::new(HashMap::new())),
            file_handlers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    // ==================== Commands ====================

    /// 注册命令（从 manifest contributes.commands 批量注册）
    pub async fn register_commands(&self, plugin_id: &str, commands: &[CommandContribution]) {
        let mut map = self.commands.write().await;
        for cmd in commands {
            let entry = CommandEntry {
                plugin_id: plugin_id.to_string(),
                command_id: cmd.id.clone(),
                title: cmd.title.clone(),
                icon: cmd.icon.clone(),
            };
            map.insert(cmd.id.clone(), entry);
        }
    }

    /// 获取所有已注册的命令
    pub async fn list_commands(&self) -> Vec<CommandEntry> {
        self.commands.read().await.values().cloned().collect()
    }

    /// 获取指定插件的命令
    pub async fn get_plugin_commands(&self, plugin_id: &str) -> Vec<CommandEntry> {
        self.commands
            .read()
            .await
            .values()
            .filter(|e| e.plugin_id == plugin_id)
            .cloned()
            .collect()
    }

    // ==================== Views ====================

    /// 注册视图（从 manifest contributes.views 批量注册）
    pub async fn register_views(&self, plugin_id: &str, views: &[ViewContribution]) {
        let mut map = self.views.write().await;
        for view in views {
            let entry = ViewEntry {
                plugin_id: plugin_id.to_string(),
                view_id: view.id.clone(),
                view_type: view.view_type.clone(),
                title: view.title.clone(),
                component: view.component.clone(),
            };
            map.insert(view.id.clone(), entry);
        }
    }

    /// 获取指定类型的视图（sidebar/toolbox/statusbar）
    pub async fn get_views_by_type(&self, view_type: &str) -> Vec<ViewEntry> {
        self.views
            .read()
            .await
            .values()
            .filter(|e| e.view_type == view_type)
            .cloned()
            .collect()
    }

    /// 获取所有已注册的视图
    pub async fn list_views(&self) -> Vec<ViewEntry> {
        self.views.read().await.values().cloned().collect()
    }

    // ==================== Terminal ====================

    /// 注册终端处理器
    pub async fn register_terminal_handlers(
        &self,
        plugin_id: &str,
        input_handlers: &[String],
        output_parsers: &[String],
    ) {
        let mut map = self.terminal_handlers.write().await;
        map.insert(
            plugin_id.to_string(),
            TerminalHandlers {
                plugin_id: plugin_id.to_string(),
                input_handlers: input_handlers.to_vec(),
                output_parsers: output_parsers.to_vec(),
            },
        );
    }

    /// 获取所有终端处理器
    pub async fn list_terminal_handlers(&self) -> Vec<TerminalHandlers> {
        self.terminal_handlers.read().await.values().cloned().collect()
    }

    // ==================== HTTP Endpoints ====================

    /// 注册 HTTP 端点
    pub async fn register_http_endpoint(&self, plugin_id: &str, path: &str) {
        let mut map = self.http_endpoints.write().await;
        map.insert(
            path.to_string(),
            HttpEndpointEntry {
                plugin_id: plugin_id.to_string(),
                path: path.to_string(),
            },
        );
    }

    /// 查找注册的 HTTP 端点
    pub async fn find_http_endpoint(&self, path: &str) -> Option<HttpEndpointEntry> {
        self.http_endpoints.read().await.get(path).cloned()
    }

    /// 注册外部工具端点（从 manifest contributes.toolProviders）
    pub async fn register_tool_providers(&self, plugin_id: &str, providers: &[ToolProviderContribution]) {
        let mut map = self.http_endpoints.write().await;
        for provider in providers {
            let full_path = format!("/api/plugin/{}/{}", plugin_id, provider.endpoint.trim_start_matches('/'));
            map.insert(
                full_path.clone(),
                HttpEndpointEntry {
                    plugin_id: plugin_id.to_string(),
                    path: full_path,
                },
            );
        }
    }

    // ==================== File Handlers ====================

    /// 注册文件处理器（从 manifest contributes.fileHandlers 批量注册）
    pub async fn register_file_handlers(&self, plugin_id: &str, handlers: &[FileHandlerContribution]) {
        let mut map = self.file_handlers.write().await;
        for handler in handlers {
            let entry = FileHandlerEntry {
                plugin_id: plugin_id.to_string(),
                handler_id: handler.id.clone(),
                extensions: handler.extensions.clone(),
                viewer: handler.viewer.clone(),
                icon: handler.icon.clone(),
            };
            map.insert(handler.id.clone(), entry);
        }
    }

    /// 根据文件扩展名查找匹配的处理器
    pub async fn find_file_handler(&self, extension: &str) -> Option<FileHandlerEntry> {
        let map = self.file_handlers.read().await;
        for entry in map.values() {
            if entry.extensions.iter().any(|e| e == extension) {
                return Some(entry.clone());
            }
        }
        None
    }

    /// 获取所有已注册的文件处理器
    pub async fn list_file_handlers(&self) -> Vec<FileHandlerEntry> {
        self.file_handlers.read().await.values().cloned().collect()
    }

    // ==================== Cleanup ====================

    /// 移除插件的所有注册（停用时调用）
    pub async fn unregister_plugin(&self, plugin_id: &str) {
        {
            let mut map = self.commands.write().await;
            map.retain(|_, v| v.plugin_id != plugin_id);
        }
        {
            let mut map = self.views.write().await;
            map.retain(|_, v| v.plugin_id != plugin_id);
        }
        {
            let mut map = self.terminal_handlers.write().await;
            map.remove(plugin_id);
        }
        {
            let mut map = self.http_endpoints.write().await;
            map.retain(|_, v| v.plugin_id != plugin_id);
        }
        {
            let mut map = self.file_handlers.write().await;
            map.retain(|_, v| v.plugin_id != plugin_id);
        }
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}
