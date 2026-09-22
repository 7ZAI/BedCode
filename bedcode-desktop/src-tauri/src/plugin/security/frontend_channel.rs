//! 前端插件通道身份（frontend channel identity，审计票 06 / P0-5）
//!
//! Tauri 的 `#[tauri::command]` **拿不到调用者身份**：同一 webview 内宿主代码与全部插件前端
//! 代码同权，`plugin_*` 命令参数里的 `plugin_id` 因此是**自报值**——任一插件前端可直接
//! `invoke('plugin_storage_get', { pluginId: '受害者', key })` 读写他人存储。本模块给出一个
//! 可验证的身份面，形如「凭证 → 身份」的两级结构：
//!
//! - **loader 会话密钥**（宿主前端面凭证）：每次页面加载**首个调用者生效**、只签发一次。
//!   宿主前端 bootstrap 在**导入任何插件模块之前**取得并保存在模块作用域变量里（不进全局、
//!   不进 storage）；插件代码开始运行时（其模块已被导入）密钥已被占位 → 取不到。
//!   页面加载时由宿主重置（[`crate::plugin::manager::host::PluginHost::reset_frontend_loader_session`]，
//!   挂在 Tauri 的 `on_page_load` 钩子），使 dev 下页面刷新可重新取得。
//! - **插件通道令牌**（插件面凭证）：[`FrontendChannelRegistry::issue_token`] 校验 loader 密钥与
//!   插件激活态后签发，**停用即回收**；令牌决定身份，命令参数里的 `plugin_id` 只作「目标」，
//!   两者不符即拒绝。
//!
//! 凭据本身是 256 位随机值，用 `==` 比较（非恒定时间）：攻击者本地无比较预言机，每次探测要
//! 走一次 IPC 往返（毫秒级），逐字节时序差异不可利用。
//!
//! **诚实边界（不宣称硬隔离）**：同 realm 的原型/时序篡改理论上仍可尝试窃取凭据或他人
//! context——这正是本票裁决 2 删掉 `sandbox: 'isolated'` 虚假承诺的原因。本模块关闭的是
//! 「一行 `invoke` 自报 plugin_id」这条通道，不是「前端与被审代码同权」这个事实。

use crate::AppError;
use std::collections::HashMap;
use std::sync::Mutex;

/// 凭据熵（字节）：loader 会话密钥与插件令牌同规格
const CREDENTIAL_BYTES: usize = 32;

/// 一次前端页面加载内有效的凭证集合
#[derive(Default)]
pub struct FrontendChannelRegistry {
    /// 当前页面加载的 loader 会话密钥（`None` = 尚未签发，等待宿主前端 bootstrap 取用）
    loader_session: Mutex<Option<String>>,
    /// 插件通道令牌 → 插件 id（令牌决定身份）
    tokens: Mutex<HashMap<String, String>>,
    /// 插件 id → 令牌（覆盖签发时回收旧令牌用）
    plugin_tokens: Mutex<HashMap<String, String>>,
}

/// 凭证解析出的身份
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelIdentity {
    /// 宿主前端（loader 会话密钥）：可操作任意插件目标
    Host,
    /// 某插件前端（通道令牌）
    Plugin(String),
}

