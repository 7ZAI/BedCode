//! QR Token Service
//!
//! QR 码令牌业务服务 - 负责 QR 配对的业务流程

use std::sync::Arc;
use tokio::sync::Mutex;
use crate::shared::auth::qr_token::QrToken;

/// QR Token 管理服务 - 业务层实现
/// 负责 QR token 的生成、验证和生命周期管理
pub struct QrTokenService {
    /// 当前活跃的 QR token
    current_token: Arc<Mutex<Option<QrToken>>>,
}

impl QrTokenService {
    /// 创建新的 QR token 服务
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
    /// 验证通过后标记为已使用（一次性）
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
                    token.used = true;
                    tracing::info!("QR token verified successfully");
                    Ok(())
                }
            }
        }
    }

    /// 获取当前活跃 token 信息
    pub async fn get_active(&self) -> Option<(String, u64, u64)> {
        let guard = self.current_token.lock().await;
        guard.as_ref().and_then(|token| {
            if token.is_expired() {
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

impl Default for QrTokenService {
    fn default() -> Self {
        Self::new()
    }
}