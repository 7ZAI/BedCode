//! Device Model - Paired device and authentication

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Paired device record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pairing {
    pub id: String,
    pub device_name: String,
    pub device_fingerprint: String,
    pub public_key: String,
    pub address: Option<String>,
    pub session_token: Option<String>,
    pub paired_at: DateTime<Utc>,
    pub last_seen: Option<DateTime<Utc>>,
    pub is_active: bool,
}

/// JWT claims for device authentication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtClaims {
    pub sub: String,
    pub iss: String,
    pub iat: u64,
    pub exp: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
}

impl JwtClaims {
    pub fn new(
        subject: String,
        device_name: Option<String>,
        fingerprint: Option<String>,
        expires_in_secs: u64,
    ) -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            sub: subject,
            iss: "BedCode".to_string(),
            iat: now,
            exp: now + expires_in_secs,
            device_name,
            fingerprint,
        }
    }

    pub fn is_expired(&self) -> bool {
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.exp < now
    }

    pub fn remaining_secs(&self) -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        if self.exp > now {
            self.exp - now
        } else {
            0
        }
    }
}