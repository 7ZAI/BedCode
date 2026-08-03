//! 宿主能力：键值存储（按插件隔离）

use super::HostError;

/// 插件键值存储
///
/// 数据按 plugin_id 隔离，需要 `storage` 权限（默认授予）
pub trait HostStorage {
    /// 获取值；键不存在返回 `Ok(None)`
    fn storage_get(&self, key: &str) -> Result<Option<serde_json::Value>, HostError>;

    /// 设置值
    fn storage_set(&self, key: &str, value: &serde_json::Value) -> Result<(), HostError>;

    /// 删除值；键不存在也视为成功
    fn storage_delete(&self, key: &str) -> Result<(), HostError>;
}
