//! mDNS 广播相关常量

/// mDNS 服务实例名前缀
///
/// 实例名格式为 "BedCode-{hostname}"
pub const SERVICE_NAME_PREFIX: &str = "BedCode-";

/// TXT Record key：平台标识
pub const TXT_KEY_PLATFORM: &str = "platform";

/// TXT Record value：桌面端平台
pub const TXT_VALUE_PLATFORM: &str = "desktop";

/// TXT Record key：设备名称
pub const TXT_KEY_DEVICE_NAME: &str = "device_name";

/// TXT Record key：应用版本
pub const TXT_KEY_VERSION: &str = "version";

/// 无法获取主机名时的默认值
pub const DEFAULT_HOSTNAME: &str = "Desktop";
