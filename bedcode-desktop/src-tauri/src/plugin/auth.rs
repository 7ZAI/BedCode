//! host-auth 宿主侧接线（路径 B：WIT 绑定 + `Host` impl + 域函数 + 自报，四件同处）
//!
//! wasm-core 纯净性收口票 02 批次 04 自内核迁出（`host_api/auth.rs` 整文件删除，
//! 内核反向锁 `auth_domain_must_not_return_to_wasm_core` 防回接）。与 pty / mdns /
//! ws / http 等「窄端口」域不同，host-auth **没有独立能力 crate**——装配方就是宿主
//! 本 crate（孤儿规则见 [`super::bindings`] 模块文档），域函数与 WIT impl 同处。
//!
//! ## 域面形状（v15 起逐步收敛后的现状）
//!
//! - **密钥托管**（v15 secret-store）：主库 `plugin_secrets` 表 + read-through 缓存，
//!   属主 = 调用方插件实例的 plugin_id（guest 无法伪造；越权在 SQL 层即无命中——
//!   权限门之外的第二道闸）；
//! - **认证域设置写入**（v18 `auth-setting-set`）：键白名单 + 正整数校验，`settings`
//!   表是宿主真源（TTL 由宿主命令面读取后传给插件）；
//! - **链路身份 Kd 公钥材料读取**（v19 保留面；生物凭证面已随 v34 / B-downsink 退役，
//!   公钥托管与验签执行在认证中心插件私有库）；
//! - **认证中心显式注册 + 组合式认证原语**（v32 / ADR 0031 K1/K6）：单中心唯一性
//!   仲裁在 [`bedcode_wasm_core::host_api::auth_center`] 注册表（**真源仍在内核**——
//!   内核 `boot.rs` 的 L2 启动门与 `activation.rs` 停用回收同表消费，见下文），
//!   本域只做权限门 + 薄分派；`auth-method-invoke` 是**零解析窄转发**（ADR 0032
//!   L2 红线③：不拆 params、不解释 method 语义）。
//!
//! ## 哪些机制**刻意留在宿主**（AGENTS §5.1.3 允许的薄壳）
//!
//! - **权限门**（`auth` 位，全部 10 条原语同一门）：复用 wasm-core 的
//!   `host_api::check_permission`（同一份 PermissionManager、同一条拒绝 warn 路径）；
//! - **明文不落日志红线**（AGENTS §8）：secret 写入只记 `value.len()`，错误消息
//!   不含值内容；
//! - **认证域设置白名单**：`settings` 表是宿主真源，插件只能写本域两个 TTL 键。
//!
//! ## 注册表为什么留在内核（票 02 批次 04 显式裁决）
//!
//! `auth_center` 注册表被四方消费：内核 `boot.rs` 的 L2 启动门（按在册中心做对账）、
//! 内核 `activation.rs` 的停用回收、宿主 lib 的裁决面/桥接门
//! （`utils/auth/auth_center.rs`）、`test_support` 测试闸门。它是 ADR 0031 显式裁决的
//! 「通用注册表」薄壳；迁出会迫使内核启动门重建（内核无法点名宿主类型）。本域经
//! `bedcode_wasm_core::host_api::auth_center` 公开面取用，不得复制仲裁判据。

use bedcode_host_kit::ports::downcast_host;
use bedcode_host_kit::{HostModule, HostModuleDesc, ModuleEntry, WasmPluginState};
use bedcode_plugin_api::permission::PERMISSION_AUTH;
use bedcode_wasm_core::host_api::auth_center;
use bedcode_wasm_core::host_api::check_permission;
use bedcode_wasm_core::host_api::context::{DbScope, PermissionScope, SecretsScope, WasmHostContext};
use bedcode_wasm_core::runtime_util::block_on_async;
use chrono::Utc;
use rusqlite::OptionalExtension;

use crate::plugin::bindings::bedcode;

// 能力模块白名单条目（宿主自报；生效白名单 = 内核 IN_CRATE ∪ 宿主自报）。路径 B 域
// 的自报静态住在本 crate（宿主 lib 即最终二进制）⇒ 无需强制引用行。
bedcode_host_kit::expect_host_module!(MODULE_NAME);

/// 能力模块名（白名单键即装载期日志与错误文案里的模块名）
pub const MODULE_NAME: &str = "auth";

