//! Secure Storage Traits
//!
//! 安全存储接口定义 - 跨平台共享

use crate::Result;

// ==================== Storage Trait Definitions ====================

/// 安全存储 trait - 设备私钥和服务端私钥的安全存储
pub trait SecureStorageTrait: Send + Sync {
    fn store_device_key(&self, key: &str) -> Result<()>;
    fn get_device_key(&self) -> Result<Option<String>>;
    fn delete_device_key(&self) -> Result<()>;

    fn store_server_key(&self, key: &str) -> Result<()>;
    fn get_server_key(&self) -> Result<Option<String>>;
    fn delete_server_key(&self) -> Result<()>;
}

/// 证书存储 trait
pub trait CertificateStorageTrait: Send + Sync {
    fn store_certificate(&self, cert: &str) -> Result<()>;
    fn get_certificate(&self) -> Result<Option<String>>;
    fn delete_certificate(&self) -> Result<()>;
}

/// Token 存储 trait
pub trait TokenStorageTrait: Send + Sync {
    fn store_token(&self, token: &str) -> Result<()>;
    fn get_token(&self) -> Result<Option<String>>;
    fn delete_token(&self) -> Result<()>;

    /// 存储设备 ID
    fn store_device_id(&self, device_id: &str) -> Result<()>;
    /// 获取设备 ID
    fn get_device_id(&self) -> Result<Option<String>>;
    /// 存储设备指纹
    fn store_device_fingerprint(&self, fingerprint: &str) -> Result<()>;
    /// 获取设备指纹
    fn get_device_fingerprint(&self) -> Result<Option<String>>;
}