//! Pairing Data Structures
//!
//! 配对基础数据结构 - 不包含业务逻辑

use chrono::{DateTime, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize, Serializer};
use std::time::{Duration, Instant};

use crate::system::constants::auth::PAIRING_CODE_DIGITS;

/// 配对码有效期（秒）
pub const PAIRING_CODE_TTL_SECS: u64 = 60;

/// 配对码（6 位数字）
#[derive(Debug, Clone)]
pub struct PairingCode {
    /// 配对码
    pub code: String,
    /// 创建时间
    pub created_at: DateTime<Utc>,
    /// 有效期（秒）- 原始 TTL
    pub expires_in: u64,
    /// 创建时间 (内部使用)
    pub created_instant: Option<Instant>,
}

impl PairingCode {
    /// 生成新的 6 位数字配对码
    pub fn generate() -> Self {
        let mut rng = rand::thread_rng();
        let code: String = (0..PAIRING_CODE_DIGITS).map(|_| rng.gen_range(0..10).to_string()).collect();

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

/// 序列化 PairingCode 时，expires_in 使用剩余时间
impl Serialize for PairingCode {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeStruct;

        let mut state = serializer.serialize_struct("PairingCode", 3)?;
        state.serialize_field("code", &self.code)?;
        state.serialize_field("created_at", &self.created_at)?;
        // 序列化时使用剩余时间，而不是原始 TTL
        state.serialize_field("expires_in", &self.remaining_seconds())?;
        state.end()
    }
}

/// 反序列化 PairingCode
impl<'de> Deserialize<'de> for PairingCode {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct PairingCodeData {
            code: String,
            created_at: DateTime<Utc>,
            expires_in: u64,
        }

        let data = PairingCodeData::deserialize(deserializer)?;
        Ok(PairingCode {
            code: data.code,
            created_at: data.created_at,
            expires_in: data.expires_in,
            // 反序列化时无法恢复 Instant，使用 created_at 作为 fallback
            created_instant: None,
        })
    }
}

impl Default for PairingCode {
    fn default() -> Self {
        Self::generate()
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

