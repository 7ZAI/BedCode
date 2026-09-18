//! mDNS 共享类型定义

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// mDNS 服务类型
pub const SERVICE_TYPE: &str = "_bedcode._tcp.local.";

/// mDNS 广播配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvertiseConfig {
    /// 实例名（如 "BedCode-DESKTOP-X1"）
    pub service_name: String,
    /// 服务端口
    pub port: u16,
    /// TXT 记录键值对
    pub txt_records: HashMap<String, String>,
}

impl AdvertiseConfig {
    /// 输入校验（§8 输入校验在 Rust 端）：空服务名 / 端口 0 / 实例名超长均拒绝
    pub fn validate(&self) -> crate::Result<()> {
        if self.service_name.trim().is_empty() {
            return Err(crate::AppError::InvalidInput("mDNS 服务名不能为空".to_string()));
        }
        if self.port == 0 {
            return Err(crate::AppError::InvalidInput("mDNS 服务端口不能为 0".to_string()));
        }
        // RFC 6763：实例名 ≤ 63 字节（单个 label），此处按宽松上限防异常输入
        if self.service_name.len() > 255 {
            return Err(crate::AppError::InvalidInput(format!(
                "mDNS 实例名超过 255 字节: {}",
                self.service_name.len()
            )));
        }
        Ok(())
    }
}
