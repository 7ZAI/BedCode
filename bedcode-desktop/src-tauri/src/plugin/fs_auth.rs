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
    /// 请求授权的文件路径（单路径请求为单元素；批量请求含全部未授权路径）
    paths: Vec<String>,
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
    /// Tauri AppHandle（用于发送弹窗事件；无头上下文如测试中为 None，弹窗层直接拒绝）
    app_handle: Option<Arc<tauri::AppHandle>>,
}

impl FsAuthChecker {
    /// 创建文件访问校验器
    ///
    /// `app_handle` 为 None 时（无头/测试上下文）弹窗授权层不可用，直接拒绝
    pub fn new(
        storage: Arc<PluginStorage>,
        app_handle: Option<Arc<tauri::AppHandle>>,
    ) -> Self {
        // 路径白名单：.claude/ 子目录（Claude Code 配置目录）
        // 不在此处硬编码绝对路径，运行时动态匹配路径后缀
        let path_whitelist = Vec::new();

        // 插件白名单：受信任的内置插件
        // - auto-task: 自动化任务插件，操作预配置目录
        // - file-transfer: 内网文件传输插件，共享目录由用户在插件设置页显式配置，
        //   信任模型 = 配对 + 用户显式配置的目录白名单，插件自身第一方可信
        let mut plugin_whitelist = HashSet::new();
        plugin_whitelist.insert("com.bedcode.auto-task".to_string());
        plugin_whitelist.insert("com.bedcode.file-transfer".to_string());

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
                for path in &request.paths {
                    if let Err(e) = self.save_granted_path(&request.plugin_id, path).await {
                        tracing::warn!("fs_auth: failed to save granted path: {}", e);
                    }
                }
            }
            let _ = request.reply_tx.send(allowed);
        }
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
                Some(c) => c,
                None => {
                    tracing::warn!(plugin_id = %plugin_id, path = %path, "fs_auth: path canonicalization failed");
                    return false;
                }
            };

            // 路径白名单 / 插件白名单 / 已授权前缀 → 直接放行
            if self.match_path_whitelist(&canonical) {
                continue;
            }
            if self.plugin_whitelist.contains(plugin_id) {
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

        // 无头上下文（测试）没有 AppHandle，无法弹窗：移除已入队请求，保守拒绝
        let Some(app_handle) = self.app_handle.as_ref() else {
            let mut pending = self.pending_requests.lock().await;
            pending.retain(|r| r.request_id != request_id);
            tracing::warn!(
                plugin_id = %plugin_id,
                "fs_auth: no app_handle in headless context, denying auth request"
            );
            return false;
        };

        if let Err(e) = app_handle.emit("plugin:fs-auth-request", payload) {
            // 事件未送达前端：请求永远不会被响应，移除已入队条目避免 pending 泄漏
            let mut pending = self.pending_requests.lock().await;
            pending.retain(|r| r.request_id != request_id);
            tracing::error!(error = %e, "fs_auth: failed to emit auth request event");
            return false;
        }

        // 等待用户回复（超时 30 秒自动拒绝）
        match tokio::time::timeout(std::time::Duration::from_secs(30), reply_rx).await {
            Ok(Ok(allowed)) => {
                tracing::info!(
                    plugin_id = %plugin_id,
                    paths = ?paths,
                    allowed = allowed,
                    "fs_auth: user responded (batch)"
                );
                allowed
            }
            _ => {
                tracing::warn!(
                    plugin_id = %plugin_id,
                    paths = ?paths,
                    "fs_auth: batch auth request timed out or cancelled"
                );
                let mut pending = self.pending_requests.lock().await;
                pending.retain(|r| r.request_id != request_id);
                false
            }
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
                // 与检查路径同一规范化（含 \?\ 剥离），保证两端格式一致
                if let Some(prefix_path) = Self::canonicalize_path(prefix_str) {
                    // Path::strip_prefix 按组件剥离：成功即表示 canonical 位于授权前缀之下，
                    // 组件边界天然防止 `.bedcode` 误匹配 `.bedcode-other` 这类相邻目录
                    if canonical.strip_prefix(&prefix_path).is_ok() {
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

        // 发送弹窗事件到前端
        let payload = serde_json::json!({
            "requestId": request_id,
            "pluginId": plugin_id,
            "path": path,
            "operation": operation.to_string(),
        });

        // 无头上下文（测试）没有 AppHandle，无法弹窗：移除已入队请求，保守拒绝
        let Some(app_handle) = self.app_handle.as_ref() else {
            let mut pending = self.pending_requests.lock().await;
            pending.retain(|r| r.request_id != request_id);
            tracing::warn!(
                plugin_id = %plugin_id,
                path = %path,
                "fs_auth: no app_handle in headless context, denying auth request"
            );
            return false;
        };

        if let Err(e) = app_handle.emit("plugin:fs-auth-request", payload) {
            // 事件未送达前端：请求永远不会被响应，移除已入队条目避免 pending 泄漏
            let mut pending = self.pending_requests.lock().await;
            pending.retain(|r| r.request_id != request_id);
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
    ///
    /// Windows 上 `canonicalize` 返回 `\\?\C:\...` verbatim 格式，而 fallback
    /// 分支（父目录尚不存在）只能返回普通路径——两者格式不一致会导致与已授权
    /// 前缀的匹配失败（首次写入新子目录文件时误弹窗）。此处统一剥掉 `\\?\` 前缀。
    fn canonicalize_path(path: &str) -> Option<PathBuf> {
        let p = Path::new(path);
        // 文件可能不存在（如即将写入的文件），使用父目录 canonicalize
        let result = if p.exists() {
            p.canonicalize().ok()
        } else if let Some(parent) = p.parent() {
            // 父目录可能存在
            if parent.exists() {
                let canon_parent = parent.canonicalize().ok()?;
                let file_name = p.file_name()?;
                Some(canon_parent.join(file_name))
            } else {
                // 父目录也不存在：直接使用路径（后续 fs_write 会创建）。
                // 规范化分隔符——canonicalize 在 Windows 上统一为 `\`，
                // 否则与已授权前缀的匹配会因 `/` 与 `\` 混用而失败
                let raw = p.to_string_lossy();
                #[cfg(windows)]
                let normalized = PathBuf::from(raw.replace('/', "\\"));
                #[cfg(not(windows))]
                let normalized = PathBuf::from(raw.into_owned());
                Some(normalized)
            }
        } else {
            Some(p.to_path_buf())
        };
        result.map(|pb| strip_verbatim_prefix(&pb))
    }
}

/// 剥掉 Windows canonicalize 的 `\\?\` verbatim 前缀，统一路径格式
#[cfg(windows)]
fn strip_verbatim_prefix(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        path.to_path_buf()
    }
}

#[cfg(not(windows))]
fn strip_verbatim_prefix(path: &Path) -> PathBuf {
    path.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::plugin::storage::PluginStorage;
    use std::sync::Arc;

    /// 内存数据库 + 无头 AppHandle（None）的校验器：无法弹窗，未授权路径应保守拒绝
    async fn headless_checker() -> FsAuthChecker {
        let db = Database::new(&std::path::Path::new(":memory:")).unwrap();
        db.init_schema().unwrap();
        // Mutex 为 tokio::sync::Mutex（super::* 引入），与 PluginStorage 签名一致
        FsAuthChecker::new(Arc::new(PluginStorage::new(Arc::new(Mutex::new(db)))), None)
    }

    #[tokio::test]
    async fn check_batch_whitelist_path_bypasses_dialog() {
        let checker = headless_checker().await;
        // 路径白名单（.claude/ 目录段）命中 → 直接放行，无需弹窗
        let path = std::env::temp_dir()
            .join(".claude")
            .join("settings.json")
            .to_string_lossy()
            .to_string();
        assert!(checker.check_batch("com.bedcode.test", &[path], FsOp::Read).await);
        assert!(checker.pending_requests.lock().await.is_empty());
    }

    #[tokio::test]
    async fn check_batch_ungranted_headless_denied_and_pending_cleaned() {
        let checker = headless_checker().await;
        // 未授权路径 + 无头上下文：保守拒绝，且不残留 pending 条目（泄漏回归）
        let path = std::env::temp_dir().to_string_lossy().to_string();
        assert!(!checker.check_batch("com.bedcode.test", &[path], FsOp::Read).await);
        assert!(checker.pending_requests.lock().await.is_empty());
    }

    #[tokio::test]
    async fn check_batch_empty_paths_returns_true() {
        let checker = headless_checker().await;
        assert!(checker.check_batch("com.bedcode.test", &[], FsOp::Read).await);
        assert!(checker.pending_requests.lock().await.is_empty());
    }

    /// canonicalize_path：父目录也不存在（首次写入新子目录文件）时应规范化分隔符
    #[test]
    fn canonicalize_path_normalizes_separators() {
        let fake = format!(
            "{}/sub-not-exist/deep-not-exist/file.jsonl",
            std::env::temp_dir().to_string_lossy()
        );
        let canon = FsAuthChecker::canonicalize_path(&fake).expect("fallback must succeed");
        #[cfg(windows)]
        assert!(
            !canon.to_string_lossy().contains('/'),
            "fallback path must use backslash on Windows"
        );
        assert_eq!(canon.to_string_lossy().as_ref(), fake.replace('/', "\\"));
    }

    /// 已授权前缀：边界匹配 + 尚不存在的子路径（混合分隔符）也应放行
    #[tokio::test]
    async fn granted_path_prefix_respects_separator_boundary() {
        let checker = headless_checker().await;
        let base = std::env::temp_dir();
        let granted_dir = base.join("fs-auth-granted");
        std::fs::create_dir_all(&granted_dir).unwrap();
        let granted = granted_dir.to_string_lossy().to_string();

        // 保存授权前缀（父目录形式）
        checker
            .save_granted_path("com.bedcode.test", &format!("{}/data.jsonl", granted))
            .await
            .unwrap();

        // 前缀内、尚不存在的子目录 + 混合分隔符 → 放行（回归首次写新目录场景）
        let inside = format!("{}/conversations/new.jsonl", granted);
        assert!(checker.check_batch("com.bedcode.test", &[inside], FsOp::Write).await);

        // 相邻目录（前缀后紧跟非分隔符）不放行
        let adjacent = format!("{}2/file.jsonl", granted);
        assert!(!checker.check_batch("com.bedcode.test", &[adjacent], FsOp::Write).await);

        std::fs::remove_dir_all(&granted_dir).unwrap();
    }
}
