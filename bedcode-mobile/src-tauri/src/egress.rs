//! Egress Policy：外网访问声明与授权（fail-closed）
//!
//! 三层校验（借鉴 fs_auth 三层：白名单 → 声明 → 弹窗授权，AGENTS.md §7）：
//! - **L1 桌面端目标**：目标 host:port = 连接模块当前/最近目标（配对/会话内，
//!   无需声明；host:port 由连接模块注入，见 `add_desktop_target`）
//! - **L2 静态声明**：宿主内置（GitHub API，`system::constants::egress`）+ 插件
//!   manifest `preauthUrls`（插件加载时 `register_plugin_urls` 注册）
//! - **L3 授权弹窗**：未命中声明 → 首次请求弹窗，用户确认后放行（会话级默认
//!   + 可选持久「不再询问」，spec §9 D7）
//! - **拒绝**：以上均未通过 → fail-closed，请求不发，返回
//!   `EXTERNAL_URL_NOT_DECLARED` / `EXTERNAL_URL_DENIED`。
//!
//! 裁决与记忆在 Rust 端（§8 安全红线）：授权记忆存 Rust 持久层
//! （`egress_grants.json`，app_data_dir），不落 localStorage（spec §5.6 机制要点 1）。
//! 弹窗桥：Rust emit `egress_consent_request` → 前端渲染 → `egress_consent_resolve`
//! 回 Rust 裁决（oneshot 通道 + 超时兜底，超时视为拒绝）。

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock, RwLock as StdRwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::system::constants::egress::HOST_BUILTIN_URL_PATTERNS;
use crate::AppError;
use crate::Result;

// ==================== 常量与错误码 ====================

/// Egress 拒绝错误码：URL 未声明（fail-closed）
pub const ERROR_URL_NOT_DECLARED: &str = "EXTERNAL_URL_NOT_DECLARED";
/// Egress 拒绝错误码：用户拒绝（弹窗 deny）
pub const ERROR_URL_DENIED: &str = "EXTERNAL_URL_DENIED";

/// 弹窗桥等待前端响应的超时（超时视为拒绝，fail-closed）
pub const CONSENT_TIMEOUT: Duration = Duration::from_secs(30);

/// 授权记忆持久文件（app_data_dir 下）
const GRANTS_FILE: &str = "egress_grants.json";

// ==================== URL 轻量解析（仅 Egress 匹配用） ====================

/// 轻量解析后的 URL 成分（HTTP/HTTPS 场景足够）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiteUrl {
    pub scheme: String,
    pub host: String,
    pub port: Option<u16>,
    /// 不含 query 的路径（默认 "/"）
    pub path: String,
}

/// 轻量 URL 解析：`scheme://host[:port]/path...`。host 归一化小写；
/// 无法解析返回 None（判定方按拒绝处理，fail-closed）。
pub fn parse_url_lite(url: &str) -> Option<LiteUrl> {
    let scheme_end = url.find("://")?;
    let scheme = url[..scheme_end].to_lowercase();
    let rest = &url[scheme_end + 3..];
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    // authority 可能含 query（无 path 时）——取到 ? 为止
    let authority = authority.split('?').next().unwrap_or(authority);
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) => (h.to_lowercase(), p.parse::<u16>().ok()),
        None => (authority.to_lowercase(), None),
    };
    if host.is_empty() {
        return None;
    }
    // path 剥 query
    let path = match path.split('?').next() {
        Some(p) if !p.is_empty() => p,
        _ => "/",
    };
    Some(LiteUrl {
        scheme,
        host,
        port,
        path: path.to_string(),
    })
}

// ==================== URL 声明模式（preauthUrls / 宿主内置） ====================

