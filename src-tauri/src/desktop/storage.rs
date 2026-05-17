//! Desktop Secure Storage Implementation
//!
//! 使用系统密钥库 (keyring) 安全存储密钥和证书

use crate::Result;
use crate::shared::auth::storage::{SecureStorageTrait, CertificateStorageTrait, TokenStorageTrait};

const SERVICE_NAME: &str = "bedcode";

use keyring::Entry;

/// 安全存储 - 桌面端实现
/// 使用系统密钥库存储私钥
pub struct SecureStorage {
    device_key_entry: Entry,
    server_key_entry: Entry,
}

impl SecureStorage {
    pub fn new() -> Result<Self> {
        Ok(Self {
            device_key_entry: Entry::new(SERVICE_NAME, "device_private_key")?,
            server_key_entry: Entry::new(SERVICE_NAME, "server_private_key")?,
        })
    }

    pub fn store_device_key(&self, key: &str) -> Result<()> {
        self.device_key_entry.set_password(key)?;
        tracing::info!("Device key stored securely");
        Ok(())
    }

    pub fn get_device_key(&self) -> Result<Option<String>> {
        match self.device_key_entry.get_password() {
            Ok(key) => Ok(Some(key)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

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

    pub fn store_server_key(&self, key: &str) -> Result<()> {
        self.server_key_entry.set_password(key)?;
        tracing::info!("Server key stored securely");
        Ok(())
    }

    pub fn get_server_key(&self) -> Result<Option<String>> {
        match self.server_key_entry.get_password() {
            Ok(key) => Ok(Some(key)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

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

/// 证书存储 - 桌面端实现
pub struct CertificateStorage {
    cert_entry: Entry,
}

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

/// Token 存储 - 桌面端实现
pub struct TokenStorage {
    token_entry: Entry,
}

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