//! Global State Module
//!
//! 全局单例管理器和 Token 存储

use std::sync::Arc;
use std::sync::OnceLock;

use crate::auth::AuthManager;
use crate::connection::manager::ConnectionManager;
use crate::plugin::manager::PluginManager;
use crate::session::SessionManager;
use crate::system::info::SystemInfo;

// ==================== Global Token ====================

/// 全局 Token 存储（移动端）
///
/// 前端启动时从 localStorage 读取并设置，发送消息时自动注入
static GLOBAL_TOKEN: std::sync::RwLock<String> = std::sync::RwLock::new(String::new());

/// 设置全局 Token
pub fn set_global_token(token: &str) {
    let mut guard = GLOBAL_TOKEN.write().unwrap();
    *guard = token.to_string();
    tracing::info!("[GlobalToken] Token updated, length={}", token.len());
}

/// 获取全局 Token
pub fn get_global_token() -> String {
    let guard = GLOBAL_TOKEN.read().unwrap();
    guard.clone()
}

/// 清除全局 Token
pub fn clear_global_token() {
    let mut guard = GLOBAL_TOKEN.write().unwrap();
    *guard = String::new();
    tracing::info!("[GlobalToken] Token cleared");
}

// ==================== Manager Singletons ====================

/// 全局连接管理器单例
static CONNECTION_MANAGER: OnceLock<Arc<ConnectionManager>> = OnceLock::new();

/// 全局认证管理器单例
static AUTH_MANAGER: OnceLock<Arc<AuthManager>> = OnceLock::new();

/// 全局会话管理器单例
static SESSION_MANAGER: OnceLock<Arc<SessionManager>> = OnceLock::new();

/// 获取连接管理器
pub fn get_connection_manager() -> Arc<ConnectionManager> {
    CONNECTION_MANAGER.get_or_init(|| ConnectionManager::new()).clone()
}

/// 获取认证管理器
pub fn get_auth_manager() -> Arc<AuthManager> {
    AUTH_MANAGER
        .get_or_init(|| {
            let conn = get_connection_manager();
            AuthManager::new(conn)
        })
        .clone()
}

/// 获取会话管理器
pub fn get_session_manager() -> Arc<SessionManager> {
    SESSION_MANAGER
        .get_or_init(|| {
            let conn = get_connection_manager();
            SessionManager::new(conn)
        })
        .clone()
}

// ==================== System Info ====================

/// 全局系统信息单例
static SYSTEM_INFO: OnceLock<Arc<SystemInfo>> = OnceLock::new();

/// 初始化系统信息（在 lib.rs 启动流程中调用一次）
pub fn init_system_info(info: SystemInfo) -> Arc<SystemInfo> {
    let arc = Arc::new(info);
    let _ = SYSTEM_INFO.set(arc.clone());
    arc
}

/// 获取系统信息
///
/// # Panics
/// 如果 init_system_info 未调用则 panic
pub fn get_system_info() -> Arc<SystemInfo> {
    SYSTEM_INFO.get().expect("SystemInfo not initialized").clone()
}

/// 尝试获取系统信息（未初始化返回 None，供 best-effort 路径使用）
pub fn try_get_system_info() -> Option<Arc<SystemInfo>> {
    SYSTEM_INFO.get().cloned()
}

// ==================== Plugin Manager ====================

/// 全局插件管理器单例
static PLUGIN_MANAGER: OnceLock<Arc<PluginManager>> = OnceLock::new();

/// 初始化插件管理器（在 lib.rs setup 中调用）
pub fn init_plugin_manager(manager: Arc<PluginManager>) -> Arc<PluginManager> {
    let _ = PLUGIN_MANAGER.set(manager.clone());
    manager
}

/// 获取插件管理器
///
/// # Panics
/// 如果 init_plugin_manager 未调用则 panic
pub fn get_plugin_manager() -> Arc<PluginManager> {
    PLUGIN_MANAGER.get().expect("PluginManager not initialized").clone()
}

/// 尝试获取插件管理器（未初始化返回 None，供 best-effort 路径使用）
pub fn try_get_plugin_manager() -> Option<Arc<PluginManager>> {
    PLUGIN_MANAGER.get().cloned()
}

// ==================== File Service ====================

// ==================== Link Crypto Context（issue 09） ====================

/// 链路加密运行期上下文
///
/// 设置开关与 pin 存于 WebView localStorage（issue 05/06 的 TS 侧），而常驻
/// 事件 WS 建连在 Rust 侧——前端经 `set_link_crypto_context` 命令把当前态
/// 推送到此，建连时读取。缺省全关：未推送前事件 WS 保持明文（与现状一致）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkCryptoContext {
    /// 主开关（对应移动端 trafficEncryption.enabled）
    pub enabled: bool,
    /// 严格模式：协商被拒/失败时断连报错而非明文续跑
    pub strict_mode: bool,
    /// 事件通道子开关（对应桌面 encryptWsEvent）
    pub encrypt_ws_event: bool,
    /// 已 pin 的桌面端身份公钥（base64）；None = 未配对/未下发
    pub kd_public_b64: Option<String>,
}

impl Default for LinkCryptoContext {
    fn default() -> Self {
        Self { enabled: false, strict_mode: false, encrypt_ws_event: true, kd_public_b64: None }
    }
}

static LINK_CRYPTO_CONTEXT: std::sync::RwLock<LinkCryptoContext> =
    std::sync::RwLock::new(LinkCryptoContext {
        enabled: false,
        strict_mode: false,
        encrypt_ws_event: true,
        kd_public_b64: None,
    });

/// 读取链路加密运行期上下文快照
pub fn get_link_crypto_context() -> LinkCryptoContext {
    LINK_CRYPTO_CONTEXT.read().unwrap().clone()
}

/// 更新链路加密运行期上下文（前端 set_link_crypto_context 命令调用）
pub fn set_link_crypto_context(ctx: LinkCryptoContext) {
    *LINK_CRYPTO_CONTEXT.write().unwrap() = ctx;
}

/// 更新已 pin 的桌面端身份公钥（认证成功时随 auth 响应落地，issue：修复 pin 断链）
///
/// 仅在有值时覆盖：None 不清除既有 pin——防主动降级攻击抹除信任锚
/// （与前端 notePin 的语义一致：协商失败不清 pin）。
pub fn update_link_crypto_pin(kd_public_b64: Option<String>) {
    if let Some(kd) = kd_public_b64 {
        LINK_CRYPTO_CONTEXT.write().unwrap().kd_public_b64 = Some(kd);
    }
}

/// 事件 WS 是否应发起加密协商：主开关 ∧ 事件子开关 ∧ 已持有 pin。
/// 协商依赖配对期下发的桌面端身份公钥作信任锚，三者缺一即明文。
pub fn is_event_encryption_active() -> bool {
    let ctx = get_link_crypto_context();
    ctx.enabled && ctx.encrypt_ws_event && ctx.kd_public_b64.is_some()
}
