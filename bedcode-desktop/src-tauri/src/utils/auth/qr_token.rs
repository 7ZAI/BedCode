//! QR Token Management
//!
//! One-time tokens for QR-based device pairing with configurable TTL.

use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

use crate::system::constants::auth::QR_TOKEN_BYTES;

/// 128-bit random hex token for QR code connection
#[derive(Debug, Clone)]
pub struct QrToken {
    pub token: String,
    pub created_at: Instant,
    pub ttl_secs: u64,
    pub used: bool,
}

impl QrToken {
    pub fn new(ttl_secs: u64) -> Self {
        let random_bytes: [u8; QR_TOKEN_BYTES] = rand::random();
        let token = hex::encode(random_bytes);

        Self {
            token,
            created_at: Instant::now(),
            ttl_secs,
            used: false,
        }
    }

    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed().as_secs() >= self.ttl_secs
    }
}

#[derive(Debug)]
pub struct QrTokenManager {
    current_token: Arc<Mutex<Option<QrToken>>>,
}

impl Default for QrTokenManager {
    fn default() -> Self {
        Self::new()
    }
}

impl QrTokenManager {
    pub fn new() -> Self {
        Self {
            current_token: Arc::new(Mutex::new(None)),
        }
    }

    /// 生成新的 QR token，替换旧的
    pub async fn generate(&self, ttl_secs: u64) -> String {
        let token = QrToken::new(ttl_secs);
        let token_str = token.token.clone();
        *self.current_token.lock().await = Some(token);
        token_str
    }

    /// 验证 token：存在、未过期、未使用
    /// 验证通过后清除 token（一次性，需重新生成）
    pub async fn verify(&self, input: &str) -> crate::Result<()> {
        let mut guard = self.current_token.lock().await;

        tracing::debug!(
            "QR token verify: input length={}, current_token present={}",
            input.len(),
            guard.is_some()
        );

        match guard.as_mut() {
            None => Err(crate::AppError::Auth("No active QR token".to_string())),
            Some(token) => {
                tracing::debug!(
                    "Current token: length={}, used={}, expired={}, input_matches={}",
                    token.token.len(),
                    token.used,
                    token.is_expired(),
                    token.token == input
                );

                if token.is_expired() {
                    *guard = None;
                    Err(crate::AppError::Auth("QR token expired".to_string()))
                } else if token.used {
                    Err(crate::AppError::Auth("QR token already used".to_string()))
                } else if token.token != input {
                    Err(crate::AppError::Auth("Invalid QR token".to_string()))
                } else {
                    // 消费 token：清除而非仅标记 used
                    // 前端需收到事件后重新生成新二维码
                    *guard = None;
                    tracing::info!("QR token consumed and cleared");
                    Ok(())
                }
            }
        }
    }

    /// 获取当前活跃 token 信息（排除已过期和已使用的）
    pub async fn get_active(&self) -> Option<(String, u64, u64)> {
        let guard = self.current_token.lock().await;
        guard.as_ref().and_then(|token| {
            if token.is_expired() || token.used {
                None
            } else {
                let elapsed = token.created_at.elapsed().as_secs();
                let remaining = token.ttl_secs.saturating_sub(elapsed);
                Some((token.token.clone(), token.ttl_secs, remaining))
            }
        })
    }

    /// 清除当前 token
    pub async fn clear(&self) {
        *self.current_token.lock().await = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_qr_token_generate_and_verify() {
        let manager = QrTokenManager::new();
        let token = manager.generate(300).await;
        assert_eq!(token.len(), 32);

        assert!(manager.verify(&token).await.is_ok());

        // 验证后 token 被消耗，get_active 返回 None
        assert!(manager.get_active().await.is_none());

        // 重复使用应失败（token 已被清除）
        assert!(manager.verify(&token).await.is_err());
    }

    #[tokio::test]
    async fn test_qr_token_invalid_token() {
        let manager = QrTokenManager::new();
        manager.generate(300).await;
        assert!(manager.verify("invalid_token_32_chars_here!!!").await.is_err());
    }

    #[tokio::test]
    async fn test_qr_token_clear() {
        let manager = QrTokenManager::new();
        manager.generate(300).await;
        manager.clear().await;
        assert!(manager.get_active().await.is_none());
    }

    #[tokio::test]
    async fn test_qr_token_expired() {
        let manager = QrTokenManager::new();
        let token = manager.generate(0).await; // TTL=0, immediately expired
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        assert!(manager.verify(&token).await.is_err());
    }

    /// 并发单次使用（票据 29）：两个并发 verify 只允许一个成功
    #[tokio::test]
    async fn test_qr_token_concurrent_single_use() {
        let manager = Arc::new(QrTokenManager::new());
        let token = manager.generate(300).await;

        let m1 = Arc::clone(&manager);
        let t1 = token.clone();
        let h1 = tokio::spawn(async move { m1.verify(&t1).await.is_ok() });
        let m2 = Arc::clone(&manager);
        let t2 = token.clone();
        let h2 = tokio::spawn(async move { m2.verify(&t2).await.is_ok() });

        let (r1, r2) = tokio::join!(h1, h2);
        let ok_count = [r1.unwrap(), r2.unwrap()].iter().filter(|&&b| b).count();
        assert_eq!(ok_count, 1, "并发 verify 必须恰好一个成功（单次使用语义）");
    }

    /// TTL 边界（票据 29）：`elapsed == ttl` 视为过期（is_expired 用 `>=`）
    #[test]
    fn test_qr_token_ttl_boundary_elapsed_equals_ttl() {
        let token = QrToken::new(1);
        // 模拟 1 秒后：elapsed >= ttl → 过期。真实等待 1.1s 验证边界语义
        std::thread::sleep(std::time::Duration::from_millis(1100));
        assert!(token.is_expired(), "elapsed == ttl 应视为过期（>= 语义）");
    }

    /// get_active 对已过期 token 返回 None（票据 29：当前只测 verify 路径的过期）
    #[tokio::test]
    async fn test_qr_token_get_active_returns_none_when_expired() {
        let manager = QrTokenManager::new();
        manager.generate(0).await; // 立即过期
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        assert!(manager.get_active().await.is_none(), "过期 token 不应出现在活跃列表");
    }

    /// get_active 返回剩余秒数（正值，未过期时）
    #[tokio::test]
    async fn test_qr_token_get_active_returns_remaining() {
        let manager = QrTokenManager::new();
        manager.generate(300).await;
        let active = manager.get_active().await.expect("未过期 token 应活跃");
        assert_eq!(active.0.len(), 32);
        assert_eq!(active.1, 300);
        assert!(active.2 <= 300, "剩余秒数应 ≤ TTL");
    }
}
