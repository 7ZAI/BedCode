//! 文件系统访问校验器
//!
//! 三层策略：路径白名单（持久化）→ 插件白名单（持久化）→ 弹窗授权
//!
//! 所有白名单通过 PluginStorage 持久化，无硬编码。
//! 插件请求文件访问时按优先级校验：
//! 1. 路径白名单：预定义安全路径前缀，匹配即放行
//! 2. 插件白名单：受信任的插件直接放行
//! 3. 已授权路径前缀（持久化）：用户之前授权并记住的路径
//! 4. 弹窗授权：询问用户，授权后可选持久化记住路径前缀

use crate::plugin::storage::PluginStorage;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::Emitter;
use tokio::sync::{Mutex, oneshot};

/// 文件操作类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsOp {
    Read,
    Write,
}

impl std::fmt::Display for FsOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FsOp::Read => write!(f, "read"),
            FsOp::Write => write!(f, "write"),
        }
    }
}

/// 待处理的授权请求
struct PendingRequest {
    request_id: String,
    plugin_id: String,
    /// 请求授权的文件路径（单路径请求为单元素；批量请求含全部未授权路径）
    paths: Vec<String>,
    reply_tx: oneshot::Sender<bool>,
}

/// 文件系统访问校验器
pub struct FsAuthChecker {
    /// 持久化白名单存储
    storage: Arc<PluginStorage>,
    /// 待处理的弹窗授权请求
    pending_requests: Arc<Mutex<Vec<PendingRequest>>>,
    /// 弹窗授权句柄（None 时无头/测试上下文：授权请求拒绝，check/request_auth 返回 false）
    app_handle: Option<Arc<tauri::AppHandle>>,
}

/// 存储键名常量
/// 路径白名单存储在 plugin_id="__system__" 下的 key
const SYSTEM_PLUGIN_ID: &str = "__system__";
const STORAGE_KEY_PATH_WHITELIST: &str = "fs_path_whitelist";
const STORAGE_KEY_PLUGIN_WHITELIST: &str = "fs_plugin_whitelist";
const GRANTED_PATHS_KEY: &str = "fs_granted_paths";

/// 弹窗授权超时（秒）
const AUTH_TIMEOUT_SECS: u64 = 30;

impl FsAuthChecker {
    /// 创建文件访问校验器
    pub fn new(
        storage: Arc<PluginStorage>,
        app_handle: Option<Arc<tauri::AppHandle>>,
    ) -> Self {
        Self {
            storage,
            pending_requests: Arc::new(Mutex::new(Vec::new())),
            app_handle,
        }
    }

    /// 校验文件访问权限
    pub async fn check(&self, plugin_id: &str, path: &str, operation: FsOp) -> bool {
        let canonical = match Self::canonicalize_path(path) {
            Some(p) => p,
            None => {
                tracing::warn!(plugin_id = %plugin_id, path = %path, "fs_auth: path canonicalization failed");
                return false;
            }
        };

        // 第一层：路径白名单
        if self.check_path_whitelist(&canonical).await {
            tracing::debug!(plugin_id = %plugin_id, path = %path, "fs_auth: allowed by path whitelist");
            return true;
        }

        // 第二层：插件白名单
        if self.check_plugin_whitelist(plugin_id).await {
            tracing::debug!(plugin_id = %plugin_id, path = %path, "fs_auth: allowed by plugin whitelist");
            return true;
        }

        // 第三层：已授权路径前缀（持久化）
        if self.check_granted_path(plugin_id, &canonical).await {
            tracing::debug!(plugin_id = %plugin_id, path = %path, "fs_auth: allowed by previously granted path");
            return true;
        }

        // 第四层：弹窗授权
        self.request_user_auth(plugin_id, path, operation).await
    }

