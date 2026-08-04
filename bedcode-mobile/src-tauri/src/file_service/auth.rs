//! Bearer Token 守卫（移动端文件服务鉴权，规格 4.5 节）
//!
//! 移动端文件服务是独立端口的 actix-web 服务，无法复用桌面的 JWT 中间件，
//! 只认宿主登记的随机 Bearer Token：
//! - 服务启动时生成（rand 32 字节 → base64url），**内存态不落盘**
//! - 经已认证的现有 WS 公告给桌面端（announce.rs）
//! - 配对解除/服务停止即 revoke 失效
//!
//! 安全要点：
//! - verify 使用恒定时间比较，防时序侧信道
//! - Debug/日志一律脱敏，token 本体不进任何日志字段

use base64::Engine;
use rand::RngCore;
use std::sync::RwLock;

/// Token 长度（字节，随机熵来源）
const TOKEN_RANDOM_BYTES: usize = 32;

/// Bearer Token 守卫（内存态，不落盘）
///
/// 单 token 模型：移动文件服务同时只服务配对的桌面端一个消费者，
/// 服务启停与 token 生命周期绑定（启动 generate、停止 revoke）
pub struct BearerTokenGuard {
    /// 当前有效 token（None = 服务未运行/已吊销）
    token: RwLock<Option<String>>,
}

impl BearerTokenGuard {
    /// 创建空守卫（服务启动时调用 [`generate`](Self::generate)）
    pub fn new() -> Self {
        Self {
            token: RwLock::new(None),
        }
    }

    /// 生成新 token（服务启动时调用）并返回
    ///
    /// 32 字节 OS 随机数 → base64url（无填充），仅内存保存
    pub fn generate(&self) -> String {
        let mut bytes = [0u8; TOKEN_RANDOM_BYTES];
        rand::rngs::OsRng.fill_bytes(&mut bytes);
        let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);

        let mut guard = self.token.write().unwrap_or_else(|e| e.into_inner());
        *guard = Some(token.clone());
        // 只记录长度，token 本体不落日志（脱敏）
        tracing::info!("file service bearer token generated (len={})", token.len());
        token
    }

    /// 校验请求携带的 token 是否与当前登记一致（恒定时间比较）
    ///
    /// 未生成/已吊销时一律拒绝
    pub fn verify(&self, presented: &str) -> bool {
        let guard = self.token.read().unwrap_or_else(|e| e.into_inner());
        match guard.as_deref() {
            Some(expected) => constant_time_eq(expected.as_bytes(), presented.as_bytes()),
            None => false,
        }
    }

    /// 吊销 token（服务停止/解配时调用），之后 verify 一律 false
    pub fn revoke(&self) {
        let mut guard = self.token.write().unwrap_or_else(|e| e.into_inner());
        if guard.take().is_some() {
            tracing::info!("file service bearer token revoked");
        }
    }

    /// 是否存在有效 token（服务运行中）
    pub fn is_active(&self) -> bool {
        self.token
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
    }

    /// 取当前 token 副本（仅供 WS 控制面 Announce 使用，经已认证通道发送）
    ///
    /// 除公告外任何代码不得调用；日志永远不输出返回值
    pub fn current_for_announce(&self) -> Option<String> {
        self.token.read().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl Default for BearerTokenGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for BearerTokenGuard {
    /// 脱敏：只暴露是否有有效 token，绝不输出 token 本体
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BearerTokenGuard")
            .field("active", &self.is_active())
            .finish()
    }
}

/// 恒定时间字节串比较（防时序侧信道）
///
/// 长度不等直接 false（长度本身非机密）；等长时逐字节异或累积，
/// 比较路径不随内容提前退出
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_verify_revoke_lifecycle() {
        let guard = BearerTokenGuard::new();
        assert!(!guard.is_active());
        assert!(!guard.verify("anything"));

        let token = guard.generate();
        assert!(guard.is_active());
        assert!(guard.verify(&token));
        assert!(!guard.verify("wrong-token"));
        assert!(!guard.verify(""));

        guard.revoke();
        assert!(!guard.is_active());
        assert!(!guard.verify(&token));
    }

    #[test]
    fn test_generate_produces_distinct_tokens() {
        let guard = BearerTokenGuard::new();
        let t1 = guard.generate();
        let t2 = guard.generate();
        assert_ne!(t1, t2);
        // 32 字节 base64url 无填充 = 43 字符
        assert_eq!(t1.len(), 43);
        // 旧 token 被新 token 顶替后立即失效
        assert!(!guard.verify(&t1));
        assert!(guard.verify(&t2));
    }

    #[test]
    fn test_constant_time_eq() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
        assert!(constant_time_eq(b"", b""));
    }

    #[test]
    fn test_debug_is_redacted() {
        let guard = BearerTokenGuard::new();
        let token = guard.generate();
        let debug = format!("{:?}", guard);
        assert!(!debug.contains(&token), "Debug output must not leak token");
    }
}
