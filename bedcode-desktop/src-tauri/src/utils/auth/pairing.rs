//! Pairing Data Structures
//!
//! 配对基础数据结构 - 不包含业务逻辑

use chrono::{DateTime, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize, Serializer};
use std::time::{Duration, Instant};

use crate::system::constants::auth::PAIRING_CODE_DIGITS;

/// 配对码有效期（秒）— re-export 供外部使用
pub use crate::system::constants::auth::PAIRING_CODE_TTL_SECS;

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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    /// 构造 fallback 场景（created_instant=None，模拟反序列化后的对象）
    fn fallback_code(created_at: DateTime<Utc>) -> PairingCode {
        PairingCode {
            code: "123456".to_string(),
            created_at,
            expires_in: PAIRING_CODE_TTL_SECS,
            created_instant: None,
        }
    }

    #[test]
    fn test_generate_creates_six_digit_code() {
        let code = PairingCode::generate();
        assert_eq!(code.code.len(), PAIRING_CODE_DIGITS);
        // 每一位都必须是数字（不能含字母或符号）
        assert!(code.code.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn test_generate_sets_created_at_expiry_and_instant() {
        let code = PairingCode::generate();
        // 创建时间应在当前时刻前后 5 秒内
        let now = Utc::now();
        assert!((now - code.created_at).num_seconds().abs() <= 5);
        assert_eq!(code.expires_in, PAIRING_CODE_TTL_SECS);
        // 内存态 Instant 必须存在（序列化前走快路径）
        assert!(code.created_instant.is_some());
    }

    #[test]
    fn test_fresh_code_not_expired() {
        let code = PairingCode::generate();
        assert!(!code.is_expired());
        assert!(code.remaining_seconds() > 0);
        assert!(code.remaining_seconds() <= PAIRING_CODE_TTL_SECS);
    }

    #[test]
    fn test_is_expired_fallback_with_old_created_at() {
        // fallback 分支：created_at 早于 TTL（60s）+ 60s 余量 → 必然过期
        let past = Utc::now() - chrono::Duration::seconds(120);
        assert!(fallback_code(past).is_expired());
    }

    #[test]
    fn test_is_expired_fallback_with_recent_created_at() {
        // fallback 分支：created_at 在 1 秒前，远未到 60s TTL
        let recent = Utc::now() - chrono::Duration::seconds(1);
        assert!(!fallback_code(recent).is_expired());
    }

    #[test]
    fn test_remaining_seconds_uses_backdated_instant() {
        // created_instant 回拨 10 秒：剩余 = 60 - 10.x 秒，as_secs 截断为 49
        let code = PairingCode {
            code: "123456".to_string(),
            created_at: Utc::now(),
            expires_in: PAIRING_CODE_TTL_SECS,
            created_instant: Some(Instant::now() - Duration::from_secs(10)),
        };
        assert_eq!(code.remaining_seconds(), 49);
    }

    #[test]
    fn test_remaining_seconds_fallback_old_is_zero() {
        // fallback 分支：过期后剩余时间必须钳制为 0，不能下溢
        let past = Utc::now() - chrono::Duration::seconds(120);
        assert_eq!(fallback_code(past).remaining_seconds(), 0);
    }

    #[test]
    fn test_remaining_seconds_fallback_recent() {
        // fallback 分支：5 秒前创建 → 剩余 60 - 5 = 55
        let recent = Utc::now() - chrono::Duration::seconds(5);
        assert_eq!(fallback_code(recent).remaining_seconds(), 55);
    }

    #[test]
    fn test_verify_accepts_correct_code() {
        let code = PairingCode::generate();
        assert!(code.verify(&code.code));
    }

    #[test]
    fn test_verify_rejects_wrong_code() {
        let code = PairingCode::generate();
        // 生成码与期望值无关，直接写一个不同的字面量
        let wrong = if code.code == "000000" { "111111" } else { "000000" };
        assert!(!code.verify(wrong));
    }

    #[test]
    fn test_verify_rejects_expired_code() {
        // fallback 构造过期场景：码正确但已过期 → 校验必须失败
        let past = Utc::now() - chrono::Duration::seconds(120);
        let code = fallback_code(past);
        assert!(!code.verify(&code.code));
    }

    #[test]
    fn test_serialize_uses_remaining_time_not_raw_ttl() {
        // created_instant 回拨 10 秒：序列化 expires_in 应为剩余时间（49），而非原始 TTL 60
        let code = PairingCode {
            code: "123456".to_string(),
            created_at: Utc::now(),
            expires_in: PAIRING_CODE_TTL_SECS,
            created_instant: Some(Instant::now() - Duration::from_secs(10)),
        };
        let json = serde_json::to_value(&code).unwrap();
        assert_eq!(json["code"], Value::String("123456".to_string()));
        assert_eq!(json["expires_in"], Value::from(49));
    }

    #[test]
    fn test_deserialize_roundtrip_clears_instant() {
        let code = PairingCode::generate();
        let json = serde_json::to_string(&code).unwrap();
        let decoded: PairingCode = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.code, code.code);
        assert_eq!(decoded.created_at, code.created_at);
        // 反序列化产物只能走 created_at fallback，内存态 Instant 不可恢复
        assert!(decoded.created_instant.is_none());
    }

    #[test]
    fn test_pending_device_roundtrip() {
        let device = PendingDevice {
            device_id: "dev-42".to_string(),
            device_name: "My Phone".to_string(),
            device_fingerprint: "fp-abc".to_string(),
            requested_at: Utc::now(),
        };
        let json = serde_json::to_string(&device).unwrap();
        let decoded: PendingDevice = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.device_id, "dev-42");
        assert_eq!(decoded.device_name, "My Phone");
        assert_eq!(decoded.device_fingerprint, "fp-abc");
        assert_eq!(decoded.requested_at, device.requested_at);
    }
}

