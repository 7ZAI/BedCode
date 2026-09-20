//! 宿主能力：密钥托管（secret-store，按插件属主隔离）+ 认证记录面（只读 / 撤销）

use super::HostError;

/// 密钥托管（v15 secret-store）+ 认证记录面（v18）
///
/// 宿主托管的凭据存储：JWT 密钥 / 配对种子等敏感值经宿主持久化于主库
/// `plugin_secrets` 表，按调用方插件实例属主隔离（guest 无法伪造属主）。
/// 权限 `auth`（声明即信任）；密钥明文不落宿主日志（只记长度）。
///
/// v18 记录面返回**内核原始记录**（`pairings` / `connection_history` 表真源），
/// 排序、过滤与解读归插件（ADR 0022 裁剪线：宿主不做产品判定）；
/// `pairings` 的凭据列（session token / public key）不出内核。
pub trait HostAuth {
    /// 读取属主密钥；键不存在返回 `Ok(None)`
    fn auth_secret_get(&self, key: &str) -> Result<Option<String>, HostError>;

    /// 写入/覆盖属主密钥（覆盖写替换旧值）
    fn auth_secret_set(&self, key: &str, value: &str) -> Result<(), HostError>;

    /// 删除属主密钥（键不存在也视为成功）
    fn auth_secret_delete(&self, key: &str) -> Result<(), HostError>;

    /// 列举属主密钥名（不返回值本身，供诊断/清理）
    fn auth_secret_keys(&self) -> Result<Vec<String>, HostError>;

    /// 已配对设备原始记录（含软删行 `isActive=false`，无序；JSON 数组）
    ///
    /// 全量返回是刻意的：撤销检测依赖「已撤销记录仍可见」，只回活跃集合会让
    /// 撤销判定 fail-open。
    fn auth_trusted_devices_list(&self) -> Result<serde_json::Value, HostError>;

    /// 撤销信任（软删 `isActive=false` + 连带删除该设备连接历史）；
    /// 返回是否命中记录，未知 id 幂等 `false`
    fn auth_trusted_device_revoke(&self, id: &str) -> Result<bool, HostError>;

    /// 设备连接历史原始记录（`device_id` = `pairings.id`；JSON 数组）
    fn auth_connection_history_list(&self, device_id: &str) -> Result<serde_json::Value, HostError>;

    /// 认证域设置项写入（键白名单 `pairing_code_ttl` / `qr_token_ttl`，十进制秒数）
    fn auth_setting_set(&self, key: &str, value: &str) -> Result<(), HostError>;

    /// 清空某设备的连接历史（v19 函数级追加，票 14）；返回是否命中了至少一条记录
    ///
    /// 与 `auth_trusted_device_revoke` 的连带删除同语义（后者是撤销配对时的隐式清理，
    /// 本函数是设备页「清空历史」的显式动作）；**不影响配对状态**。
    fn auth_connection_history_clear(&self, device_id: &str) -> Result<bool, HostError>;
}