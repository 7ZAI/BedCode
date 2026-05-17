//! Pairing Service
//!
//! 配对业务服务 - 包含设备配对的管理逻辑

use std::sync::Arc;
use tokio::sync::Mutex;
use crate::shared::auth::pairing::{PairingCode, PendingDevice};

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

    /// 生成新的配对码
    pub async fn generate_code(&self) -> PairingCode {
        let code = PairingCode::generate();
        let mut current = self.current_code.lock().await;
        *current = Some(code.clone());

        tracing::info!("Generated pairing code: {}", code.code);
        code
    }

    /// 获取当前配对码
    pub async fn get_current_code(&self) -> Option<PairingCode> {
        let current = self.current_code.lock().await;
        current.as_ref().filter(|c| !c.is_expired()).cloned()
    }

    /// 验证配对码
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