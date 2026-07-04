//! QR Token Management
//!
//! One-time tokens for QR-based device pairing with configurable TTL.

use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

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
        let random_bytes: [u8; 16] = rand::random();
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

        tracing::debug!("QR token verify: input length={}, current_token present={}",
            input.len(), guard.is_some());

        match guard.as_mut() {
            None => Err(crate::AppError::Auth("No active QR token".to_string())),
            Some(token) => {
                tracing::debug!("Current token: length={}, used={}, expired={}, input_matches={}",
                    token.token.len(), token.used, token.is_expired(), token.token == input);

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
}
