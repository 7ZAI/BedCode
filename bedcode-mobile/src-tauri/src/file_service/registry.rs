//! 文件服务挂载注册表（移动端）
//!
//! 与桌面端 `bedcode-desktop/src-tauri/src/plugin/file_service/registry.rs` 同构
//! （两端各自实现、不建共享 crate，见内网文件传输插件规格第 4 节）。
//!
//! 管理插件挂载的文件服务端点（mounts）、对端文件服务信息（peers，
//! 由 WS 控制面 Announce 填充）、上传会话与策略钩子分发。
//!
//! 与桌面端的关键差异：
//! - 移动端挂载触发独立 HTTP 服务的启停（首个挂载启动、末个摘除停止，见 server.rs）
//! - 挂载/卸载后需经 WS 控制面向桌面端 Announce/Withdraw（见 announce.rs）
//! - 上传钩子双路径：WASM 插件经 PluginManager 实例调用；ts-only 插件经
//!   Tauri command 挂载时走 Webview 事件桥（与桌面端 call_webview_hook 同构）

use crate::file_service::cipher::{PassthroughCipher, TransportCipher};
use crate::file_service::sandbox;
use crate::file_service::saf_tree;
use crate::file_service::upload::UploadSessionManager;
use bedcode_plugin_api_mobile::{
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
    /// WASM 插件：经 PluginManager 调用实例导出 `on_upload_request`
    Wasm,
    /// TS-only 插件：经前端 webview 事件桥转发（Tauri command 通道挂载）
    Webview,
    /// 无钩子：fail-closed 拒绝所有上传（TS 挂载未提供 onUploadRequest 时）
    None,
}

/// 挂载条目
#[derive(Clone)]
pub struct MountEntry {
    /// 所属插件 ID
    pub plugin_id: String,
    /// 挂载点名称（URL 段）
    pub mount_path: String,
    /// 允许目录根（canonicalize 后，已去重取最外层；真实路径根）
    pub roots: Vec<PathBuf>,
    /// SAF 树根（content://tree/... URI；持久化授权，M2）
    ///
    /// 共享目录 SAF 化的挂载形态：list/download 经 SafIo（list_tree 遍历 /
    /// 中转复制）服务，不再走 std::fs 真实路径。免 fs_auth（授权由系统
    /// 持久化 URI 权限承载）与 canonicalize（content:// 无路径语义）。
    pub saf_roots: Vec<String>,
    /// 允许的操作集合
    pub operations: Vec<FileOperation>,
    /// 上传策略钩子目标
    pub hook: HookTarget,
    /// 传输加密拦截器（MVP 为直通，见 cipher 模块）
    pub cipher: Arc<dyn TransportCipher>,
}

/// 文件服务注册表（全局单例，见 state::get_file_service）
pub struct FileServiceRegistry {
    /// 挂载表：(plugin_id, mount_path) → 挂载条目
    mounts: RwLock<HashMap<(String, String), MountEntry>>,
    /// 对端文件服务信息表（对端 = 桌面端，经 sync 推送填充）
    peers: RwLock<HashMap<String, PeerFileService>>,
    /// 上传会话管理器
    upload_sessions: Arc<UploadSessionManager>,
    /// Webview 钩子待回复表：request_id → 回复通道
    ///
    /// 前端 Tauri command 经 [`respond_upload_hook`](Self::respond_upload_hook) 回填
    pending_hook_replies: Mutex<HashMap<String, oneshot::Sender<UploadHookDecision>>>,
    /// Tauri AppHandle（Webview 钩子事件发送；经 [`set_app_handle`](Self::set_app_handle) 注入）
    app_handle: RwLock<Option<tauri::AppHandle>>,
    /// SAF 存储访问实现（M2 三端点；生产 = default_saf_io，测试注入 fake）
    saf_io: RwLock<Option<Arc<dyn crate::plugin::saf_io::SafIo>>>,
    /// 接收落点下载目录（M2 上传目标语义；懒解析自 app_handle 并缓存，测试可预置）
    downloads_dir: RwLock<Option<PathBuf>>,
    /// SAF 中转缓存目录（M2 download 端点 cache 中转；懒解析自 app_handle 并缓存，测试可预置）
    relay_dir: RwLock<Option<PathBuf>>,
}

