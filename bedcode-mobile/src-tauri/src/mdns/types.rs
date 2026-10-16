//! mDNS 共享类型定义

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// mDNS 服务类型
pub const SERVICE_TYPE: &str = "_bedcode._tcp.local.";

/// mDNS 发现到的服务信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredService {
    /// 实例名，如 "BedCode-DESKTOP-X1"
    pub instance_name: String,
    /// 主机名，如 "DESKTOP-X1.local."
    pub host_name: String,
    /// 解析后的 IP 地址
    pub address: String,
    /// 服务端口
    pub port: u16,
    /// TXT 记录键值对
    pub txt_records: HashMap<String, String>,
    /// 子类型（如 platform=desktop）
    pub platform: String,
    /// 用户可读的设备名（from device_name TXT record）
    pub device_name: String,
}
