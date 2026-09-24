//! 文件系统访问校验器
//!
//! ## 三层策略（票 07 收敛；此前的「四层」里第一层是死数据 + 子串 hack）
//!
//! 1. **第一方集成目录预授权**（[`FIRST_PARTY_TRUSTED_DIRS`]）：只对具名第一方插件、
//!    只在其归属清单点名的目录段内免弹窗——不是「白名单插件任意路径放行」，
//!    也不是「路径里出现 `.claude/` 就放行」；
//! 2. **已授权路径前缀**（弹窗时勾选记住，持久化在插件私有存储）；
//! 3. **弹窗授权**（前两层都未命中；无头上下文没有弹窗通道 → 保守拒绝）。
//!
//! 每条判定都在日志里点明**命中的是哪一层**（`layer = ...`），排障时不必靠猜：
//! 免弹窗来源不唯一（清单 / 记住的授权 / 用户刚同意），无层号日志就无法回答
//! 「为什么这次没弹框」。
//!
//! ## 为什么第一层不再是「全局路径白名单」
//!
//! 旧实现按 `.claude/` **子串**匹配，对**所有**带 `fs:read` 的插件生效：任意位置的
//! 同名目录段（`/tmp/attacker-controlled/.claude/x`）都免弹窗，等于把「访问未授权
//! 目录按需弹窗」的兜底架空；而它的真实消费者只有两个第一方插件（agent-hub 分发
//! 技能到 `~/.claude/skills`、terminal-session 写项目集成目录）。改造后：
//! 第三方 `fs:read` 插件读 `~/.claude/**` 必须过弹窗（红测断言），第一方按归属清单免弹窗。
//!
//! ## 不在本校验器里的事
//!
//! 任务单元（core-task 池线程）**不得触发弹窗**——判据由调用侧走 [`FsAuthChecker::is_granted`]，
//! 未授权直接 fail-visible 拒绝（见 `host_impl::task`）。

use crate::wasm_core::storage::PluginStorage;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::Emitter;
use tokio::sync::{oneshot, Mutex};

/// 命中的授权层（日志与拒绝文案用它说明判据来源）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsGrantLayer {
    /// 第一方集成目录预授权（按插件 id 归属）
    FirstPartyDir,
    /// 用户此前授权并记住的路径前缀
    Persisted,
}

impl FsGrantLayer {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FirstPartyDir => "first-party-dir",
            Self::Persisted => "persisted-grant",
        }
    }
}

