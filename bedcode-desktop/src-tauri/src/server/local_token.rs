//! Local WS Token Manager
//!
//! 为桌面端本地 WebSocket 通道（/ws/terminal/local）签发短期一次性令牌。
//! 环回 IP 校验之外的第二道防线：防止本机其他进程（恶意网页/脚本）
//! 连本地端口免认证订阅 PTY 输出。

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use uuid::Uuid;

/// 本地通道令牌管理器（单例）
pub struct LocalTokenManager {
    tokens: Mutex<HashMap<String, Instant>>,
}

impl LocalTokenManager {
    /// 令牌有效期：WebView 连接是即时动作，30s 足够且降低泄露窗口
    const TTL: Duration = Duration::from_secs(30);

    pub fn global() -> Arc<Self> {
        static INSTANCE: OnceLock<Arc<LocalTokenManager>> = OnceLock::new();
        INSTANCE
            .get_or_init(|| Arc::new(Self { tokens: Mutex::new(HashMap::new()) }))
            .clone()
    }

    /// 签发一次性令牌（128 位随机，TTL 30s）
    ///
    /// 签发时顺带清理过期令牌，防止长期运行内存增长
    pub fn issue(&self) -> String {
        let token = Uuid::new_v4().to_string();
        let now = Instant::now();
        let mut tokens = self.tokens.lock().expect("local token mutex poisoned");
        tokens.retain(|_, issued| now.duration_since(*issued) < Self::TTL);
        tokens.insert(token.clone(), now);
        token
    }

    /// 校验并消费令牌（一次性）：存在且未过期 → 移除并返回 true
    ///
    /// 消费语义：即使令牌被截获也只能使用一次
    pub fn verify_and_consume(&self, token: &str) -> bool {
        let now = Instant::now();
        let mut tokens = self.tokens.lock().expect("local token mutex poisoned");
        match tokens.remove(token) {
            Some(issued) if now.duration_since(issued) < Self::TTL => true,
            _ => false,
        }
    }
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    fn new_manager() -> LocalTokenManager {
        LocalTokenManager {
            tokens: Mutex::new(HashMap::new()),
        }
    }

    #[test]
    fn test_issue_and_verify_consumes_once() {
        let manager = new_manager();
        let token = manager.issue();
        assert!(!token.is_empty());

        // 首次校验消费成功
        assert!(manager.verify_and_consume(&token));
        // 二次消费失败（一次性）
        assert!(!manager.verify_and_consume(&token));
    }

    #[test]
    fn test_unknown_token_rejected() {
        let manager = new_manager();
        assert!(!manager.verify_and_consume("nonexistent-token"));
    }

    #[test]
    fn test_expired_token_rejected() {
        let manager = new_manager();
        let token = manager.issue();

        // 把令牌过期（测试直接改插入时间）
        {
            let mut tokens = manager.tokens.lock().unwrap();
            let entry = tokens.get_mut(&token).unwrap();
            *entry = Instant::now() - LocalTokenManager::TTL - Duration::from_secs(1);
        }

        assert!(!manager.verify_and_consume(&token));
    }

    #[test]
    fn test_issue_cleans_expired_tokens() {
        let manager = new_manager();
        let t1 = manager.issue();
        let t2 = manager.issue();

        // 手动让第一个令牌过期（t2 仍有效）
        {
            let mut tokens = manager.tokens.lock().unwrap();
            let expired = tokens.iter_mut().find(|(t, _)| **t != t2).unwrap();
            *expired.1 = Instant::now() - LocalTokenManager::TTL - Duration::from_secs(1);
        }

        // 再次签发触发清理：t1 被淘汰，有效 t2 + 新签 t3 保留
        let t3 = manager.issue();
        let tokens = manager.tokens.lock().unwrap();
        assert_eq!(tokens.len(), 2);
        assert!(!tokens.contains_key(&t1));
        assert!(tokens.contains_key(&t2));
        assert!(tokens.contains_key(&t3));
    }
}
