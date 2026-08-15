//! 文件服务挂载注册表
//!
//! 管理插件挂载的文件服务端点（mounts）、对端文件服务信息（peers，
//! 阶段 2 由 WS 控制面填充）、上传会话与策略钩子分发。
//!
//! 钩子分发（规格 4.2）：仅在上传会话创建时调用一次，同步阻塞握手，
//! 2 秒超时；超时/插件异常一律拒绝（fail-closed）。

use crate::enums::file_service::MountAnnouncement;
use crate::plugin::file_service::cipher::{PassthroughCipher, TransportCipher};
use crate::plugin::file_service::sandbox;
use crate::plugin::file_service::transfer::{
    self, transition_batch, BatchDecision, BatchError, BatchState, TransferBatch, TransferRequestDto,
    APPROVED_BATCH_TTL, DEFAULT_APPROVAL_TIMEOUT_SECS, MAX_APPROVAL_TIMEOUT_SECS,
    MIN_APPROVAL_TIMEOUT_SECS,
};
use crate::plugin::file_service::upload::{self, UploadSessionError, UploadSessionManager};
use crate::plugin::fs_auth::{FsAuthChecker, FsOp};
use crate::plugin::PluginHost;
use bedcode_plugin_api::{
    FileOperation, MountOptions, PeerFileService, TransferRequestMeta, UploadHookDecision,
    UploadRequestMeta,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{oneshot, Mutex, RwLock};

/// 批内文件清单数量上限（信任边界：防对端超大清单滥用，hook 前 fail-fast）
const MAX_BATCH_FILES: usize = 1000;

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
    /// 供对端 browse/download（只读暴露）；旧语义中 Upload 也落此，已被
    /// `downloads_dir` 取代（spec 方向模型：接收落点 = 下载目录，不落共享 roots）。
    pub roots: Vec<PathBuf>,
    /// 允许的操作集合
    pub operations: Vec<FileOperation>,
    /// 上传策略钩子目标
    pub hook: HookTarget,
    /// 传输加密拦截器（MVP 为直通，见 cipher 模块）
    pub cipher: Arc<dyn TransportCipher>,
    /// 接收落点（接收对端 upload 的目录，对齐 spec“下载目录 = 接收落点语义”
    /// 与移动端 MediaStore.Downloads 设计对称）。声明 Upload 时优先用此解析目标路径，
    /// 跳 roots 沙箱；为 None（旧插件未传）时回退到 roots 语义保后兼容。
    pub downloads_dir: Option<PathBuf>,
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
    /// Webview 批钩子待回复表：request_id → 回复通道（v2）
    ///
    /// 与 pending_hook_replies 分离：批钩子与上传钩子可并发等待，
    /// 共用一个 map 会让两者的 request_id 互相覆盖
    pending_transfer_replies: Mutex<HashMap<String, oneshot::Sender<UploadHookDecision>>>,
    /// 传输批表：batch_id → 批记录（v2，内存态不持久化）
    batches: RwLock<HashMap<String, TransferBatch>>,
    /// per-(plugin, mount) 批准超时（v2，10–600s，默认 60）
    approval_timeouts: RwLock<HashMap<(String, String), Duration>>,
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
            pending_transfer_replies: Mutex::new(HashMap::new()),
            batches: RwLock::new(HashMap::new()),
            approval_timeouts: RwLock::new(HashMap::new()),
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
        // v2：传输批 sweeper（pending 超时自动拒绝 / approved 24h 清理）
        self.spawn_batch_sweeper();
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

        // 接收落点（downloads_dir，spec 方向模型：上传接收不落共享 roots，落专设
        // 下载目录）。与 roots 同级校验：过宿主写授权（对端 upload 的写入边界，
        // 绕过等于任意插件可声明任意绝对路径为落点）、canonicalize 且必须是目录
        // （fail-fast，避免上传时才 500）。空字符串视作未传（回退 roots 语义）
        let downloads_dir = match options.downloads_dir.as_deref().filter(|s| !s.is_empty()) {
            Some(dir) => {
                if !self.fs_auth.check(plugin_id, dir, FsOp::Write).await {
                    return Err(crate::AppError::Auth(format!(
                        "mount '{}': downloads_dir '{}' not authorized by user",
                        options.mount_path, dir
                    )));
                }
                let canonical = PathBuf::from(dir).canonicalize().map_err(|e| {
                    crate::AppError::InvalidInput(format!(
                        "mount '{}': downloads_dir '{}' not accessible: {}",
                        options.mount_path, dir, e
                    ))
                })?;
                if !canonical.is_dir() {
                    return Err(crate::AppError::InvalidInput(format!(
                        "mount '{}': downloads_dir '{}' is not a directory",
                        options.mount_path, canonical.display()
                    )));
                }
                Some(canonical)
            }
            None => None,
        };

        let entry = MountEntry {
            plugin_id: plugin_id.to_string(),
            mount_path: options.mount_path.clone(),
            roots,
            operations: options.operations.clone(),
            hook,
            // MVP 直通加密缝；未来接入 E2E 加密时在此注入真实实现
            cipher: Arc::new(PassthroughCipher),
            // 接收落点：canonical 绝对路径，上传目标解析经 resolve_upload_target_within_roots
            // 再校验（父目录 canonicalize + starts_with，拦 symlink 逃逸）
            downloads_dir,
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

        // 清理宿主异常退出遗留的孤儿临时文件：后台扫描（best effort，失败不影响挂载）。
        // 大目录（NAS/深目录）扫描可能耗时数十秒，不能阻塞 wasm 挂载调用——
        // 慢宿主工作移出调用路径后，宿主延迟与插件执行预算彻底解耦
        spawn_orphan_cleanup(plugin_id, &options.mount_path, entry.roots.clone());

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

    // ==================== Transfer Batch（v2 异步批量批准） ====================

    /// 处理 POST /transfer-request：批钩子三路分流 → 建批 / 202 / 403（spec 2.1）
    ///
    /// - allow → 建批 Approved（200，发送方可立即建 session）
    /// - ask → 建批 Pending + 本地事件 `filesrv:transfer_request`（202）
    /// - deny → 不建批无记录，Err(PolicyDenied(reason))（403）
    ///
    /// 钩子 fail-closed：超时/插件异常/挂载不存在一律 deny（复用 UPLOAD_HOOK_TIMEOUT）
    pub async fn create_transfer_request(
        &self,
        plugin_id: &str,
        mount_path: &str,
        req: &TransferRequestDto,
    ) -> Result<BatchDecision, BatchError> {
        // 信任边界校验（对端可控输入，hook 调用前 fail-fast）：批 ID 非空限长、
        // 清单非空且数量有上限（防空批污染批表 / 超大清单滥用）；路径/大小逐项
        // 不校验（仅元数据展示，session 创建时的沙箱与钩子才是写路径守卫）
        if req.batch_id.is_empty() || req.batch_id.len() > 128 {
            return Err(BatchError::GatingDenied("invalid batch id".to_string()));
        }
        if req.files.is_empty() || req.files.len() > MAX_BATCH_FILES {
            return Err(BatchError::GatingDenied("invalid batch file list".to_string()));
        }
        let decision = self.call_transfer_hook(plugin_id, mount_path, req).await;

        if decision.allow {
            self.insert_batch(plugin_id, mount_path, req, BatchState::Approved)
                .await;
            tracing::info!(
                batch_id = %req.batch_id,
                plugin_id = %plugin_id,
                mount = %mount_path,
                files = req.files.len(),
                "transfer batch approved by hook"
            );
            Ok(BatchDecision::Approved)
        } else if decision.ask {
            self.insert_batch(plugin_id, mount_path, req, BatchState::Pending)
                .await;
            // ask 分流：通知接收端插件（pending 卡 + 批级 toast 数据源）
            self.emit_filesrv_event(
                "filesrv:transfer_request",
                serde_json::json!({
                    "batchId": req.batch_id,
                    "pluginId": plugin_id,
                    "mountPath": mount_path,
                    "files": req.files,
                    "totalSize": req.total_size,
                }),
            )
            .await;
            tracing::info!(
                batch_id = %req.batch_id,
                plugin_id = %plugin_id,
                mount = %mount_path,
                files = req.files.len(),
                "transfer batch pending (ask), awaiting user response"
            );
            Ok(BatchDecision::Pending)
        } else {
            let reason = decision
                .reason
                .unwrap_or_else(|| "transfer request denied by hook".to_string());
            tracing::info!(
                batch_id = %req.batch_id,
                plugin_id = %plugin_id,
                mount = %mount_path,
                reason = %reason,
                "transfer request denied by batch hook"
            );
            Err(BatchError::PolicyDenied(reason))
        }
    }

    /// 建批记录（批准超时从 per-mount 配置快照；批不持久化）
    async fn insert_batch(
        &self,
        plugin_id: &str,
        mount_path: &str,
        req: &TransferRequestDto,
        state: BatchState,
    ) {
        let timeout = {
            let timeouts = self.approval_timeouts.read().await;
            timeouts
                .get(&(plugin_id.to_string(), mount_path.to_string()))
                .copied()
                .unwrap_or_else(|| Duration::from_secs(DEFAULT_APPROVAL_TIMEOUT_SECS))
        };
        let batch = TransferBatch {
            batch_id: req.batch_id.clone(),
            plugin_id: plugin_id.to_string(),
            mount_path: mount_path.to_string(),
            files: req.files.clone(),
            total_size: req.total_size,
            state,
            created_at: Instant::now(),
            last_active: Instant::now(),
            approval_timeout: timeout,
        };
        self.batches.write().await.insert(req.batch_id.clone(), batch);
    }

    /// 应答命令：pending → approved（校验归属 plugin）
    ///
    /// 他插件应答同一批 → NotFound（不泄露存在性，spec §3.3）。
    /// 迁移成功后发布 resolved + 跨端推送（发送方据此调度批内任务，spec 14.2）；
    /// 与移动端 registry.approve_transfer 对称，缺发布会导致发送方永远等不到应答
    pub async fn approve_transfer(
        &self,
        plugin_id: &str,
        batch_id: &str,
    ) -> Result<(), BatchError> {
        {
            let mut batches = self.batches.write().await;
            let batch = batches.get_mut(batch_id).ok_or_else(|| {
                BatchError::NotFound(format!("transfer batch not found: {}", batch_id))
            })?;
            if batch.plugin_id != plugin_id {
                return Err(BatchError::NotFound(format!(
                    "transfer batch not found: {}",
                    batch_id
                )));
            }
            transition_batch(batch, BatchState::Approved)?;
        }
        tracing::info!(batch_id = %batch_id, plugin_id = %plugin_id, "transfer batch approved by user");
        self.publish_batch_resolved(batch_id, "approved", "").await;
        Ok(())
    }

    /// 应答命令：pending → rejected(UserRejected)（校验归属 plugin）
    ///
    /// 迁移成功后发布 resolved + 跨端推送（发送方据此置批内任务 rejected，spec 14.2）
    pub async fn reject_transfer(
        &self,
        plugin_id: &str,
        batch_id: &str,
    ) -> Result<(), BatchError> {
        {
            let mut batches = self.batches.write().await;
            let batch = batches.get_mut(batch_id).ok_or_else(|| {
                BatchError::NotFound(format!("transfer batch not found: {}", batch_id))
            })?;
            if batch.plugin_id != plugin_id {
                return Err(BatchError::NotFound(format!(
                    "transfer batch not found: {}",
                    batch_id
                )));
            }
            transition_batch(
                batch,
                BatchState::Rejected {
                    reason: transfer::RejectReason::UserRejected,
                },
            )?;
        }
        tracing::info!(batch_id = %batch_id, plugin_id = %plugin_id, "transfer batch rejected by user");
        self.publish_batch_resolved(batch_id, "rejected", "user-rejected")
            .await;
        Ok(())
    }

    /// session 创建 gating（spec 2.2）：Approved → Ok(批引用)；其他 → Err
    ///
    /// 批存在但非 approved（pending / rejected）与批不存在均拒绝，
    /// 防 ask 模式下绕过批上下文直传 /upload（fail-closed）
    pub async fn check_batch(
        &self,
        plugin_id: &str,
        mount_path: &str,
        batch_id: &str,
    ) -> Result<TransferBatch, BatchError> {
        let batches = self.batches.read().await;
        let batch = batches.get(batch_id).ok_or_else(|| {
            BatchError::GatingDenied("batch-not-found".to_string())
        })?;
        // 归属校验：批属于其他插件/挂载时不泄露存在性
        if batch.plugin_id != plugin_id || batch.mount_path != mount_path {
            return Err(BatchError::GatingDenied("batch-not-found".to_string()));
        }
        match batch.state {
            BatchState::Approved => Ok(batch.clone()),
            BatchState::Pending => Err(BatchError::GatingDenied("batch-not-approved".to_string())),
            BatchState::Rejected { .. } => {
                Err(BatchError::GatingDenied("batch-rejected".to_string()))
            }
        }
    }

    /// 批内 session 活动刷新（建 session 成功时调用，approved 24h TTL 依据）
    pub async fn touch_batch(&self, batch_id: &str) {
        if let Some(batch) = self.batches.write().await.get_mut(batch_id) {
            batch.last_active = Instant::now();
        }
    }

    /// 设置 per-mount 批准超时（10–600s 校验，默认 60；仅 ask 策略生效）
    pub async fn set_approval_timeout(
        &self,
        plugin_id: &str,
        mount_path: &str,
        secs: u64,
    ) -> Result<(), BatchError> {
        if !(MIN_APPROVAL_TIMEOUT_SECS..=MAX_APPROVAL_TIMEOUT_SECS).contains(&secs) {
            return Err(BatchError::InvalidTimeout(secs));
        }
        self.approval_timeouts
            .write()
            .await
            .insert((plugin_id.to_string(), mount_path.to_string()), Duration::from_secs(secs));
        tracing::info!(
            plugin_id = %plugin_id,
            mount = %mount_path,
            timeout_secs = secs,
            "approval timeout set"
        );
        Ok(())
    }

    /// sweeper 一次扫描：pending 超时 → rejected(Timeout)；approved 24h 无活动 → 清理
    ///
    /// 返回本次**pending 超时**的批（调用方逐批发布 transfer_resolved + 跨端推送）；
    /// approved 24h 清理是纯兜底（批准已生效、无 pending 卡可消费事件），静默移除
    pub async fn sweep_batches(&self) -> Vec<transfer::ExpiredBatch> {
        let now = Instant::now();
        let mut expired_pending = Vec::new();
        {
            let mut batches = self.batches.write().await;
            let mut to_remove: Vec<String> = Vec::new();
            for (id, batch) in batches.iter_mut() {
                match batch.state {
                    BatchState::Pending
                        if now.duration_since(batch.created_at) > batch.approval_timeout =>
                    {
                        batch.state = BatchState::Rejected {
                            reason: transfer::RejectReason::Timeout,
                        };
                        expired_pending.push(transfer::ExpiredBatch {
                            batch_id: id.clone(),
                            decision: "rejected".to_string(),
                            reason: "timeout".to_string(),
                        });
                        to_remove.push(id.clone());
                        tracing::info!(
                            batch_id = %id,
                            timeout_secs = batch.approval_timeout.as_secs(),
                            "transfer batch pending timeout, auto-rejected"
                        );
                    }
                    BatchState::Approved
                        if now.duration_since(batch.last_active) > APPROVED_BATCH_TTL =>
                    {
                        to_remove.push(id.clone());
                        tracing::info!(
                            batch_id = %id,
                            "transfer batch approved, idle > 24h, cleaned up"
                        );
                    }
                    _ => {}
                }
            }
            for id in to_remove {
                batches.remove(&id);
            }
        }
        expired_pending
    }

    /// 本地取消接收中的上传会话（接收端取消命令，session 级）
    ///
    /// 清理 .part + 推送 `filesrv:receiving_done(cancelled)`；
    /// 发送方 session 丢失后自动重建从头传（v1 语义兜底）
    pub async fn cancel_receiving_session(
        &self,
        plugin_id: &str,
        session_id: &str,
    ) -> Result<(), UploadSessionError> {
        // 归属校验：session 必须属于该插件（防跨插件取消劫持）
        let (sid, mount) = match self
            .upload_sessions
            .owner_of(session_id)
            .await
        {
            Some((pid, m)) if pid == plugin_id => (session_id.to_string(), m),
            _ => return Err(UploadSessionError::NotFound(session_id.to_string())),
        };
        self.upload_sessions.cancel(&sid, plugin_id, &mount).await?;
        self.emit_filesrv_event(
            "filesrv:receiving_done",
            serde_json::json!({
                "sessionId": sid,
                "state": "cancelled",
                "reason": null,
            }),
        )
        .await;
        tracing::info!(
            session_id = %session_id,
            plugin_id = %plugin_id,
            "receiving session cancelled (local)"
        );
        Ok(())
    }

    /// 批已解决发布入口：本地 `filesrv:transfer_resolved` + 跨端推送 TransferApproval
    ///
    /// 调用方：approve/reject 命令、批 sweeper（pending 超时）
    pub async fn publish_batch_resolved(&self, batch_id: &str, decision: &str, reason: &str) {
        self.emit_filesrv_event(
            "filesrv:transfer_resolved",
            serde_json::json!({
                "batchId": batch_id,
                "decision": decision,
                "reason": reason,
            }),
        )
        .await;
        self.push_transfer_approval(batch_id, decision, reason).await;
    }

    /// 跨端推送传输批应答（桌面 → 移动，经 SyncData 广播）
    ///
    /// 与 emit_peer_changed 的 sync_tx 通道同模式；无头环境（纯单测）静默跳过
    async fn push_transfer_approval(&self, batch_id: &str, decision: &str, reason: &str) {
        let Some(ctx) = crate::system::app_context::AppContext::try_global() else {
            return;
        };
        if let Err(e) = ctx.sync_tx().send(crate::events::DesktopSyncEvent::TransferApproval {
            batch_id: batch_id.to_string(),
            decision: decision.to_string(),
            reason: reason.to_string(),
        }) {
            // 无接收者（移动端未连接）是常态，仅 debug
            tracing::debug!(
                batch_id = %batch_id,
                "transfer approval sync event not delivered: {}",
                e
            );
        }
    }

    /// 跨端收到的批应答（移动 → 桌面，经 WS file_service 消息）：
    /// 双通道发布 `filesrv:transfer_approval`（发送方插件订阅）
    pub async fn publish_transfer_approval(&self, batch_id: &str, decision: &str, reason: &str) {
        self.emit_filesrv_event(
            "filesrv:transfer_approval",
            serde_json::json!({
                "batchId": batch_id,
                "decision": decision,
                "reason": reason,
            }),
        )
        .await;
        tracing::info!(
            batch_id = %batch_id,
            decision = %decision,
            reason = %reason,
            "transfer approval received from peer, published"
        );
    }

    /// 调用批量传输请求钩子（fail-closed，spec 2.1）
    ///
    /// 三路分流：Wasm → host.call_transfer_hook / Webview →
    /// call_webview_batch_hook / None → deny；2 秒超时，任何错误一律拒绝
    async fn call_transfer_hook(
        &self,
        plugin_id: &str,
        mount_path: &str,
        req: &TransferRequestDto,
    ) -> UploadHookDecision {
        let hook = {
            let mounts = self.mounts.read().await;
            match mounts.get(&(plugin_id.to_string(), mount_path.to_string())) {
                Some(entry) => entry.hook.clone(),
                None => return UploadHookDecision::deny("mount not found"),
            }
        };

        match hook {
            HookTarget::None => UploadHookDecision::deny("mount has no transfer hook"),
            HookTarget::Wasm => {
                let host = self.plugin_host.read().await.clone();
                let Some(host) = host else {
                    tracing::warn!(
                        plugin_id = %plugin_id,
                        "transfer hook: plugin host not injected yet, denying (fail-closed)"
                    );
                    return UploadHookDecision::deny("plugin host not ready");
                };
                let meta = TransferRequestMeta {
                    batch_id: req.batch_id.clone(),
                    files: req.files.clone(),
                    total_size: req.total_size,
                };
                let meta_json = serde_json::to_string(&meta).unwrap_or_default();
                let plugin_id = plugin_id.to_string();
                match tokio::time::timeout(
                    UPLOAD_HOOK_TIMEOUT,
                    host.call_transfer_hook(&plugin_id, &meta_json),
                )
                .await
                {
                    Ok(decision) => decision,
                    Err(_) => {
                        tracing::warn!(
                            plugin_id = %plugin_id,
                            mount = %mount_path,
                            "transfer hook timed out (2s), denying (fail-closed)"
                        );
                        UploadHookDecision::deny("transfer hook timed out")
                    }
                }
            }
            HookTarget::Webview => self.call_webview_batch_hook(plugin_id, mount_path, req).await,
        }
    }

    /// Webview 批钩子：emit `filesrv:transfer_request_hook` 事件 + oneshot 等待
    /// 回复（2 秒超时 fail-closed），与 call_webview_hook 同构
    async fn call_webview_batch_hook(
        &self,
        plugin_id: &str,
        mount_path: &str,
        req: &TransferRequestDto,
    ) -> UploadHookDecision {
        use tauri::Emitter;

        let Some(app_handle) = self.app_handle.as_ref() else {
            return UploadHookDecision::deny("webview hook unavailable in headless context");
        };

        let request_id = uuid::Uuid::new_v4().to_string();
        let (reply_tx, reply_rx) = oneshot::channel();
        self.pending_transfer_replies
            .lock()
            .await
            .insert(request_id.clone(), reply_tx);

        let meta = TransferRequestMeta {
            batch_id: req.batch_id.clone(),
            files: req.files.clone(),
            total_size: req.total_size,
        };
        let payload = serde_json::json!({
            "requestId": request_id,
            "pluginId": plugin_id,
            "mountPath": mount_path,
            "meta": meta,
        });
        if let Err(e) = app_handle.emit("filesrv:transfer_request_hook", payload) {
            self.pending_transfer_replies.lock().await.remove(&request_id);
            tracing::error!(
                plugin_id = %plugin_id,
                "webview transfer hook emit failed: {}",
                e
            );
            return UploadHookDecision::deny("webview transfer hook emit failed");
        }

        match tokio::time::timeout(UPLOAD_HOOK_TIMEOUT, reply_rx).await {
            Ok(Ok(decision)) => decision,
            _ => {
                self.pending_transfer_replies.lock().await.remove(&request_id);
                tracing::warn!(
                    plugin_id = %plugin_id,
                    mount = %mount_path,
                    "webview transfer hook timed out (2s), denying (fail-closed)"
                );
                UploadHookDecision::deny("webview transfer hook timed out")
            }
        }
    }

    /// 回填 Webview 批钩子决定（TS 通道命令调用；request 不存在返回 false）
    pub async fn respond_transfer_hook(
        &self,
        request_id: &str,
        decision: UploadHookDecision,
    ) -> bool {
        let tx = self.pending_transfer_replies.lock().await.remove(request_id);
        match tx {
            Some(tx) => tx.send(decision).is_ok(),
            None => false,
        }
    }

    /// 双通道发布文件服务本地事件（Tauri 事件 + 插件消息总线，仿 emit_peer_changed）
    ///
    /// 发射失败只 warn，不影响主流程（事件通道为 best-effort 通知）
    pub(crate) async fn emit_filesrv_event(&self, event: &str, payload: serde_json::Value) {
        use tauri::Emitter;

        // 通道 1：Tauri 事件（前端 UI 订阅，如 pending 批卡 / 接收任务列表）
        if let Some(app_handle) = self.app_handle.as_ref() {
            if let Err(e) = app_handle.emit(event, &payload) {
                tracing::warn!(
                    event = %event,
                    "emit {} failed: {}",
                    event,
                    e
                );
            }
        }

        // 通道 2：插件消息总线（WASM 插件后端经 host_bus_subscribe 订阅）
        let host = self.plugin_host.read().await.clone();
        if let Some(host) = host {
            host.message_bus()
                .publish(event, "host", payload);
        } else {
            tracing::debug!(
                event = %event,
                "plugin host not injected yet, bus publish skipped"
            );
        }
    }

    /// 启动传输批 sweeper（间隔 1s：pending 超时自动拒绝 + approved 24h 清理）
    ///
    /// 必须在 tokio runtime 上下文内调用；超时批逐条发布
    /// transfer_resolved + 跨端推送（发送方据此 rejected(timeout)）
    fn spawn_batch_sweeper(self: &Arc<Self>) {
        let registry = self.clone();
        crate::system::error_boundary::spawn_with_error_boundary(
            "transfer_batch_sweeper",
            async move {
                let mut interval = tokio::time::interval(Duration::from_secs(1));
                interval.tick().await; // 首个 tick 立即返回，跳过
                loop {
                    interval.tick().await;
                    let expired = registry.sweep_batches().await;
                    for batch in expired {
                        registry
                            .publish_batch_resolved(&batch.batch_id, &batch.decision, &batch.reason)
                            .await;
                    }
                }
            },
        );
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

    /// 强制推送对端在线状态（Query 探测回复路径使用）
    ///
    /// 与 [`set_peer`](Self::set_peer) 的区别：不做信息变更去重，
    /// 即使记录未变也推送 `filesrv:peer_changed`（online=true）。
    /// 插件 activate 后主动 Query 探测时，若信息未变会被 set_peer 去重
    /// 吞掉推送，插件端对端列表将无法恢复。
    pub async fn push_peer(&self, peer_id: &str, info: PeerFileService) {
        {
            let mut peers = self.peers.write().await;
            peers.insert(peer_id.to_string(), info);
        }
        self.emit_peer_changed(peer_id, true).await;
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

    /// 当前全部挂载的公告清单（认证成功补发快照 / Query 响应使用）
    ///
    /// 与移动端 `mount_announcements` 同构，按 (plugin_id, mount_path) 排序保证稳定输出
    pub async fn mount_announcements(&self) -> Vec<MountAnnouncement> {
        let mounts = self.mounts.read().await;
        let mut list: Vec<MountAnnouncement> = mounts
            .iter()
            .map(|((plugin_id, mount_path), entry)| MountAnnouncement {
                plugin_id: plugin_id.clone(),
                mount_path: mount_path.clone(),
                operations: entry.operations.clone(),
            })
            .collect();
        list.sort_by(|a, b| (&a.plugin_id, &a.mount_path).cmp(&(&b.plugin_id, &b.mount_path)));
        list
    }

    /// 双通道推送对端在线状态变更（Tauri 事件 + 插件消息总线）
    ///
    /// 发射失败只 warn，不影响主流程（约束：事件通道为 best-effort 通知）
    async fn emit_peer_changed(&self, peer_id: &str, online: bool) {
        use tauri::Emitter;

        // 携带对端真实设备名与 IP，供前端文件传输展示（无记录时为空串）
        let info = self.get_peer(peer_id).await;
        let payload = serde_json::json!({
            "peerId": peer_id,
            "online": online,
            "deviceName": info.as_ref().map(|i| i.device_name.clone()).unwrap_or_default(),
            "ip": info.map(|i| i.ip).unwrap_or_default(),
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

/// 后台清理孤儿上传临时文件（best effort：失败仅记录日志，不阻塞调用方）
///
/// 无运行时上下文时（理论上不会发生：mount 必在异步上下文调用）回退同步执行
fn spawn_orphan_cleanup(plugin_id: &str, mount_path: &str, roots: Vec<std::path::PathBuf>) {
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        let cleaned = upload::clean_orphan_parts(&roots);
        if cleaned > 0 {
            tracing::info!(
                plugin_id = %plugin_id,
                mount = %mount_path,
                "mount: cleaned {} orphan upload temp file(s)",
                cleaned
            );
        }
        return;
    };
    let plugin_id = plugin_id.to_string();
    let mount_path = mount_path.to_string();
    handle.spawn_blocking(move || {
        let cleaned = upload::clean_orphan_parts(&roots);
        if cleaned > 0 {
            tracing::info!(
                plugin_id = %plugin_id,
                mount = %mount_path,
                "mount: cleaned {} orphan upload temp file(s) (background)",
                cleaned
            );
        }
    });
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

    // ==================== v2 传输批 ====================

    /// 构造最小挂载条目（HookTarget::None：无钩子 → fail-closed deny）
    fn make_mount(plugin_id: &str, mount_path: &str) -> MountEntry {
        MountEntry {
            plugin_id: plugin_id.to_string(),
            mount_path: mount_path.to_string(),
            roots: Vec::new(),
            operations: vec![FileOperation::Upload],
            hook: HookTarget::None,
            cipher: Arc::new(PassthroughCipher),
            downloads_dir: None,
        }
    }

    fn make_req(batch_id: &str) -> TransferRequestDto {
        TransferRequestDto {
            batch_id: batch_id.to_string(),
            files: vec![UploadRequestMeta { relative_path: "a.txt".into(), size: 10 }],
            total_size: 10,
        }
    }

    fn make_registry() -> Arc<FileServiceRegistry> {
        // 无头 fs_auth（内存库，弹窗层不可用）/ app_handle=None；
        // 批方法不依赖挂载授权，仅需能构造注册表
        let db = crate::db::Database::new(&std::path::Path::new(":memory:")).unwrap();
        db.init_schema().unwrap();
        let storage = Arc::new(crate::plugin::storage::PluginStorage::new(Arc::new(
            tokio::sync::Mutex::new(db),
        )));
        let fs_auth = Arc::new(FsAuthChecker::new(storage, None));
        FileServiceRegistry::new(fs_auth, None)
    }

    /// 无钩子挂载（HookTarget::None）→ 批请求一律 deny（fail-closed）
    #[tokio::test]
    async fn create_transfer_request_none_hook_denies() {
        let registry = make_registry();
        registry
            .mounts
            .write()
            .await
            .insert(("p1".to_string(), "files".to_string()), make_mount("p1", "files"));

        let err = registry
            .create_transfer_request("p1", "files", &make_req("b1"))
            .await
            .unwrap_err();
        assert!(matches!(err, BatchError::PolicyDenied(_)), "got: {:?}", err);
        // deny：不建批无记录（零打扰）
        assert!(registry.batches.read().await.is_empty());
    }

    /// 挂载不存在 → deny（fail-closed）
    #[tokio::test]
    async fn create_transfer_request_mount_missing_denies() {
        let registry = make_registry();
        let err = registry
            .create_transfer_request("p1", "files", &make_req("b1"))
            .await
            .unwrap_err();
        assert!(matches!(err, BatchError::PolicyDenied(_)));
    }

    /// approve/reject 归属校验：他插件应答同一批 → NotFound（不泄露存在性）
    #[tokio::test]
    async fn approve_reject_ownership_checked() {
        let registry = make_registry();
        registry
            .insert_batch("p1", "files", &make_req("b1"), BatchState::Pending)
            .await;

        // 他插件应答 → NotFound
        assert!(matches!(
            registry.approve_transfer("other", "b1").await,
            Err(BatchError::NotFound(_))
        ));
        assert!(matches!(
            registry.reject_transfer("other", "b1").await,
            Err(BatchError::NotFound(_))
        ));
        // 批不存在 → NotFound
        assert!(matches!(
            registry.approve_transfer("p1", "nope").await,
            Err(BatchError::NotFound(_))
        ));
        // 正确归属 → 迁移成功
        registry.approve_transfer("p1", "b1").await.expect("approve ok");
        // 重复应答（非 pending）→ NotPending
        assert!(matches!(
            registry.approve_transfer("p1", "b1").await,
            Err(BatchError::NotPending(_))
        ));

        // reject 路径：pending → rejected(UserRejected)
        registry
            .insert_batch("p1", "files", &make_req("b2"), BatchState::Pending)
            .await;
        registry.reject_transfer("p1", "b2").await.expect("reject ok");
    }

    /// check_batch gating：approved → Ok；pending/rejected/不存在/异归属 → GatingDenied
    #[tokio::test]
    async fn check_batch_gating() {
        let registry = make_registry();
        registry
            .insert_batch("p1", "files", &make_req("b1"), BatchState::Pending)
            .await;
        // pending → batch-not-approved（ask 模式防绕过）
        assert_eq!(
            registry.check_batch("p1", "files", "b1").await.unwrap_err().to_string(),
            "batch-not-approved"
        );

        registry
            .insert_batch("p1", "files", &make_req("b2"), BatchState::Approved)
            .await;
        // approved → Ok(批引用)
        let batch = registry.check_batch("p1", "files", "b2").await.expect("approved ok");
        assert!(batch.state == BatchState::Approved);
        // 异挂载 → batch-not-found（不泄露）
        assert_eq!(
            registry.check_batch("p1", "other", "b2").await.unwrap_err().to_string(),
            "batch-not-found"
        );

        registry
            .insert_batch(
                "p1",
                "files",
                &make_req("b3"),
                BatchState::Rejected {
                    reason: transfer::RejectReason::UserRejected,
                },
            )
            .await;
        // rejected → batch-rejected
        assert_eq!(
            registry.check_batch("p1", "files", "b3").await.unwrap_err().to_string(),
            "batch-rejected"
        );
        // 不存在 → batch-not-found
        assert_eq!(
            registry.check_batch("p1", "files", "nope").await.unwrap_err().to_string(),
            "batch-not-found"
        );
    }

    /// set_approval_timeout 边界：9/10/600/601（10–600 校验）
    #[tokio::test]
    async fn set_approval_timeout_boundaries() {
        let registry = make_registry();
        assert!(matches!(
            registry.set_approval_timeout("p1", "files", 9).await,
            Err(BatchError::InvalidTimeout(9))
        ));
        assert!(matches!(
            registry.set_approval_timeout("p1", "files", 601).await,
            Err(BatchError::InvalidTimeout(601))
        ));
        registry.set_approval_timeout("p1", "files", 10).await.expect("10 ok");
        registry.set_approval_timeout("p1", "files", 600).await.expect("600 ok");
        let timeouts = registry.approval_timeouts.read().await;
        assert_eq!(
            timeouts.get(&("p1".to_string(), "files".to_string())),
            Some(&Duration::from_secs(600))
        );
    }

    /// sweep_batches：pending 超时 → rejected(timeout) + 返回过期批；
    /// approved 24h 无活动 → 静默清理（不产生 resolved 事件）
    #[tokio::test]
    async fn sweep_batches_timeout_and_cleanup() {
        let registry = make_registry();
        // pending 批：批准超时 1ms（直接写内部表，绕过 10–600 校验）
        registry
            .approval_timeouts
            .write()
            .await
            .insert(("p1".to_string(), "files".to_string()), Duration::from_millis(1));
        registry
            .insert_batch("p1", "files", &make_req("b1"), BatchState::Pending)
            .await;
        // approved 批：last_active 回溯 25h（超 24h TTL）
        registry
            .batches
            .write()
            .await
            .insert(
                "b2".to_string(),
                TransferBatch {
                    batch_id: "b2".into(),
                    plugin_id: "p1".into(),
                    mount_path: "files".into(),
                    files: Vec::new(),
                    total_size: 0,
                    state: BatchState::Approved,
                    created_at: Instant::now() - Duration::from_secs(25 * 3600),
                    last_active: Instant::now() - Duration::from_secs(25 * 3600),
                    approval_timeout: Duration::from_secs(60),
                },
            );
        // 未超时 pending 批（对照）：60s 超时不会过期（直接写内部表，
        // 避免与 b1 共享 1ms 的 per-mount 超时配置）
        registry
            .batches
            .write()
            .await
            .insert(
                "b3".to_string(),
                TransferBatch {
                    batch_id: "b3".into(),
                    plugin_id: "p1".into(),
                    mount_path: "files".into(),
                    files: Vec::new(),
                    total_size: 0,
                    state: BatchState::Pending,
                    created_at: Instant::now(),
                    last_active: Instant::now(),
                    approval_timeout: Duration::from_secs(60),
                },
            );

        tokio::time::sleep(Duration::from_millis(5)).await;
        let expired = registry.sweep_batches().await;
        // 仅 b1 过期（b2 清理不产生事件；b3 未超时）
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].batch_id, "b1");
        assert_eq!(expired[0].decision, "rejected");
        assert_eq!(expired[0].reason, "timeout");

        // b1/b2 已移除，b3 保留 pending
        let batches = registry.batches.read().await;
        assert!(!batches.contains_key("b1"));
        assert!(!batches.contains_key("b2"));
        assert!(batches.contains_key("b3"));
    }

    /// touch_batch：approved 批活动刷新（24h TTL 续期）
    #[tokio::test]
    async fn touch_batch_refreshes_last_active() {
        let registry = make_registry();
        registry
            .insert_batch("p1", "files", &make_req("b1"), BatchState::Approved)
            .await;
        registry.touch_batch("b1").await;
        let batch = registry.batches.read().await.get("b1").cloned().unwrap();
        assert!(batch.last_active.elapsed() < Duration::from_millis(500));
    }
}
