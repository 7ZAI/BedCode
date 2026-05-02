//! Pairing mechanism
//!
//! 提供设备配对码生成、验证和管理功能

use chrono::{DateTime, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// 配对码有效期（秒）
pub const PAIRING_CODE_TTL_SECS: u64 = 60;

/// 配对码（6 位数字）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingCode {
    /// 配对码
    pub code: String,
    /// 创建时间 (for serialization)
    pub created_at: DateTime<Utc>,
    /// 有效期（秒）
    pub expires_in: u64,
    /// 创建时间 (内部使用，不序列化)
    #[serde(skip)]
    pub created_instant: Option<Instant>,
}

impl PairingCode {
    /// 生成新的 6 位数字配对码
    pub fn generate() -> Self {
        let mut rng = rand::thread_rng();
        let code: String = (0..6).map(|_| rng.gen_range(0..10).to_string()).collect();

        Self {
            code,
            created_at: Utc::now(),
            expires_in: PAIRING_CODE_TTL_SECS,
            created_instant: Some(Instant::now()),
        }
    }

    /// 检查是否过期
    pub fn is_expired(&self) -> bool {
        if let Some(instant) = self.created_instant {
            instant.elapsed() > Duration::from_secs(self.expires_in)
        } else {
            // Fallback: use created_at time
            let now = Utc::now();
            let elapsed = now - self.created_at;
            elapsed.num_seconds() as u64 > self.expires_in
        }
    }

    /// 获取剩余有效时间（秒）
    pub fn remaining_seconds(&self) -> u64 {
        if let Some(instant) = self.created_instant {
            let elapsed = instant.elapsed();
            if elapsed >= Duration::from_secs(self.expires_in) {
                0
            } else {
                (Duration::from_secs(self.expires_in) - elapsed).as_secs()
            }
        } else {
            // Fallback calculation
            let now = Utc::now();
            let elapsed_secs = (now - self.created_at).num_seconds() as u64;
            if elapsed_secs >= self.expires_in {
                0
            } else {
                self.expires_in - elapsed_secs
            }
        }
    }

    /// 验证配对码
    pub fn verify(&self, input: &str) -> bool {
        !self.is_expired() && self.code == input
    }
}

/// 待配对设备信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingDevice {
    /// 设备 ID
    pub device_id: String,
    /// 设备名称
    pub device_name: String,
    /// 设备指纹
    pub device_fingerprint: String,
    /// 请求时间
    pub requested_at: chrono::DateTime<chrono::Utc>,
}

/// 设备指纹
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceFingerprint {
    /// 设备 ID
    pub device_id: String,
    /// 设备名称
    pub device_name: String,
    /// 平台
    pub platform: String,
    /// 时间戳
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl DeviceFingerprint {
    /// 创建新的设备指纹
    pub fn new(device_name: String, platform: String) -> Self {
        Self {
            device_id: uuid::Uuid::new_v4().to_string(),
            device_name,
            platform,
            timestamp: chrono::Utc::now(),
        }
    }
}

/// 配对服务
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

impl Default for crate::auth::PairingCode {
    fn default() -> Self {
        Self::generate()
    }
}