/// 本域提供的 WIT 接口（必须与 `bedcode.wit` 逐字一致；改错即 guest import 失配）
pub const MODULE_INTERFACES: &[&str] = &["bedcode:plugin/host-auth"];

/// 本域的权限位（必须与 `bedcode.wit` / SDK 权限表逐字一致；全域单权限位 `auth`）
pub const MODULE_PERMISSIONS: &[&str] = &["auth"];

/// 能力模块描述符（机制面：接口路径 / 权限位 / ABI 下界；**禁带产品名词**）
///
/// `abi_min = 15`：host-auth（secret-store）在 ABI v15 引入。
const DESC: HostModuleDesc = HostModuleDesc {
    name: MODULE_NAME,
    interfaces: MODULE_INTERFACES,
    permissions: MODULE_PERMISSIONS,
    abi_min: 15,
};

/// host-auth 能力模块
pub struct AuthModule;

impl HostModule for AuthModule {
    fn desc(&self) -> HostModuleDesc {
        DESC
    }

    fn register(
        &self,
        linker: &mut wasmtime::component::Linker<WasmPluginState>,
    ) -> wasmtime::Result<()> {
        bedcode::plugin::host_auth::add_to_linker::<WasmPluginState, HasSelf>(linker, |s| s)
    }
}

/// getter：让 guest 侧 import 取到可变的状态引用（与内核接线同款）
type HasSelf = wasmtime::component::HasSelf<WasmPluginState>;

/// 静态单例（供 `inventory::submit!` 取址）
static MODULE: AuthModule = AuthModule;

// 能力模块自报（linker-section 静态；收集点在 host-kit）
inventory::submit! {
    ModuleEntry { module: &MODULE }
}

// ==================== WIT 层（import 接口 → 域函数转发） ====================

/// 属主 = 调用方插件实例的 plugin_id（本 impl 自 store state 派生，guest 无法伪造）；
/// 权限门 + 主库持久化 + 明文不落日志 + 记录面白名单全部在下方域函数内实现。
impl bedcode::plugin::host_auth::Host for WasmPluginState {
    fn secret_get(&mut self, key: String) -> Result<Option<String>, String> {
        auth_secret_get(ctx_of(self), ctx_of(self), ctx_of(self), &self.plugin_id, &key)
    }

    fn secret_set(&mut self, key: String, value: String) -> Result<(), String> {
        auth_secret_set(ctx_of(self), ctx_of(self), ctx_of(self), &self.plugin_id, &key, &value)
    }

    fn secret_delete(&mut self, key: String) -> Result<(), String> {
        auth_secret_delete(ctx_of(self), ctx_of(self), ctx_of(self), &self.plugin_id, &key)
    }

    fn secret_keys(&mut self) -> Result<Vec<String>, String> {
        auth_secret_keys(ctx_of(self), ctx_of(self), &self.plugin_id)
    }

    fn auth_setting_set(&mut self, key: String, value: String) -> Result<(), String> {
        auth_setting_set(ctx_of(self), ctx_of(self), &self.plugin_id, &key, &value)
    }

    // ==================== v19 保留面（v34 修订：生物凭证面已退役） ====================

    fn link_identity_parts(&mut self) -> Result<Option<String>, String> {
        auth_link_identity_parts(ctx_of(self), &self.plugin_id)
    }

    // ==================== v33 / v34：认证中心自持凭据面接线退役（ADR 0033 + B-downsink） ====================
    // 原 `device_token_issue` / `device_token_verify` 接线随 WIT `host-auth` 两函数
    // 删除：入场密钥的生成 / 签发 / 验签归认证中心自持，宿主不持有任何设备 JWT 密码
    // 学。中心只经 `auth_secret_get/set` 存取密钥材料（属主隔离照旧）。旧产物（v32
    // SDK 构建）仍 import 这两个函数 → 实例化期被拒（`stale_artifact_rebuild_hint`
    // 点名 v33 重建），不是 trap 也不是静默降级。

    // ==================== v32：认证中心显式注册 + 组合式认证原语（ADR 0031） ====================

    fn auth_center_register(&mut self, methods: Vec<String>) -> Result<String, String> {
        auth_center_register(ctx_of(self), &self.plugin_id, methods)
    }

    fn auth_center_unregister(&mut self) -> Result<(), String> {
        auth_center_unregister(ctx_of(self), &self.plugin_id)
    }

