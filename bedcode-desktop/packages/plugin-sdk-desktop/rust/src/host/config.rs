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
}

impl ConfigKey {
    /// 全部合法配置项（宿主白名单校验用）
    pub const ALL: &'static [ConfigKey] = &[ConfigKey::NetworkPort, ConfigKey::HomeDir];

    /// 线上协议字符串（host function 传参格式）
    pub fn as_str(&self) -> &'static str {
        match self {
            ConfigKey::NetworkPort => "network.port",
            ConfigKey::HomeDir => "home_dir",
        }
    }

    /// 从协议字符串解析；不在白名单内返回 None
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "network.port" => Some(ConfigKey::NetworkPort),
            "home_dir" => Some(ConfigKey::HomeDir),
            _ => None,
        }
    }
}

/// 宿主配置读取
pub trait HostConfig {
    /// 读取白名单内的配置项；配置不可用返回 `Ok(None)`
    fn config_get(&self, key: ConfigKey) -> Result<Option<String>, HostError>;
}
