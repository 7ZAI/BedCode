//! 文件服务挂载注册表
//!
//! 管理插件挂载的文件服务端点（mounts）、对端文件服务信息（peers，
//! 阶段 2 由 WS 控制面填充）、上传会话与策略钩子分发。
//!
//! 钩子分发（规格 4.2）：仅在上传会话创建时调用一次，同步阻塞握手，
//! 2 秒超时；超时/插件异常一律拒绝（fail-closed）。

use crate::plugin::file_service::cipher::{PassthroughCipher, TransportCipher};
use crate::plugin::file_service::sandbox;
use crate::plugin::file_service::upload::{self, UploadSessionManager};
use crate::plugin::fs_auth::{FsAuthChecker, FsOp};
use crate::plugin::PluginHost;
use bedcode_plugin_api::{
    FileOperation, MountOptions, PeerFileService, UploadHookDecision, UploadRequestMeta,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{oneshot, Mutex, RwLock};

/// 上传策略钩子调用超时（规格 4.2：同步阻塞握手，2 秒超时 fail-closed）
const UPLOAD_HOOK_TIMEOUT: Duration = Duration::from_secs(2);

/// 上传策略钩子目标
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookTarget {
    /// WASM 插件：调用导出 `__bedcode_on_upload_request`
    Wasm,
    /// TS-only 插件：经前端 webview 事件桥转发（阶段 3 接入 Tauri command）
    Webview,
    /// 无钩子：fail-closed 拒绝所有上传
    None,
}

/// 挂载条目
#[derive(Clone)]
pub struct MountEntry {
    /// 所属插件 ID
    pub plugin_id: String,
    /// 挂载点名称（URL 段）
    pub mount_path: String,
    /// 允许目录根（canonicalize 后，已去重取最外层）
    pub roots: Vec<PathBuf>,
    /// 允许的操作集合
    pub operations: Vec<FileOperation>,
    /// 上传策略钩子目标
    pub hook: HookTarget,
    /// 传输加密拦截器（MVP 为直通，见 cipher 模块）
    pub cipher: Arc<dyn TransportCipher>,
}

/// 文件服务注册表（AppContext 全局持有）
pub struct FileServiceRegistry {
    /// 挂载表：(plugin_id, mount_path) → 挂载条目
    mounts: RwLock<HashMap<(String, String), MountEntry>>,
    /// 对端文件服务信息表（阶段 2 使用，本阶段先建好结构）
    peers: RwLock<HashMap<String, PeerFileService>>,
    /// 上传会话管理器
    upload_sessions: Arc<UploadSessionManager>,
    /// Webview 钩子待回复表：request_id → 回复通道
    ///
    /// 阶段 3 由前端 Tauri command 经 [`respond_upload_hook`](Self::respond_upload_hook) 回填
    pending_hook_replies: Mutex<HashMap<String, oneshot::Sender<UploadHookDecision>>>,
    /// 文件系统访问校验器（挂载授权复用宿主三层策略）
    fs_auth: Arc<FsAuthChecker>,
    /// Tauri AppHandle（Webview 钩子事件发送；无头上下文为 None）
    app_handle: Option<Arc<tauri::AppHandle>>,
    /// 插件宿主（WASM 钩子调用）
    ///
    /// 两阶段注入：注册表在 PluginHost::new() 内部创建（插件 auto-activate
    /// 可能立即挂载，早于 PluginHost 自身构造完成），宿主引用在其 Arc 化后
    /// 经 [`set_plugin_host`](Self::set_plugin_host) 注入；仅上传钩子依赖它，
    /// 挂载本身不需要
    plugin_host: RwLock<Option<Arc<PluginHost>>>,
}

impl FileServiceRegistry {
    /// 创建注册表（后台 sweeper 需在 runtime 上下文内经 [`start_background_tasks`] 启动）
    ///
    /// 在 PluginHost::new() 内部构造：插件 auto-activate 阶段可能立即调用
    /// host_filesrv_mount，此时 PluginHost 尚未构造完成，宿主引用留待注入
    pub fn new(
        fs_auth: Arc<FsAuthChecker>,
        app_handle: Option<Arc<tauri::AppHandle>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            mounts: RwLock::new(HashMap::new()),
            peers: RwLock::new(HashMap::new()),
            upload_sessions: Arc::new(UploadSessionManager::new()),
            pending_hook_replies: Mutex::new(HashMap::new()),
            fs_auth,
            app_handle,
            plugin_host: RwLock::new(None),
        })
    }

    /// 两阶段注入：PluginHost Arc 化完成后注入宿主引用（仅上传钩子依赖）
    ///
    /// 调用点：`lib.rs` setup 阶段，PluginHost 构造完成并 Arc 化后
    pub async fn set_plugin_host(&self, host: Arc<PluginHost>) {
        *self.plugin_host.write().await = Some(host);
    }

    /// 启动后台任务（必须在 tokio runtime 上下文内调用一次）
    pub fn start_background_tasks(self: &Arc<Self>) {
        UploadSessionManager::spawn_sweeper(self.upload_sessions.clone());
    }

    /// 上传会话管理器引用（controller 使用）
    pub fn upload_sessions(&self) -> &Arc<UploadSessionManager> {
        &self.upload_sessions
    }

    // ==================== Mounts ====================

    /// 挂载文件服务
    ///
    /// 校验（规格 4.3）：
    /// 1. mount_path 必须匹配 `^[a-z0-9-_]+$`（URL 段安全）
    /// 2. 每个 root 必须经宿主 fs 授权（含弹窗授权）；声明 upload 时按写授权，否则读授权
    /// 3. root 必须存在且是目录，canonicalize 后去重取最外层
    /// 4. 同插件同 mount_path 重复挂载拒绝
    ///
    /// 挂载成功后扫描 roots 清理孤儿 `.bedcode-upload-*.part`
    pub async fn mount(
        &self,
        plugin_id: &str,
        options: MountOptions,
        hook: HookTarget,
    ) -> crate::Result<MountEntry> {
        validate_mount_path(&options.mount_path)?;

        if options.roots.is_empty() {
            return Err(crate::AppError::InvalidInput(format!(
                "mount '{}': roots must not be empty",
                options.mount_path
            )));
        }

        // 声明 upload 操作时挂载点具备写入能力，按写授权校验（覆盖读）
        let fs_op = if options.operations.contains(&FileOperation::Upload) {
            FsOp::Write
        } else {
            FsOp::Read
        };
        for root in &options.roots {
            if !self.fs_auth.check(plugin_id, root, fs_op).await {
                return Err(crate::AppError::Auth(format!(
                    "mount '{}': root '{}' not authorized by user",
                    options.mount_path, root
                )));
            }
        }

        let raw_roots: Vec<PathBuf> = options.roots.iter().map(PathBuf::from).collect();
        let roots = sandbox::normalize_roots(&raw_roots).map_err(|e| {
            crate::AppError::InvalidInput(format!(
                "mount '{}': invalid roots: {}",
                options.mount_path, e
            ))
        })?;

        let entry = MountEntry {
            plugin_id: plugin_id.to_string(),
            mount_path: options.mount_path.clone(),
            roots,
            operations: options.operations.clone(),
            hook,
            // MVP 直通加密缝；未来接入 E2E 加密时在此注入真实实现
            cipher: Arc::new(PassthroughCipher),
        };

        {
            let mut mounts = self.mounts.write().await;
            let key = (plugin_id.to_string(), options.mount_path.clone());
            if mounts.contains_key(&key) {
                return Err(crate::AppError::InvalidInput(format!(
                    "mount '{}' already exists for plugin '{}'",
                    options.mount_path, plugin_id
                )));
            }
            mounts.insert(key, entry.clone());
        }

        // 清理宿主异常退出遗留的孤儿临时文件（best effort，失败不影响挂载）
        let cleaned = upload::clean_orphan_parts(&entry.roots);
        if cleaned > 0 {
            tracing::info!(
                plugin_id = %plugin_id,
                mount = %options.mount_path,
                "mount: cleaned {} orphan upload temp file(s)",
                cleaned
            );
        }

        tracing::info!(
            plugin_id = %plugin_id,
            mount = %options.mount_path,
            roots = ?entry.roots,
            "file service mounted"
        );
        // 宿主自动同步挂载可用性到移动端（不经插件，规格阶段 2）
        emit_file_service_changed(plugin_id, &entry.mount_path, true, entry.operations.clone());
        Ok(entry)
    }

    /// 更新挂载点的允许目录根（目录变更即时生效，校验规则同 mount）
    pub async fn update_roots(
        &self,
        plugin_id: &str,
        mount_path: &str,
        roots: Vec<String>,
    ) -> crate::Result<()> {
        if roots.is_empty() {
            return Err(crate::AppError::InvalidInput(format!(
                "update_roots for mount '{}': roots must not be empty",
                mount_path
            )));
        }

        let fs_op = {
            let mounts = self.mounts.read().await;
            let entry = mounts
                .get(&(plugin_id.to_string(), mount_path.to_string()))
                .ok_or_else(|| {
                    crate::AppError::NotFound(format!(
                        "mount '{}' not found for plugin '{}'",
                        mount_path, plugin_id
                    ))
                })?;
            if entry.operations.contains(&FileOperation::Upload) {
                FsOp::Write
            } else {
                FsOp::Read
            }
        };

        for root in &roots {
            if !self.fs_auth.check(plugin_id, root, fs_op).await {
                return Err(crate::AppError::Auth(format!(
                    "update_roots for mount '{}': root '{}' not authorized by user",
                    mount_path, root
                )));
            }
        }

        let raw_roots: Vec<PathBuf> = roots.iter().map(PathBuf::from).collect();
        let normalized = sandbox::normalize_roots(&raw_roots).map_err(|e| {
            crate::AppError::InvalidInput(format!(
                "update_roots for mount '{}': invalid roots: {}",
                mount_path, e
            ))
        })?;

        let mut mounts = self.mounts.write().await;
        let entry = mounts
            .get_mut(&(plugin_id.to_string(), mount_path.to_string()))
            .ok_or_else(|| {
                crate::AppError::NotFound(format!(
                    "mount '{}' not found for plugin '{}'",
                    mount_path, plugin_id
                ))
            })?;
        entry.roots = normalized;
        let operations = entry.operations.clone();

        tracing::info!(
            plugin_id = %plugin_id,
            mount = %mount_path,
            roots = ?entry.roots,
            "file service roots updated"
        );
        drop(mounts);
        // 目录变更即时生效：重新同步挂载可用性（操作集不变，事件幂等）
        emit_file_service_changed(plugin_id, mount_path, true, operations);
        Ok(())
    }

    /// 卸载挂载点（同时取消该挂载下的全部上传会话）
    pub async fn unmount(&self, plugin_id: &str, mount_path: &str) -> crate::Result<()> {
        let removed = self
            .mounts
            .write()
            .await
            .remove(&(plugin_id.to_string(), mount_path.to_string()));
        if removed.is_none() {
            return Err(crate::AppError::NotFound(format!(
                "mount '{}' not found for plugin '{}'",
                mount_path, plugin_id
            )));
        }

        let cancelled = self
            .upload_sessions
            .cancel_for_mount(plugin_id, mount_path)
            .await;
        tracing::info!(
            plugin_id = %plugin_id,
            mount = %mount_path,
            cancelled_sessions = cancelled,
            "file service unmounted"
        );
        // 宿主自动同步摘除状态到移动端（unmount 时操作集置空）
        emit_file_service_changed(plugin_id, mount_path, false, Vec::new());
        Ok(())
    }

    /// 摘除插件的全部挂载（deactivate/停用/卸载时调用，"停用插件 = 服务消失"）
    pub async fn unmount_plugin(&self, plugin_id: &str) {
        let removed: Vec<String> = {
            let mut mounts = self.mounts.write().await;
            let keys: Vec<(String, String)> = mounts
                .keys()
                .filter(|(pid, _)| pid == plugin_id)
                .cloned()
                .collect();
            keys.iter()
                .filter_map(|(_, mp)| mounts.remove(&(plugin_id.to_string(), mp.clone())).map(|_| mp.clone()))
                .collect()
        };

        for mount_path in &removed {
            let cancelled = self
                .upload_sessions
                .cancel_for_mount(plugin_id, mount_path)
                .await;
            tracing::info!(
                plugin_id = %plugin_id,
                mount = %mount_path,
                cancelled_sessions = cancelled,
                "file service unmounted (plugin lifecycle)"
            );
            emit_file_service_changed(plugin_id, mount_path, false, Vec::new());
        }
    }

    /// 获取挂载条目（不存在返回 NotFound）
    pub async fn get_entry(&self, plugin_id: &str, mount_path: &str) -> crate::Result<MountEntry> {
        let mounts = self.mounts.read().await;
        mounts
            .get(&(plugin_id.to_string(), mount_path.to_string()))
            .cloned()
            .ok_or_else(|| {
                crate::AppError::NotFound(format!(
                    "mount '{}' not found for plugin '{}'",
                    mount_path, plugin_id
                ))
            })
    }

    /// 沙箱解析：挂载点相对路径 → 沙箱内绝对路径（目标必须已存在）
    ///
    /// controller 的 /list 与 /file 端点共用此校验
    pub async fn resolve_sandboxed(
        &self,
        plugin_id: &str,
        mount_path: &str,
        rel: &str,
    ) -> crate::Result<PathBuf> {
        let entry = self.get_entry(plugin_id, mount_path).await?;
        sandbox::resolve_within_roots(&entry.roots, rel).map_err(|e| {
            crate::AppError::NotFound(format!(
                "mount '{}/{}': {}",
                plugin_id, mount_path, e
            ))
        })
    }

    // ==================== Upload Hook ====================

    /// 调用上传策略钩子（fail-closed，规格 4.2）
    ///
    /// 仅在上传会话创建时调用一次；2 秒超时，任何错误/超时一律拒绝
    pub async fn call_upload_hook(
        &self,
        plugin_id: &str,
        mount_path: &str,
        meta: &UploadRequestMeta,
    ) -> UploadHookDecision {
        let hook = {
            let mounts = self.mounts.read().await;
            match mounts.get(&(plugin_id.to_string(), mount_path.to_string())) {
                Some(entry) => entry.hook.clone(),
                None => {
                    return UploadHookDecision::deny("mount not found");
                }
            }
        };

        match hook {
            HookTarget::None => UploadHookDecision::deny("mount has no upload hook"),
            HookTarget::Wasm => {
                let host = self.plugin_host.read().await.clone();
                let Some(host) = host else {
                    tracing::warn!(
                        plugin_id = %plugin_id,
                        "upload hook: plugin host not injected yet, denying (fail-closed)"
                    );
                    return UploadHookDecision::deny("plugin host not ready");
                };
                let meta_json = serde_json::to_string(meta).unwrap_or_default();
                let plugin_id = plugin_id.to_string();
                match tokio::time::timeout(
                    UPLOAD_HOOK_TIMEOUT,
                    host.call_upload_hook(&plugin_id, &meta_json),
                )
                .await
                {
                    Ok(decision) => decision,
                    Err(_) => {
                        tracing::warn!(
                            plugin_id = %plugin_id,
                            mount = %mount_path,
                            "upload hook timed out (2s), denying (fail-closed)"
                        );
                        UploadHookDecision::deny("upload hook timed out")
                    }
                }
            }
            HookTarget::Webview => self.call_webview_hook(plugin_id, mount_path, meta).await,
        }
    }

    /// Webview 钩子：emit 事件到前端 + oneshot 等待回复（2 秒超时 fail-closed）
    ///
    /// 前端插件经 `filesrv:upload_request` 事件收到请求，回调后经 Tauri command
    /// `plugin_filesrv_respond_upload_request`（见 commands/file_service.rs）调用
    /// [`respond_upload_hook`] 回填决定；超时未回填一律拒绝
    async fn call_webview_hook(
        &self,
        plugin_id: &str,
        mount_path: &str,
        meta: &UploadRequestMeta,
    ) -> UploadHookDecision {
        use tauri::Emitter;

        let Some(app_handle) = self.app_handle.as_ref() else {
            return UploadHookDecision::deny("webview hook unavailable in headless context");
        };

        let request_id = uuid::Uuid::new_v4().to_string();
        let (reply_tx, reply_rx) = oneshot::channel();
        self.pending_hook_replies
            .lock()
            .await
            .insert(request_id.clone(), reply_tx);

        let payload = serde_json::json!({
            "requestId": request_id,
            "pluginId": plugin_id,
            "mountPath": mount_path,
            "meta": meta,
        });
        if let Err(e) = app_handle.emit("filesrv:upload_request", payload) {
            self.pending_hook_replies.lock().await.remove(&request_id);
            tracing::error!(
                plugin_id = %plugin_id,
                "webview upload hook emit failed: {}",
                e
            );
            return UploadHookDecision::deny("webview hook emit failed");
        }

        match tokio::time::timeout(UPLOAD_HOOK_TIMEOUT, reply_rx).await {
            Ok(Ok(decision)) => decision,
            _ => {
                self.pending_hook_replies.lock().await.remove(&request_id);
                tracing::warn!(
                    plugin_id = %plugin_id,
                    mount = %mount_path,
                    "webview upload hook timed out (2s), denying (fail-closed)"
                );
                UploadHookDecision::deny("webview upload hook timed out")
            }
        }
    }

    /// 回填 Webview 钩子决定（阶段 3 的 Tauri command 调用；request 不存在返回 false）
    pub async fn respond_upload_hook(&self, request_id: &str, decision: UploadHookDecision) -> bool {
        let tx = self.pending_hook_replies.lock().await.remove(request_id);
        match tx {
            Some(tx) => tx.send(decision).is_ok(),
            None => false,
        }
    }

    // ==================== Peers（阶段 2 使用） ====================

    /// 登记对端文件服务信息（WS 控制面公告时调用）
    ///
    /// 与旧信息比较，有变化时经双通道推送 `filesrv:peer_changed`
    /// （Tauri 事件 + 插件消息总线），供内网文件传输插件被动感知对端上线
    pub async fn set_peer(&self, peer_id: &str, info: PeerFileService) {
        let changed = {
            let mut peers = self.peers.write().await;
            let changed = match peers.get(peer_id) {
                Some(old) => peer_info_changed(old, &info),
                None => true,
            };
            if changed {
                peers.insert(peer_id.to_string(), info);
            }
            changed
        };
        if changed {
            self.emit_peer_changed(peer_id, true).await;
        } else {
            tracing::debug!(peer_id = %peer_id, "set_peer: no change, skip push");
        }
    }

    /// 获取对端文件服务信息
    pub async fn get_peer(&self, peer_id: &str) -> Option<PeerFileService> {
        self.peers.read().await.get(peer_id).cloned()
    }

    /// 移除对端信息（对端下线/解除配对时调用）
    ///
    /// 记录存在时经双通道推送 `filesrv:peer_changed`（online=false），
    /// 供内网文件传输插件被动感知对端下线
    pub async fn remove_peer(&self, peer_id: &str) {
        let existed = self.peers.write().await.remove(peer_id).is_some();
        if existed {
            self.emit_peer_changed(peer_id, false).await;
        } else {
            tracing::debug!(peer_id = %peer_id, "remove_peer: not present, skip push");
        }
    }

    /// 双通道推送对端在线状态变更（Tauri 事件 + 插件消息总线）
    ///
    /// 发射失败只 warn，不影响主流程（约束：事件通道为 best-effort 通知）
    async fn emit_peer_changed(&self, peer_id: &str, online: bool) {
        use tauri::Emitter;

        let payload = serde_json::json!({
            "peerId": peer_id,
            "online": online,
        });

        // 通道 1：Tauri 事件（前端 UI 订阅，如对端状态角标）
        if let Some(app_handle) = self.app_handle.as_ref() {
            if let Err(e) = app_handle.emit("filesrv:peer_changed", &payload) {
                tracing::warn!(
                    peer_id = %peer_id,
                    online = online,
                    "emit filesrv:peer_changed failed: {}",
                    e
                );
            }
        }

        // 通道 2：插件消息总线（WASM 插件后端经 host_bus_subscribe 订阅）
        let host = self.plugin_host.read().await.clone();
        if let Some(host) = host {
            host.message_bus()
                .publish("filesrv:peer_changed", "host", payload);
        } else {
            tracing::debug!(
                peer_id = %peer_id,
                "plugin host not injected yet, bus publish skipped"
            );
        }

        tracing::info!(peer_id = %peer_id, online = online, "peer_changed pushed");
    }
}