/// URL 声明模式（glob）：`[scheme://][*.]host[:port][/path-prefix]`
///
/// - `*` 仅支持子域通配前缀（`*.example.com` 匹配 `example.com` 与任意子域）
/// - scheme / port 省略时按任意匹配（仅 host 维度声明）
/// - path-prefix 为前缀匹配（默认全路径）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UrlPattern {
    pub scheme: Option<String>,
    pub wildcard_subdomain: bool,
    pub host: String,
    pub port: Option<u16>,
    pub path_prefix: Option<String>,
}

impl UrlPattern {
    /// 解析声明字符串；非法（空 host）返回 None
    pub fn parse(pattern: &str) -> Option<UrlPattern> {
        let pattern = pattern.trim();
        if pattern.is_empty() {
            return None;
        }
        let (scheme, rest) = match pattern.find("://") {
            Some(i) => (Some(pattern[..i].to_lowercase()), &pattern[i + 3..]),
            None => (None, pattern),
        };
        let (authority, path_prefix) = match rest.find('/') {
            Some(i) => {
                // glob：末尾 `*` 通配任意剩余 → 存前缀（匹配用 starts_with）
                let p = rest[i..].strip_suffix('*').unwrap_or(&rest[i..]);
                (&rest[..i], Some(p.to_string()))
            }
            None => (rest, None),
        };
        let (host_part, port) = match authority.rsplit_once(':') {
            Some((h, p)) if p.parse::<u16>().is_ok() => (h, p.parse::<u16>().ok()),
            _ => (authority, None),
        };
        let (wildcard_subdomain, host) = match host_part.strip_prefix("*.") {
            Some(h) => (true, h.to_lowercase()),
            None => (false, host_part.to_lowercase()),
        };
        if host.is_empty() {
            return None;
        }
        Some(UrlPattern {
            scheme,
            wildcard_subdomain,
            host,
            port,
            path_prefix,
        })
    }

    /// 模式是否匹配解析后的 URL
    pub fn matches(&self, url: &LiteUrl) -> bool {
        if let Some(s) = &self.scheme {
            if *s != url.scheme {
                return false;
            }
        }
        let host_matches = if self.wildcard_subdomain {
            url.host == self.host || url.host.ends_with(&format!(".{}", self.host))
        } else {
            url.host == self.host
        };
        if !host_matches {
            return false;
        }
        if let Some(p) = self.port {
            if Some(p) != url.port {
                return false;
            }
        }
        if let Some(prefix) = &self.path_prefix {
            if !url.path.starts_with(prefix) {
                return false;
            }
        }
        true
    }
}

// ==================== 判定结果与错误 ====================

/// 放行来源（日志/审计用）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrantSource {
    /// L1：桌面端目标
    L1Desktop,
    /// L2：宿主内置声明
    L2Builtin,
    /// L2：插件 preauthUrls 声明
    L2Plugin,
    /// L3：授权记忆（会话/持久）命中
    L3Memory,
}

/// 弹窗事务请求（调用方携带 request_id 发起；前端按 request_id 回执）
#[derive(Debug, Clone)]
pub struct ConsentRequest {
    /// 弹窗事务 id（复用调用方 request_id，随事件下发并回执）
    pub id: String,
    pub url: String,
    pub host: String,
    pub path: String,
    /// 调用方来源（宿主 useUpdateChecker / 插件 ai-chatbox 等）
    pub source: String,
}

/// Egress 拒绝错误（fail-closed；code 为错误码常量）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EgressError {
    pub code: &'static str,
    pub url: String,
}

impl EgressError {
    /// 未声明（任何声明/记忆均未命中）
    pub fn not_declared(url: &str) -> Self {
        Self {
            code: ERROR_URL_NOT_DECLARED,
            url: url.to_string(),
        }
    }

    /// 用户拒绝（弹窗 deny / 超时）
    pub fn denied(url: &str) -> Self {
        Self {
            code: ERROR_URL_DENIED,
            url: url.to_string(),
        }
    }
}