    fn auth_methods_list(&mut self) -> Result<Vec<String>, String> {
        auth_methods_list(ctx_of(self), &self.plugin_id)
    }

    fn auth_method_invoke(&mut self, method: String, params: String) -> Result<String, String> {
        auth_method_invoke(ctx_of(self), ctx_of(self), &self.plugin_id, &method, &params)
    }
}

/// 取本实例的宿主上下文（与内核 `HostCtxOf::host_ctx` 同一转型；类型不符即 panic
/// ——装配期编程错误，fail-visible，不静默降级）
fn ctx_of(state: &WasmPluginState) -> &WasmHostContext {
    downcast_host::<WasmHostContext>(state.host.as_ref())
}

// ==================== 域函数（自 `host_api/auth.rs` 迁入；签名逐字保留） ====================

/// **测试夹具**：以宿主身份往某插件属主的 secret-store 写一个键
///
/// 只为 `utils::auth::test_tokens::seed_keyring` 服务——那里需要把认证中心的
/// 入场密钥环种成已知密钥，才能在**不自造 JWT 密码学**的前提下签出中心认的
/// token（ADR 0033 后宿主没有签发面）。走真实的 `auth_secret_set` 实现
/// （含权限门与明文不落日志），而不是直接写库——夹具走的是真路径。
///
/// **常编译 pub（票 02 批次 04 随域迁宿主）**：`test_tokens` 同批迁到本 crate
/// `utils::auth::test_tokens`（原依赖的 wasm-core 内部函数不复存在）。
pub fn test_seed_plugin_secret(
    db: &dyn DbScope,
    secrets: &dyn SecretsScope,
    perm: &dyn PermissionScope,
    plugin_id: &str,
    key: &str,
    value: &str,
) -> Result<(), String> {
    auth_secret_set(db, secrets, perm, plugin_id, key, value)
}

/// 认证域设置项写侧白名单（v18 `auth-setting-set`）
///
/// `settings` 表是宿主真源（TTL 由宿主命令面读取后传入插件），插件只能写这一域
/// 的两个 TTL 键——白名单外一律拒绝，避免「有 auth 权限即可改任意宿主设置」。
pub(crate) const AUTH_SETTING_KEYS: &[&str] = &["pairing_code_ttl", "qr_token_ttl"];

/// 读取属主密钥（权限门 + 内存缓存 read-through + 主库真源）
pub(crate) fn auth_secret_get(
    db: &dyn DbScope,
    secrets: &dyn SecretsScope,
    perm: &dyn PermissionScope,
    plugin_id: &str,
    key: &str,
) -> Result<Option<String>, String> {
    if !check_permission(perm, plugin_id, PERMISSION_AUTH, "host_auth_secret_get") {
        return Err("permission denied".to_string());
    }
    // 缓存读
    let cache = secrets.secrets_cache().clone();
    {
        let guard = cache.read().map_err(|e| format!("secret cache poisoned: {}", e))?;
        if let Some(v) = guard.get(&(plugin_id.to_string(), key.to_string())) {
            return Ok(Some(v.clone()));
        }
    }
    // 主库读
    let db = db.database().clone();
    let pid = plugin_id.to_string();
    let k = key.to_string();
    let value: Option<String> = block_on_async(async move {
        let db = db.lock().await;
        db.conn()
            .query_row(
                "SELECT value FROM plugin_secrets WHERE plugin_id = ?1 AND key = ?2",
                rusqlite::params![pid, k],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|e| format!("database error: {}", e))
    })?;
    // 回填缓存
    if let Some(v) = &value {
        let mut guard = cache.write().map_err(|e| format!("secret cache poisoned: {}", e))?;
        guard.insert((plugin_id.to_string(), key.to_string()), v.clone());
    }
    Ok(value)
}

