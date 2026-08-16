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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permission::PermissionManager;
    use crate::test_utils::{
        block_on, MockBusAccess, MockEventEmitter, MockFileAccess, MockSessionQuery, MockStorage,
    };

    /// 构造带指定权限的测试上下文，返回 (ctx, storage, emitter, file, bus) 便于断言调用记录
    fn make_context(
        permissions: &[&str],
    ) -> (
        RustPluginContext,
        Arc<MockStorage>,
        Arc<MockEventEmitter>,
        Arc<MockFileAccess>,
        Arc<MockBusAccess>,
    ) {
        let pm = Arc::new(PermissionManager::new());
        pm.grant_permissions(
            "test.plugin",
            &permissions.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        );
        let storage = Arc::new(MockStorage::default());
        let emitter = Arc::new(MockEventEmitter::default());
        let file = Arc::new(MockFileAccess::default());
        let bus = Arc::new(MockBusAccess::default());
        let ctx = RustPluginContext::new(
            "test.plugin".into(),
            storage.clone(),
            Arc::new(MockSessionQuery),
            emitter.clone(),
            pm,
            permissions.iter().map(|s| s.to_string()).collect(),
            file.clone(),
            bus.clone(),
        );
        (ctx, storage, emitter, file, bus)
    }

    // ==================== 基础访问器 ====================

    #[test]
    fn test_plugin_id_and_granted_permissions() {
        let (ctx, _, _, _, _) = make_context(&["storage"]);
        assert_eq!(ctx.plugin_id(), "test.plugin");
        assert!(ctx.granted_permissions().contains("storage"));
        assert!(!ctx.granted_permissions().contains("session:read"));
    }

    #[test]
    fn test_has_permission_consults_manager() {
        let (ctx, _, _, _, _) = make_context(&["terminal:input"]);
        // storage 默认授予，无需显式请求
        assert!(ctx.has_permission("storage"));
        assert!(ctx.has_permission("terminal:input"));
        assert!(!ctx.has_permission("terminal:output"));
    }

    // ==================== Storage API（透传 plugin_id 前缀） ====================

    #[test]
    fn test_storage_get_passes_plugin_id_and_key() {
        let (ctx, storage, _, _, _) = make_context(&[]);
        let value = block_on(ctx.storage_get("my_key")).unwrap();
        // mock 返回 { "stored": key }，验证调用链上 plugin_id 与 key 均透传
        assert_eq!(value, Some(serde_json::json!({ "stored": "my_key" })));
        let calls = storage.calls.lock().unwrap().clone();
        assert_eq!(calls, vec![("test.plugin".to_string(), "my_key".to_string())]);
    }

    #[test]
    fn test_storage_set_and_delete_pass_plugin_id_and_key() {
        let (ctx, storage, _, _, _) = make_context(&[]);
        block_on(ctx.storage_set("k1", serde_json::json!({ "v": 1 }))).unwrap();
        block_on(ctx.storage_delete("k2")).unwrap();
        let calls = storage.calls.lock().unwrap().clone();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0], ("test.plugin".to_string(), "k1".to_string()));
        assert_eq!(calls[1], ("test.plugin".to_string(), "k2".to_string()));
    }

    // ==================== Session API（权限闸门） ====================

    #[test]
    fn test_list_sessions_requires_session_read() {
        let (ctx, _, _, _, _) = make_context(&[]);
        let err = block_on(ctx.list_sessions()).unwrap_err();
        // 权限拒绝信息含插件 ID 与所需权限，宿主据此定位
        assert!(err.to_string().contains("test.plugin"));
        assert!(err.to_string().contains("session:read"));
    }

    #[test]
    fn test_list_sessions_with_permission() {
        let (ctx, _, _, _, _) = make_context(&["session:read"]);
        let sessions = block_on(ctx.list_sessions()).unwrap();
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0], serde_json::json!({ "id": "s1" }));
    }

    #[test]
    fn test_get_session_existing_and_missing() {
        let (ctx, _, _, _, _) = make_context(&["session:read"]);
        let existing = block_on(ctx.get_session("s1")).unwrap();
        assert_eq!(existing, Some(serde_json::json!({ "id": "s1" })));
        // mock 对未知 session 返回 None，验证 Option 透传
        let missing = block_on(ctx.get_session("nope")).unwrap();
        assert_eq!(missing, None);
    }

    // ==================== Event API ====================

    #[test]
    fn test_emit_event_forwards_to_emitter() {
        let (ctx, _, emitter, _, _) = make_context(&[]);
        let payload = serde_json::json!({ "status": "done" });
        ctx.emit_event("plugin:event", payload.clone());
        assert_eq!(emitter.emit_count.load(std::sync::atomic::Ordering::SeqCst), 1);
        let emitted = emitter.emitted.lock().unwrap().clone();
        assert_eq!(emitted, vec![("plugin:event".to_string(), payload)]);
    }

    // ==================== File System API（权限闸门） ====================

    #[test]
    fn test_fs_read_requires_fs_read_permission() {
        let (ctx, _, _, file, _) = make_context(&[]);
        let err = block_on(ctx.fs_read("/data/x.txt")).unwrap_err();
        assert!(err.to_string().contains("fs:read"));
        assert!(file.reads.lock().unwrap().is_empty());
    }

    #[test]
    fn test_fs_read_passes_plugin_id_and_path() {
        let (ctx, _, _, file, _) = make_context(&["fs:read"]);
        let content = block_on(ctx.fs_read("/data/x.txt")).unwrap();
        assert_eq!(content.as_deref(), Some("content-of-/data/x.txt"));
        let reads = file.reads.lock().unwrap().clone();
        assert_eq!(reads, vec![("test.plugin".to_string(), "/data/x.txt".to_string())]);
    }

    #[test]
    fn test_fs_write_requires_fs_write_permission() {
        let (ctx, _, _, file, _) = make_context(&[]);
        let err = block_on(ctx.fs_write("/data/y.txt", "hi")).unwrap_err();
        assert!(err.to_string().contains("fs:write"));
        assert!(file.writes.lock().unwrap().is_empty());
    }

    #[test]
    fn test_fs_write_passes_data() {
        let (ctx, _, _, file, _) = make_context(&["fs:write"]);
        block_on(ctx.fs_write("/data/y.txt", "hi")).unwrap();
        let writes = file.writes.lock().unwrap().clone();
        assert_eq!(
            writes,
            vec![("test.plugin".to_string(), "/data/y.txt".to_string(), "hi".to_string())]
        );
    }

    #[test]
    fn test_fs_copy_requires_both_read_and_write() {
        let (ctx, _, _, file, _) = make_context(&["fs:read"]);
        let err = block_on(ctx.fs_copy("/a", "/b")).unwrap_err();
        assert!(err.to_string().contains("fs:read+fs:write"));
        assert!(file.copies.lock().unwrap().is_empty());

        let (ctx2, _, _, file2, _) = make_context(&["fs:read", "fs:write"]);
        block_on(ctx2.fs_copy("/a", "/b")).unwrap();
        let copies = file2.copies.lock().unwrap().clone();
        assert_eq!(
            copies,
            vec![("test.plugin".to_string(), "/a".to_string(), "/b".to_string())]
        );
    }

    // ==================== Message Bus API（权限闸门） ====================

    #[test]
    fn test_bus_publish_without_permission_is_silent_drop() {
        // 发布是 fire-and-forget：无权限时静默丢弃（不 panic、不报错），
        // 与订阅/退订的显式报错语义区分
        let (ctx, _, _, _, bus) = make_context(&[]);
        ctx.bus_publish("task:changed", serde_json::json!({ "id": 1 }));
        assert!(bus.published.lock().unwrap().is_empty());
    }

    #[test]
    fn test_bus_publish_forwards_with_permission() {
        let (ctx, _, _, _, bus) = make_context(&["bus"]);
        let payload = serde_json::json!({ "id": 1 });
        ctx.bus_publish("task:changed", payload.clone());
        let published = bus.published.lock().unwrap().clone();
        assert_eq!(
            published,
            vec![("test.plugin".to_string(), "task:changed".to_string(), payload)]
        );
    }

    #[test]
    fn test_bus_subscribe_requires_bus_permission() {
        let (ctx, _, _, _, bus) = make_context(&[]);
        let err = block_on(ctx.bus_subscribe("topic:a")).unwrap_err();
        assert!(err.to_string().contains("bus"));
        assert!(bus.subscriptions.lock().unwrap().is_empty());
    }

    #[test]
    fn test_bus_subscribe_and_unsubscribe_forward() {
        let (ctx, _, _, _, bus) = make_context(&["bus"]);
        block_on(ctx.bus_subscribe("topic:a")).unwrap();
        block_on(ctx.bus_unsubscribe("topic:a")).unwrap();
        assert_eq!(
            bus.subscriptions.lock().unwrap().clone(),
            vec![("test.plugin".to_string(), "topic:a".to_string())]
        );
        assert_eq!(
            bus.unsubscriptions.lock().unwrap().clone(),
            vec![("test.plugin".to_string(), "topic:a".to_string())]
        );
    }
}