/// Egress 判定结果
#[derive(Debug)]
pub enum EgressDecision {
    /// 放行（记录来源）
    Allow(GrantSource),
    /// 需授权弹窗（调用方应发起 consent 流程；超时/拒绝 → Deny）
    NeedConsent(ConsentRequest),
    /// 拒绝（fail-closed；错误码见 EgressError.code）
    Deny(EgressError),
}

// ==================== 授权记忆 ====================

/// 持久授权条目（egress_grants.json；「不再询问」勾选落盘）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PersistentGrant {
    /// 授权的 host（精确；小写）
    pub host: String,
    /// 可选 path 前缀（None = 全部路径）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_prefix: Option<String>,
    /// 授权时间（unix 秒；设置页展示）
    pub allowed_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct PersistentGrantStore {
    #[serde(default)]
    grants: Vec<PersistentGrant>,
}

/// 前端弹窗回执
#[derive(Debug, Clone, Copy)]
pub struct ConsentVerdict {
    pub allow: bool,
    /// 用户勾选「不再询问」→ 持久记忆
    pub persist: bool,
}

// ==================== Egress 策略引擎 ====================

/// Egress 策略引擎（全局单例；Rust 端裁决）
pub struct EgressPolicy {
    /// L1：桌面端目标 host:port 集合（连接模块注入；probe/connect 时 add）
    desktop_targets: StdRwLock<HashSet<(String, u16)>>,
    /// L2：插件 preauthUrls 声明（plugin_id → 模式列表；插件卸载时移除）
    plugin_patterns: StdRwLock<HashMap<String, Vec<UrlPattern>>>,
    /// L3：会话级授权（host → 记忆；本次连接有效，连接断开/撤销时清空）
    session_grants: StdRwLock<HashMap<String, PersistentGrant>>,
    /// L3：持久授权（加载自 GRANTS_FILE；「不再询问」落盘）
    persistent_grants: StdRwLock<Vec<PersistentGrant>>,
    /// 持久文件路径（init 时设置）
    grants_path: StdRwLock<Option<PathBuf>>,
    /// 弹窗桥：request_id → 前端回执通道
    pending_consents: tokio::sync::Mutex<HashMap<String, tokio::sync::oneshot::Sender<ConsentVerdict>>>,
}

impl EgressPolicy {
    fn new() -> Self {
        Self {
            desktop_targets: StdRwLock::new(HashSet::new()),
            plugin_patterns: StdRwLock::new(HashMap::new()),
            session_grants: StdRwLock::new(HashMap::new()),
            persistent_grants: StdRwLock::new(Vec::new()),
            grants_path: StdRwLock::new(None),
            pending_consents: tokio::sync::Mutex::new(HashMap::new()),
        }
    }

    // ---------- L1：桌面端目标注入 ----------

    /// 注入/更新桌面端目标 host:port（setApiBaseUrl / probe / ws_connect 时调用）。
    /// 集合式维护：probe 阶段（ws_connect 前）也可放行，解决 httpProbe 时序。
    pub fn add_desktop_target(&self, host: &str, port: u16) {
        self.desktop_targets
            .write()
            .unwrap()
            .insert((host.to_lowercase(), port));
        tracing::debug!(host = %host, port = %port, "egress: desktop target registered");
    }

    /// 清空桌面端目标（断开/重置连接时调用）
    pub fn clear_desktop_targets(&self) {
        self.desktop_targets.write().unwrap().clear();
    }

    /// 是否命中 L1 桌面端目标（http_request kind=desktop 校验用；
    /// port 缺省不命中——桌面端 HTTP 服务必有自定义端口）
    pub fn is_desktop_target(&self, host: &str, port: Option<u16>) -> bool {
        match port {
            Some(p) => self.desktop_targets.read().unwrap().contains(&(host.to_lowercase(), p)),
            None => false,
        }
    }

    // ---------- L2：插件声明注册 ----------

