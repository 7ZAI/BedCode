//! Rust Plugin Context (Mobile)
//!
//! Rust 端插件上下文 — 插件 activate/deactivate 时接收

use crate::permission::PermissionManager;
use std::collections::HashSet;
use std::sync::Arc;

/// 插件存储 trait
pub trait PluginStorageAccess: Send + Sync + 'static {
    fn get(&self, plugin_id: &str, key: &str) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<Option<serde_json::Value>>> + Send>>;
    fn set(&self, plugin_id: &str, key: &str, value: serde_json::Value) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send>>;
    fn delete(&self, plugin_id: &str, key: &str) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send>>;
}

/// 会话查询 trait
pub trait SessionQuery: Send + Sync + 'static {
    fn list_sessions(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<Vec<serde_json::Value>>> + Send>>;
    fn get_session(&self, session_id: &str) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<Option<serde_json::Value>>> + Send>>;
}

/// 事件发射 trait
pub trait EventEmitter: Send + Sync + 'static {
    fn emit(&self, event: &str, payload: serde_json::Value);
}

/// Rust 端插件上下文
pub struct RustPluginContext {
    plugin_id: String,
    storage: Arc<dyn PluginStorageAccess>,
    session_query: Arc<dyn SessionQuery>,
    event_emitter: Arc<dyn EventEmitter>,
    permission: Arc<PermissionManager>,
    granted_permissions: HashSet<String>,
}

impl RustPluginContext {
    pub fn new(
        plugin_id: String,
        storage: Arc<dyn PluginStorageAccess>,
        session_query: Arc<dyn SessionQuery>,
        event_emitter: Arc<dyn EventEmitter>,
        permission: Arc<PermissionManager>,
        granted_permissions: HashSet<String>,
    ) -> Self {
        Self { plugin_id, storage, session_query, event_emitter, permission, granted_permissions }
    }

    pub fn plugin_id(&self) -> &str { &self.plugin_id }
    pub fn granted_permissions(&self) -> &HashSet<String> { &self.granted_permissions }
    pub fn has_permission(&self, permission: &str) -> bool { self.permission.check(&self.plugin_id, permission) }

    pub async fn storage_get(&self, key: &str) -> anyhow::Result<Option<serde_json::Value>> { self.storage.get(&self.plugin_id, key).await }
    pub async fn storage_set(&self, key: &str, value: serde_json::Value) -> anyhow::Result<()> { self.storage.set(&self.plugin_id, key, value).await }
    pub async fn storage_delete(&self, key: &str) -> anyhow::Result<()> { self.storage.delete(&self.plugin_id, key).await }

    pub async fn list_sessions(&self) -> anyhow::Result<Vec<serde_json::Value>> {
        if !self.has_permission("session:read") { anyhow::bail!("Plugin {} lacks session:read permission", self.plugin_id); }
        self.session_query.list_sessions().await
    }
    pub async fn get_session(&self, session_id: &str) -> anyhow::Result<Option<serde_json::Value>> {
        if !self.has_permission("session:read") { anyhow::bail!("Plugin {} lacks session:read permission", self.plugin_id); }
        self.session_query.get_session(session_id).await
    }

    pub fn emit_event(&self, event: &str, payload: serde_json::Value) { self.event_emitter.emit(event, payload); }
}
