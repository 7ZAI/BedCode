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