    /// 注册插件 preauthUrls 声明（插件加载时调用；失败模式静默跳过非法项）
    pub fn register_plugin_urls(&self, plugin_id: &str, patterns: &[String]) {
        let parsed: Vec<UrlPattern> = patterns.iter().filter_map(|p| UrlPattern::parse(p)).collect();
        tracing::info!(
            plugin_id = %plugin_id,
            declared = %patterns.len(),
            parsed = %parsed.len(),
            "egress: plugin url declarations registered"
        );
        self.plugin_patterns
            .write()
            .unwrap()
            .insert(plugin_id.to_string(), parsed);
    }

    /// 移除插件声明（插件卸载时调用）
    pub fn unregister_plugin_urls(&self, plugin_id: &str) {
        self.plugin_patterns.write().unwrap().remove(plugin_id);
        tracing::debug!(plugin_id = %plugin_id, "egress: plugin url declarations removed");
    }

    // ---------- L3：授权记忆 ----------

    /// 记忆命中判定（host + 可选 path 前缀；先持久后会话）
    fn memory_hit(&self, host: &str, path: &str) -> bool {
        let hit = |g: &PersistentGrant| {
            g.host == host
                && match &g.path_prefix {
                    Some(prefix) => path.starts_with(prefix),
                    None => true,
                }
        };
        self.persistent_grants.read().unwrap().iter().any(hit) || self.session_grants.read().unwrap().values().any(hit)
    }

