//! Secure Storage
//!
//! 使用系统密钥库安全存储密钥和证书

use crate::Result;

const SERVICE_NAME: &str = "bedcode";

// ==================== Desktop Implementation (keyring) ====================

#[cfg(not(any(target_os = "android", target_os = "ios")))]
use keyring::Entry;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub struct SecureStorage {
    /// 设备私钥条目
    device_key_entry: Entry,
    /// 服务端私钥条目
    server_key_entry: Entry,
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
impl SecureStorage {
    /// 创建新的安全存储
    pub fn new() -> Result<Self> {
        Ok(Self {
            device_key_entry: Entry::new(SERVICE_NAME, "device_private_key")?,
            server_key_entry: Entry::new(SERVICE_NAME, "server_private_key")?,
        })
    }

    /// 存储设备私钥
    pub fn store_device_key(&self, key: &str) -> Result<()> {
        self.device_key_entry.set_password(key)?;
        tracing::info!("Device key stored securely");
        Ok(())
    }

    /// 获取设备私钥
    pub fn get_device_key(&self) -> Result<Option<String>> {
        match self.device_key_entry.get_password() {
            Ok(key) => Ok(Some(key)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// 删除设备私钥
    pub fn delete_device_key(&self) -> Result<()> {
        match self.device_key_entry.delete_credential() {
            Ok(()) => {
                tracing::info!("Device key deleted");
                Ok(())
            }
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    /// 存储服务端私钥
    pub fn store_server_key(&self, key: &str) -> Result<()> {
        self.server_key_entry.set_password(key)?;
        tracing::info!("Server key stored securely");
        Ok(())
    }

    /// 获取服务端私钥
    pub fn get_server_key(&self) -> Result<Option<String>> {
        match self.server_key_entry.get_password() {
            Ok(key) => Ok(Some(key)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// 删除服务端私钥
    pub fn delete_server_key(&self) -> Result<()> {
        match self.server_key_entry.delete_credential() {
            Ok(()) => {
                tracing::info!("Server key deleted");
                Ok(())
            }
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
impl Default for SecureStorage {
    fn default() -> Self {
        Self::new().expect("Failed to create secure storage")
    }
}

// ==================== Mobile Implementation (in-memory/file-based) ====================

#[cfg(any(target_os = "android", target_os = "ios"))]
use std::sync::RwLock;

#[cfg(any(target_os = "android", target_os = "ios"))]
pub struct SecureStorage {
    /// 设备私钥
    device_key: RwLock<Option<String>>,
    /// 服务端私钥
    server_key: RwLock<Option<String>>,
}

#[cfg(any(target_os = "android", target_os = "ios"))]
impl SecureStorage {
    /// 创建新的安全存储
    pub fn new() -> Result<Self> {
        Ok(Self {
            device_key: RwLock::new(None),
            server_key: RwLock::new(None),
        })
    }

    /// 存储设备私钥
    pub fn store_device_key(&self, key: &str) -> Result<()> {
        let mut guard = self.device_key.write().unwrap();
        *guard = Some(key.to_string());
        tracing::info!("Device key stored (mobile)");
        Ok(())
    }

    /// 获取设备私钥
    pub fn get_device_key(&self) -> Result<Option<String>> {
        let guard = self.device_key.read().unwrap();
        Ok(guard.clone())
    }

    /// 删除设备私钥
    pub fn delete_device_key(&self) -> Result<()> {
        let mut guard = self.device_key.write().unwrap();
        *guard = None;
        tracing::info!("Device key deleted (mobile)");
        Ok(())
    }

    /// 存储服务端私钥
    pub fn store_server_key(&self, key: &str) -> Result<()> {
        let mut guard = self.server_key.write().unwrap();
        *guard = Some(key.to_string());
        tracing::info!("Server key stored (mobile)");
        Ok(())
    }

    /// 获取服务端私钥
    pub fn get_server_key(&self) -> Result<Option<String>> {
        let guard = self.server_key.read().unwrap();
        Ok(guard.clone())
    }

    /// 删除服务端私钥
    pub fn delete_server_key(&self) -> Result<()> {
        let mut guard = self.server_key.write().unwrap();
        *guard = None;
        tracing::info!("Server key deleted (mobile)");
        Ok(())
    }
}

#[cfg(any(target_os = "android", target_os = "ios"))]
impl Default for SecureStorage {
    fn default() -> Self {
        Self::new().expect("Failed to create secure storage")
    }
}

// ==================== Certificate Storage ====================

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub struct CertificateStorage {
    cert_entry: Entry,
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
impl CertificateStorage {
    pub fn for_device(device_id: &str) -> Result<Self> {
        Ok(Self {
            cert_entry: Entry::new(SERVICE_NAME, &format!("cert_{}", device_id))?,
        })
    }

    pub fn store_certificate(&self, cert: &str) -> Result<()> {
        self.cert_entry.set_password(cert)?;
        tracing::info!("Certificate stored for device");
        Ok(())
    }

    pub fn get_certificate(&self) -> Result<Option<String>> {
        match self.cert_entry.get_password() {
            Ok(cert) => Ok(Some(cert)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn delete_certificate(&self) -> Result<()> {
        match self.cert_entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(any(target_os = "android", target_os = "ios"))]
pub struct CertificateStorage {
    device_id: String,
    cert: RwLock<Option<String>>,
}

#[cfg(any(target_os = "android", target_os = "ios"))]
impl CertificateStorage {
    pub fn for_device(device_id: &str) -> Result<Self> {
        Ok(Self {
            device_id: device_id.to_string(),
            cert: RwLock::new(None),
        })
    }

    pub fn store_certificate(&self, cert: &str) -> Result<()> {
        let mut guard = self.cert.write().unwrap();
        *guard = Some(cert.to_string());
        tracing::info!("Certificate stored for device {} (mobile)", self.device_id);
        Ok(())
    }

    pub fn get_certificate(&self) -> Result<Option<String>> {
        let guard = self.cert.read().unwrap();
        Ok(guard.clone())
    }

    pub fn delete_certificate(&self) -> Result<()> {
        let mut guard = self.cert.write().unwrap();
        *guard = None;
        Ok(())
    }
}

// ==================== Token Storage ====================

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub struct TokenStorage {
    token_entry: Entry,
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
impl TokenStorage {
    pub fn new() -> Result<Self> {
        Ok(Self {
            token_entry: Entry::new(SERVICE_NAME, "session_token")?,
        })
    }

    pub fn store_token(&self, token: &str) -> Result<()> {
        self.token_entry.set_password(token)?;
        Ok(())
    }

    pub fn get_token(&self) -> Result<Option<String>> {
        match self.token_entry.get_password() {
            Ok(token) => Ok(Some(token)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn delete_token(&self) -> Result<()> {
        match self.token_entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(any(target_os = "android", target_os = "ios"))]
pub struct TokenStorage {
    token: RwLock<Option<String>>,
}

#[cfg(any(target_os = "android", target_os = "ios"))]
impl TokenStorage {
    pub fn new() -> Result<Self> {
        Ok(Self {
            token: RwLock::new(None),
        })
    }

    pub fn store_token(&self, token: &str) -> Result<()> {
        let mut guard = self.token.write().unwrap();
        *guard = Some(token.to_string());
        Ok(())
    }

    pub fn get_token(&self) -> Result<Option<String>> {
        let guard = self.token.read().unwrap();
        Ok(guard.clone())
    }

    pub fn delete_token(&self) -> Result<()> {
        let mut guard = self.token.write().unwrap();
        *guard = None;
        Ok(())
    }
}
