//! 宿主能力：密钥托管（secret-store，按插件属主隔离）

use super::HostError;

/// 密钥托管（v15，host-auth）
///
/// 宿主托管的凭据存储：JWT 密钥 / 配对种子等敏感值经宿主持久化于主库
/// `plugin_secrets` 表，按调用方插件实例属主隔离（guest 无法伪造属主）。
/// 权限 `auth`（声明即信任）；密钥明文不落宿主日志（只记长度）。
pub trait HostAuth {
    /// 读取属主密钥；键不存在返回 `Ok(None)`
    fn auth_secret_get(&self, key: &str) -> Result<Option<String>, HostError>;

    /// 写入/覆盖属主密钥（覆盖写替换旧值）
    fn auth_secret_set(&self, key: &str, value: &str) -> Result<(), HostError>;

    /// 删除属主密钥（键不存在也视为成功）
    fn auth_secret_delete(&self, key: &str) -> Result<(), HostError>;

    /// 列举属主密钥名（不返回值本身，供诊断/清理）
    fn auth_secret_keys(&self) -> Result<Vec<String>, HostError>;
}