    /// 记忆授权（L3 命中后调用）：persist=true 落盘（「不再询问」），否则会话级
    fn record_grant(&self, host: &str, path_prefix: Option<String>, persist: bool) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let grant = PersistentGrant {
            host: host.to_string(),
            path_prefix,
            allowed_at: now,
        };
        if persist {
            self.persistent_grants.write().unwrap().push(grant.clone());
            self.save_persistent_grants();
        } else {
            self.session_grants.write().unwrap().insert(host.to_string(), grant);
        }
        tracing::info!(host = %host, persist = %persist, "egress: url granted");
    }

    /// 撤销全部授权（设置页「撤销」；清会话 + 清持久文件）
    pub fn revoke_all_grants(&self) {
        self.session_grants.write().unwrap().clear();
        self.persistent_grants.write().unwrap().clear();
        self.save_persistent_grants();
        tracing::info!("egress: all grants revoked");
    }

    /// 列出全部授权（会话 + 持久；设置页展示）
    pub fn list_grants(&self) -> Vec<PersistentGrant> {
        let mut grants: Vec<PersistentGrant> = self.session_grants.read().unwrap().values().cloned().collect();
        grants.extend(self.persistent_grants.read().unwrap().iter().cloned());
        grants
    }

    /// 持久授权落盘（egress_grants.json；失败仅告警，不阻断放行）
    fn save_persistent_grants(&self) {
        let path = match self.grants_path.read().unwrap().clone() {
            Some(p) => p,
            None => return,
        };
        let store = PersistentGrantStore {
            grants: self.persistent_grants.read().unwrap().clone(),
        };
        let content = match serde_json::to_string_pretty(&store) {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("egress: serialize grants failed: {}", e);
                return;
            }
        };
        if let Some(parent) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                tracing::error!(error = %e, "egress: create grants dir failed");
                return;
            }
        }
        if let Err(e) = std::fs::write(&path, content) {
            tracing::error!(error = %e, path = %path.display(), "egress: save grants failed");
        }
    }

    // ---------- 弹窗桥 ----------

    /// 发起授权弹窗并等待前端回执（超时视为拒绝，fail-closed）。
    /// 调用方（http_request / 插件 host_http_fetch）传入自己的 request_id。
    pub async fn request_consent(&self, app: &tauri::AppHandle, request: ConsentRequest) -> Result<bool> {
        use tauri::Emitter;
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.pending_consents.lock().await.insert(request.id.clone(), tx);
        let payload = serde_json::json!({
            "request_id": request.id,
            "url": request.url,
            "host": request.host,
            "path": request.path,
            "source": request.source,
        });
        app.emit("egress_consent_request", payload)
            .map_err(|e| AppError::Egress(format!("egress consent emit failed: {e}")))?;
        tracing::info!(
            request_id = %request.id,
            host = %request.host,
            source = %request.source,
            "egress: consent requested"
        );
        match tokio::time::timeout(CONSENT_TIMEOUT, rx).await {
            Ok(Ok(verdict)) => {
                if verdict.allow {
                    self.record_grant(&request.host, None, verdict.persist);
                    Ok(true)
                } else {
                    tracing::warn!(request_id = %request.id, "egress: consent denied by user");
                    Ok(false)
                }
            }
            Ok(Err(_)) | Err(_) => {
                // 通道断 / 超时 → 拒绝 + 清理，防 map 膨胀
                self.pending_consents.lock().await.remove(&request.id);
                tracing::warn!(request_id = %request.id, "egress: consent timeout or channel closed");
                Ok(false)
            }
        }
    }

    /// 前端回执（egress_consent_resolve 命令调用）；request_id 不存在返回 false
    pub async fn resolve_consent(&self, request_id: &str, allow: bool, persist: bool) -> bool {
        let mut pending = self.pending_consents.lock().await;
        match pending.remove(request_id) {
            Some(tx) => {
                let _ = tx.send(ConsentVerdict { allow, persist });
                true
            }
            None => false,
        }
    }

    // ---------- 核心判定 ----------

    /// 三层判定（同步；L1/L2/记忆命中 → Allow；未命中 → NeedConsent / Deny）
    ///
    /// `source`：调用方来源描述（宿主 useUpdateChecker / 插件 ai-chatbox 等），
    /// 弹窗展示与日志使用。
    pub fn decide(&self, url: &str, source: &str) -> EgressDecision {
        let parsed = match parse_url_lite(url) {
            Some(p) => p,
            None => {
                return EgressDecision::Deny(EgressError::not_declared(url));
            }
        };

        // L1：桌面端目标（配对/会话内，无需声明）
        {
            let targets = self.desktop_targets.read().unwrap();
            if let Some(port) = parsed.port {
                if targets.contains(&(parsed.host.clone(), port)) {
                    return EgressDecision::Allow(GrantSource::L1Desktop);
                }
            }
        }

        // L2：宿主内置声明
        let builtin_hit = HOST_BUILTIN_URL_PATTERNS
            .iter()
            .filter_map(|p| UrlPattern::parse(p))
            .any(|p| p.matches(&parsed));
        if builtin_hit {
            return EgressDecision::Allow(GrantSource::L2Builtin);
        }

        // L2：插件 preauthUrls 声明
        let plugin_hit = self
            .plugin_patterns
            .read()
            .unwrap()
            .values()
            .flatten()
            .any(|p| p.matches(&parsed));
        if plugin_hit {
            return EgressDecision::Allow(GrantSource::L2Plugin);
        }

        // L3：授权记忆命中
        if self.memory_hit(&parsed.host, &parsed.path) {
            return EgressDecision::Allow(GrantSource::L3Memory);
        }

        // 未命中 → 需弹窗（fail-closed：调用方不弹则拒绝）
        EgressDecision::NeedConsent(ConsentRequest {
            id: String::new(), // 调用方填充 request_id
            url: url.to_string(),
            host: parsed.host,
            path: parsed.path,
            source: source.to_string(),
        })
    }
}

// ==================== 全局单例 ====================

static EGRESS: OnceLock<Arc<EgressPolicy>> = OnceLock::new();

/// 获取全局 Egress 策略引擎
pub fn policy() -> Arc<EgressPolicy> {
    EGRESS.get_or_init(|| Arc::new(EgressPolicy::new())).clone()
}

