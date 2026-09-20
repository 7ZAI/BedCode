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

    // ==================== v19 函数级追加（票 07：认证链 HTTP 面下沉） ====================
    //
    // 认证执行编排移插件（用户裁定，ADR 0022 修订口径）后的回调面：密钥托管、
    // `pairings` / `connection_history` 表与生物凭证公钥留宿主，本组原语只做
    // 无业务语义的记录存取与密码学验签。权限统一 `auth`。

    /// 信任记录写入（内核 `add_pairing` 语义）→ 配对记录 id
    ///
    /// `record_json`（camelCase）：`{ deviceName, fingerprint, publicKey?,
    /// address?, uidHash? }`——`publicKey` **缺省 = 保留既有值**（传空串会清掉
    /// 生物凭证，缺省语义专防此坑）。`uidHash` 命中存量设备时复用原记录 id
    /// （连接历史 / connect_count / 生物凭证不分裂）。
    fn auth_trusted_device_upsert(&self, record_json: &str) -> Result<String, HostError>;

    /// 连接计数 / last_seen 刷新（内核 `update_pairing_last_seen` 语义，不改设备名）
    fn auth_trusted_device_touch(&self, fingerprint: &str) -> Result<(), HostError>;

    /// 连接历史追加（内核 `record_connection_event_by_fingerprint` 语义：
    /// 指纹不存在时静默跳过）。`record_json`：`{ fingerprint, method, result,
    /// address? }`（method/result 取内核常量字符串）
    fn auth_connection_history_record(&self, record_json: &str) -> Result<(), HostError>;

    /// 「已配对且绑定生物凭证公钥」查询（挑战签发闸门）。公钥本身不出口（凭据红线）
    fn auth_biometric_credential_bound(&self, fingerprint: &str) -> Result<bool, HostError>;

    /// 生物认证签名验证（P-256 ECDSA）：用**宿主托管**的绑定公钥验 `message`；
    /// 未配对 / 未绑定公钥返回 `Ok(false)`。密钥与公钥不出宿主
    fn auth_biometric_verify_signature(&self, fingerprint: &str, message: &str, signature: &str) -> Result<bool, HostError>;

    /// 链路身份 Kd 公钥材料读取（只含公开材料）；未就绪返回 `Ok(None)`。
    /// JSON：`{ publicB64, fingerprint }`
    fn auth_link_identity_parts(&self) -> Result<Option<serde_json::Value>, HostError>;

    /// 绑定/解绑生物凭证公钥（内核 `update_pairing_public_key` 语义：只改凭证
    /// 不动 connect_count / last_seen）；`public_key` 空串 = 解绑。
    /// 未配对返回 `Ok(false)`；成功 `Ok(true)`
    fn auth_biometric_credential_bind(&self, fingerprint: &str, public_key: &str) -> Result<bool, HostError>;

    /// 设备认证 JWT 签发（宿主 `JwtService` 同一路径，密钥不出宿主）→ token；
    /// `device_name` / `fingerprint` 空串 = None
    fn auth_device_token_issue(&self, sub: &str, device_name: &str, fingerprint: &str) -> Result<String, HostError>;

    /// 设备认证 JWT 验签：有效 → `Ok(claims JSON)`；无效 → `Err("expired")`
    /// （过期）| `Err("invalid")`（其余）。用户文案映射归插件。
    fn auth_device_token_verify(&self, token: &str) -> Result<String, HostError>;
}