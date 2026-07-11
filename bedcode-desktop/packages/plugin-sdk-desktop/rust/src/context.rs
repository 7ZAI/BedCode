//! Rust Plugin Context
//!
//! Rust 端插件上下文 — 插件 activate/deactivate 时接收，提供宿主能力访问

use crate::permission::PermissionManager;
use std::collections::HashSet;
use std::sync::Arc;

/// 插件存储 trait（由主应用实现，解耦对 Database 的直接依赖）
pub trait PluginStorageAccess: Send + Sync + 'static {
    fn get(&self, plugin_id: &str, key: &str) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<Option<serde_json::Value>>> + Send>>;
    fn set(&self, plugin_id: &str, key: &str, value: serde_json::Value) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send>>;
    fn delete(&self, plugin_id: &str, key: &str) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send>>;
}

/// 会话查询 trait（由主应用实现，提供只读会话信息）
pub trait SessionQuery: Send + Sync + 'static {
    fn list_sessions(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<Vec<serde_json::Value>>> + Send>>;
    fn get_session(&self, session_id: &str) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<Option<serde_json::Value>>> + Send>>;
}

/// 事件发射 trait（由主应用实现，向前端发送事件）
pub trait EventEmitter: Send + Sync + 'static {
    fn emit(&self, event: &str, payload: serde_json::Value);
}

/// Rust 端插件上下文
pub struct RustPluginContext {
    /// 插件 ID
    plugin_id: String,
    /// 插件存储访问
    storage: Arc<dyn PluginStorageAccess>,
    /// 会话查询
    session_query: Arc<dyn SessionQuery>,
    /// 事件发射
    event_emitter: Arc<dyn EventEmitter>,
    /// 权限管理器
    permission: Arc<PermissionManager>,
    /// 已授予权限
    granted_permissions: HashSet<String>,
}

impl RustPluginContext {
    /// 创建新的 RustPluginContext
    pub fn new(
        plugin_id: String,
        storage: Arc<dyn PluginStorageAccess>,
        session_query: Arc<dyn SessionQuery>,
        event_emitter: Arc<dyn EventEmitter>,
        permission: Arc<PermissionManager>,
        granted_permissions: HashSet<String>,
    ) -> Self {
        Self {
            plugin_id,
            storage,
            session_query,
            event_emitter,
            permission,
            granted_permissions,
        }
    }

    /// 获取插件 ID
    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }

    /// 获取已授予权限
    pub fn granted_permissions(&self) -> &HashSet<String> {
        &self.granted_permissions
    }

    /// 检查权限
    pub fn has_permission(&self, permission: &str) -> bool {
        self.permission.check(&self.plugin_id, permission)
    }

    // ==================== Storage API ====================

    /// 获取存储值
    pub async fn storage_get(&self, key: &str) -> anyhow::Result<Option<serde_json::Value>> {
        self.storage.get(&self.plugin_id, key).await
    }

    /// 设置存储值
    pub async fn storage_set(&self, key: &str, value: serde_json::Value) -> anyhow::Result<()> {
        self.storage.set(&self.plugin_id, key, value).await
    }

    /// 删除存储值
    pub async fn storage_delete(&self, key: &str) -> anyhow::Result<()> {
        self.storage.delete(&self.plugin_id, key).await
    }

    // ==================== Session API ====================

    /// 列出所有会话
    pub async fn list_sessions(&self) -> anyhow::Result<Vec<serde_json::Value>> {
        if !self.has_permission("session:read") {
            anyhow::bail!("Plugin {} lacks session:read permission", self.plugin_id);
        }
        self.session_query.list_sessions().await
    }

    /// 获取单个会话
    pub async fn get_session(&self, session_id: &str) -> anyhow::Result<Option<serde_json::Value>> {
        if !self.has_permission("session:read") {
            anyhow::bail!("Plugin {} lacks session:read permission", self.plugin_id);
        }
        self.session_query.get_session(session_id).await
    }

    // ==================== Event API ====================

    /// 向前端发送事件
    pub fn emit_event(&self, event: &str, payload: serde_json::Value) {
        self.event_emitter.emit(event, payload);
    }
}
