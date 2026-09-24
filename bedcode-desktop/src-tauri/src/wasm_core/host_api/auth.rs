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

use crate::wasm_core::host_api::context::WasmHostContext;
use crate::wasm_core::runtime_util::block_on_async;
use crate::wasm_core::permission::PERMISSION_AUTH;
use chrono::Utc;
use rusqlite::OptionalExtension;

/// 认证域设置项写侧白名单（v18 `auth-setting-set`）
///
/// `settings` 表是宿主真源（TTL 由宿主命令面读取后传入插件），插件只能写这一域
/// 的两个 TTL 键——白名单外一律拒绝，避免「有 auth 权限即可改任意宿主设置」。
pub(crate) const AUTH_SETTING_KEYS: &[&str] = &["pairing_code_ttl", "qr_token_ttl"];

/// 读取属主密钥（权限门 + 内存缓存 read-through + 主库真源）
pub(crate) fn auth_secret_get(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    key: &str,
) -> Result<Option<String>, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_AUTH, "host_auth_secret_get") {
        return Err("permission denied".to_string());
    }
    // 缓存读
    let cache = host_ctx.secrets_cache.clone();
    {
        let guard = cache.read().map_err(|e| format!("secret cache poisoned: {}", e))?;
        if let Some(v) = guard.get(&(plugin_id.to_string(), key.to_string())) {
            return Ok(Some(v.clone()));
        }
    }
    // 主库读
    let db = host_ctx.db.clone();
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
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    key: &str,
    value: &str,
) -> Result<(), String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_AUTH, "host_auth_secret_set") {
        return Err("permission denied".to_string());
    }
    // 明文不落日志：只记长度
    tracing::info!(
        plugin_id = %plugin_id,
        key = %key,
        value_len = value.len(),
        "host_auth_secret_set: secret stored (length only)"
    );
    let db = host_ctx.db.clone();
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
    host_ctx
        .secrets_cache
        .write()
        .map_err(|e| format!("secret cache poisoned: {}", e))?
        .remove(&(plugin_id.to_string(), key.to_string()));
    Ok(())
}

/// 删除属主密钥（键不存在也视为成功；缓存同步移除）
pub(crate) fn auth_secret_delete(host_ctx: &WasmHostContext, plugin_id: &str, key: &str) -> Result<(), String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_AUTH, "host_auth_secret_delete") {
        return Err("permission denied".to_string());
    }
    let db = host_ctx.db.clone();
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
    host_ctx
        .secrets_cache
        .write()
        .map_err(|e| format!("secret cache poisoned: {}", e))?
        .remove(&(plugin_id.to_string(), key.to_string()));
    Ok(())
}