/// 写入/覆盖属主密钥（覆盖写；缓存失效后由下次 get 回填）
pub(crate) fn auth_secret_set(
    db: &dyn DbScope,
    secrets: &dyn SecretsScope,
    perm: &dyn PermissionScope,
    plugin_id: &str,
    key: &str,
    value: &str,
) -> Result<(), String> {
    if !check_permission(perm, plugin_id, PERMISSION_AUTH, "host_auth_secret_set") {
        return Err("permission denied".to_string());
    }
    // 明文不落日志：只记长度
    tracing::info!(
        plugin_id = %plugin_id,
        key = %key,
        value_len = value.len(),
        "host_auth_secret_set: secret stored (length only)"
    );
    let db = db.database().clone();
    let pid = plugin_id.to_string();
    let k = key.to_string();
    let v = value.to_string();
    let now = Utc::now().to_rfc3339();
    block_on_async(async move {
        let db = db.lock().await;
        db.conn()
            .execute(
                "INSERT INTO plugin_secrets (plugin_id, key, value, updated_at) \
                 VALUES (?1, ?2, ?3, ?4) \
                 ON CONFLICT(plugin_id, key) DO UPDATE SET value = ?3, updated_at = ?4",
                rusqlite::params![pid, k, v, now],
            )
            .map_err(|e| format!("database error: {}", e))?;
        Ok::<(), String>(())
    })?;
    // 缓存失效
    secrets
        .secrets_cache()
        .write()
        .map_err(|e| format!("secret cache poisoned: {}", e))?
        .remove(&(plugin_id.to_string(), key.to_string()));
    Ok(())
}

/// 删除属主密钥（键不存在也视为成功；缓存同步移除）
pub(crate) fn auth_secret_delete(
    db: &dyn DbScope,
    secrets: &dyn SecretsScope,
    perm: &dyn PermissionScope,
    plugin_id: &str,
    key: &str,
) -> Result<(), String> {
    if !check_permission(perm, plugin_id, PERMISSION_AUTH, "host_auth_secret_delete") {
        return Err("permission denied".to_string());
    }
    let db = db.database().clone();
    let pid = plugin_id.to_string();
    let k = key.to_string();
    block_on_async(async move {
        let db = db.lock().await;
        db.conn()
            .execute(
                "DELETE FROM plugin_secrets WHERE plugin_id = ?1 AND key = ?2",
                rusqlite::params![pid, k],
            )
            .map_err(|e| format!("database error: {}", e))?;
        Ok::<(), String>(())
    })?;
    secrets
        .secrets_cache()
        .write()
        .map_err(|e| format!("secret cache poisoned: {}", e))?
        .remove(&(plugin_id.to_string(), key.to_string()));
    Ok(())
}

/// 列举属主密钥名（不返回值本身，供诊断/清理）
pub(crate) fn auth_secret_keys(
    db: &dyn DbScope,
    perm: &dyn PermissionScope,
    plugin_id: &str,
) -> Result<Vec<String>, String> {
    if !check_permission(perm, plugin_id, PERMISSION_AUTH, "host_auth_secret_keys") {
        return Err("permission denied".to_string());
    }
    let db = db.database().clone();
    let pid = plugin_id.to_string();
    block_on_async(async move {
        let db = db.lock().await;
        let mut stmt = db
            .conn()
            .prepare("SELECT key FROM plugin_secrets WHERE plugin_id = ?1 ORDER BY key")
            .map_err(|e| format!("database error: {}", e))?;
        let rows = stmt
            .query_map(rusqlite::params![pid], |row| row.get::<_, String>(0))
            .map_err(|e| format!("database error: {}", e))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| format!("database error: {}", e))
    })
}

// ==================== v18 认证域设置写入（保留） ====================
//
// v24（2026-09-22 认证记录下沉）：主库 `pairings` / `connection_history` 表退役，
// 认证记录归认证中心插件私有库。原记录面（trusted-devices-list / trusted-device-revoke
// / connection-history-list / connection-history-clear / trusted-device-upsert /
// trusted-device-touch / connection-history-record）随 WIT 删除；生物凭证原语
// （biometric-*）保留至 v34 再退役，公钥托管改挂 `plugin_secrets`。settings 表保留
// （配置域，认证中心 TTL 读取依赖）。