impl FileServiceRegistry {
    /// 创建注册表（后台 sweeper 需在 runtime 上下文内经 [`start_background_tasks`] 启动）
    pub fn new() -> Arc<Self> {
        Self::with_saf_io(crate::plugin::saf_io::default_saf_io())
    }

    /// 创建注册表并注入 SafIo 实现（端点测试注入 fake；生产用 [`new`](Self::new)）
    pub fn with_saf_io(saf_io: Arc<dyn crate::plugin::saf_io::SafIo>) -> Arc<Self> {
        Arc::new(Self {
            mounts: RwLock::new(HashMap::new()),
            peers: RwLock::new(HashMap::new()),
            upload_sessions: Arc::new(UploadSessionManager::new()),
            pending_hook_replies: Mutex::new(HashMap::new()),
            app_handle: RwLock::new(None),
            saf_io: RwLock::new(Some(saf_io)),
            downloads_dir: RwLock::new(None),
            relay_dir: RwLock::new(None),
        })
    }

    /// 注入 Tauri AppHandle（Tauri command 通道挂载时调用，幂等；Webview 钩子经它 emit 事件）
    pub async fn set_app_handle(&self, handle: tauri::AppHandle) {
        // 已注入则跳过，避免每次挂载都取写锁
        if self.app_handle.read().await.is_some() {
            return;
        }
        *self.app_handle.write().await = Some(handle);
    }

    // ==================== SAF 化辅助（M2） ====================

    /// SAF 存储访问实现（三端点 list/download/upload 落位用；None = 未注入）
    pub async fn saf_io(&self) -> Option<Arc<dyn crate::plugin::saf_io::SafIo>> {
        self.saf_io.read().await.clone()
    }

    /// 注入 SafIo 实现（端点测试替换 fake）
    pub async fn set_saf_io(&self, saf: Arc<dyn crate::plugin::saf_io::SafIo>) {
        *self.saf_io.write().await = Some(saf);
    }

    /// 接收落点下载目录（M2 上传目标语义；懒解析自 app_handle 并缓存）
    ///
    /// 解析链与命令层/WASM host 共用（android_plugins.rs resolve_app_downloads_dir）：
    /// Kotlin 桥外部私有目录 → app_data/Downloads 回退。外部存储不可用的设备上
    /// 上传会话临时文件与回退落位（rename）都落到该目录，与下载方向私有回退一致。
    pub async fn downloads_dir(&self) -> Option<PathBuf> {
        if let Some(dir) = self.downloads_dir.read().await.as_ref() {
            return Some(dir.clone());
        }
        let handle = self.app_handle.read().await.clone()?;
        let dir = PathBuf::from(crate::plugin::android_plugins::resolve_app_downloads_dir(&handle).await?);
        *self.downloads_dir.write().await = Some(dir.clone());
        Some(dir)
    }

    /// 预置下载目录（端点测试注入临时目录）
    pub async fn set_downloads_dir(&self, dir: PathBuf) {
        *self.downloads_dir.write().await = Some(dir);
    }

    /// SAF 中转缓存目录（M2 download 端点 cache 中转；懒解析自 app_handle 并缓存）
    ///
    /// app cache/bedcode_downloads（系统可清理；副本生命周期短，见 saf_tree 模块）。
    pub async fn relay_dir(&self) -> Option<PathBuf> {
        if let Some(dir) = self.relay_dir.read().await.as_ref() {
            return Some(dir.clone());
        }
        use tauri::Manager;
        let handle = self.app_handle.read().await.clone()?;
        let dir = handle.path().app_cache_dir().ok()?.join("bedcode_downloads");
        if let Err(e) = std::fs::create_dir_all(&dir) {
            tracing::error!(
                error = %e,
                path = %dir.display(),
                "registry: failed to create saf relay dir"
            );
            return None;
        }
        *self.relay_dir.write().await = Some(dir.clone());
        Some(dir)
    }