/// 初始化（lib.rs setup 调用）：设置持久层路径并加载既有授权
pub fn init(app_data_dir: PathBuf) {
    let p = policy();
    let path = app_data_dir.join(GRANTS_FILE);
    let loaded = match std::fs::read_to_string(&path) {
        Ok(content) => match serde_json::from_str::<PersistentGrantStore>(&content) {
            Ok(store) => store.grants,
            Err(e) => {
                tracing::warn!(error = %e, path = %path.display(), "egress: grants file malformed, start empty");
                Vec::new()
            }
        },
        Err(_) => Vec::new(),
    };
    *p.grants_path.write().unwrap() = Some(path);
    *p.persistent_grants.write().unwrap() = loaded;
    tracing::info!(
        grants = %p.persistent_grants.read().unwrap().len(),
        "egress: policy initialized"
    );
}

// ==================== 跳转（redirect）重校验 ====================

/// 跳转目标是否为私网/回环/链路本地 IP（字面量判定）。
///
/// 主机名无法解析为 IP 时按公网处理：首跳已过 L1/L2/L3，DNS 内网不在本
/// 策略面（阻断面聚焦「302 → 内网 IP / 云元数据 169.254.x.x」的经典 SSRF）。
fn is_private_ip(host: &str) -> bool {
    host.parse::<std::net::IpAddr>()
        .map(|ip| match ip {
            std::net::IpAddr::V4(v4) => v4.is_private() || v4.is_loopback() || v4.is_link_local(),
            std::net::IpAddr::V6(v6) => v6.is_loopback() || v6.is_unicast_link_local(),
        })
        .unwrap_or(false)
}

/// 跳转裁决（纯函数，供 `redirect_policy` 与单测共用）：
///
/// reqwest 默认跟随 10 跳且不重过 egress L1/L2/L3——外部 API 302 到内网/
/// 云元数据即直连（SSRF 面）。同步策略上下文无法弹窗，故私有目标跳转按
/// 白名单 fail-closed：
/// - 公网目标：跟随（首跳已过 L1/L2/L3 校验，公网→公网无新增面）；
/// - 已声明桌面端目标（L1）：跟随（本机 LAN 服务站内跳转）；
/// - 与链上前序 URL 同源：跟随（同 host:port 跳转不变更目标面）；
/// - 其余私有目标：Stop（调用方拿到 3xx 自行处理）。
pub fn redirect_decision(next_url: &str, previous: &[&str]) -> bool {
    let Some(parsed) = parse_url_lite(next_url) else {
        return false; // 非法跳转目标 fail-closed
    };
    if !is_private_ip(&parsed.host) {
        return true;
    }
    if policy().is_desktop_target(&parsed.host, parsed.port) {
        return true;
    }
    let prev: Vec<LiteUrl> = previous.iter().filter_map(|u| parse_url_lite(u)).collect();
    if prev.iter().any(|p| p.host == parsed.host && p.port == parsed.port) {
        return true; // 同源跳转
    }
    // 前序全为私有地址：私网→私网链路（如 NAS 服务跳转），维持跟随
    !prev.is_empty() && prev.iter().all(|p| is_private_ip(&p.host))
}