/// 认证域设置项写入（键白名单 + 值校验；读取走宿主配置面）
///
/// 校验在 Rust 端（最终仲裁）：键必须落在 [`AUTH_SETTING_KEYS`]，值必须是正整数
/// 十进制秒数——非法值显性报错，不写入半合法数据。
pub(crate) fn auth_setting_set(
    db: &dyn DbScope,
    perm: &dyn PermissionScope,
    plugin_id: &str,
    key: &str,
    value: &str,
) -> Result<(), String> {
    if !check_permission(perm, plugin_id, PERMISSION_AUTH, "host_auth_setting_set") {
        return Err("permission denied".to_string());
    }
    if !AUTH_SETTING_KEYS.contains(&key) {
        return Err(format!("setting key not in auth domain whitelist: {}", key));
    }
    if value.parse::<u64>().map(|v| v == 0).unwrap_or(true) {
        return Err(format!(
            "setting '{}' must be a positive integer (got '{}')",
            key, value
        ));
    }
    let db = db.database().clone();
    let k = key.to_string();
    let v = value.to_string();
    block_on_async(async move {
        let db = db.lock().await;
        db.set_setting(&k, &v).map_err(|e| format!("database error: {}", e))
    })?;
    tracing::info!(
        plugin_id = %plugin_id,
        key = %key,
        "host_auth_setting_set: auth domain setting updated"
    );
    Ok(())
}

// ==================== v19 保留面（v24 修订：公钥托管在 plugin_secrets） ====================

/// 链路身份 Kd 公钥材料读取（`link_crypto::identity_parts` 语义）；未就绪 → None
pub(crate) fn auth_link_identity_parts(
    perm: &dyn PermissionScope,
    plugin_id: &str,
) -> Result<Option<String>, String> {
    if !check_permission(perm, plugin_id, PERMISSION_AUTH, "host_auth_link_identity_parts") {
        return Err("permission denied".to_string());
    }
    match bedcode_server_core::link_crypto::identity_parts() {
        Some((fingerprint, public_b64)) => {
            let payload = serde_json::json!({
                "publicB64": public_b64,
                "fingerprint": fingerprint,
            });
            Ok(Some(payload.to_string()))
        }
        None => Ok(None),
    }
}

// ==================== v32：认证中心显式注册 + 组合式认证原语（ADR 0031） ====================
//
// 单中心注册表本体 + 唯一性仲裁 + 停用回收在内核 `host_api/auth_center.rs`
//（真源留内核的裁决见模块文档）；本面只做权限门（全部复用既有 `auth` 权限位，K8）
// + 对注册表的薄分派。`auth-method-invoke` 是**零解析窄转发**（ADR 0032 L2 红线③）。

/// 注册本插件为认证中心（K1；单中心仲裁 K4 在注册表内，重复注册标点名在册属主）
pub(crate) fn auth_center_register(
    perm: &dyn PermissionScope,
    plugin_id: &str,
    methods: Vec<String>,
) -> Result<String, String> {
    if !check_permission(perm, plugin_id, PERMISSION_AUTH, "host_auth_center_register") {
        return Err("permission denied".to_string());
    }
    auth_center::register(plugin_id, methods)
}

/// 注销本插件的认证中心角色（仅属主本人；无中心在册幂等成功）
pub(crate) fn auth_center_unregister(
    perm: &dyn PermissionScope,
    plugin_id: &str,
) -> Result<(), String> {
    if !check_permission(perm, plugin_id, PERMISSION_AUTH, "host_auth_center_unregister") {
        return Err("permission denied".to_string());
    }
    auth_center::unregister(plugin_id)
}

/// 列取当前认证中心登记的认证方式（组合式认证的发现端，K6；无中心 fail-closed）
pub(crate) fn auth_methods_list(
    perm: &dyn PermissionScope,
    plugin_id: &str,
) -> Result<Vec<String>, String> {
    if !check_permission(perm, plugin_id, PERMISSION_AUTH, "host_auth_methods_list") {
        return Err("permission denied".to_string());
    }
    let Some(entry) = auth_center::center() else {
        tracing::warn!(plugin_id = %plugin_id, deny_kind = "no_center", "auth-methods-list without a registered auth center");
        return Err("no auth center registered".to_string());
    };
    Ok(entry.methods)
}