    /// 处理用户授权回复
    pub async fn respond(&self, request_id: &str, allowed: bool, remember: bool) {
        // 短锁：取回请求后立即释放，持久化 await 不持有 pending 锁
        let request = {
            let mut pending = self.pending_requests.lock().await;
            match pending.iter().position(|r| r.request_id == request_id) {
                Some(idx) => pending.remove(idx),
                None => return,
            }
        };

        if allowed && remember {
            for path in &request.paths {
                if let Err(e) = self.save_granted_path(&request.plugin_id, path).await {
                    tracing::warn!(error = %e, "fs_auth: failed to save granted path");
                }
            }
        }
        let _ = request.reply_tx.send(allowed);
    }

    /// 批量请求目录授权
    ///
    /// 已授权/白名单路径直接放行；未授权路径合并为**一次**弹窗询问，
    /// 全部同意才返回 `true`（任一拒绝或超时即 `false`）。
    /// 供插件 activate 时集中申请数据目录访问权。
    pub async fn check_batch(&self, plugin_id: &str, paths: &[String], operation: FsOp) -> bool {
        let mut ungranted: Vec<String> = Vec::new();

        for path in paths {
            let canonical = match Self::canonicalize_path(path) {
                Some(p) => p,
                None => {
                    tracing::warn!(plugin_id = %plugin_id, path = %path, "fs_auth: path canonicalization failed");
                    return false;
                }
            };

            // 路径白名单 / 插件白名单 / 已授权前缀 → 直接放行
            if self.check_path_whitelist(&canonical).await {
                continue;
            }
            if self.check_plugin_whitelist(plugin_id).await {
                continue;
            }
            if self.check_granted_path(plugin_id, &canonical).await {
                continue;
            }
            ungranted.push(path.clone());
        }

        if ungranted.is_empty() {
            return true;
        }

        self.request_user_auth_batch(plugin_id, &ungranted, operation).await
    }

    /// 弹窗请求用户授权（批量：一次弹窗展示全部未授权路径）
    async fn request_user_auth_batch(
        &self,
        plugin_id: &str,
        paths: &[String],
        operation: FsOp,
    ) -> bool {
        let request_id = uuid::Uuid::new_v4().to_string();
        let (reply_tx, reply_rx) = oneshot::channel();

        {
            let mut pending = self.pending_requests.lock().await;
            pending.push(PendingRequest {
                request_id: request_id.clone(),
                plugin_id: plugin_id.to_string(),
                paths: paths.to_vec(),
                reply_tx,
            });
        }

        // 发送弹窗事件到前端（paths 数组 + path 兼容字段 = 首个路径）
        let payload = serde_json::json!({
            "requestId": request_id,
            "pluginId": plugin_id,
            "paths": paths,
            "path": paths.first().cloned().unwrap_or_default(),
            "operation": operation.to_string(),
        });

        // 事件未送达（无头/测试上下文无 app_handle 同样视为未送达）：请求永远不会
        // 被响应，移除已入队条目避免 pending 泄漏
        let emit_result = match &self.app_handle {
            Some(app) => app
                .emit("plugin:fs-auth-request", payload)
                .map_err(|e| format!("emit failed: {}", e)),
            None => Err("app_handle unavailable".to_string()),
        };
        if let Err(err) = emit_result {
            let mut pending = self.pending_requests.lock().await;
            pending.retain(|r| r.request_id != request_id);
            tracing::error!(error = %err, "fs_auth: failed to deliver auth request event");
            return false;
        }

        match tokio::time::timeout(
            std::time::Duration::from_secs(AUTH_TIMEOUT_SECS),
            reply_rx,
        )
        .await
        {
            Ok(Ok(allowed)) => {
                tracing::info!(plugin_id = %plugin_id, paths = ?paths, allowed = allowed, "fs_auth: user responded (batch)");
                allowed
            }
            _ => {
                tracing::warn!(plugin_id = %plugin_id, paths = ?paths, "fs_auth: batch auth request timed out or cancelled");
                let mut pending = self.pending_requests.lock().await;
                pending.retain(|r| r.request_id != request_id);
                false
            }
        }
    }

    // ==================== 路径白名单管理 ====================

