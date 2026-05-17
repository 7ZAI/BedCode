//! Mobile Secure Storage Implementation
//!
//! 移动端安全存储实现 - 使用内存存储

use crate::Result;
use crate::shared::auth::storage::{SecureStorageTrait, CertificateStorageTrait, TokenStorageTrait};
use std::sync::RwLock;

/// 安全存储 - 移动端实现
/// 使用内存存储（应用关闭后丢失，适合临时 token 等场景）
pub struct SecureStorage {
    device_key: RwLock<Option<String>>,
    server_key: RwLock<Option<String>>,
}

impl SecureStorage {
    pub fn new() -> Result<Self> {
        Ok(Self {
            device_key: RwLock::new(None),
            server_key: RwLock::new(None),
        })
    }

    pub fn store_device_key(&self, key: &str) -> Result<()> {
        let mut guard = self.device_key.write().unwrap();
        *guard = Some(key.to_string());
        tracing::info!("Device key stored (mobile)");
        Ok(())
    }

    pub fn get_device_key(&self) -> Result<Option<String>> {
        let guard = self.device_key.read().unwrap();
        Ok(guard.clone())
    }

    pub fn delete_device_key(&self) -> Result<()> {
        let mut guard = self.device_key.write().unwrap();
        *guard = None;
        tracing::info!("Device key deleted (mobile)");
        Ok(())
    }

    pub fn store_server_key(&self, key: &str) -> Result<()> {
        let mut guard = self.server_key.write().unwrap();
        *guard = Some(key.to_string());
        tracing::info!("Server key stored (mobile)");
        Ok(())
    }

    pub fn get_server_key(&self) -> Result<Option<String>> {
        let guard = self.server_key.read().unwrap();
        Ok(guard.clone())
    }

    pub fn delete_server_key(&self) -> Result<()> {
        let mut guard = self.server_key.write().unwrap();
        *guard = None;
        tracing::info!("Server key deleted (mobile)");
        Ok(())
    }
}

impl SecureStorageTrait for SecureStorage {
    fn store_device_key(&self, key: &str) -> Result<()> {
        Self::store_device_key(self, key)
    }
    fn get_device_key(&self) -> Result<Option<String>> {
        Self::get_device_key(self)
    }
    fn delete_device_key(&self) -> Result<()> {
        Self::delete_device_key(self)
    }
    fn store_server_key(&self, key: &str) -> Result<()> {
        Self::store_server_key(self, key)
    }
    fn get_server_key(&self) -> Result<Option<String>> {
        Self::get_server_key(self)
    }
    fn delete_server_key(&self) -> Result<()> {
        Self::delete_server_key(self)
    }
}

impl Default for SecureStorage {
    fn default() -> Self {
        Self::new().expect("Failed to create secure storage")
    }
}

/// 证书存储 - 移动端实现
pub struct CertificateStorage {
    device_id: String,
    cert: RwLock<Option<String>>,
}

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

impl CertificateStorageTrait for CertificateStorage {
    fn store_certificate(&self, cert: &str) -> Result<()> {
        Self::store_certificate(self, cert)
    }
    fn get_certificate(&self) -> Result<Option<String>> {
        Self::get_certificate(self)
    }
    fn delete_certificate(&self) -> Result<()> {
        Self::delete_certificate(self)
    }
}

/// Token 存储 - 移动端实现
pub struct TokenStorage {
    token: RwLock<Option<String>>,
}

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

impl TokenStorageTrait for TokenStorage {
    fn store_token(&self, token: &str) -> Result<()> {
        Self::store_token(self, token)
    }
    fn get_token(&self) -> Result<Option<String>> {
        Self::get_token(self)
    }
    fn delete_token(&self) -> Result<()> {
        Self::delete_token(self)
    }
}