/// 第一方免弹窗目录的形态
#[derive(Debug, Clone, Copy)]
enum TrustedDir {
    /// 家目录下的相对前缀（`~/.agents/skills` 这类宿主已知位置）
    Home(&'static str),
    /// 任意项目根下的同名**目录段**（agent CLI 的项目级配置目录约定：
    /// `<project>/.claude` / `.codex` / `.pi` / `.opencode`）。
    /// 按路径段全等匹配，因此 `.claudex/` 与 `/x.claude` 都不命中——旧实现用
    /// `contains(".claude/")` 子串，相邻名字也能命中。
    ProjectSegment(&'static str),
}

/// 第一方插件的集成目录归属清单（票 07）
///
/// 逐条写明「谁、为什么必须免弹窗」，新增条目要说得出消费它的函数；说不出归属的
/// 一律不加——让它走弹窗 + 记住，而不是往这张表里塞特权。判据：该目录的位置由
/// **第三方 CLI 的约定**决定（插件无从让用户挑），且每次会话都会访问。
const FIRST_PARTY_TRUSTED_DIRS: &[(&str, &[TrustedDir])] = &[
    (
        // agent-hub 技能库：规范库在 `~/.agents/skills`，分发目标由
        // `plugins/agent-hub/rust/src/skills.rs::TARGET_SEGS` 决定（claude / pi 家级私有目录）。
        // 分发与落后检测逐文件读写这些目录，弹窗会把一次「同步技能」拆成 N 次点击。
        "com.bedcode.agent-hub",
        &[
            TrustedDir::Home(".agents"),
            TrustedDir::Home(".claude/skills"),
            TrustedDir::Home(".pi/agent/skills"),
        ],
    ),
    (
        // terminal-session 的 agent 集成面：`task/hooks.rs` 在会话启动前把 hooks / 扩展
        // 写进项目根的 `.claude` / `.codex` / `.pi` / `.opencode`，并清理全局
        // `~/.claude/settings.json` 里属于本插件的那段。项目根由用户选，目录段名由
        // 各 CLI 约定——只有段名是能写进清单的那一半。
        "com.bedcode.terminal-session",
        &[
            TrustedDir::ProjectSegment(".claude"),
            TrustedDir::ProjectSegment(".codex"),
            TrustedDir::ProjectSegment(".pi"),
            TrustedDir::ProjectSegment(".opencode"),
        ],
    ),
];

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
    pub fn new(storage: Arc<PluginStorage>, app_handle: Option<Arc<tauri::AppHandle>>) -> Self {
        Self {
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

        if let Some(layer) = self.matched_layer(plugin_id, &canonical).await {
            tracing::debug!(
                plugin_id = %plugin_id,
                path = %path,
                layer = layer.as_str(),
                "fs_auth: allowed without dialog"
            );
            return true;
        }

        // 第三层：弹窗授权（前两层都未命中才走到这里）
        self.request_user_auth(plugin_id, path, operation).await
    }

    /// 免弹窗判据（第一、二层）：命中则返回**命中的是哪一层**
    ///
    /// 单点给 `check` / `check_batch` / `is_granted` 三处用——三条入口的免弹窗范围
    /// 必须逐字相同，否则「无弹窗面」（WASI 预打开 / 任务单元）与「弹窗面」会给出
    /// 两套答案（同一目录一边可写一边被拒）。
    async fn matched_layer(&self, plugin_id: &str, canonical: &Path) -> Option<FsGrantLayer> {
        if self.first_party_dir_matches(plugin_id, canonical) {
            return Some(FsGrantLayer::FirstPartyDir);
        }
        if self.check_granted_path(plugin_id, canonical).await {
            return Some(FsGrantLayer::Persisted);
        }
        None
    }

    /// 第一方集成目录判定（见 [`first_party_dir_matches_with_home`]）
    fn first_party_dir_matches(&self, plugin_id: &str, canonical: &Path) -> bool {
        first_party_dir_matches_with_home(plugin_id, canonical, dirs::home_dir().as_deref())
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

            // 第一方目录 / 已授权前缀 → 直接放行（与单路径 check 同一判据）
            if self.matched_layer(plugin_id, &canonical).await.is_some() {
                continue;
            }
            ungranted.push(path.clone());
        }

        if ungranted.is_empty() {
            return true;
        }

        self.request_user_auth_batch(plugin_id, &ungranted, operation).await
    }

    /// 查询路径是否已授权（第一方目录 / 持久化授权），**不弹窗**
    ///
    /// 两个无弹窗消费者共用它，语义都是「没有授权就是没有」：
    /// - WASI 预打开目录校验：只为已授权目录建 preopen，防止插件借自身 storage 配置
    ///   （config 可由插件写）指向任意路径绕过授权弹窗；
    /// - 任务单元（core-task 池线程）：未授权即 fail-visible 拒绝，绝不从池线程弹窗
    ///   （弹窗会占用池槽位最长 30s，且用户在错误的时机看到错误的问题）。
    pub async fn is_granted(&self, plugin_id: &str, path: &str) -> bool {
        let Some(canonical) = Self::canonicalize_path(path) else {
            return false;
        };
        self.matched_layer(plugin_id, &canonical).await.is_some()
    }

    /// 弹窗请求用户授权（批量：一次弹窗展示全部未授权路径）
    async fn request_user_auth_batch(&self, plugin_id: &str, paths: &[String], operation: FsOp) -> bool {
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

    /// 检查已授权路径前缀（第二层）
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
        match tokio::time::timeout(std::time::Duration::from_secs(30), reply_rx).await {
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

    /// 持久化授权路径前缀（用户勾选「记住」后的落账动作）
    ///
    /// `pub(crate)`：除 `respond` 之外，闭环用例需要预置「用户已授权」状态——票 07 后
    /// 这是无弹窗放行的**唯一**测试入口（旧写法靠 `.claude` 路径白名单，已退役）。
    pub(crate) async fn save_granted_path(&self, plugin_id: &str, path: &str) -> anyhow::Result<()> {
        let storage_key = "fs_granted_paths".to_string();

        let mut granted: Vec<serde_json::Value> = match self.storage.get(plugin_id, &storage_key).await {
            Ok(Some(serde_json::Value::Array(arr))) => arr,
            _ => Vec::new(),
        };

        // 授权粒度精确化：目录 → 目录本身；已存在文件 → 文件本身；不存在路径
        // （将写入/创建）→ 父目录。不再无条件提取父目录——预授权 home 直子目录
        // （~/.codex、~/.pi、~/.npmrc 等）时父目录为 home 根，一次授权覆盖整个
        // home，架空"访问未授权目录按需弹窗"的兜底（实测 fs_granted_paths 落
        // home 根，任何访问均前缀命中"已授权"、永不弹窗）
        let prefix = if path.is_empty() {
            String::new()
        } else {
            let p = Path::new(path);
            if p.is_dir() {
                p.to_string_lossy().to_string()
            } else if p.exists() {
                p.to_string_lossy().to_string()
            } else {
                p.parent()
                    .map(|pp| pp.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.to_string())
            }
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

/// 第一方集成目录判定的可注入 home 变体（测试用临时目录构造伪 HOME，
/// 与 `component.rs::resolve_preopen_dirs_with_home` 同一形态）
///
/// `home = None` 时 `Home` 形态**不放开**（只剩段名形态可用）：取不到家目录就把
/// `~/.agents` 这类清单退化成「任意位置的 `.agents` 段」是反向的降级。
fn first_party_dir_matches_with_home(plugin_id: &str, canonical: &Path, home: Option<&Path>) -> bool {
    let Some((_, dirs)) = FIRST_PARTY_TRUSTED_DIRS.iter().find(|(id, _)| *id == plugin_id) else {
        return false;
    };
    dirs.iter().any(|d| match d {
        TrustedDir::Home(rel) => match home {
            // 组件边界匹配（strip_prefix）：`~/.agents` 不覆盖 `~/.agentsx`
            Some(h) => canonical.strip_prefix(h.join(rel)).is_ok(),
            None => false,
        },
        TrustedDir::ProjectSegment(seg) => path_has_named_segment(canonical, seg),
    })
}

/// 路径的**自身或任一祖先目录段**是否恰为 `seg`（段名全等，不是子串）
///
/// 命中 `<project>/.claude`（目录本身）与 `<project>/.claude/settings.json`、
/// `<project>/.claude/hooks/x.py`（后代），不命中 `<project>/.claudex/…`
/// 与 `<project>/x.claude/…`——旧实现用 `contains(".claude/")` 子串，相邻命名一并放过。
fn path_has_named_segment(canonical: &Path, seg: &str) -> bool {
    canonical
        .ancestors()
        .any(|a| a.file_name().is_some_and(|name| name == seg))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::wasm_core::storage::PluginStorage;
    use std::sync::Arc;

    /// 内存数据库 + 无头 AppHandle（None）的校验器：无法弹窗，未授权路径应保守拒绝
    async fn headless_checker() -> FsAuthChecker {
        let db = Database::new(&std::path::Path::new(":memory:")).unwrap();
        db.init_schema().unwrap();
        // Mutex 为 tokio::sync::Mutex（super::* 引入），与 PluginStorage 签名一致
        FsAuthChecker::new(Arc::new(PluginStorage::new(Arc::new(Mutex::new(db)))), None)
    }

    /// canonical 后的临时目录根：`matched_layer` 收的是生产形态（已 canonicalize）路径，
    /// 拿未规范化的 `temp_dir()` 比对会因符号链接（macOS `/var` → `/private/var`）
    /// 让前缀与段名判定错位，测出与实现无关的红
    fn canonical_temp_dir() -> PathBuf {
        std::fs::canonicalize(std::env::temp_dir()).expect("temp dir must be canonicalizable")
    }

    /// 票 07 红测本体：**第三方** `fs:read` 插件读任意位置的 `.claude/` 不再免弹窗
    ///
    /// 旧实现按 `.claude/` 子串放行所有插件，`/tmp/x/.claude/settings.json` 这种
    /// 攻击者可控位置也免弹窗——「访问未授权目录按需弹窗」的兜底被架空。
    /// 无头上下文没有弹窗通道 → 未授权即拒；这正是判据：改造前这里返回 true。
    #[tokio::test]
    async fn third_party_cannot_silently_read_claude_dir() {
        let checker = headless_checker().await;
        let path = std::env::temp_dir()
            .join(".claude")
            .join("settings.json")
            .to_string_lossy()
            .to_string();
        assert!(
            !checker
                .check_batch("com.bedcode.test", &[path.clone()], FsOp::Read)
                .await,
            "第三方插件不得静默读 .claude 目录段: {path}"
        );
        assert!(
            !checker.check("com.bedcode.test", &path, FsOp::Read).await,
            "单路径入口同判据（check 与 check_batch 不能两套答案）"
        );
        assert!(
            checker.pending_requests.lock().await.is_empty(),
            "拒绝路径不得残留 pending"
        );
    }

    /// 第一方按归属清单免弹窗：terminal-session 写项目集成目录（段名形态）
    #[tokio::test]
    async fn first_party_project_integration_dirs_stay_silent() {
        let checker = headless_checker().await;
        for seg in [".claude", ".codex", ".pi", ".opencode"] {
            let path = std::env::temp_dir()
                .join("some-project")
                .join(seg)
                .join("settings.json")
                .to_string_lossy()
                .to_string();
            assert!(
                checker
                    .check_batch("com.bedcode.terminal-session", &[path.clone()], FsOp::Write)
                    .await,
                "会话启动前写项目集成目录是本插件的产品面，不得弹窗: {path}"
            );
        }
    }

    /// 收紧的另一半：第一方插件**清单外**的路径不再任意放行
    ///
    /// 旧「插件白名单 = 任意路径免弹窗」把 terminal-session / file-transfer 变成
    /// 全盘可读可写；改造后它们与第三方一样只覆盖到具名目录，其余走弹窗 + 记住。
    #[tokio::test]
    async fn first_party_outside_declared_dirs_requires_grant() {
        let checker = headless_checker().await;
        let outside = std::env::temp_dir()
            .join("home-not-declared")
            .join("secrets.env")
            .to_string_lossy()
            .to_string();
        for plugin in ["com.bedcode.terminal-session", "com.bedcode.agent-hub"] {
            assert!(
                !checker.check_batch(plugin, &[outside.clone()], FsOp::Read).await,
                "{plugin} 读清单外路径必须走授权，不得免弹窗"
            );
        }
        // file-transfer 的白名单条目已删：它一个 fs 原语都不调（走 peer-net），
        // 留着特权只剩风险没有收益
        assert!(
            !checker
                .check_batch("com.bedcode.file-transfer", &[outside], FsOp::Read)
                .await,
            "file-transfer 不再享有 fs 特权（零消费者）"
        );
    }

    #[tokio::test]
    async fn check_batch_ungranted_headless_denied_and_pending_cleaned() {
        let checker = headless_checker().await;
        // 未授权路径 + 无头上下文：保守拒绝，且不残留 pending 条目（泄漏回归）
        let path = std::env::temp_dir().to_string_lossy().to_string();
        assert!(!checker.check_batch("com.bedcode.test", &[path], FsOp::Read).await);
        assert!(checker.pending_requests.lock().await.is_empty());
    }

    /// 命中层的归属必须说得出来（日志「为什么这次没弹框」全靠它）
    #[tokio::test]
    async fn matched_layer_names_the_reason_for_no_dialog() {
        let checker = headless_checker().await;
        let base = canonical_temp_dir();
        let dir = base.join("fs-auth-layer");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("f.txt");

        // 未授权：无层可报
        assert_eq!(
            checker.matched_layer("com.bedcode.test", &file).await,
            None,
            "未授权路径不得凭空报出一层"
        );

        // 持久化授权 → persisted-grant
        checker
            .save_granted_path("com.bedcode.test", &file.to_string_lossy())
            .await
            .unwrap();
        assert_eq!(
            checker.matched_layer("com.bedcode.test", &file).await,
            Some(FsGrantLayer::Persisted)
        );

        // 第一方目录 → first-party-dir（与持久化层分得开）
        let claude = base.join("proj").join(".claude").join("settings.json");
        assert_eq!(
            checker.matched_layer("com.bedcode.terminal-session", &claude).await,
            Some(FsGrantLayer::FirstPartyDir)
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 清单判据表（可注入 home 的纯函数面，逐形态锁边界）
    #[test]
    fn first_party_dir_rules_match_segments_and_home_prefixes() {
        let home = std::path::Path::new("/home/u");
        let p = |s: &str| std::path::PathBuf::from(s);

        // Home 形态：家目录下按组件前缀命中，相邻命名与「别处的同名目录」都不命中
        assert!(first_party_dir_matches_with_home(
            "com.bedcode.agent-hub",
            &p("/home/u/.agents/skills/x/SKILL.md"),
            Some(home)
        ));
        assert!(first_party_dir_matches_with_home(
            "com.bedcode.agent-hub",
            &p("/home/u/.claude/skills/a.md"),
            Some(home)
        ));
        assert!(
            !first_party_dir_matches_with_home(
                "com.bedcode.agent-hub",
                &p("/home/u/.claude/settings.json"),
                Some(home)
            ),
            "agent-hub 只拿到 skills 子树，不是整个 ~/.claude"
        );
        assert!(!first_party_dir_matches_with_home(
            "com.bedcode.agent-hub",
            &p("/home/other/.agents/x"),
            Some(home)
        ));
        assert!(
            !first_party_dir_matches_with_home("com.bedcode.agent-hub", &p("/home/u/.agentsx/y"), Some(home)),
            "组件边界：前缀不得吃掉相邻目录名"
        );
        // 取不到 home → Home 形态不放开（绝不退化成「任意位置的 .agents 段」）
        assert!(!first_party_dir_matches_with_home(
            "com.bedcode.agent-hub",
            &p("/home/u/.agents/skills/x"),
            None
        ));

        // ProjectSegment 形态：段名全等，非子串
        assert!(first_party_dir_matches_with_home(
            "com.bedcode.terminal-session",
            &p("/srv/proj/.claude/hooks/x.py"),
            None
        ));
        assert!(
            first_party_dir_matches_with_home("com.bedcode.terminal-session", &p("/srv/proj/.claude"), None),
            "集成目录本身（read_dir / 建目录）也要覆盖"
        );
        assert!(
            !first_party_dir_matches_with_home("com.bedcode.terminal-session", &p("/srv/proj/.claudex/a"), None),
            "子串不得放过相邻段名"
        );
        assert!(!first_party_dir_matches_with_home(
            "com.bedcode.terminal-session",
            &p("/srv/proj/x.claude/a"),
            None
        ));
        // 收紧的本体：会话项目根本身**不在**清单里（今天它免弹窗 = 全盘可读）
        assert!(
            !first_party_dir_matches_with_home("com.bedcode.terminal-session", &p("/srv/proj/src/main.rs"), None),
            "项目根文件浏览须走弹窗 + 记住，不再有任意路径特权"
        );

        // 未列入清单的插件：一律不放开
        for id in ["com.bedcode.test", "com.bedcode.ai-chatbox", "com.example.third"] {
            assert!(
                !first_party_dir_matches_with_home(id, &p("/home/u/.claude/settings.json"), Some(home)),
                "{id} 不在第一方清单里"
            );
        }
    }

    /// 清单本身是审计面：条目非空、id 不重复、只放第一方
    #[test]
    fn first_party_list_is_well_formed() {
        let mut seen: Vec<&str> = Vec::new();
        for (id, dirs) in FIRST_PARTY_TRUSTED_DIRS {
            assert!(!dirs.is_empty(), "{id} 占了条目却不给目录，等于回到任意路径放行");
            assert!(id.starts_with("com.bedcode."), "清单只放第一方: {id}");
            assert!(
                !seen.contains(id),
                "同一插件 id 不得出现两次（第一个会被静默忽略）: {id}"
            );
            seen.push(id);
        }
        // 已知消费者清单（增删条目必须同时交代这里与票 07 的归属注释）
        assert_eq!(seen, vec!["com.bedcode.agent-hub", "com.bedcode.terminal-session"]);
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
        // 平台感知断言：Windows 规范化分隔符为 `\`，其余平台保持原样
        // （canonicalize_path 的 fallback 在 cfg(windows) 下 replace 分隔符，
        //  非 Windows 直接原样返回——见函数注释）
        #[cfg(not(windows))]
        assert_eq!(canon.to_string_lossy().as_ref(), fake);
        #[cfg(windows)]
        assert_eq!(
            canon.to_string_lossy().as_ref(),
            fake.replace('/', "\\"),
            "fallback path must use backslash on Windows"
        );
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

    // ==================== is_granted（WASI 预打开校验，无弹窗） ====================

    /// `is_granted` 的免弹窗集合 == `matched_layer` 的集合（票 07 后不再等于「任意路径」）
    ///
    /// 它是 WASI 预打开与任务单元的唯一判据：这里放开一分，那两条无弹窗通道就放开一分。
    #[tokio::test]
    async fn is_granted_covers_first_party_dirs_and_persisted_grants_only() {
        let checker = headless_checker().await;
        // 第三方 + 任意位置的 .claude 段 → 不放开（旧实现在这里返回 true）
        let third_party = std::env::temp_dir().join(".claude").to_string_lossy().to_string();
        assert!(
            !checker.is_granted("com.bedcode.test", &third_party).await,
            "第三方插件不得经 is_granted 静默拿到 .claude 目录"
        );
        // 第一方清单内 → 放开（且不经弹窗）
        assert!(
            checker
                .is_granted(
                    "com.bedcode.terminal-session",
                    &std::env::temp_dir()
                        .join("proj/.claude/settings.json")
                        .to_string_lossy()
                )
                .await
        );
        // 第一方清单外 → 不放开（旧「插件白名单 = 任意路径」已退役）
        assert!(
            !checker
                .is_granted("com.bedcode.terminal-session", &std::env::temp_dir().to_string_lossy())
                .await,
            "白名单插件的全盘特权已退役"
        );
        // 持久化授权 → 放开，且只对获授权的插件放开
        let dir = canonical_temp_dir().join("fs-auth-isgranted");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.to_string_lossy().to_string();
        assert!(!checker.is_granted("com.bedcode.test", &path).await);
        checker.save_granted_path("com.bedcode.test", &path).await.unwrap();
        assert!(checker.is_granted("com.bedcode.test", &path).await);
        assert!(!checker.is_granted("com.bedcode.other", &path).await);
        std::fs::remove_dir_all(&dir).ok();
    }
}