impl FrontendChannelRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// 重置为「新的一次页面加载」：旧 loader 密钥与全部插件令牌一律失效
    ///
    /// 返回被回收的插件令牌数（日志用）。页面加载钩子与前端显式重启各调用一次。
    pub fn reset(&self) -> usize {
        let mut session = self.loader_session.lock().unwrap_or_else(|e| e.into_inner());
        let had_session = session.take().is_some();
        let mut tokens = self.tokens.lock().unwrap_or_else(|e| e.into_inner());
        let revoked = tokens.len();
        tokens.clear();
        self.plugin_tokens.lock().unwrap_or_else(|e| e.into_inner()).clear();
        tracing::debug!(
            had_loader_session = had_session,
            revoked_tokens = revoked,
            "[PluginChannel] 前端通道会话已重置"
        );
        revoked
    }

    /// 签发 loader 会话密钥（宿主前端 bootstrap；**首个调用者生效**）
    pub fn issue_loader_session(&self) -> crate::Result<String> {
        let mut session = self.loader_session.lock().unwrap_or_else(|e| e.into_inner());
        if session.is_some() {
            // 不返回既有密钥：重复取用只可能是插件前端在事后尝试自取
            return Err(AppError::Plugin(
                "frontend loader session already issued for this page load".to_string(),
            ));
        }
        let credential = generate_credential();
        *session = Some(credential.clone());
        tracing::info!("[PluginChannel] 前端 loader 会话密钥已签发（宿主面凭证）");
        Ok(credential)
    }

    /// 校验 loader 会话密钥
    pub fn verify_loader_session(&self, candidate: &str) -> bool {
        let session = self.loader_session.lock().unwrap_or_else(|e| e.into_inner());
        matches!(session.as_deref(), Some(current) if current == candidate)
    }

    /// 为已激活插件签发通道令牌（覆盖旧令牌并回收）
    pub fn issue_token(&self, loader_session: &str, plugin_id: &str) -> crate::Result<String> {
        if !self.verify_loader_session(loader_session) {
            return Err(AppError::Plugin(
                "invalid frontend loader session credential".to_string(),
            ));
        }
        let token = generate_credential();
        {
            let mut plugin_tokens = self.plugin_tokens.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(old) = plugin_tokens.insert(plugin_id.to_string(), token.clone()) {
                self.tokens.lock().unwrap_or_else(|e| e.into_inner()).remove(&old);
            }
        }
        self.tokens
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(token.clone(), plugin_id.to_string());
        tracing::debug!(
            plugin_id = %plugin_id,
            "[PluginChannel] 插件前端通道令牌已签发"
        );
        Ok(token)
    }

    /// 回收某插件的通道令牌（停用 / 卸载）
    pub fn revoke_plugin(&self, plugin_id: &str) {
        let mut plugin_tokens = self.plugin_tokens.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(token) = plugin_tokens.remove(plugin_id) {
            self.tokens.lock().unwrap_or_else(|e| e.into_inner()).remove(&token);
            tracing::debug!(
                plugin_id = %plugin_id,
                "[PluginChannel] 插件前端通道令牌已回收"
            );
        }
    }

    /// 解析凭证：插件令牌优先，其次 loader 会话密钥；都命不中返回 `None`
    pub fn resolve(&self, credential: &str) -> Option<ChannelIdentity> {
        if credential.is_empty() {
            return None;
        }
        if let Some(plugin_id) = self
            .tokens
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(credential)
            .cloned()
        {
            return Some(ChannelIdentity::Plugin(plugin_id));
        }
        if self.verify_loader_session(credential) {
            return Some(ChannelIdentity::Host);
        }
        None
    }

    /// 插件面命令的身份裁决：`target_plugin_id` 是命令参数里的目标插件
    ///
    /// - 宿主面凭证（loader 会话密钥）→ 放行任意目标（宿主配置页 / 宿主 `pluginInvoke` 的职权）；
    /// - 插件面令牌 → 目标必须等于令牌身份，否则拒绝；
    /// - 凭证无效 → 拒绝（**fail-closed**，绝不落回「按参数 plugin_id 放行」）。
    pub fn authorize(&self, target_plugin_id: &str, credential: &str) -> crate::Result<()> {
        match self.resolve(credential) {
            Some(ChannelIdentity::Host) => Ok(()),
            Some(ChannelIdentity::Plugin(caller)) => {
                if caller == target_plugin_id {
                    Ok(())
                } else {
                    tracing::warn!(
                        plugin_id = %target_plugin_id,
                        caller = %caller,
                        "[PluginChannel] 插件前端跨插件调用被拒绝"
                    );
                    Err(AppError::Plugin(format!(
                        "Plugin '{}' may not act as plugin '{}'",
                        caller, target_plugin_id
                    )))
                }
            }
            None => {
                tracing::warn!(
                    plugin_id = %target_plugin_id,
                    "[PluginChannel] 缺少有效通道凭证，插件面命令被拒绝"
                );
                Err(AppError::Plugin(format!(
                    "Missing or invalid channel credential for plugin '{}'",
                    target_plugin_id
                )))
            }
        }
    }

}

