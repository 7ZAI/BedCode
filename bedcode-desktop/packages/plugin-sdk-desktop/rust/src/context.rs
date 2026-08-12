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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::{block_on, MockEventEmitter, MockSessionQuery, MockStorage};
    use crate::PermissionManager;

    /// 构造带指定权限的测试上下文
    fn make_context(permissions: &[&str]) -> (RustPluginContext, Arc<MockStorage>, Arc<MockEventEmitter>) {
        let pm = Arc::new(PermissionManager::new());
        pm.grant_permissions(
            "test.plugin",
            &permissions.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        );
        let storage = Arc::new(MockStorage::default());
        let emitter = Arc::new(MockEventEmitter::default());
        let ctx = RustPluginContext::new(
            "test.plugin".into(),
            storage.clone(),
            Arc::new(MockSessionQuery),
            emitter.clone(),
            pm,
            permissions.iter().map(|s| s.to_string()).collect(),
        );
        (ctx, storage, emitter)
    }

    // ==================== 基础访问器 ====================

    #[test]
    fn test_plugin_id_and_granted_permissions() {
        let (ctx, _, _) = make_context(&["storage"]);
        assert_eq!(ctx.plugin_id(), "test.plugin");
        assert!(ctx.granted_permissions().contains("storage"));
        assert!(!ctx.granted_permissions().contains("session:read"));
    }

    #[test]
    fn test_has_permission_consults_manager() {
        let (ctx, _, _) = make_context(&["terminal:input"]);
        // storage 默认授予，无需显式请求
        assert!(ctx.has_permission("storage"));
        assert!(ctx.has_permission("terminal:input"));
        assert!(!ctx.has_permission("terminal:output"));
    }

    // ==================== Storage API（透传 plugin_id 前缀） ====================

    #[test]
    fn test_storage_get_passes_plugin_id_and_key() {
        let (ctx, storage, _) = make_context(&[]);
        let value = block_on(ctx.storage_get("my_key")).unwrap();
        // mock 返回 { "stored": key }，验证调用链上 plugin_id 与 key 均透传
        assert_eq!(value, Some(serde_json::json!({ "stored": "my_key" })));
        let calls = storage.calls.lock().unwrap().clone();
        assert_eq!(calls, vec![("test.plugin".to_string(), "my_key".to_string())]);
    }

    #[test]
    fn test_storage_set_and_delete_pass_plugin_id_and_key() {
        let (ctx, storage, _) = make_context(&[]);
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
        let (ctx, _, _) = make_context(&[]);
        let err = block_on(ctx.list_sessions()).unwrap_err();
        // 权限拒绝信息含插件 ID 与所需权限，宿主据此定位
        assert!(err.to_string().contains("test.plugin"));
        assert!(err.to_string().contains("session:read"));
    }

    #[test]
    fn test_list_sessions_with_permission() {
        let (ctx, _, _) = make_context(&["session:read"]);
        let sessions = block_on(ctx.list_sessions()).unwrap();
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0], serde_json::json!({ "id": "s1" }));
    }

    #[test]
    fn test_get_session_requires_session_read() {
        let (ctx, _, _) = make_context(&[]);
        let err = block_on(ctx.get_session("s1")).unwrap_err();
        assert!(err.to_string().contains("session:read"));
    }

    #[test]
    fn test_get_session_existing_and_missing() {
        let (ctx, _, _) = make_context(&["session:read"]);
        let existing = block_on(ctx.get_session("s1")).unwrap();
        assert_eq!(existing, Some(serde_json::json!({ "id": "s1" })));
        // mock 对未知 session 返回 None，验证 Option 透传
        let missing = block_on(ctx.get_session("nope")).unwrap();
        assert_eq!(missing, None);
    }

    // ==================== Event API ====================

    #[test]
    fn test_emit_event_forwards_to_emitter() {
        let (ctx, _, emitter) = make_context(&[]);
        let payload = serde_json::json!({ "status": "done" });
        ctx.emit_event("plugin:event", payload.clone());
        assert_eq!(emitter.emit_count.load(std::sync::atomic::Ordering::SeqCst), 1);
        let emitted = emitter.emitted.lock().unwrap().clone();
        assert_eq!(emitted, vec![("plugin:event".to_string(), payload)]);
    }
}
