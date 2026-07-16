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

/// 文件系统访问 trait（由主应用实现，解耦对 FsAuthChecker 的直接依赖）
pub trait FileAccess: Send + Sync + 'static {
    fn read_file(
        &self, plugin_id: &str, path: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<Option<String>>> + Send>>;
    fn write_file(
        &self, plugin_id: &str, path: &str, data: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send>>;
    fn copy_file(
        &self, plugin_id: &str, src: &str, dst: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send>>;
}

/// 消息总线访问 trait（由主应用实现，解耦对 MessageBus 的直接依赖）
pub trait BusAccess: Send + Sync + 'static {
    fn publish(&self, plugin_id: &str, topic: &str, payload: serde_json::Value);
    fn subscribe(
        &self, plugin_id: &str, topic: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send>>;
    fn unsubscribe(
        &self, plugin_id: &str, topic: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send>>;
}

/// Rust 端插件上下文
pub struct RustPluginContext {
    plugin_id: String,
    storage: Arc<dyn PluginStorageAccess>,
    session_query: Arc<dyn SessionQuery>,
    event_emitter: Arc<dyn EventEmitter>,
    permission: Arc<PermissionManager>,
    granted_permissions: HashSet<String>,
    file: Arc<dyn FileAccess>,
    bus: Arc<dyn BusAccess>,
}

impl RustPluginContext {
    pub fn new(
        plugin_id: String,
        storage: Arc<dyn PluginStorageAccess>,
        session_query: Arc<dyn SessionQuery>,
        event_emitter: Arc<dyn EventEmitter>,
        permission: Arc<PermissionManager>,
        granted_permissions: HashSet<String>,
        file: Arc<dyn FileAccess>,
        bus: Arc<dyn BusAccess>,
    ) -> Self {
        Self { plugin_id, storage, session_query, event_emitter, permission, granted_permissions, file, bus }
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

    // ==================== File System API ====================

    /// 读取文件内容（需 fs:read 权限）
    pub async fn fs_read(&self, path: &str) -> anyhow::Result<Option<String>> {
        if !self.has_permission(crate::permission::PERMISSION_FS_READ) {
            anyhow::bail!("Plugin {} lacks fs:read permission", self.plugin_id);
        }
        self.file.read_file(&self.plugin_id, path).await
    }

    /// 写入文件内容，自动创建父目录（需 fs:write 权限）
    pub async fn fs_write(&self, path: &str, data: &str) -> anyhow::Result<()> {
        if !self.has_permission(crate::permission::PERMISSION_FS_WRITE) {
            anyhow::bail!("Plugin {} lacks fs:write permission", self.plugin_id);
        }
        self.file.write_file(&self.plugin_id, path, data).await
    }

    /// 复制文件，自动创建目标父目录（需 fs:read + fs:write 权限）
    pub async fn fs_copy(&self, src: &str, dst: &str) -> anyhow::Result<()> {
        if !self.has_permission(crate::permission::PERMISSION_FS_READ) || !self.has_permission(crate::permission::PERMISSION_FS_WRITE) {
            anyhow::bail!("Plugin {} lacks fs:read+fs:write permission for copy", self.plugin_id);
        }
        self.file.copy_file(&self.plugin_id, src, dst).await
    }

    // ==================== Message Bus API ====================

    /// 发布消息到总线（需 bus 权限）
    pub fn bus_publish(&self, topic: &str, payload: serde_json::Value) {
        if !self.has_permission(crate::permission::PERMISSION_BUS) {
            return;
        }
        self.bus.publish(&self.plugin_id, topic, payload);
    }

    /// 订阅 topic（需 bus 权限）
    pub async fn bus_subscribe(&self, topic: &str) -> anyhow::Result<()> {
        if !self.has_permission(crate::permission::PERMISSION_BUS) {
            anyhow::bail!("Plugin {} lacks bus permission", self.plugin_id);
        }
        self.bus.subscribe(&self.plugin_id, topic).await
    }

    /// 取消订阅（需 bus 权限）
    pub async fn bus_unsubscribe(&self, topic: &str) -> anyhow::Result<()> {
        if !self.has_permission(crate::permission::PERMISSION_BUS) {
            anyhow::bail!("Plugin {} lacks bus permission", self.plugin_id);
        }
        self.bus.unsubscribe(&self.plugin_id, topic).await
    }
}