    /// 添加路径白名单
    pub async fn add_path_whitelist(&self, path: &str) -> anyhow::Result<()> {
        let canonical = Self::canonicalize_path(path)
            .unwrap_or_else(|| PathBuf::from(path));
        let canonical_str = canonical.to_string_lossy().to_string();
        let mut list = self.get_path_whitelist().await.unwrap_or_default();
        if list.contains(&canonical_str) {
            return Ok(());
        }
        list.push(canonical_str);
        self.storage.set(SYSTEM_PLUGIN_ID, STORAGE_KEY_PATH_WHITELIST, serde_json::Value::Array(
            list.into_iter().map(serde_json::Value::String).collect()
        )).await?;
        Ok(())
    }

    /// 移除路径白名单
    pub async fn remove_path_whitelist(&self, path: &str) -> anyhow::Result<()> {
        let canonical = Self::canonicalize_path(path)
            .unwrap_or_else(|| PathBuf::from(path));
        let canonical_str = canonical.to_string_lossy().to_string();
        let mut list = self.get_path_whitelist().await.unwrap_or_default();
        list.retain(|p| p != &canonical_str);
        self.storage.set(SYSTEM_PLUGIN_ID, STORAGE_KEY_PATH_WHITELIST, serde_json::Value::Array(
            list.into_iter().map(serde_json::Value::String).collect()
        )).await?;
        Ok(())
    }

    /// 获取路径白名单
    pub async fn get_path_whitelist(&self) -> anyhow::Result<Vec<String>> {
        match self.storage.get(SYSTEM_PLUGIN_ID, STORAGE_KEY_PATH_WHITELIST).await? {
            Some(serde_json::Value::Array(arr)) => {
                Ok(arr.into_iter().filter_map(|v| v.as_str().map(String::from)).collect())
            }
            _ => Ok(Vec::new()),
        }
    }

    // ==================== 插件白名单管理 ====================

    /// 添加插件白名单
    pub async fn add_plugin_whitelist(&self, plugin_id: &str) -> anyhow::Result<()> {
        let mut list = self.get_plugin_whitelist().await.unwrap_or_default();
        if list.contains(&plugin_id.to_string()) {
            return Ok(());
        }
        list.push(plugin_id.to_string());
        self.storage.set(SYSTEM_PLUGIN_ID, STORAGE_KEY_PLUGIN_WHITELIST, serde_json::Value::Array(
            list.into_iter().map(serde_json::Value::String).collect()
        )).await?;
        Ok(())
    }

    /// 移除插件白名单
    pub async fn remove_plugin_whitelist(&self, plugin_id: &str) -> anyhow::Result<()> {
        let mut list = self.get_plugin_whitelist().await.unwrap_or_default();
        list.retain(|p| p != plugin_id);
        self.storage.set(SYSTEM_PLUGIN_ID, STORAGE_KEY_PLUGIN_WHITELIST, serde_json::Value::Array(
            list.into_iter().map(serde_json::Value::String).collect()
        )).await?;
        Ok(())
    }

    /// 获取插件白名单
    pub async fn get_plugin_whitelist(&self) -> anyhow::Result<Vec<String>> {
        match self.storage.get(SYSTEM_PLUGIN_ID, STORAGE_KEY_PLUGIN_WHITELIST).await? {
            Some(serde_json::Value::Array(arr)) => {
                Ok(arr.into_iter().filter_map(|v| v.as_str().map(String::from)).collect())
            }
            _ => Ok(Vec::new()),
        }
    }

    // ==================== 内部方法 ====================

    /// 检查路径白名单
    async fn check_path_whitelist(&self, canonical: &Path) -> bool {
        let whitelist = match self.get_path_whitelist().await {
            Ok(list) => list,
            Err(_) => return false,
        };
        for prefix_str in &whitelist {
            if let Ok(prefix_path) = PathBuf::from(prefix_str).canonicalize() {
                if canonical.starts_with(&prefix_path) {
                    return true;
                }
            }
        }
        false
    }

    /// 检查插件白名单
    async fn check_plugin_whitelist(&self, plugin_id: &str) -> bool {
        let whitelist = match self.get_plugin_whitelist().await {
            Ok(list) => list,
            Err(_) => return false,
        };
        whitelist.iter().any(|p| p == plugin_id)
    }