/// 经认证中心执行一次认证方式调用（K6 零解析窄转发）。
///
/// 边界（spec §4.3.1）：无中心 → `no auth center registered`（fail-closed）；
/// method 不在注册表 → 点名 method 与在册列表（安全闸门）；调用传输失败 →
/// `auth center unavailable: <原因>`；中心返回错误信封 → **原样透传**（业务拒绝，
/// 不吞成宿主错误）。
pub(crate) fn auth_method_invoke(
    perm: &dyn PermissionScope,
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    method: &str,
    params: &str,
) -> Result<String, String> {
    if !check_permission(perm, plugin_id, PERMISSION_AUTH, "host_auth_method_invoke") {
        return Err("permission denied".to_string());
    }
    auth_center::invoke_auth_method(host_ctx, method, params)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bedcode_wasm_core::host_api::grant_permissions;
    use bedcode_wasm_core::test_support::build_host_ctx_at;
    use std::path::Path;
    use std::sync::Arc;

    /// 文件后备库宿主上下文（重启持久化测试用；`build_host_ctx_at(Some(path))`）
    fn file_host_ctx(db_path: &Path) -> Arc<WasmHostContext> {
        build_host_ctx_at(Some(db_path))
    }

    #[test]
    fn test_set_get_roundtrip_and_overwrite() {
        let host_ctx = build_host_ctx_at(None);
        grant_permissions(&host_ctx, "com.bedcode.test-a", &[PERMISSION_AUTH]);
        assert_eq!(
            auth_secret_get(
                host_ctx.as_ref(),
                host_ctx.as_ref(),
                host_ctx.as_ref(),
                "com.bedcode.test-a",
                "jwt.key"
            )
            .unwrap(),
            None
        );
        auth_secret_set(
            host_ctx.as_ref(),
            host_ctx.as_ref(),
            host_ctx.as_ref(),
            "com.bedcode.test-a",
            "jwt.key",
            "secret-v1",
        )
        .unwrap();
        assert_eq!(
            auth_secret_get(
                host_ctx.as_ref(),
                host_ctx.as_ref(),
                host_ctx.as_ref(),
                "com.bedcode.test-a",
                "jwt.key"
            )
            .unwrap()
            .as_deref(),
            Some("secret-v1")
        );
        // 覆盖写
        auth_secret_set(
            host_ctx.as_ref(),
            host_ctx.as_ref(),
            host_ctx.as_ref(),
            "com.bedcode.test-a",
            "jwt.key",
            "secret-v2",
        )
        .unwrap();
        assert_eq!(
            auth_secret_get(
                host_ctx.as_ref(),
                host_ctx.as_ref(),
                host_ctx.as_ref(),
                "com.bedcode.test-a",
                "jwt.key"
            )
            .unwrap()
            .as_deref(),
            Some("secret-v2")
        );
    }

    #[test]
    fn test_owner_isolation_across_plugins() {
        let host_ctx = build_host_ctx_at(None);
        grant_permissions(&host_ctx, "com.bedcode.test-a", &[PERMISSION_AUTH]);
        grant_permissions(&host_ctx, "com.bedcode.test-b", &[PERMISSION_AUTH]);
        auth_secret_set(
            host_ctx.as_ref(),
            host_ctx.as_ref(),
            host_ctx.as_ref(),
            "com.bedcode.test-a",
            "seed",
            "a-secret",
        )
        .unwrap();
        // B 同名 key 读不到 A 的值（命名空间隔离）
        assert_eq!(
            auth_secret_get(
                host_ctx.as_ref(),
                host_ctx.as_ref(),
                host_ctx.as_ref(),
                "com.bedcode.test-b",
                "seed"
            )
            .unwrap(),
            None
        );
        // B 删不掉 A 的密钥
        auth_secret_delete(
            host_ctx.as_ref(),
            host_ctx.as_ref(),
            host_ctx.as_ref(),
            "com.bedcode.test-b",
            "seed",
        )
        .unwrap();
        assert_eq!(
            auth_secret_get(
                host_ctx.as_ref(),
                host_ctx.as_ref(),
                host_ctx.as_ref(),
                "com.bedcode.test-a",
                "seed"
            )
            .unwrap()
            .as_deref(),
            Some("a-secret")
        );
        // B 写同名 key 不覆盖 A
        auth_secret_set(
            host_ctx.as_ref(),
            host_ctx.as_ref(),
            host_ctx.as_ref(),
            "com.bedcode.test-b",
            "seed",
            "b-secret",
        )
        .unwrap();
        assert_eq!(
            auth_secret_get(
                host_ctx.as_ref(),
                host_ctx.as_ref(),
                host_ctx.as_ref(),
                "com.bedcode.test-a",
                "seed"
            )
            .unwrap()
            .as_deref(),
            Some("a-secret")
        );
        assert_eq!(
            auth_secret_get(
                host_ctx.as_ref(),
                host_ctx.as_ref(),
                host_ctx.as_ref(),
                "com.bedcode.test-b",
                "seed"
            )
            .unwrap()
            .as_deref(),
            Some("b-secret")
        );
    }

    #[test]
    fn test_delete_and_keys() {
        let host_ctx = build_host_ctx_at(None);
        grant_permissions(&host_ctx, "com.bedcode.test-a", &[PERMISSION_AUTH]);
        auth_secret_set(
            host_ctx.as_ref(),
            host_ctx.as_ref(),
            host_ctx.as_ref(),
            "com.bedcode.test-a",
            "k1",
            "v1",
        )
        .unwrap();
        auth_secret_set(
            host_ctx.as_ref(),
            host_ctx.as_ref(),
            host_ctx.as_ref(),
            "com.bedcode.test-a",
            "k2",
            "v2",
        )
        .unwrap();
        let keys = auth_secret_keys(host_ctx.as_ref(), host_ctx.as_ref(), "com.bedcode.test-a").unwrap();
        assert_eq!(keys, vec!["k1".to_string(), "k2".to_string()]);
        auth_secret_delete(
            host_ctx.as_ref(),
            host_ctx.as_ref(),
            host_ctx.as_ref(),
            "com.bedcode.test-a",
            "k1",
        )
        .unwrap();
        // 幂等删除
        auth_secret_delete(
            host_ctx.as_ref(),
            host_ctx.as_ref(),
            host_ctx.as_ref(),
            "com.bedcode.test-a",
            "k1",
        )
        .unwrap();
        assert_eq!(
            auth_secret_get(
                host_ctx.as_ref(),
                host_ctx.as_ref(),
                host_ctx.as_ref(),
                "com.bedcode.test-a",
                "k1"
            )
            .unwrap(),
            None
        );
        assert_eq!(
            auth_secret_keys(host_ctx.as_ref(), host_ctx.as_ref(), "com.bedcode.test-a").unwrap(),
            vec!["k2".to_string()]
        );
    }

    #[test]
    fn test_permission_denied_without_auth_grant() {
        let host_ctx = build_host_ctx_at(None);
        // 未授权 auth 权限的插件 → 全部四函数拒绝（Rust 端最终仲裁）
        assert_eq!(
            auth_secret_get(
                host_ctx.as_ref(),
                host_ctx.as_ref(),
                host_ctx.as_ref(),
                "com.bedcode.no-auth",
                "k"
            )
            .unwrap_err(),
            "permission denied"
        );
        assert_eq!(
            auth_secret_set(
                host_ctx.as_ref(),
                host_ctx.as_ref(),
                host_ctx.as_ref(),
                "com.bedcode.no-auth",
                "k",
                "v"
            )
            .unwrap_err(),
            "permission denied"
        );
        assert_eq!(
            auth_secret_delete(
                host_ctx.as_ref(),
                host_ctx.as_ref(),
                host_ctx.as_ref(),
                "com.bedcode.no-auth",
                "k"
            )
            .unwrap_err(),
            "permission denied"
        );
        assert_eq!(
            auth_secret_keys(host_ctx.as_ref(), host_ctx.as_ref(), "com.bedcode.no-auth").unwrap_err(),
            "permission denied"
        );
    }

    #[test]
    fn test_persistence_across_restart() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("persist.db");
        // 第一代上下文：写入
        {
            let host_ctx = file_host_ctx(&db_path);
            grant_permissions(&host_ctx, "com.bedcode.test-a", &[PERMISSION_AUTH]);
            auth_secret_set(
                host_ctx.as_ref(),
                host_ctx.as_ref(),
                host_ctx.as_ref(),
                "com.bedcode.test-a",
                "jwt.key",
                "persisted-secret",
            )
            .unwrap();
        }
        // 第二代上下文（全新内存缓存 + 全新连接）：重启后密钥稳定
        {
            let host_ctx = file_host_ctx(&db_path);
            grant_permissions(&host_ctx, "com.bedcode.test-a", &[PERMISSION_AUTH]);
            assert_eq!(
                auth_secret_get(
                    host_ctx.as_ref(),
                    host_ctx.as_ref(),
                    host_ctx.as_ref(),
                    "com.bedcode.test-a",
                    "jwt.key"
                )
                .unwrap()
                .as_deref(),
                Some("persisted-secret")
            );
        }
    }

    // ==================== v24：biometric 公钥托管（B-downsink 已退役） ====================
    // 生物凭证公钥托管 + 验签执行已随 v34 下沉认证中心插件私有库，宿主不再托管任何
    // 设备侧凭证材料。原 biometric 往返/验签用例一并删除（随 v34）。

    /// 设置写入：白名单 + 正整数校验；合法值落宿主 settings 表（宿主命令面据此取 TTL）
    #[test]
    fn test_auth_setting_set_whitelist_and_persist() {
        let host_ctx = build_host_ctx_at(None);
        grant_permissions(&host_ctx, "com.bedcode.terminal-session", &[PERMISSION_AUTH]);

        // 白名单外键拒绝（settings 表是宿主真源，不接受任意键写入）
        let err = auth_setting_set(
            host_ctx.as_ref(),
            host_ctx.as_ref(),
            "com.bedcode.terminal-session",
            "network.port",
            "1",
        )
        .unwrap_err();
        assert!(err.contains("not in auth domain whitelist"), "got: {err}");
        // 非正整数拒绝（0 / 负数 / 非数字均不写入）
        for bad in ["0", "-1", "abc", ""] {
            let err = auth_setting_set(
                host_ctx.as_ref(),
                host_ctx.as_ref(),
                "com.bedcode.terminal-session",
                "pairing_code_ttl",
                bad,
            )
            .unwrap_err();
            assert!(err.contains("positive integer"), "值 {bad:?} 必须拒绝, got: {err}");
        }

        auth_setting_set(
            host_ctx.as_ref(),
            host_ctx.as_ref(),
            "com.bedcode.terminal-session",
            "pairing_code_ttl",
            "600",
        )
        .unwrap();
        auth_setting_set(
            host_ctx.as_ref(),
            host_ctx.as_ref(),
            "com.bedcode.terminal-session",
            "qr_token_ttl",
            "120",
        )
        .unwrap();
        // 锁经 block_on_async（普通测试线程 blocking_lock 不可靠）
        block_on_async(async {
            let db = host_ctx.database().lock().await;
            assert_eq!(db.get_setting("pairing_code_ttl").unwrap().as_deref(), Some("600"));
            assert_eq!(db.get_setting("qr_token_ttl").unwrap().as_deref(), Some("120"));
        });
    }

    /// 设置写入的持久化：写后新建上下文（模拟重启）仍可读回
    #[test]
    fn test_auth_setting_set_persists_across_restart() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("settings.db");
        {
            let host_ctx = file_host_ctx(&db_path);
            grant_permissions(&host_ctx, "com.bedcode.terminal-session", &[PERMISSION_AUTH]);
            auth_setting_set(
                host_ctx.as_ref(),
                host_ctx.as_ref(),
                "com.bedcode.terminal-session",
                "pairing_code_ttl",
                "900",
            )
            .unwrap();
        }
        {
            let host_ctx = file_host_ctx(&db_path);
            // 锁经 block_on_async（普通测试线程 blocking_lock 不可靠）
            block_on_async(async {
                let db = host_ctx.database().lock().await;
                assert_eq!(
                    db.get_setting("pairing_code_ttl").unwrap().as_deref(),
                    Some("900"),
                    "TTL 写入 settings 表且重启后稳定"
                );
            });
        }
    }

    // ==================== 白名单 / 自报三件一致（与 crypto 及 port 域样板同款） ====================

    /// 白名单声明、接口路径、权限位三件与本域常量逐字一致
    ///
    /// `expected_host_modules()` 含本模块名即证明 `expect_host_module!` 行仍在且被收集
    /// （漏了 ⇒ 装载期 unlisted 点名；本用例让它在单测就红）。
    #[test]
    fn host_module_declaration_matches_domain_constants() {
        assert!(
            bedcode_host_kit::expected_host_modules().contains(&MODULE_NAME),
            "能力模块白名单缺 {MODULE_NAME}（expect_host_module! 行被删 / 未收集）"
        );
        assert_eq!(MODULE_NAME, "auth", "白名单键即装载期日志与错误文案里的模块名");
        assert_eq!(
            MODULE_INTERFACES,
            &["bedcode:plugin/host-auth"],
            "接口路径必须与 WIT 契约逐字一致（改错即 guest import 失配）"
        );
        assert_eq!(
            MODULE_PERMISSIONS,
            &["auth"],
            "权限位必须与 `bedcode.wit` / SDK 权限表逐字一致（全域单权限位）"
        );
    }
}