    /// 预置中转缓存目录（端点测试注入临时目录）
    pub async fn set_relay_dir(&self, dir: PathBuf) {
        *self.relay_dir.write().await = Some(dir);
    }

    /// 测试辅助：直接注入挂载条目（绕过 fs_auth / plugin manager 依赖）
    #[cfg(test)]
    pub async fn insert_entry_for_test(&self, entry: MountEntry) {
        let key = (entry.plugin_id.clone(), entry.mount_path.clone());
        self.mounts.write().await.insert(key, entry);
    }

    /// 启动后台任务（必须在 tokio runtime 上下文内调用一次）
    pub fn start_background_tasks(self: &Arc<Self>) {
        UploadSessionManager::spawn_sweeper(self.upload_sessions.clone());
    }

    /// 上传会话管理器引用（server 使用）
    pub fn upload_sessions(&self) -> &Arc<UploadSessionManager> {
        &self.upload_sessions
    }

    // ==================== Mounts ====================

    /// 挂载文件服务
    ///
    /// 校验（规格 4.3）：
    /// 1. mount_path 必须匹配 `^[a-z0-9-_]+$`（URL 段安全）
    /// 2. 每个 root 必须经宿主 fs 授权；声明 upload 时按写授权，否则读授权
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

        // 根分流（M2 SAF 化）：SAF 树根（content://tree/...）免 fs_auth 与
        // canonicalize（持久化授权经 ContentResolver 生效，无路径语义）；
        // 真实路径根保持现有校验（fs_auth + normalize_roots）
        let saf_roots: Vec<String> = options
            .roots
            .iter()
            .filter(|r| saf_tree::is_saf_tree_uri(r))
            .cloned()
            .collect();
        let real_roots: Vec<PathBuf> = options
            .roots
            .iter()
            .filter(|r| !saf_tree::is_saf_tree_uri(r))
            .map(PathBuf::from)
            .collect();

        // 声明 upload 操作时挂载点具备写入能力，按写授权校验（覆盖读）
        let fs_op = if options.operations.contains(&FileOperation::Upload) {
            crate::plugin::fs_auth::FsOp::Write
        } else {
            crate::plugin::fs_auth::FsOp::Read
        };
        let fs_auth = crate::state::get_plugin_manager().fs_auth().clone();
        for root in &real_roots {
            let root_str = root.to_string_lossy();
            if !fs_auth.check(plugin_id, &root_str, fs_op).await {
                return Err(crate::AppError::Auth(format!(
                    "mount '{}': root '{}' not authorized by user",
                    options.mount_path, root_str
                )));
            }
        }

        let roots = if real_roots.is_empty() {
            // 全 SAF 根挂载：真实路径根为空合法（SAF 分支自行解析）
            Vec::new()
        } else {
            sandbox::normalize_roots(&real_roots).map_err(|e| {
                crate::AppError::InvalidInput(format!(
                    "mount '{}': invalid roots: {}",
                    options.mount_path, e
                ))
            })?
        };

