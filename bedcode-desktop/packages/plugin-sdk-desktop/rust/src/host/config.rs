//! 宿主能力：配置读取（白名单即枚举）

use super::HostError;

/// 可读宿主配置项（白名单）
///
/// 白名单 = 本枚举本身：宿主侧 match 穷尽所有变体，
/// 结构性杜绝"白名单声明了但实现缺失"的漂移。
/// 新增配置项：加变体 + 宿主 match 补实现，编译器强制两端同步。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConfigKey {
    /// 服务器实际运行端口（端口冲突时返回重新分配后的值）
    NetworkPort,
    /// 用户主目录绝对路径
    HomeDir,
    /// 宿主当前 Unix 毫秒时间戳
    ///
    /// wasm32-unknown-unknown 无系统时钟（`SystemTime::now()`/`Instant::now()`
    /// 均 panic），需要真实时间的插件一律经此获取，禁止直接调 std 时间 API。
    /// 不可用时返回 `Ok(None)`，调用方降级（如用 0/计数器）。
    CurrentTimeMs,
    /// 宿主操作系统平台（std::env::consts::OS 值：windows / linux / macos / …）
    ///
    /// wasm32-unknown-unknown 无法感知宿主 OS，需要按平台选择命令包装
    /// （如 inline 命令 sh -c vs cmd /C）的插件经此获取。
    OsPlatform,
    /// 配对码有效期（秒，十进制字符串；v19 追加，票 14）
    ///
    /// 真源是宿主 `settings` 表的 `pairing_code_ttl` 键；写入走 `host-auth`
    /// `auth-setting-set`（键白名单同两键），本枚举只提供读取通道。
    PairingCodeTtl,
    /// 二维码（QR token）有效期（秒，十进制字符串；v19 追加，票 14）
    QrTokenTtl,
}

impl ConfigKey {
    /// 全部合法配置项（宿主白名单校验用）
    pub const ALL: &'static [ConfigKey] = &[
        ConfigKey::NetworkPort,
        ConfigKey::HomeDir,
        ConfigKey::CurrentTimeMs,
        ConfigKey::OsPlatform,
        ConfigKey::PairingCodeTtl,
        ConfigKey::QrTokenTtl,
    ];

    /// 线上协议字符串（host function 传参格式）
    pub fn as_str(&self) -> &'static str {
        match self {
            ConfigKey::NetworkPort => "network.port",
            ConfigKey::HomeDir => "home_dir",
            ConfigKey::CurrentTimeMs => "system.time_ms",
            ConfigKey::OsPlatform => "os.platform",
            ConfigKey::PairingCodeTtl => "pairing_code_ttl",
            ConfigKey::QrTokenTtl => "qr_token_ttl",
        }
    }

    /// 从协议字符串解析；不在白名单内返回 None
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "network.port" => Some(ConfigKey::NetworkPort),
            "home_dir" => Some(ConfigKey::HomeDir),
            "system.time_ms" => Some(ConfigKey::CurrentTimeMs),
            "os.platform" => Some(ConfigKey::OsPlatform),
            "pairing_code_ttl" => Some(ConfigKey::PairingCodeTtl),
            "qr_token_ttl" => Some(ConfigKey::QrTokenTtl),
            _ => None,
        }
    }
}

/// 宿主配置读取
pub trait HostConfig {
    /// 读取白名单内的配置项；配置不可用返回 `Ok(None)`
    fn config_get(&self, key: ConfigKey) -> Result<Option<String>, HostError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wire_strings_are_contract() {
        // 线上协议字符串被 host function 传参直接使用，宿主侧按字面量解析，
        // 改动会导致新旧插件/宿主失配 —— 逐字面量锁死
        assert_eq!(ConfigKey::NetworkPort.as_str(), "network.port");
        assert_eq!(ConfigKey::HomeDir.as_str(), "home_dir");
        assert_eq!(ConfigKey::CurrentTimeMs.as_str(), "system.time_ms");
        assert_eq!(ConfigKey::OsPlatform.as_str(), "os.platform");
        // 票 14：与 `host-auth` `auth-setting-set` 的键白名单同字面量（读写同键）
        assert_eq!(ConfigKey::PairingCodeTtl.as_str(), "pairing_code_ttl");
        assert_eq!(ConfigKey::QrTokenTtl.as_str(), "qr_token_ttl");
    }

    #[test]
    fn test_round_trip_all_keys() {
        for key in ConfigKey::ALL {
            assert_eq!(ConfigKey::from_str(key.as_str()), Some(*key));
        }
    }

    #[test]
    fn test_all_contains_exactly_six_keys() {
        // 白名单即枚举本身：ALL 必须穷尽全部变体，新增配置项时此处同步断言
        assert_eq!(ConfigKey::ALL.len(), 6);
        assert!(ConfigKey::ALL.contains(&ConfigKey::NetworkPort));
        assert!(ConfigKey::ALL.contains(&ConfigKey::HomeDir));
        assert!(ConfigKey::ALL.contains(&ConfigKey::CurrentTimeMs));
        assert!(ConfigKey::ALL.contains(&ConfigKey::OsPlatform));
        // 票 14：认证域两键（配对码 / QR 有效期），写侧走 host-auth auth-setting-set
        assert!(ConfigKey::ALL.contains(&ConfigKey::PairingCodeTtl));
        assert!(ConfigKey::ALL.contains(&ConfigKey::QrTokenTtl));
    }

    #[test]
    fn test_unknown_key_returns_none() {
        // 宿主白名单校验依赖 from_str 拒绝未知键，勿放行模糊匹配
        assert_eq!(ConfigKey::from_str("network.port2"), None);
        assert_eq!(ConfigKey::from_str("PORT"), None);
        assert_eq!(ConfigKey::from_str(""), None);
    }
}
