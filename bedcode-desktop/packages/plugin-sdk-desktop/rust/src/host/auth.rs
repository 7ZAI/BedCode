//! 宿主能力：密钥托管（secret-store，按插件属主隔离）+ 链路身份公开材料

use super::HostError;

/// 密钥托管（v15 secret-store）+ 链路身份（v19 保留，v24 修订，v34 再收窄）
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
/// - 连接历史读写 → 认证中心私有库。
///
/// **v34（B-downsink，2026-09-30）**：生物凭证面（`auth_biometric_credential_bound`
/// / `auth_biometric_verify_signature` / `auth_biometric_credential_bind`）随 WIT
/// `host-auth` 三函数一起退役——生物公钥托管与验签执行下沉认证中心私有库
/// （`auth_records::biometric_key_*` + WASM 内 p256），宿主不再托管任何设备侧
/// 凭证材料。
///
/// 保留面：secret-store、`auth_setting_set`（settings 表配置域）、
/// link-identity-parts。
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

    /// 链路身份 Kd 公钥材料读取（只含公开材料）；未就绪返回 `Ok(None)`。
    /// JSON：`{ publicB64, fingerprint }`
    fn auth_link_identity_parts(&self) -> Result<Option<serde_json::Value>, HostError>;

    // ==================== v33：设备入场 JWT 签发/验签原语退役（ADR 0033） ====================
    // 原 `auth_device_token_issue` / `auth_device_token_verify` 两方法**删除**：入场
    // 密钥的生成 / 签发 / 验签归认证中心自持（`com.bedcode.terminal-session` 的
    // `pairing::jwt` + `pairing::keys`），宿主不再持有任何设备 JWT 密码学。认证中心
    // 只经 `host-auth` 的 `secret-*` 取存密钥材料（属主隔离照旧）。
    // 破坏性：旧产物（v32 SDK 构建）实例化期即被拒（`stale_artifact_rebuild_hint`
    // 点名 v33 重建），不是 trap 也不是静默降级。

    // ==================== v32：认证中心显式注册 + 组合式认证原语（ADR 0031） ====================
    // 认证中心 ≡ 微服务 auth server，区别只在于**没有也不需要服务发现**：注册表就是
    // 宿主进程内的发现协议（激活期注册，单中心唯一性仲裁在宿主）。4 函数权限均 `auth`，
    // **桌面独有**（ADR 0022 双端偏离：移动端不承载服务端网关与认证中心，mobile ABI 不动）。

    /// 注册本插件为**认证中心**（单中心角色）。入参 = 本中心提供的认证方式标识列表
    /// （**声明式**：宿主不解释每个 method 的业务语义）；成功返回中心句柄
    /// `authc-<uuid>`。已有中心在册 → `Err`（点名在册属主）；无 `auth` 权限 → `Err`。
    /// 停用后由宿主 `purge_for_plugin` 兜底回收。
    fn auth_center_register(&self, methods: Vec<String>) -> Result<String, HostError>;

    /// 注销本插件的认证中心角色（仅属主本人可调；非属主 → `Err`；无中心在册视为成功）
    fn auth_center_unregister(&self) -> Result<(), HostError>;

    /// 列取当前中心注册时声明的认证方式（无中心在册 → `Err`）——组合式认证的发现端
    fn auth_methods_list(&self) -> Result<Vec<String>, HostError>;

    /// 经认证中心执行一次认证方式调用（**零解析窄转发**：`params` 原样透传、返回值
    /// 原样透回，宿主只校验 `method` 在册——安全闸门判据，非语义解释）。
    /// 无中心 → `Err`（fail-closed）；中心不可用 → `Err`；中心业务拒绝 → 原样透传
    fn auth_method_invoke(&self, method: &str, params: &str) -> Result<String, HostError>;
}
