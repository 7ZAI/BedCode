//! System Information
//!
//! 全局系统基本信息（OS / 设备名称 / IP 地址），启动时采集一次，
//! 挂载到 state.rs 全局单例供引用（state::get_system_info()）

use serde::Serialize;

/// 系统基本信息
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfo {
    /// OS 名称（std::env::consts::OS：android / windows / linux / macos）
    pub os_name: String,
    /// OS 版本（Android 为 Build.VERSION.RELEASE，如 "13"），获取失败为空串
    pub os_version: String,
    /// 用户设置的设备名称（Android 读系统设置设备名，如 "xiaomi k30"）
    pub device_name: String,
    /// 主机名
    pub hostname: String,
    /// 本地 IPv4 地址（排除回环与链路本地）
    pub local_ips: Vec<String>,
    /// 应用版本
    pub app_version: String,
}

impl SystemInfo {
    /// 采集当前系统信息（需在 Tokio 运行时内调用，Android 经插件异步获取）
    pub async fn collect() -> Self {
        let android_info = crate::plugin::android_plugins::get_android_device_info().await;

        let hostname = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_default();

        Self {
            os_name: std::env::consts::OS.to_string(),
            os_version: android_info
                .as_ref()
                .map(|i| i.os_version.clone())
                .unwrap_or_default(),
            // Android 平台 hostname 恒为 "localhost"，设备名插件不可用时直接
            // 走 `{os}-{ip}` 组合名，避免所有设备同名（多设备无区分度）
            device_name: device_name(
                android_info.as_ref(),
                if cfg!(target_os = "android") && android_info.is_none() {
                    ""
                } else {
                    &hostname
                },
            ),
            hostname,
            local_ips: local_ip_addresses(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    /// 采集未完成时的兜底实例（前端早于采集 invoke 时不 panic）
    pub fn fallback() -> Self {
        Self {
            os_name: std::env::consts::OS.to_string(),
            os_version: String::new(),
            device_name: crate::system::constants::auth::DEFAULT_DEVICE_NAME.to_string(),
            hostname: String::new(),
            local_ips: Vec::new(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

/// 用户设置的设备名称
///
/// Android 读系统设置中的设备名（Settings.Global device_name，用户可在
/// 设置中修改，如小米手机 "xiaomi k30"），取不到时依次回退：机型
/// （Build.MODEL）→ `{os}-{ip}` 组合名；非 Android 平台取 hostname，
/// 同样以 `{os}-{ip}` 兜底
fn device_name(android_info: Option<&crate::plugin::android_plugins::AndroidDeviceInfo>, hostname: &str) -> String {
    let fallback = || {
        let ips = local_ip_addresses();
        match ips.first() {
            Some(ip) => format!("{}-{}", std::env::consts::OS, ip),
            None => crate::system::constants::auth::DEFAULT_DEVICE_NAME.to_string(),
        }
    };
    if let Some(info) = android_info {
        let name = info.device_name.trim();
        if !name.is_empty() {
            return name.to_string();
        }
        let model = info.model.trim();
        if !model.is_empty() {
            return model.to_string();
        }
        return fallback();
    }
    if !hostname.is_empty() {
        return hostname.to_string();
    }
    fallback()
}

/// 本地 IPv4 地址（排除回环与链路本地，与桌面端 get_local_ip_addresses 逻辑一致）
fn local_ip_addresses() -> Vec<String> {
    local_ip_address::list_afinet_netifas()
        .map(|interfaces| {
            interfaces
                .into_iter()
                .filter(|(_, ip)| match ip {
                    std::net::IpAddr::V4(ipv4) => !ipv4.is_loopback() && !ipv4.is_link_local(),
                    std::net::IpAddr::V6(_) => false,
                })
                .map(|(_, ip)| ip.to_string())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::android_plugins::AndroidDeviceInfo;
    use crate::system::constants::auth::DEFAULT_DEVICE_NAME;

    #[test]
    fn test_local_ip_addresses() {
        // 无网环境可为空列表，但不应 panic
        let _ = local_ip_addresses();
    }

    #[test]
    fn test_device_name_fallback() {
        // 非 Android（无插件信息）回退 hostname
        let name = device_name(None, "my-host");
        assert_eq!(name, "my-host");
        // 非 Android 且无 hostname：回退 os-ip 组合名或默认值
        let name = device_name(None, "");
        if local_ip_addresses().is_empty() {
            assert_eq!(name, DEFAULT_DEVICE_NAME);
        } else {
            assert!(name.starts_with(&format!("{}-", std::env::consts::OS)));
        }
        // Android 插件信息：优先用户设备名，其次机型
        let info = AndroidDeviceInfo {
            device_name: "xiaomi k30".to_string(),
            model: "Redmi K30".to_string(),
            manufacturer: "Xiaomi".to_string(),
            os_version: "13".to_string(),
            sdk_int: 33,
        };
        assert_eq!(device_name(Some(&info), "host"), "xiaomi k30");
        let mut info_empty = info.clone();
        info_empty.device_name = "  ".to_string();
        assert_eq!(device_name(Some(&info_empty), "host"), "Redmi K30");
        // 用户设备名与机型都拿不到：os-ip 兜底
        let mut info_none = info.clone();
        info_none.device_name = "".to_string();
        info_none.model = "".to_string();
        let name = device_name(Some(&info_none), "host");
        if local_ip_addresses().is_empty() {
            assert_eq!(name, DEFAULT_DEVICE_NAME);
        } else {
            assert!(name.starts_with(&format!("{}-", std::env::consts::OS)));
        }
    }

    #[test]
    fn test_collect_non_android() {
        // cargo test 运行在宿主（非 Android），走 hostname 回退路径
        let info = tokio::runtime::Runtime::new()
            .expect("create runtime")
            .block_on(SystemInfo::collect());
        assert!(!info.os_name.is_empty());
        assert!(!info.app_version.is_empty());
        assert_eq!(info.app_version, env!("CARGO_PKG_VERSION"));
        // 非 Android 平台设备名 = hostname（或默认值），不为空
        assert!(!info.device_name.is_empty());
    }
}
