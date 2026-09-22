//! 宿主能力：密钥托管（secret-store，按插件属主隔离）+ 生物凭证验签 + JWT 签发

use super::HostError;

/// 密钥托管（v15 secret-store）+ 生物凭证 / JWT 密码学面（v19 保留，v24 修订）
///
/// 宿主托管的凭据存储：JWT 密钥 / 配对种子等敏感值经宿主持久化于主库
/// `plugin_secrets` 表，按调用方插件实例属主隔离（guest 无法伪造属主）。
/// 权限 `auth`（声明即信任）；密钥明文不落宿主日志（只记长度）。
///
/// v24（2026-09-22 用户裁定「认证记录下沉」）：`pairings` / `connection_history`
/// 不再留宿主主库，认证记录归认证中心（`com.bedcode.terminal-session`）私有库，
/// 由互调 api 服务——本 trait 的**记录面七函数退役删除**（`auth_trusted_devices_list`
/// / `auth_trusted_device_revoke` / `auth_connection_history_list` /
/// `auth_connection_history_clear` / `auth_trusted_device_upsert` /
/// `auth_trusted_device_touch` / `auth_connection_history_record`）：
///
/// - 配对记录读写 → 认证中心私有库（插件侧直接查）；
/// - 连接历史读写 → 认证中心私有库；
/// - 生物凭证公钥 → 宿主 `plugin_secrets`（§8 凭据红线：公钥不出宿主，
///   验签执行点在宿主）——`biometric-credential-bound` 判定改为查托管公钥
///   存在性，配对状态由认证中心私有库判定。
///
/// 保留面：secret-store、`auth_setting_set`（settings 表配置域）、biometric-*、
/// device-token-*、link-identity-parts。
pub trait HostAuth {
    /// 读取属主密钥；键不存在返回 `Ok(None)`
    fn auth_secret_get(&self, key: &str) -> Result<Option<String>, HostError>;

    /// 写入/覆盖属主密钥（覆盖写替换旧值）
    fn auth_secret_set(&self, key: &str, value: &str) -> Result<(), HostError>;

    /// 删除属主密钥（键不存在也视为成功）
    fn auth_secret_delete(&self, key: &str) -> Result<(), HostError>;

    /// 列举属主密钥名（不返回值本身，供诊断/清理）
    fn auth_secret_keys(&self) -> Result<Vec<String>, HostError>;

    /// 认证域设置项写入（键白名单 `pairing_code_ttl` / `qr_token_ttl`，十进制秒数）
    fn auth_setting_set(&self, key: &str, value: &str) -> Result<(), HostError>;

    // ==================== v19 保留面（v24 修订语义：公钥托管在 plugin_secrets） ====================

    /// 「已绑定生物凭证公钥」查询（挑战签发闸门之一：**配对状态由认证中心私有库
    /// 判定**，本原语只查宿主托管的公钥存在性）。公钥本身不出口（凭据红线）
    fn auth_biometric_credential_bound(&self, fingerprint: &str) -> Result<bool, HostError>;

    /// 生物认证签名验证（P-256 ECDSA）：用**宿主托管**的绑定公钥验 `message`；
    /// 未绑定公钥返回 `Ok(false)`。密钥与公钥不出宿主
    fn auth_biometric_verify_signature(&self, fingerprint: &str, message: &str, signature: &str) -> Result<bool, HostError>;

    /// 链路身份 Kd 公钥材料读取（只含公开材料）；未就绪返回 `Ok(None)`。
    /// JSON：`{ publicB64, fingerprint }`
    fn auth_link_identity_parts(&self) -> Result<Option<serde_json::Value>, HostError>;

    /// 绑定/解绑生物凭证公钥（宿主托管语义：只改凭证不动计数——与认证登录路径的
    /// 计数语义刻意不同）；`public_key` 空串 = 解绑。未找到配对记录返回 `Ok(false)`；
    /// 成功 `Ok(true)`
    fn auth_biometric_credential_bind(&self, fingerprint: &str, public_key: &str) -> Result<bool, HostError>;

    /// 设备认证 JWT 签发（宿主 `JwtService` 同一路径，密钥不出宿主）→ token；
    /// `device_name` / `fingerprint` 空串 = None
    fn auth_device_token_issue(&self, sub: &str, device_name: &str, fingerprint: &str) -> Result<String, HostError>;

    /// 设备认证 JWT 验签：有效 → `Ok(claims JSON)`；无效 → `Err("expired")`
    /// （过期）| `Err("invalid")`（其余）。用户文案映射归插件。
    fn auth_device_token_verify(&self, token: &str) -> Result<String, HostError>;
}