    /// 检查已授权路径前缀
    async fn check_granted_path(&self, plugin_id: &str, canonical: &Path) -> bool {
        let storage_key = GRANTED_PATHS_KEY.to_string();
        let granted = match self.storage.get(plugin_id, &storage_key).await {
            Ok(Some(serde_json::Value::Array(arr))) => arr,
            _ => return false,
        };
        for prefix_val in &granted {
            if let Some(prefix_str) = prefix_val.as_str() {
                if let Ok(prefix_path) = PathBuf::from(prefix_str).canonicalize() {
                    if canonical.starts_with(&prefix_path) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// 弹窗请求用户授权
    async fn request_user_auth(&self, plugin_id: &str, path: &str, operation: FsOp) -> bool {
        let request_id = uuid::Uuid::new_v4().to_string();
        let (reply_tx, reply_rx) = oneshot::channel();

        {
            let mut pending = self.pending_requests.lock().await;
            pending.push(PendingRequest {
                request_id: request_id.clone(),
                plugin_id: plugin_id.to_string(),
                paths: vec![path.to_string()],
                reply_tx,
            });
        }

        let payload = serde_json::json!({
            "requestId": request_id,
            "pluginId": plugin_id,
            "path": path,
            "operation": operation.to_string(),
        });

        // 事件未送达（无头/测试上下文无 app_handle 同样视为未送达）：请求永远不会
        // 被响应，移除已入队条目避免 pending 泄漏
        let emit_result = match &self.app_handle {
            Some(app) => app
                .emit("plugin:fs-auth-request", payload)
                .map_err(|e| format!("emit failed: {}", e)),
            None => Err("app_handle unavailable".to_string()),
        };
        if let Err(err) = emit_result {
            let mut pending = self.pending_requests.lock().await;
            pending.retain(|r| r.request_id != request_id);
            tracing::error!(error = %err, "fs_auth: failed to deliver auth request event");
            return false;
        }

        match tokio::time::timeout(
            std::time::Duration::from_secs(AUTH_TIMEOUT_SECS),
            reply_rx,
        )
        .await
        {
            Ok(Ok(allowed)) => {
                tracing::info!(plugin_id = %plugin_id, path = %path, allowed = allowed, "fs_auth: user responded");
                allowed
            }
            _ => {
                tracing::warn!(plugin_id = %plugin_id, path = %path, "fs_auth: auth request timed out or cancelled");
                let mut pending = self.pending_requests.lock().await;
                pending.retain(|r| r.request_id != request_id);
                false
            }
        }
    }

    /// 持久化授权路径前缀
    async fn save_granted_path(&self, plugin_id: &str, path: &str) -> anyhow::Result<()> {
        let storage_key = GRANTED_PATHS_KEY.to_string();
        let mut granted: Vec<serde_json::Value> = match self.storage.get(plugin_id, &storage_key).await {
            Ok(Some(serde_json::Value::Array(arr))) => arr,
            _ => Vec::new(),
        };
        // 提取路径的父目录作为前缀（更通用的授权范围）
        let prefix = if path.is_empty() {
            String::new()
        } else {
            Path::new(path)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| path.to_string())
        };
        if !prefix.is_empty() {
            granted.push(serde_json::Value::String(prefix));
        }
        self.storage.set(plugin_id, &storage_key, serde_json::Value::Array(granted)).await?;
        Ok(())
    }

    /// 规范化路径（解析 ..、符号链接等）
    fn canonicalize_path(path: &str) -> Option<PathBuf> {
        let p = Path::new(path);
        if p.exists() {
            p.canonicalize().ok()
        } else if let Some(parent) = p.parent() {
            if parent.exists() {
                let canon_parent = parent.canonicalize().ok()?;
                let file_name = p.file_name()?;
                Some(canon_parent.join(file_name))
            } else {
                Some(p.to_path_buf())
            }
        } else {
            Some(p.to_path_buf())
        }
    }
}