        let entry = MountEntry {
            plugin_id: plugin_id.to_string(),
            mount_path: options.mount_path.clone(),
            roots,
            saf_roots,
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

        // 清理宿主异常退出遗留的孤儿临时文件：后台扫描（best effort，失败不影响挂载）。
        // 大目录（NAS/深目录）扫描可能耗时数十秒，不能阻塞 wasm 挂载调用——
        // 慢宿主工作移出调用路径后，宿主延迟与插件执行预算彻底解耦（见 FUEL_PER_CALL）。
        // 仅扫描真实路径根（SAF 树根无文件系统语义）；下载目录一并扫描（接收方向
        // 会话临时文件落私有下载目录，崩溃遗留 .part 需兜底清理）
        let downloads_dir = self.downloads_dir().await;
        spawn_orphan_cleanup(plugin_id, &options.mount_path, entry.roots.clone(), downloads_dir);

        tracing::info!(
            plugin_id = %plugin_id,
            mount = %options.mount_path,
            roots = ?entry.roots,
            "file service mounted"
        );
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
                crate::plugin::fs_auth::FsOp::Write
            } else {
                crate::plugin::fs_auth::FsOp::Read
            }
        };

        // 根分流同 mount：SAF 树根免 fs_auth / normalize，真实路径根保持现有校验
        let saf_roots: Vec<String> = roots
            .iter()
            .filter(|r| saf_tree::is_saf_tree_uri(r))
            .cloned()
            .collect();
        let real_roots: Vec<PathBuf> = roots
            .iter()
            .filter(|r| !saf_tree::is_saf_tree_uri(r))
            .map(PathBuf::from)
            .collect();

        let fs_auth = crate::state::get_plugin_manager().fs_auth().clone();
        for root in &real_roots {
            let root_str = root.to_string_lossy();
            if !fs_auth.check(plugin_id, &root_str, fs_op).await {
                return Err(crate::AppError::Auth(format!(
                    "update_roots for mount '{}': root '{}' not authorized by user",
                    mount_path, root_str
                )));
            }
        }

        let normalized = if real_roots.is_empty() {
            Vec::new()
        } else {
            sandbox::normalize_roots(&real_roots).map_err(|e| {
                crate::AppError::InvalidInput(format!(
                    "update_roots for mount '{}': invalid roots: {}",
                    mount_path, e
                ))
            })?
        };

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
        entry.saf_roots = saf_roots;

        tracing::info!(
            plugin_id = %plugin_id,
            mount = %mount_path,
            roots = ?entry.roots,
            "file service roots updated"
        );
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
                .filter_map(|(_, mp)| {
                    mounts
                        .remove(&(plugin_id.to_string(), mp.clone()))
                        .map(|_| mp.clone())
                })
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

    /// 当前挂载数量（服务启停判断：0 = 无挂载）
    pub async fn mount_count(&self) -> usize {
        self.mounts.read().await.len()
    }

    /// 当前全部挂载的公告信息（announce.rs 使用）
    pub async fn mount_announcements(
        &self,
    ) -> Vec<crate::enums::file_service::MountAnnouncement> {
        let mounts = self.mounts.read().await;
        mounts
            .values()
            .map(|e| crate::enums::file_service::MountAnnouncement {
                plugin_id: e.plugin_id.clone(),
                mount_path: e.mount_path.clone(),
                operations: e.operations.clone(),
            })
            .collect()
    }

    /// 沙箱解析：挂载点相对路径 → 沙箱内绝对路径（目标必须已存在）
    ///
    /// server 的 /list 与 /file 端点共用此校验
    pub async fn resolve_sandboxed(
        &self,
        plugin_id: &str,
        mount_path: &str,
        rel: &str,
    ) -> crate::Result<PathBuf> {
        let entry = self.get_entry(plugin_id, mount_path).await?;
        sandbox::resolve_within_roots(&entry.roots, rel).map_err(|e| {
            crate::AppError::NotFound(format!("mount '{}/{}': {}", plugin_id, mount_path, e))
        })
    }

    // ==================== Upload Hook ====================

    /// 调用上传策略钩子（fail-closed，规格 4.2）
    ///
    /// 仅在上传会话创建时调用一次
    ///
    /// 按挂载条目的钩子目标分派：WASM 实例导出（Wasm）、前端事件桥（Webview）、
    /// 无钩子（None，fail-closed 拒绝所有上传）
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
                // 挂载不存在 → fail-closed
                None => return UploadHookDecision::deny("mount not found"),
            }
        };

        match hook {
            HookTarget::None => UploadHookDecision::deny("mount has no upload hook"),
            HookTarget::Wasm => self.call_wasm_hook(plugin_id, mount_path, meta).await,
            HookTarget::Webview => self.call_webview_hook(plugin_id, mount_path, meta).await,
        }
    }

    /// WASM 钩子：经 PluginManager 的 WASM 实例调用导出 `on_upload_request`
    async fn call_wasm_hook(
        &self,
        plugin_id: &str,
        mount_path: &str,
        meta: &UploadRequestMeta,
    ) -> UploadHookDecision {
        let meta_json = serde_json::to_string(meta).unwrap_or_default();
        let manager = crate::state::get_plugin_manager();
        let plugin_id = plugin_id.to_string();

        match tokio::time::timeout(
            UPLOAD_HOOK_TIMEOUT,
            manager.call_upload_hook(&plugin_id, &meta_json),
        )
        .await
        {
            Ok(Some(decision_json)) => {
                // 插件返回决定 JSON；解析失败一律 fail-closed
                match serde_json::from_str::<UploadHookDecision>(&decision_json) {
                    Ok(decision) => decision,
                    Err(e) => {
                        tracing::warn!(
                            plugin_id = %plugin_id,
                            mount = %mount_path,
                            error = %e,
                            "upload hook returned invalid decision JSON, denying (fail-closed)"
                        );
                        UploadHookDecision::deny("invalid upload hook decision")
                    }
                }
            }
            Ok(None) => {
                tracing::warn!(
                    plugin_id = %plugin_id,
                    mount = %mount_path,
                    "upload hook unavailable (plugin not loaded / missing export), denying (fail-closed)"
                );
                UploadHookDecision::deny("upload hook unavailable")
            }
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

    /// Webview 钩子：emit 事件到前端 + oneshot 等待回复（2 秒超时 fail-closed）
    ///
    /// 与桌面端 `call_webview_hook` 同构：payload 字段一致，前端插件经
    /// Tauri command `plugin_filesrv_respond_upload_request` 回填决定
    async fn call_webview_hook(
        &self,
        plugin_id: &str,
        mount_path: &str,
        meta: &UploadRequestMeta,
    ) -> UploadHookDecision {
        use tauri::Emitter;

        let app_handle = self.app_handle.read().await.clone();
        let Some(app_handle) = app_handle else {
            tracing::warn!(
                plugin_id = %plugin_id,
                mount = %mount_path,
                "webview upload hook unavailable: app handle not injected, denying (fail-closed)"
            );
            return UploadHookDecision::deny("webview hook unavailable");
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

    /// 回填 Webview 钩子决定（Tauri command 调用；request 不存在/已超时返回 false）
    pub async fn respond_upload_hook(&self, request_id: &str, decision: UploadHookDecision) -> bool {
        let tx = self.pending_hook_replies.lock().await.remove(request_id);
        match tx {
            Some(tx) => tx.send(decision).is_ok(),
            None => false,
        }
    }

    // ==================== Peers ====================

    /// 登记对端文件服务信息（桌面 → 移动 sync 推送时调用）
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
        let app_handle = self.app_handle.read().await.clone();
        if let Some(handle) = app_handle {
            if let Err(e) = handle.emit("filesrv:peer_changed", &payload) {
                tracing::warn!(
                    peer_id = %peer_id,
                    online = online,
                    "emit filesrv:peer_changed failed: {}",
                    e
                );
            }
        } else {
            tracing::debug!(
                peer_id = %peer_id,
                "app_handle not injected, Tauri event skipped"
            );
        }

        // 通道 2：插件消息总线（WASM 插件后端经 host_bus_subscribe 订阅）
        // 与桌面端 plugin_host 检查对齐：管理器未初始化（setup 未完成）时跳过，
        // 避免 panic；激活晚于事件的场景由插件 activate 主动 Query 兜底
        if let Some(pm) = crate::state::try_get_plugin_manager() {
            pm.message_bus()
                .publish("filesrv:peer_changed", "host", payload);
        } else {
            tracing::debug!(
                peer_id = %peer_id,
                "plugin manager not initialized, bus publish skipped"
            );
        }

        tracing::info!(peer_id = %peer_id, online = online, "peer_changed pushed");
    }
}

/// 比较新旧对端信息是否有变化（用于去重：重复公告相同内容时不重复推送）
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
fn spawn_orphan_cleanup(plugin_id: &str, mount_path: &str, roots: Vec<PathBuf>, downloads_dir: Option<PathBuf>) {
    let dirs: Vec<PathBuf> = roots
        .into_iter()
        .chain(downloads_dir)
        .collect();
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        let cleaned = crate::file_service::upload::clean_orphan_parts(&dirs);
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
        let cleaned = crate::file_service::upload::clean_orphan_parts(&dirs);
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
}