/// 发射文件服务挂载可用性变更事件（经 SyncData 广播同步到移动端）
///
/// 宿主自动发出、不经插件（规格阶段 2）；无头环境（AppContext 未初始化，
/// 如纯单测）静默跳过
fn emit_file_service_changed(
    plugin_id: &str,
    mount_path: &str,
    available: bool,
    operations: Vec<FileOperation>,
) {
    let Some(ctx) = crate::system::app_context::AppContext::try_global() else {
        return;
    };
    if let Err(e) = ctx
        .sync_tx()
        .send(crate::events::DesktopSyncEvent::FileServiceChanged {
            plugin_id: plugin_id.to_string(),
            mount_path: mount_path.to_string(),
            available,
            operations,
        })
    {
        // 无接收者（移动端未连接）是常态，仅 debug
        tracing::debug!(
            plugin_id = %plugin_id,
            mount = %mount_path,
            "file service changed event not delivered: {}",
            e
        );
    }
}

/// 比较新旧对端信息是否有变化（用于去重：重复 Announce 相同内容时不重复推送）
///
/// 比较维度：IP、端口、Token、挂载点列表（按 plugin_id+mount_path 排序后比较 operations）
fn peer_info_changed(old: &PeerFileService, new: &PeerFileService) -> bool {
    if old.ip != new.ip || old.port != new.port || old.token != new.token {
        return true;
    }
    if old.mounts.len() != new.mounts.len() {
        return true;
    }
    // 挂载列表按 (plugin_id, mount_path) 排序后逐条比较 operations
    let mut old_mounts: Vec<_> = old.mounts.iter().collect();
    let mut new_mounts: Vec<_> = new.mounts.iter().collect();
    old_mounts.sort_by(|a, b| (&a.plugin_id, &a.mount_path).cmp(&(&b.plugin_id, &b.mount_path)));
    new_mounts.sort_by(|a, b| (&a.plugin_id, &a.mount_path).cmp(&(&b.plugin_id, &b.mount_path)));
    for (o, n) in old_mounts.iter().zip(new_mounts.iter()) {
        if o.plugin_id != n.plugin_id
            || o.mount_path != n.mount_path
            || o.operations != n.operations
        {
            return true;
        }
    }
    false
}

/// 校验挂载点名称：必须匹配 `^[a-z0-9-_]+$`（URL 段安全，防止路径注入）
fn validate_mount_path(mount_path: &str) -> crate::Result<()> {
    const MAX_LEN: usize = 64;
    if mount_path.is_empty() || mount_path.len() > MAX_LEN {
        return Err(crate::AppError::InvalidInput(format!(
            "mount path must be 1-{} chars, got {} chars",
            MAX_LEN,
            mount_path.len()
        )));
    }
    let valid = mount_path
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
    if !valid {
        return Err(crate::AppError::InvalidInput(format!(
            "mount path '{}' must match ^[a-z0-9-_]+$",
            mount_path
        )));
    }
    Ok(())
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_mount_path() {
        assert!(validate_mount_path("files").is_ok());
        assert!(validate_mount_path("media-2_go").is_ok());
        assert!(validate_mount_path("").is_err());
        assert!(validate_mount_path("Files").is_err());
        assert!(validate_mount_path("my files").is_err());
        assert!(validate_mount_path("../evil").is_err());
        assert!(validate_mount_path("a/b").is_err());
        assert!(validate_mount_path(&"x".repeat(65)).is_err());
    }
}