/// reqwest 跳转策略：`redirect_decision` 的适配层（同步裁决，见其文档）
pub fn redirect_policy() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(|attempt| {
        let prev: Vec<&str> = attempt.previous().iter().map(|u| u.as_str()).collect();
        if redirect_decision(attempt.url().as_str(), &prev) {
            attempt.follow()
        } else {
            attempt.stop()
        }
    })
}

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_url_lite_basic() {
        let u = parse_url_lite("https://api.github.com/repos/x/y/releases/latest?per_page=1").unwrap();
        assert_eq!(u.scheme, "https");
        assert_eq!(u.host, "api.github.com");
        assert_eq!(u.port, None);
        assert_eq!(u.path, "/repos/x/y/releases/latest");

        let u = parse_url_lite("http://192.168.1.5:4455/api/health").unwrap();
        assert_eq!(u.scheme, "http");
        assert_eq!(u.host, "192.168.1.5");
        assert_eq!(u.port, Some(4455));
        assert_eq!(u.path, "/api/health");

        // 无 path → 默认 "/"
        let u = parse_url_lite("https://example.com").unwrap();
        assert_eq!(u.path, "/");
        // 非法 → None
        assert!(parse_url_lite("not-a-url").is_none());
        assert!(parse_url_lite("").is_none());
    }

    #[test]
    fn url_pattern_parse_and_match() {
        // 完整模式
        let p = UrlPattern::parse("https://api.github.com/*").unwrap();
        assert_eq!(p.scheme.as_deref(), Some("https"));
        assert_eq!(p.host, "api.github.com");
        assert_eq!(p.path_prefix.as_deref(), Some("/"));
        assert!(p.matches(&parse_url_lite("https://api.github.com/repos/x").unwrap()));
        assert!(!p.matches(&parse_url_lite("http://api.github.com/repos/x").unwrap()));

        // 子域通配
        let p = UrlPattern::parse("https://*.openai.com/*").unwrap();
        assert!(p.wildcard_subdomain);
        assert!(p.matches(&parse_url_lite("https://api.openai.com/v1/chat").unwrap()));
        assert!(p.matches(&parse_url_lite("https://openai.com/x").unwrap()));
        assert!(!p.matches(&parse_url_lite("https://evilopenai.com/x").unwrap()));

        // 裸 host（scheme/port 任意）+ path 前缀
        let p = UrlPattern::parse("example.com/api").unwrap();
        assert!(p.scheme.is_none());
        assert!(p.matches(&parse_url_lite("https://example.com/api/v1/list").unwrap()));
        assert!(p.matches(&parse_url_lite("http://example.com/api").unwrap()));
        assert!(!p.matches(&parse_url_lite("https://example.com/other").unwrap()));

        // 非法
        assert!(UrlPattern::parse("").is_none());
        assert!(UrlPattern::parse("*.").is_none());
    }

    /// L2 宿主内置：GitHub API 放行（useUpdateChecker 路径）
    #[test]
    fn builtin_github_allowed() {
        let p = policy();
        assert!(matches!(
            p.decide(
                "https://api.github.com/repos/bedcode/releases/latest",
                "useUpdateChecker"
            ),
            EgressDecision::Allow(GrantSource::L2Builtin)
        ));
        // 其它域名未声明 → NeedConsent
        assert!(matches!(
            p.decide("https://unknown.example.com/x", "useUpdateChecker"),
            EgressDecision::NeedConsent(_)
        ));
    }

    /// L1 桌面端目标：注入后放行；未注入 → NeedConsent
    #[test]
    fn l1_desktop_target_allowed() {
        let p = policy();
        p.clear_desktop_targets();
        assert!(matches!(
            p.decide("http://192.168.1.5:4455/api/health", "host"),
            EgressDecision::NeedConsent(_)
        ));
        p.add_desktop_target("192.168.1.5", 4455);
        assert!(matches!(
            p.decide("http://192.168.1.5:4455/api/health", "host"),
            EgressDecision::Allow(GrantSource::L1Desktop)
        ));
        // 其它端口不算桌面端目标
        assert!(matches!(
            p.decide("http://192.168.1.5:9999/api/health", "host"),
            EgressDecision::NeedConsent(_)
        ));
        p.clear_desktop_targets();
    }

    // ==================== 跳转重校验 ====================

    /// 公网跳转：跟随（首跳已过 L1/L2/L3，公网→公网无新增面）
    #[test]
    fn redirect_public_followed() {
        assert!(redirect_decision(
            "https://cdn.example.com/file",
            &["https://api.github.com/x"],
        ));
        assert!(redirect_decision("https://a.com", &[]));
    }

    /// SSRF 阻断：外网 302 → 内网/回环/链路本地（云元数据 169.254.169.254）Stop
    #[test]
    fn redirect_public_to_private_stopped() {
        assert!(!redirect_decision(
            "http://192.168.1.5:9999/api",
            &["https://api.example.com/x"],
        ));
        assert!(!redirect_decision(
            "http://127.0.0.1:8000/meta",
            &["https://api.example.com/x"],
        ));
        assert!(!redirect_decision(
            "http://169.254.169.254/latest/meta-data",
            &["https://api.example.com/x"],
        ));
        // 非法跳转目标 fail-closed
        assert!(!redirect_decision("not-a-url", &["https://api.example.com/x"]));
    }

    /// 私网合法跳转放行：已声明桌面端目标 / 同源 / 私网→私网链
    #[test]
    fn redirect_private_whitelisted() {
        // 已声明桌面端目标
        policy().add_desktop_target("192.168.1.5", 4455);
        assert!(redirect_decision(
            "http://192.168.1.5:4455/api/other",
            &["https://api.example.com/x"],
        ));
        // 同源跳转（desktop 目标站内 302 到自身其它路径）
        assert!(redirect_decision(
            "http://192.168.1.5:4455/api/b",
            &["http://192.168.1.5:4455/api/a"],
        ));
        // 私网→私网链（NAS 站内跳转）
        assert!(redirect_decision(
            "http://192.168.1.9/x",
            &["http://192.168.1.5:4455/a"],
        ));
        // 公网链中存在私网前序仍阻断（混合链 fail-closed）
        assert!(!redirect_decision(
            "http://192.168.1.9/x",
            &["http://192.168.1.5:4455/a", "https://api.example.com/y"],
        ));
        policy().clear_desktop_targets();
    }

    /// L2 插件声明：注册后放行；卸载后拒绝
    #[test]
    fn plugin_declarations_registered_and_removed() {
        let p = policy();
        p.register_plugin_urls(
            "com.bedcode.ai-chatbox",
            &[
                "https://*.openai.com/*".to_string(),
                "https://api.deepseek.com/*".to_string(),
            ],
        );
        assert!(matches!(
            p.decide("https://api.openai.com/v1/chat/completions", "plugin:ai-chatbox"),
            EgressDecision::Allow(GrantSource::L2Plugin)
        ));
        assert!(matches!(
            p.decide("https://api.deepseek.com/chat/completions", "plugin:ai-chatbox"),
            EgressDecision::Allow(GrantSource::L2Plugin)
        ));
        // 未声明的其它域名 → NeedConsent
        assert!(matches!(
            p.decide("https://custom.example.com/v1", "plugin:ai-chatbox"),
            EgressDecision::NeedConsent(_)
        ));
        p.unregister_plugin_urls("com.bedcode.ai-chatbox");
        assert!(matches!(
            p.decide("https://api.openai.com/v1/chat/completions", "plugin:ai-chatbox"),
            EgressDecision::NeedConsent(_)
        ));
    }

    /// L3 授权记忆：会话级 grant 放行 + 撤销后重新需要弹窗
    #[test]
    fn session_grant_memory() {
        let p = policy();
        // 清空避免与持久文件互相干扰（测试环境无 init，persistent 为空）
        p.revoke_all_grants();
        assert!(matches!(
            p.decide("https://custom.example.com/api", "host"),
            EgressDecision::NeedConsent(_)
        ));
        p.record_grant("custom.example.com", None, false);
        assert!(matches!(
            p.decide("https://custom.example.com/api", "host"),
            EgressDecision::Allow(GrantSource::L3Memory)
        ));
        p.revoke_all_grants();
        assert!(matches!(
            p.decide("https://custom.example.com/api", "host"),
            EgressDecision::NeedConsent(_)
        ));
    }

    /// 非法 URL → fail-closed 拒绝（非弹窗——无法展示的 URL 不应打扰用户）
    #[test]
    fn malformed_url_denied() {
        let p = policy();
        match p.decide("not-a-url", "host") {
            EgressDecision::Deny(e) => assert_eq!(e.code, ERROR_URL_NOT_DECLARED),
            other => panic!("expected Deny, got {other:?}"),
        }
    }
}