/// 列举属主密钥名（不返回值本身，供诊断/清理）
pub(crate) fn auth_secret_keys(host_ctx: &WasmHostContext, plugin_id: &str) -> Result<Vec<String>, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_AUTH, "host_auth_secret_keys") {
        return Err("permission denied".to_string());
    }
    let db = host_ctx.db.clone();
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
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    key: &str,
    value: &str,
) -> Result<(), String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_AUTH, "host_auth_setting_set") {
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
    let db = host_ctx.db.clone();
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
// 本组保留的是**凭据与密码学面**：生物凭证公钥托管 + 验签执行在宿主（§8 红线），
// 公钥存 `plugin_secrets`（属主键下，key = `biometric:<fingerprint>`）；配对状态
// 判定由认证中心私有库负责（插件侧先查自己库）。

/// 「已绑定生物凭证公钥」查询（挑战签发闸门之一）：只查宿主托管公钥存在性
/// （`plugin_secrets` 中该指纹键非空）——配对状态由认证中心私有库判定（插件侧先
/// 查自己库）。公钥不出口（凭据红线）
pub(crate) fn auth_biometric_credential_bound(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    fingerprint: &str,
) -> Result<bool, String> {
    if !super::check_permission(
        host_ctx,
        plugin_id,
        PERMISSION_AUTH,
        "host_auth_biometric_credential_bound",
    ) {
        return Err("permission denied".to_string());
    }
    let secret_key = biometric_secret_key(fingerprint);
    auth_secret_get(host_ctx, plugin_id, &secret_key)
        .map(|v| v.map(|s| !s.is_empty()).unwrap_or(false))
}

/// 生物认证签名验证（P-256 ECDSA，SPKI DER base64 公钥 + r||s base64 签名）：
/// 用宿主托管的绑定公钥验 `message`；未配对 / 未绑定 → `Ok(false)`。
/// 验签执行点在宿主，密钥与公钥不出宿主（凭据红线）。
/// 生物凭证公钥在宿主 `plugin_secrets` 中的属主键（v24 认证记录下沉：公钥
/// 不随 pairings 入插件库，§8 凭据红线留宿主托管）
///
/// key 形状 `biometric:<fingerprint>`——指纹是公开记录面字段，不构成泄露；
/// 值（SPKI DER base64 公钥）本身不出宿主。
fn biometric_secret_key(fingerprint: &str) -> String {
    format!("biometric:{fingerprint}")
}

/// 生物认证签名验证（P-256 ECDSA，SPKI DER base64 公钥 + r||s base64 签名）：
/// 用宿主托管的绑定公钥（`plugin_secrets`）验 `message`；未绑定 → `Ok(false)`。
/// 验签执行点在宿主，密钥与公钥不出宿主（凭据红线）。
pub(crate) fn auth_biometric_verify_signature(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    fingerprint: &str,
    message: &str,
    signature: &str,
) -> Result<bool, String> {
    if !super::check_permission(
        host_ctx,
        plugin_id,
        PERMISSION_AUTH,
        "host_auth_biometric_verify_signature",
    ) {
        return Err("permission denied".to_string());
    }
    let secret_key = biometric_secret_key(fingerprint);
    let Some(public_key) = auth_secret_get(host_ctx, plugin_id, &secret_key)? else {
        return Ok(false); // 未绑定公钥
    };
    let verified = crate::utils::auth::biometric::verify_biometric_signature(
        &public_key,
        message,
        signature,
    );
    Ok(verified.is_ok())
}

/// 绑定/解绑生物凭证公钥（宿主托管语义：**只改凭证不动 connect_count /
/// last_seen**——与认证登录路径的计数语义刻意不同）；`public_key` 空串 = 解绑。
/// 未找到配对记录返回 `Ok(false)`；成功 `Ok(true)`（配对状态由认证中心私有库
/// 判定，本原语不再触碰配对记录）。
pub(crate) fn auth_biometric_credential_bind(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    fingerprint: &str,
    public_key: &str,
) -> Result<bool, String> {
    if !super::check_permission(
        host_ctx,
        plugin_id,
        PERMISSION_AUTH,
        "host_auth_biometric_credential_bind",
    ) {
        return Err("permission denied".to_string());
    }
    let secret_key = biometric_secret_key(fingerprint);
    if public_key.is_empty() {
        // 解绑：删除托管公钥（键不存在也视为成功，与 secret-delete 同语义）
        auth_secret_delete(host_ctx, plugin_id, &secret_key)?;
        return Ok(true);
    }
    auth_secret_set(host_ctx, plugin_id, &secret_key, public_key)?;
    tracing::info!(
        plugin_id = %plugin_id,
        public_key_len = public_key.len(),
        "host_auth_biometric_credential_bind: public key bound (length only)"
    );
    Ok(true)
}

/// 设备认证 JWT 签发（宿主 `JwtService` 同一代码路径：密钥托管在 secret-store，
/// 签发执行点留宿主——插件编排、宿主签发，密钥不出宿主）
pub(crate) fn auth_device_token_issue(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    sub: &str,
    device_name: &str,
    fingerprint: &str,
) -> Result<String, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_AUTH, "host_auth_device_token_issue") {
        return Err("permission denied".to_string());
    }
    if sub.is_empty() {
        return Err("auth error: empty subject".to_string());
    }
    let jwt = crate::utils::auth::jwt::JwtService::new();
    jwt.generate_token(
        sub.to_string(),
        (!device_name.is_empty()).then(|| device_name.to_string()),
        (!fingerprint.is_empty()).then(|| fingerprint.to_string()),
    )
    .map_err(|e| format!("auth error: jwt issue failed: {}", e))
}

