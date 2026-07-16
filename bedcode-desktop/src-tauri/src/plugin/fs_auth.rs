//! 文件系统访问校验器
//!
//! 三层策略：路径白名单 → 插件白名单 → 弹窗授权
//!
//! 插件请求文件访问时，按优先级校验：
//! 1. 路径白名单：预定义安全路径前缀，匹配即放行
//! 2. 插件白名单：受信任的内置插件直接放行
//! 3. 弹窗授权：询问用户，授权后记住路径前缀

use crate::plugin::storage::PluginStorage;
use std::collections::HashSet;
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
    /// 请求 ID（UUID）
    request_id: String,
    /// 请求授权的插件 ID
    plugin_id: String,
    /// 请求授权的文件路径
    path: String,
    /// 回复通道
    reply_tx: oneshot::Sender<bool>,
}

/// 文件系统访问校验器
pub struct FsAuthChecker {
    /// 路径白名单前缀列表（canonicalize 后的绝对路径）
    path_whitelist: Vec<PathBuf>,
    /// 插件白名单（plugin_id → true）
    plugin_whitelist: HashSet<String>,
    /// 插件存储（持久化已授权路径）
    storage: Arc<PluginStorage>,
    /// 待处理的弹窗授权请求
    pending_requests: Arc<Mutex<Vec<PendingRequest>>>,
    /// Tauri AppHandle（用于发送弹窗事件）
    app_handle: Arc<tauri::AppHandle>,
}

impl FsAuthChecker {
    /// 创建文件访问校验器
    pub fn new(
        storage: Arc<PluginStorage>,
        app_handle: Arc<tauri::AppHandle>,
    ) -> Self {
        // 路径白名单：.claude/ 子目录（Claude Code 配置目录）
        // 不在此处硬编码绝对路径，运行时动态匹配路径后缀
        let path_whitelist = Vec::new();

        // 插件白名单：受信任的内置插件
        let mut plugin_whitelist = HashSet::new();
        plugin_whitelist.insert("com.bedcode.auto-task".to_string());

        Self {
            path_whitelist,
            plugin_whitelist,
            storage,
            pending_requests: Arc::new(Mutex::new(Vec::new())),
            app_handle,
        }
    }

    /// 校验文件访问权限
    ///
    /// 返回 true 表示允许访问，false 表示拒绝
    pub async fn check(&self, plugin_id: &str, path: &str, operation: FsOp) -> bool {
        let canonical = match Self::canonicalize_path(path) {
            Some(p) => p,
            None => {
                tracing::warn!(
                    plugin_id = %plugin_id,
                    path = %path,
                    "fs_auth: path canonicalization failed"
                );
                return false;
            }
        };

        // 第一层：路径白名单
        if self.match_path_whitelist(&canonical) {
            tracing::debug!(
                plugin_id = %plugin_id,
                path = %path,
                "fs_auth: allowed by path whitelist"
            );
            return true;
        }

        // 第二层：插件白名单
        if self.plugin_whitelist.contains(plugin_id) {
            tracing::debug!(
                plugin_id = %plugin_id,
                path = %path,
                "fs_auth: allowed by plugin whitelist"
            );
            return true;
        }

        // 第三层：已授权路径前缀（持久化）
        if self.check_granted_path(plugin_id, &canonical).await {
            tracing::debug!(
                plugin_id = %plugin_id,
                path = %path,
                "fs_auth: allowed by previously granted path"
            );
            return true;
        }

        // 第三层：弹窗授权
        self.request_user_auth(plugin_id, path, operation).await
    }

    /// 处理用户授权回复（由前端 Tauri command 调用）
    pub async fn respond(&self, request_id: &str, allowed: bool, remember: bool) {
        let mut pending = self.pending_requests.lock().await;
        if let Some(idx) = pending.iter().position(|r| r.request_id == request_id) {
            let request = pending.remove(idx);
            if allowed && remember {
                if let Err(e) = self.save_granted_path(&request.plugin_id, &request.path).await {
                    tracing::warn!("fs_auth: failed to save granted path: {}", e);
                }
            }
            let _ = request.reply_tx.send(allowed);
        }
    }

    /// 路径白名单匹配
    ///
    /// 匹配规则：路径中包含 `.claude/` 目录段，或以插件数据目录为前缀
    fn match_path_whitelist(&self, canonical: &Path) -> bool {
        let path_str = canonical.to_string_lossy();

        // 匹配 .claude/ 目录（跨平台：/ 和 \）
        let separators = ['/', '\\'];
        for sep in separators {
            if path_str.contains(&format!("{}.claude{}", sep, sep)) {
                return true;
            }
            // 路径以 .claude 结尾的目录
            if path_str.ends_with(&format!("{}.claude", sep)) {
                return true;
            }
        }

        // 匹配插件数据目录前缀
        for prefix in &self.path_whitelist {
            if canonical.starts_with(prefix) {
                return true;
            }
        }

        false
    }

    /// 检查已授权路径前缀
    async fn check_granted_path(&self, plugin_id: &str, canonical: &Path) -> bool {
        let storage_key = format!("fs_granted_paths");
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
                path: path.to_string(),
                reply_tx,
            });
        }

        // 发送弹窗事件到前端
        let payload = serde_json::json!({
            "requestId": request_id,
            "pluginId": plugin_id,
            "path": path,
            "operation": operation.to_string(),
        });

        if let Err(e) = self.app_handle.emit("plugin:fs-auth-request", payload) {
            tracing::error!(error = %e, "fs_auth: failed to emit auth request event");
            return false;
        }

        // 等待用户回复（超时 30 秒自动拒绝）
        match tokio::time::timeout(
            std::time::Duration::from_secs(30),
            reply_rx,
        )
        .await
        {
            Ok(Ok(allowed)) => {
                tracing::info!(
                    plugin_id = %plugin_id,
                    path = %path,
                    allowed = allowed,
                    "fs_auth: user responded"
                );
                allowed
            }
            _ => {
                tracing::warn!(
                    plugin_id = %plugin_id,
                    path = %path,
                    "fs_auth: auth request timed out or cancelled"
                );
                // 超时后移除 pending request
                let mut pending = self.pending_requests.lock().await;
                pending.retain(|r| r.request_id != request_id);
                false
            }
        }
    }

    /// 持久化授权路径前缀
    async fn save_granted_path(
        &self,
        plugin_id: &str,
        path: &str,
    ) -> anyhow::Result<()> {
        let storage_key = "fs_granted_paths".to_string();

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

        self.storage
            .set(plugin_id, &storage_key, serde_json::Value::Array(granted))
            .await?;

        Ok(())
    }

    /// 规范化路径（解析 ..、符号链接等）
    fn canonicalize_path(path: &str) -> Option<PathBuf> {
        let p = Path::new(path);
        // 文件可能不存在（如即将写入的文件），使用父目录 canonicalize
        if p.exists() {
            p.canonicalize().ok()
        } else if let Some(parent) = p.parent() {
            // 父目录可能存在
            if parent.exists() {
                let canon_parent = parent.canonicalize().ok()?;
                let file_name = p.file_name()?;
                Some(canon_parent.join(file_name))
            } else {
                // 父目录也不存在，直接使用路径（后续 fs_write 会创建）
                Some(p.to_path_buf())
            }
        } else {
            Some(p.to_path_buf())
        }
    }
}
