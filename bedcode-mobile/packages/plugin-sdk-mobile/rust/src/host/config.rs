//! 宿主能力：配置读取（白名单即枚举）

use super::HostError;

/// 可读宿主配置项（白名单）
///
/// 白名单 = 本枚举本身：宿主侧 match 穷尽所有变体，
/// 结构性杜绝"白名单声明了但实现缺失"的漂移。
/// 新增配置项：加变体 + 宿主 match 补实现，编译器强制两端同步。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConfigKey {
    /// 应用下载目录绝对路径（Android 外部私有下载目录，免权限）
    ///
    /// 解析策略：`Context.getExternalFilesDir(Environment.DIRECTORY_DOWNLOADS)`
    /// （`/storage/emulated/0/Android/data/com.bedcode.mobile/files/Download`）
    /// 宿主侧惰性创建目录（不存在时 `create_dir_all`）
    AppDownloadsDir,
}

impl ConfigKey {
    /// 全部合法配置项（宿主白名单校验用）
    pub const ALL: &'static [ConfigKey] = &[ConfigKey::AppDownloadsDir];

    /// 线上协议字符串（host function 传参格式）
    pub fn as_str(&self) -> &'static str {
        match self {
            ConfigKey::AppDownloadsDir => "app.downloads_dir",
        }
    }

    /// 从协议字符串解析；不在白名单内返回 None
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "app.downloads_dir" => Some(ConfigKey::AppDownloadsDir),
            _ => None,
        }
    }
}

/// 宿主配置读取
pub trait HostConfig {
    /// 读取白名单内的配置项；配置不可用返回 `Ok(None)`
    fn config_get(&self, key: ConfigKey) -> Result<Option<String>, HostError>;
}