/// 设备认证 JWT 验签（`verify_token_with_expiry` 语义）。错误归类：
/// `JwtError::TokenExpired` → "expired"；其余 → "invalid"（用户文案映射归插件）。
pub(crate) fn auth_device_token_verify(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    token: &str,
) -> Result<String, String> {
    use crate::utils::auth::jwt::JwtError;
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_AUTH, "host_auth_device_token_verify") {
        return Err("permission denied".to_string());
    }
    let jwt = crate::utils::auth::jwt::JwtService::new();
    match jwt.verify_token_with_expiry(token) {
        Ok(claims) => serde_json::to_string(&claims).map_err(|e| format!("auth error: claims serialize failed: {}", e)),
        Err(e) => Err(match e {
            JwtError::TokenExpired => "expired".to_string(),
            _ => "invalid".to_string(),
        }),
    }
}

/// 链路身份 Kd 公钥材料读取（`link_crypto::identity_parts` 语义）；未就绪 → None
pub(crate) fn auth_link_identity_parts(host_ctx: &WasmHostContext, plugin_id: &str) -> Result<Option<String>, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_AUTH, "host_auth_link_identity_parts") {
        return Err("permission denied".to_string());
    }
    match crate::server::core::link_crypto::identity_parts() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wasm_core::host_api::tests::{build_host_ctx, grant_permissions};
    use std::sync::Arc;

    /// 文件后备库宿主上下文（重启持久化测试用；构造路径与 host_impl::tests::build_host_ctx 同构）
    fn file_host_ctx(db_path: &std::path::Path) -> Arc<WasmHostContext> {
        let db = crate::db::Database::new(db_path).expect("open db");
        db.init_schema().expect("init schema（含 plugin_secrets）");
        let db = Arc::new(tokio::sync::Mutex::new(db));
        let storage = Arc::new(crate::wasm_core::storage::PluginStorage::new(db.clone()));
        let fs_auth = Arc::new(crate::wasm_core::security::fs_auth::FsAuthChecker::new(
            storage.clone(),
            None,
        ));
        Arc::new(WasmHostContext::new(
            db,
            Arc::new(tokio::sync::Mutex::new(Default::default())),
            storage,
            None,
            Arc::new(crate::wasm_core::permission::PermissionManager::new()),
            fs_auth,
            Arc::new(crate::wasm_core::bus::MessageBus::new()),
            crate::wasm_core::manager::capability::test_registry(),
        ))
    }

    #[test]
    fn test_set_get_roundtrip_and_overwrite() {
        let host_ctx = build_host_ctx();
        grant_permissions(
            &host_ctx,
            "com.bedcode.test-a",
            &[crate::wasm_core::permission::PERMISSION_AUTH],
        );
        assert_eq!(
            auth_secret_get(&host_ctx, "com.bedcode.test-a", "jwt.key").unwrap(),
            None
        );
        auth_secret_set(&host_ctx, "com.bedcode.test-a", "jwt.key", "secret-v1").unwrap();
        assert_eq!(
            auth_secret_get(&host_ctx, "com.bedcode.test-a", "jwt.key")
                .unwrap()
                .as_deref(),
            Some("secret-v1")
        );
        // 覆盖写
        auth_secret_set(&host_ctx, "com.bedcode.test-a", "jwt.key", "secret-v2").unwrap();
        assert_eq!(
            auth_secret_get(&host_ctx, "com.bedcode.test-a", "jwt.key")
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
            &[crate::wasm_core::permission::PERMISSION_AUTH],
        );
        grant_permissions(
            &host_ctx,
            "com.bedcode.test-b",
            &[crate::wasm_core::permission::PERMISSION_AUTH],
        );
        auth_secret_set(&host_ctx, "com.bedcode.test-a", "seed", "a-secret").unwrap();
        // B 同名 key 读不到 A 的值（命名空间隔离）
        assert_eq!(auth_secret_get(&host_ctx, "com.bedcode.test-b", "seed").unwrap(), None);
        // B 删不掉 A 的密钥
        auth_secret_delete(&host_ctx, "com.bedcode.test-b", "seed").unwrap();
        assert_eq!(
            auth_secret_get(&host_ctx, "com.bedcode.test-a", "seed")
                .unwrap()
                .as_deref(),
            Some("a-secret")
        );
        // B 写同名 key 不覆盖 A
        auth_secret_set(&host_ctx, "com.bedcode.test-b", "seed", "b-secret").unwrap();
        assert_eq!(
            auth_secret_get(&host_ctx, "com.bedcode.test-a", "seed")
                .unwrap()
                .as_deref(),
            Some("a-secret")
        );
        assert_eq!(
            auth_secret_get(&host_ctx, "com.bedcode.test-b", "seed")
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
            &[crate::wasm_core::permission::PERMISSION_AUTH],
        );
        auth_secret_set(&host_ctx, "com.bedcode.test-a", "k1", "v1").unwrap();
        auth_secret_set(&host_ctx, "com.bedcode.test-a", "k2", "v2").unwrap();
        let keys = auth_secret_keys(&host_ctx, "com.bedcode.test-a").unwrap();
        assert_eq!(keys, vec!["k1".to_string(), "k2".to_string()]);
        auth_secret_delete(&host_ctx, "com.bedcode.test-a", "k1").unwrap();
        // 幂等删除
        auth_secret_delete(&host_ctx, "com.bedcode.test-a", "k1").unwrap();
        assert_eq!(auth_secret_get(&host_ctx, "com.bedcode.test-a", "k1").unwrap(), None);
        assert_eq!(
            auth_secret_keys(&host_ctx, "com.bedcode.test-a").unwrap(),
            vec!["k2".to_string()]
        );
    }

    #[test]
    fn test_permission_denied_without_auth_grant() {
        let host_ctx = build_host_ctx();
        // 未授权 auth 权限的插件 → 全部四函数拒绝（Rust 端最终仲裁）
        assert_eq!(
            auth_secret_get(&host_ctx, "com.bedcode.no-auth", "k").unwrap_err(),
            "permission denied"
        );
        assert_eq!(
            auth_secret_set(&host_ctx, "com.bedcode.no-auth", "k", "v").unwrap_err(),
            "permission denied"
        );
        assert_eq!(
            auth_secret_delete(&host_ctx, "com.bedcode.no-auth", "k").unwrap_err(),
            "permission denied"
        );
        assert_eq!(
            auth_secret_keys(&host_ctx, "com.bedcode.no-auth").unwrap_err(),
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
                &[crate::wasm_core::permission::PERMISSION_AUTH],
            );
            auth_secret_set(&host_ctx, "com.bedcode.test-a", "jwt.key", "persisted-secret").unwrap();
        }
        // 第二代上下文（全新内存缓存 + 全新连接）：重启后密钥稳定
        {
            let host_ctx = file_host_ctx(&db_path);
            grant_permissions(
                &host_ctx,
                "com.bedcode.test-a",
                &[crate::wasm_core::permission::PERMISSION_AUTH],
            );
            assert_eq!(
                auth_secret_get(&host_ctx, "com.bedcode.test-a", "jwt.key")
                    .unwrap()
                    .as_deref(),
                Some("persisted-secret")
            );
        }
    }

    // ==================== v24：biometric 公钥托管（plugin_secrets）语义 ====================

    /// 绑定公钥 → `biometric:<fingerprint>` 键落 plugin_secrets；bound 判定读回；
    /// 解绑（空串）删除键。验签执行点不变（见 verify 函数），公钥不出宿主。
    #[test]
    fn test_biometric_credential_bind_bound_unbind_roundtrip() {
        let host_ctx = build_host_ctx();
        grant_permissions(
            &host_ctx,
            "com.bedcode.terminal-session",
            &[crate::wasm_core::permission::PERMISSION_AUTH],
        );

        // 未绑定：bound = false
        assert!(
            !auth_biometric_credential_bound(&host_ctx, "com.bedcode.terminal-session", "fp-1").unwrap()
        );

        // 绑定：落 plugin_secrets（属主键 + biometric:<fp>），值可读回
        assert!(
            auth_biometric_credential_bind(&host_ctx, "com.bedcode.terminal-session", "fp-1", "SPKI-BASE64")
                .unwrap()
        );
        assert!(
            auth_biometric_credential_bound(&host_ctx, "com.bedcode.terminal-session", "fp-1").unwrap(),
            "绑定后 bound = true（公钥托管存在性）"
        );
        // 锁经 block_on_async 获取（普通测试线程无 runtime 上下文，tokio
        // Mutex::blocking_lock 的 CachedParkThread 路径在此环境不可靠——实证
        // 死锁；统一走 ambient runtime）
        let stored: Option<String> = block_on_async(async {
            let db = host_ctx.db.lock().await;
            db.conn()
                .query_row(
                    "SELECT value FROM plugin_secrets WHERE plugin_id = ?1 AND key = ?2",
                    rusqlite::params!["com.bedcode.terminal-session", "biometric:fp-1"],
                    |row| row.get(0),
                )
                .optional()
                .expect("query")
        });
        assert_eq!(stored.as_deref(), Some("SPKI-BASE64"), "公钥托管在 plugin_secrets");

        // 解绑（空串）：删除键
        assert!(
            auth_biometric_credential_bind(&host_ctx, "com.bedcode.terminal-session", "fp-1", "").unwrap()
        );
        assert!(
            !auth_biometric_credential_bound(&host_ctx, "com.bedcode.terminal-session", "fp-1").unwrap(),
            "解绑后 bound = false"
        );
    }

    /// 验签：无绑定公钥 → false；绑定后 → 真实 P-256 验签结果（验签执行点在宿主）
    #[test]
    fn test_biometric_verify_signature_uses_host_managed_key() {
        let host_ctx = build_host_ctx();
        grant_permissions(
            &host_ctx,
            "com.bedcode.terminal-session",
            &[crate::wasm_core::permission::PERMISSION_AUTH],
        );

        // 未绑定：直接 false（不触碰验签）
        assert!(
            !auth_biometric_verify_signature(&host_ctx, "com.bedcode.terminal-session", "fp-1", "msg", "sig")
                .unwrap()
        );

        // 造一对 P-256 密钥对：绑定公钥 → 验签成功；篡改消息/签名 → 失败
        use base64::Engine;
        use p256::ecdsa::signature::Signer;
        use p256::ecdsa::SigningKey;
        use p256::pkcs8::EncodePublicKey;
        let signing_key = SigningKey::random(&mut rand::thread_rng());
        let spki_b64 = base64::engine::general_purpose::STANDARD.encode(
            signing_key
                .verifying_key()
                .to_public_key_der()
                .expect("encode public key")
                .as_bytes(),
        );
        let message = "biometric-verify-test";
        let signature: p256::ecdsa::Signature = signing_key.sign(message.as_bytes());
        let (r, s) = signature.split_scalars();
        let mut raw = r.to_bytes().to_vec();
        raw.extend_from_slice(&s.to_bytes());
        let sig_b64 = base64::engine::general_purpose::STANDARD.encode(&raw);

        auth_biometric_credential_bind(
            &host_ctx,
            "com.bedcode.terminal-session",
            "fp-1",
            &spki_b64,
        )
        .unwrap();
        assert!(
            auth_biometric_verify_signature(
                &host_ctx,
                "com.bedcode.terminal-session",
                "fp-1",
                message,
                &sig_b64,
            )
            .unwrap(),
            "正确签名 + 宿主托管公钥 → 验签通过"
        );
        assert!(
            !auth_biometric_verify_signature(
                &host_ctx,
                "com.bedcode.terminal-session",
                "fp-1",
                "tampered",
                &sig_b64,
            )
            .unwrap(),
            "篡改消息 → 验签失败"
        );
    }

    /// 设置写入：白名单 + 正整数校验；合法值落内核 settings 表（宿主命令面据此取 TTL）
    #[test]
    fn test_auth_setting_set_whitelist_and_persist() {
        let host_ctx = build_host_ctx();
        grant_permissions(
            &host_ctx,
            "com.bedcode.terminal-session",
            &[crate::wasm_core::permission::PERMISSION_AUTH],
        );

        // 白名单外键拒绝（settings 表是宿主真源，不接受任意键写入）
        let err = auth_setting_set(&host_ctx, "com.bedcode.terminal-session", "network.port", "1").unwrap_err();
        assert!(err.contains("not in auth domain whitelist"), "got: {err}");
        // 非正整数拒绝（0 / 负数 / 非数字均不写入）
        for bad in ["0", "-1", "abc", ""] {
            let err = auth_setting_set(&host_ctx, "com.bedcode.terminal-session", "pairing_code_ttl", bad).unwrap_err();
            assert!(err.contains("positive integer"), "值 {bad:?} 必须拒绝, got: {err}");
        }

        auth_setting_set(&host_ctx, "com.bedcode.terminal-session", "pairing_code_ttl", "600").unwrap();
        auth_setting_set(&host_ctx, "com.bedcode.terminal-session", "qr_token_ttl", "120").unwrap();
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
                &[crate::wasm_core::permission::PERMISSION_AUTH],
            );
            auth_setting_set(&host_ctx, "com.bedcode.terminal-session", "pairing_code_ttl", "900").unwrap();
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