/// 生成 32 字节随机凭据（小写十六进制）
fn generate_credential() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; CREDENTIAL_BYTES];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loader_session_issued_only_once_and_verifiable() {
        let registry = FrontendChannelRegistry::new();
        let session = registry.issue_loader_session().expect("first issue ok");

        assert!(registry.verify_loader_session(&session));
        assert!(!registry.verify_loader_session("forged"));

        // 第二个调用者拿不到密钥（插件前端事后自取的防线）
        let second = registry.issue_loader_session();
        assert!(second.is_err(), "重复取用必须拒绝，实际: {second:?}");
        assert!(registry.verify_loader_session(&session), "拒绝重复取用不得使既有密钥失效");
    }

    #[test]
    fn token_requires_loader_session_and_resolves_to_plugin() {
        let registry = FrontendChannelRegistry::new();
        let session = registry.issue_loader_session().unwrap();

        // 反例：伪造 / 空 loader 密钥一律拒绝
        assert!(registry.issue_token("forged", "com.test.a").is_err());
        assert!(registry.issue_token("", "com.test.a").is_err());

        let token = registry.issue_token(&session, "com.test.a").expect("issue ok");
        assert_eq!(registry.resolve(&token), Some(ChannelIdentity::Plugin("com.test.a".into())));
        assert_eq!(registry.resolve(&session), Some(ChannelIdentity::Host));
        assert_eq!(registry.resolve("forged"), None);
        assert_eq!(registry.resolve(""), None);
    }

    #[test]
    fn reissue_revokes_previous_token_and_deactivate_revokes_current() {
        let registry = FrontendChannelRegistry::new();
        let session = registry.issue_loader_session().unwrap();

        let first = registry.issue_token(&session, "com.test.a").unwrap();
        let second = registry.issue_token(&session, "com.test.a").unwrap();
        assert_ne!(first, second, "重新签发必须换新令牌");
        assert_eq!(registry.resolve(&first), None, "旧令牌必须失效");
        assert_eq!(registry.resolve(&second), Some(ChannelIdentity::Plugin("com.test.a".into())));

        registry.revoke_plugin("com.test.a");
        assert_eq!(registry.resolve(&second), None, "停用后令牌必须失效");
        // 只回收目标插件：宿主密钥与其它插件不受影响
        assert_eq!(registry.resolve(&session), Some(ChannelIdentity::Host));
        let other = registry.issue_token(&session, "com.test.b").unwrap();
        registry.revoke_plugin("com.test.a");
        assert_eq!(registry.resolve(&other), Some(ChannelIdentity::Plugin("com.test.b".into())));
    }

    #[test]
    fn reset_invalidates_session_and_all_tokens() {
        let registry = FrontendChannelRegistry::new();
        let session = registry.issue_loader_session().unwrap();
        let token = registry.issue_token(&session, "com.test.a").unwrap();

        let revoked = registry.reset();
        assert_eq!(revoked, 1, "重置必须报告回收的令牌数");
        assert!(!registry.verify_loader_session(&session), "页面加载后旧 loader 密钥失效");
        assert_eq!(registry.resolve(&token), None, "页面加载后插件令牌失效");
        // 新页面加载可重新签发（dev 刷新路径）
        let fresh = registry.issue_loader_session().expect("reissue after reset");
        assert_ne!(fresh, session);
    }

    /// 宿主面凭证可操作任意目标；插件令牌只能操作自己
    #[test]
    fn authorize_scopes_plugin_token_to_its_own_target() {
        let registry = FrontendChannelRegistry::new();
        let session = registry.issue_loader_session().unwrap();
        let alice = registry.issue_token(&session, "com.test.alice").unwrap();

        // 宿主面：任意目标放行（宿主配置页读写插件存储、宿主 pluginInvoke 的职权）
        assert!(registry.authorize("com.test.alice", &session).is_ok());
        assert!(registry.authorize("com.test.bob", &session).is_ok());

        // 插件面正例：自己的目标
        assert!(registry.authorize("com.test.alice", &alice).is_ok());

        // 反例：以他人身份行事 —— 必须拒绝且错误信息点明两侧身份
        let err = registry
            .authorize("com.test.bob", &alice)
            .expect_err("跨插件调用必须被拒绝");
        assert!(
            err.to_string().contains("com.test.alice") && err.to_string().contains("com.test.bob"),
            "错误信息必须点名调用者与目标，实际: {err}"
        );
    }

    /// 凭证缺失 / 伪造 / 已回收一律拒绝（fail-closed）
    #[test]
    fn authorize_rejects_missing_forged_and_revoked_credentials() {
        let registry = FrontendChannelRegistry::new();
        let session = registry.issue_loader_session().unwrap();
        let token = registry.issue_token(&session, "com.test.alice").unwrap();

        for credential in ["", "forged", "deadbeef"] {
            let err = registry
                .authorize("com.test.alice", credential)
                .expect_err("无有效凭证必须拒绝");
            assert!(
                err.to_string().contains("Missing or invalid channel credential"),
                "实际: {err}"
            );
        }

        registry.revoke_plugin("com.test.alice");
        assert!(
            registry.authorize("com.test.alice", &token).is_err(),
            "停用回收后的令牌不得再放行"
        );
        // 页面加载（前端重启）后同样失效，但宿主面在重置前仍有效
        assert!(registry.authorize("com.test.alice", &session).is_ok());
        registry.reset();
        assert!(registry.authorize("com.test.alice", &session).is_err());
    }

    #[test]
    fn credentials_are_unique_and_high_entropy() {
        let registry = FrontendChannelRegistry::new();
        let session = registry.issue_loader_session().unwrap();
        assert_eq!(session.len(), CREDENTIAL_BYTES * 2, "32 字节 → 64 位十六进制");

        let a = registry.issue_token(&session, "com.test.a").unwrap();
        let b = registry.issue_token(&session, "com.test.b").unwrap();
        assert_ne!(a, b);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
