//! Pairing Service
//!
//! 配对业务服务 - 包含设备配对的管理逻辑

use crate::system::constants::auth::PAIRING_CODE_TTL_SECS;
use crate::utils::auth::pairing::{PairingCode, PendingDevice};
use std::sync::Arc;
use tokio::sync::Mutex;

/// 配对服务 - 业务层实现
/// 负责配对码的生成、验证和待配对设备的管理
pub struct PairingService {
    /// 当前配对码
    current_code: Arc<Mutex<Option<PairingCode>>>,
    /// 待配对设备列表
    pending_devices: Arc<Mutex<Vec<PendingDevice>>>,
}

impl PairingService {
    /// 创建新的配对服务
    pub fn new() -> Self {
        Self {
            current_code: Arc::new(Mutex::new(None)),
            pending_devices: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// 生成新的配对码（使用默认 TTL）
    pub async fn generate_code(&self) -> PairingCode {
        self.generate_code_with_ttl(PAIRING_CODE_TTL_SECS).await
    }

    /// 生成新的配对码，指定有效期（秒）
    pub async fn generate_code_with_ttl(&self, ttl_secs: u64) -> PairingCode {
        let code = PairingCode::generate_with_ttl(ttl_secs);
        let mut current = self.current_code.lock().await;
        *current = Some(code.clone());

        tracing::info!("Generated pairing code: {} (TTL: {}s)", code.code, ttl_secs);
        code
    }

    /// 获取当前配对码
    pub async fn get_current_code(&self) -> Option<PairingCode> {
        let current = self.current_code.lock().await;
        current.as_ref().filter(|c| !c.is_expired()).cloned()
    }

    /// 验证配对码并消耗（单次使用）
    /// 验证成功后自动清除配对码，防止重复使用
    pub async fn verify_and_consume_code(&self, input: &str) -> bool {
        let mut current = self.current_code.lock().await;
        if let Some(code) = current.as_ref() {
            let valid = code.verify(input);
            if valid {
                tracing::info!("Pairing code verified and consumed");
                // 验证成功即消耗配对码
                *current = None;
            } else if code.is_expired() {
                tracing::warn!("Pairing code expired");
                // 过期也清除
                *current = None;
            } else {
                tracing::warn!("Invalid pairing code");
            }
            valid
        } else {
            tracing::warn!("No pairing code available");
            false
        }
    }

    /// 验证配对码（不消耗）
    #[deprecated(note = "Use verify_and_consume_code for single-use verification")]
    pub async fn verify_code(&self, input: &str) -> bool {
        let current = self.current_code.lock().await;
        if let Some(code) = current.as_ref() {
            let valid = code.verify(input);
            if valid {
                tracing::info!("Pairing code verified successfully");
            } else if code.is_expired() {
                tracing::warn!("Pairing code expired");
            } else {
                tracing::warn!("Invalid pairing code");
            }
            valid
        } else {
            tracing::warn!("No pairing code available");
            false
        }
    }

    /// 添加待配对设备
    pub async fn add_pending_device(&self, device: PendingDevice) {
        let mut pending = self.pending_devices.lock().await;
        pending.push(device);
    }

    /// 获取待配对设备列表
    pub async fn get_pending_devices(&self) -> Vec<PendingDevice> {
        let pending = self.pending_devices.lock().await;
        pending.clone()
    }

    /// 移除待配对设备
    pub async fn remove_pending_device(&self, device_id: &str) {
        let mut pending = self.pending_devices.lock().await;
        pending.retain(|d| d.device_id != device_id);
    }

    /// 清除当前配对码
    pub async fn clear_code(&self) {
        let mut current = self.current_code.lock().await;
        *current = None;
        tracing::info!("Pairing code cleared");
    }
}

impl Default for PairingService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 单次使用（票据 09）：同一 code 连续调用两次，第二次必须 false
    #[tokio::test]
    async fn verify_and_consume_code_is_single_use() {
        let svc = PairingService::new();
        let code = svc.generate_code().await;
        assert!(svc.verify_and_consume_code(&code.code).await, "首次验证应成功");
        assert!(
            !svc.verify_and_consume_code(&code.code).await,
            "验证成功后 code 已消耗，二次必须失败"
        );
    }

    /// 无配对码时返回 false
    #[tokio::test]
    async fn verify_without_code_returns_false() {
        let svc = PairingService::new();
        assert!(!svc.verify_and_consume_code("whatever").await);
    }

    /// 错误 code 不消耗（可重试正确 code）
    #[tokio::test]
    async fn wrong_code_does_not_consume() {
        let svc = PairingService::new();
        let code = svc.generate_code().await;
        assert!(!svc.verify_and_consume_code("wrong-code").await);
        // 错误尝试后正确 code 仍可用（未被误消耗）
        assert!(svc.verify_and_consume_code(&code.code).await);
    }

    /// 过期 code 被清除（票据 09）：get_current_code 返回 None
    #[tokio::test]
    async fn expired_code_cleared_and_unverifiable() {
        let svc = PairingService::new();
        // TTL=0 立即过期
        let code = svc.generate_code_with_ttl(0).await;
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        assert!(svc.get_current_code().await.is_none(), "过期 code 不应可获取");
        assert!(
            !svc.verify_and_consume_code(&code.code).await,
            "过期 code 验证必须失败"
        );
    }

    /// get_current_code 过滤过期、返回未过期
    #[tokio::test]
    async fn get_current_code_returns_fresh_only() {
        let svc = PairingService::new();
        let code = svc.generate_code_with_ttl(300).await;
        let current = svc.get_current_code().await.expect("未过期 code 应可获取");
        assert_eq!(current.code, code.code);
        // 未验证前不消耗
        assert!(svc.get_current_code().await.is_some());
    }
}
