//! 密钥托管域宿主实现（v15 host-auth / secret-store）
//!
//! 属主隔离：属主 = 调用方插件实例的 plugin_id（由 wasm_runtime::component 的
//! Host trait 绑定传入，guest 无法伪造/旁路）；不同插件天然隔离，越权访问
//! 其它命名空间在 SQL 层即无命中（权限门之外的第二道闸）。
//!
//! 真源 = 主库 `plugin_secrets` 表（schema.sql，重启持久化）；内存缓存为
//! read-through（get 命中直接返回，set/delete 失效对应键）。
//!
//! 明文不落日志红线（AGENTS.md §8）：本模块日志只记 `value.len()`，禁止打印
//! 值本身；错误消息不含值内容。

#[cfg(test)]
use crate::host_api::context::WasmHostContext;
use crate::permission::PERMISSION_AUTH;
use crate::runtime_util::block_on_async;
use chrono::Utc;
use rusqlite::OptionalExtension;

/// **测试夹具**：以宿主身份往某插件属主的 secret-store 写一个键
///
/// 只为 `utils::auth::test_tokens::seed_keyring` 服务——那里需要把认证中心的
/// 入场密钥环种成已知密钥，才能在**不自造 JWT 密码学**的前提下签出中心认的
/// token（ADR 0033 后宿主没有签发面）。走真实的 `auth_secret_set` 实现
/// （含权限门与明文不落日志），而不是直接写库——夹具走的是真路径。
/// 整核抽出：test_tokens 迁入本 crate 后需常编译（lib 集成测试消费），故本夹具
/// 不带 `#[cfg(test)]`。
pub fn test_seed_plugin_secret(
    db: &dyn crate::host_api::context::DbScope,
    secrets: &dyn crate::host_api::context::SecretsScope,
    perm: &dyn crate::host_api::context::PermissionScope,
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
    db: &dyn crate::host_api::context::DbScope,
    secrets: &dyn crate::host_api::context::SecretsScope,
    perm: &dyn crate::host_api::context::PermissionScope,
    plugin_id: &str,
    key: &str,
) -> Result<Option<String>, String> {
    if !super::check_permission(perm, plugin_id, PERMISSION_AUTH, "host_auth_secret_get") {
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
    db: &dyn crate::host_api::context::DbScope,
    secrets: &dyn crate::host_api::context::SecretsScope,
    perm: &dyn crate::host_api::context::PermissionScope,
    plugin_id: &str,
    key: &str,
    value: &str,
) -> Result<(), String> {
    if !super::check_permission(perm, plugin_id, PERMISSION_AUTH, "host_auth_secret_set") {
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
    db: &dyn crate::host_api::context::DbScope,
    secrets: &dyn crate::host_api::context::SecretsScope,
    perm: &dyn crate::host_api::context::PermissionScope,
    plugin_id: &str,
    key: &str,
) -> Result<(), String> {
    if !super::check_permission(perm, plugin_id, PERMISSION_AUTH, "host_auth_secret_delete") {
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
    db: &dyn crate::host_api::context::DbScope,
    perm: &dyn crate::host_api::context::PermissionScope,
    plugin_id: &str,
) -> Result<Vec<String>, String> {
    if !super::check_permission(perm, plugin_id, PERMISSION_AUTH, "host_auth_secret_keys") {
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
// 认证记录归认证中心插件私有库。原记录面（trusted-devices-list /
// trusted-device-revoke / connection-history-list / connection-history-clear /
// trusted-device-upsert / trusted-device-touch / connection-history-record）
// 随 WIT 删除；生物凭证原语（biometric-*）保留，公钥托管改挂 `plugin_secrets`
// （见下方 v19 保留面注释）。settings 表保留（配置域，认证中心 TTL 读取依赖）。

/// 认证域设置项写入（键白名单 + 值校验；读取走宿主配置面）
///
/// 校验在 Rust 端（最终仲裁）：键必须落在 [`AUTH_SETTING_KEYS`]，值必须是正整数
/// 十进制秒数——非法值显性报错，不写入半合法数据。
pub(crate) fn auth_setting_set(
    db: &dyn crate::host_api::context::DbScope,
    perm: &dyn crate::host_api::context::PermissionScope,
    plugin_id: &str,
    key: &str,
    value: &str,
) -> Result<(), String> {
    if !super::check_permission(perm, plugin_id, PERMISSION_AUTH, "host_auth_setting_set") {
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
//
// v24（2026-09-22 用户裁定「认证记录下沉」）：主库 `pairings` / `connection_history`
// 表退役，认证记录归认证中心（`com.bedcode.terminal-session`）私有库经互调 api
// 服务。原记录面写原语（trusted-device-upsert / trusted-device-touch /
// connection-history-record）与删原语（v18 revoked / history-clear）随 WIT 删除。
// 本组保留的是**凭据与密码学面**：链路身份 Kd 公钥材料读取（link-identity-parts）。
// **生物凭证面（原 biometric-* 三原语）已随 v34 退役**（B-downsink）：公钥托管与
// 验签执行下沉认证中心插件私有库（`auth_records::biometric_key_*` + WASM 内
// p256），宿主不再托管任何设备侧凭证材料。

/// 链路身份 Kd 公钥材料读取（`link_crypto::identity_parts` 语义）；未就绪 → None
pub(crate) fn auth_link_identity_parts(
    perm: &dyn crate::host_api::context::PermissionScope,
    plugin_id: &str,
) -> Result<Option<String>, String> {
    if !super::check_permission(perm, plugin_id, PERMISSION_AUTH, "host_auth_link_identity_parts") {
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
// 单中心注册表本体 + 唯一性仲裁 + 停用回收在 `host_api/auth_center.rs`；本面只做
// 权限门（全部复用既有 `auth` 权限位，K8）+ 对注册表的薄分派。`auth-method-invoke`
// 是**零解析窄转发**（ADR 0032 L2 红线③）：宿主不拆 `params`、不解释 `method` 的
// 业务含义，只校验「method 在注册表内」（安全闸门判据，不是解释，B1 不命中），
// 把调用转发到认证中心的 `auth-grant` 互调 api 并原样透回。

/// 注册本插件为认证中心（K1；单中心仲裁 K4 在注册表内，重复注册标点名在册属主）
pub(crate) fn auth_center_register(
    perm: &dyn crate::host_api::context::PermissionScope,
    plugin_id: &str,
    methods: Vec<String>,
) -> Result<String, String> {
    if !super::check_permission(perm, plugin_id, PERMISSION_AUTH, "host_auth_center_register") {
        return Err("permission denied".to_string());
    }
    crate::host_api::auth_center::register(plugin_id, methods)
}

/// 注销本插件的认证中心角色（仅属主本人；无中心在册幂等成功）
pub(crate) fn auth_center_unregister(
    perm: &dyn crate::host_api::context::PermissionScope,
    plugin_id: &str,
) -> Result<(), String> {
    if !super::check_permission(perm, plugin_id, PERMISSION_AUTH, "host_auth_center_unregister") {
        return Err("permission denied".to_string());
    }
    crate::host_api::auth_center::unregister(plugin_id)
}

/// 列取当前认证中心登记的认证方式（组合式认证的发现端，K6；无中心 fail-closed）
pub(crate) fn auth_methods_list(
    perm: &dyn crate::host_api::context::PermissionScope,
    plugin_id: &str,
) -> Result<Vec<String>, String> {
    if !super::check_permission(perm, plugin_id, PERMISSION_AUTH, "host_auth_methods_list") {
        return Err("permission denied".to_string());
    }
    let Some(entry) = crate::host_api::auth_center::center() else {
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
    perm: &dyn crate::host_api::context::PermissionScope,
    host_ctx: &crate::host_api::context::WasmHostContext,
    plugin_id: &str,
    method: &str,
    params: &str,
) -> Result<String, String> {
    if !super::check_permission(perm, plugin_id, PERMISSION_AUTH, "host_auth_method_invoke") {
        return Err("permission denied".to_string());
    }
    crate::host_api::auth_center::invoke_auth_method(host_ctx, method, params)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host_api::tests::build_host_ctx;
    use crate::host_api::grant_permissions;
    use std::sync::Arc;

    /// 文件后备库宿主上下文（重启持久化测试用；构造路径与 host_impl::tests::build_host_ctx 同构）
    fn file_host_ctx(db_path: &std::path::Path) -> Arc<WasmHostContext> {
        let db = crate::db::Database::new(db_path).expect("open db");
        db.init_schema().expect("init schema（含 plugin_secrets）");
        let db = Arc::new(tokio::sync::Mutex::new(db));
        let storage = Arc::new(crate::storage::PluginStorage::new(db.clone()));
        let fs_auth = Arc::new(crate::security::fs_auth::FsAuthChecker::new(
            storage.clone(),
            None,
        ));
        Arc::new(WasmHostContext::new(
            db,
            Arc::new(tokio::sync::Mutex::new(Default::default())),
            storage,
            None,
            Arc::new(crate::permission::PermissionManager::new()),
            fs_auth,
            Arc::new(crate::bus::MessageBus::new()),
            crate::manager::capability::test_registry(),
        ))
    }

    #[test]
    fn test_set_get_roundtrip_and_overwrite() {
        let host_ctx = build_host_ctx();
        grant_permissions(
            &host_ctx,
            "com.bedcode.test-a",
            &[crate::permission::PERMISSION_AUTH],
        );
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
        let host_ctx = build_host_ctx();
        grant_permissions(
            &host_ctx,
            "com.bedcode.test-a",
            &[crate::permission::PERMISSION_AUTH],
        );
        grant_permissions(
            &host_ctx,
            "com.bedcode.test-b",
            &[crate::permission::PERMISSION_AUTH],
        );
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
        let host_ctx = build_host_ctx();
        grant_permissions(
            &host_ctx,
            "com.bedcode.test-a",
            &[crate::permission::PERMISSION_AUTH],
        );
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
        let host_ctx = build_host_ctx();
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
            grant_permissions(
                &host_ctx,
                "com.bedcode.test-a",
                &[crate::permission::PERMISSION_AUTH],
            );
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
            grant_permissions(
                &host_ctx,
                "com.bedcode.test-a",
                &[crate::permission::PERMISSION_AUTH],
            );
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
    // 生物凭证公钥托管 + 验签执行已随 v34 下沉认证中心插件私有库
    // （`auth_records::biometric_key_*` + WASM 内 p256），宿主不再托管任何
    // 设备侧凭证材料。原 `test_biometric_credential_bind_bound_unbind_roundtrip`
    // 与 `test_biometric_verify_signature_uses_host_managed_key` 一并删除。

    /// 设置写入：白名单 + 正整数校验；合法值落内核 settings 表（宿主命令面据此取 TTL）
    #[test]
    fn test_auth_setting_set_whitelist_and_persist() {
        let host_ctx = build_host_ctx();
        grant_permissions(
            &host_ctx,
            "com.bedcode.terminal-session",
            &[crate::permission::PERMISSION_AUTH],
        );

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
        // 锁经 block_on_async（普通测试线程 blocking_lock 不可靠，见 biometric roundtrip）
        block_on_async(async {
            let db = host_ctx.db.lock().await;
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
            grant_permissions(
                &host_ctx,
                "com.bedcode.terminal-session",
                &[crate::permission::PERMISSION_AUTH],
            );
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
            // 锁经 block_on_async（普通测试线程 blocking_lock 不可靠，见 biometric roundtrip）
            block_on_async(async {
                let db = host_ctx.db.lock().await;
                assert_eq!(
                    db.get_setting("pairing_code_ttl").unwrap().as_deref(),
                    Some("900"),
                    "TTL 写入内核 settings 表且重启后稳定"
                );
            });
        }
    }
}
