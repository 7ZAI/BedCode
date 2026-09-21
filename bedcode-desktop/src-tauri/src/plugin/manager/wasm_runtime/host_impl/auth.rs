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

use crate::plugin::manager::wasm_runtime::{block_on_async, WasmHostContext};
use crate::plugin::permission::PERMISSION_AUTH;
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

// ==================== v18 认证记录面（只读 / 撤销 + 认证域设置写入） ====================
//
// 真源 = 内核表（`pairings` / `connection_history` / `settings`，spec D3「不动」表）；
// 宿主只做「读原始记录 / 软删 / 白名单设置写入」，排序、active 过滤与展示组织归插件。
// 凭据红线（AGENTS.md §8）：`pairings.session_token` / `public_key` 不出内核，
// 记录 JSON 只含公开视图列。

/// 已配对设备原始记录（JSON 数组；含软删行，不排序）
///
/// 全量返回含 `is_active = 0` 的行是刻意的：撤销检测（policy 的信任策略）依赖
/// 「已撤销记录仍可见」——只回活跃集合会让「已撤销」与「从未配对」不可区分，
/// 撤销判定退化为 fail-open。凭据列（session_token / public_key）不出内核。
pub(crate) fn auth_trusted_devices_list(host_ctx: &WasmHostContext, plugin_id: &str) -> Result<String, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_AUTH, "host_auth_trusted_devices_list") {
        return Err("permission denied".to_string());
    }
    let db = host_ctx.db.clone();
    let rows = block_on_async(async move {
        let db = db.lock().await;
        let mut stmt = db
            .conn()
            .prepare(
                "SELECT id, device_name, device_fingerprint, address, paired_at, last_seen, connect_count, is_active \
                 FROM pairings",
            )
            .map_err(|e| format!("database error: {}", e))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(serde_json::json!({
                    "id": row.get::<_, String>(0)?,
                    "deviceName": row.get::<_, String>(1)?,
                    "deviceFingerprint": row.get::<_, String>(2)?,
                    "address": row.get::<_, Option<String>>(3)?,
                    "pairedAt": row.get::<_, String>(4)?,
                    "lastSeen": row.get::<_, Option<String>>(5)?,
                    "connectCount": row.get::<_, i32>(6)?,
                    "isActive": row.get::<_, i32>(7)? == 1,
                }))
            })
            .map_err(|e| format!("database error: {}", e))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| format!("database error: {}", e))
    })?;
    tracing::debug!(
        plugin_id = %plugin_id,
        count = rows.len(),
        "host_auth_trusted_devices_list: raw pairing records returned"
    );
    serde_json::to_string(&rows).map_err(|e| format!("trusted devices serialize: {}", e))
}

/// 撤销信任（软删 + 连带删除连接历史）；返回是否命中记录
///
/// 语义 = 内核 `remove_pairing`（`is_active = 0` 软删 + 删该设备连接历史），
/// 亦保持宿主现状「撤销不断开在线连接」（spec M5，属新协议工作不在本批次）。
/// 未知 id 幂等返回 `false`；已软删记录命中即 `true` 且不重复写。
pub(crate) fn auth_trusted_device_revoke(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    id: &str,
) -> Result<bool, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_AUTH, "host_auth_trusted_device_revoke") {
        return Err("permission denied".to_string());
    }
    let db = host_ctx.db.clone();
    let target = id.to_string();
    let removed = block_on_async(async move {
        let db = db.lock().await;
        let active: Option<i32> = db
            .conn()
            .query_row(
                "SELECT is_active FROM pairings WHERE id = ?1",
                rusqlite::params![target],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| format!("database error: {}", e))?;
        match active {
            None => Ok::<bool, String>(false),
            // 已软删：幂等命中，不重复写（与插件侧「已撤销记录再次撤销仍报 removed」一致）
            Some(0) => Ok(true),
            Some(_) => {
                db.remove_pairing(&target)
                    .map_err(|e| format!("remove pairing {}: {}", target, e))?;
                Ok(true)
            }
        }
    })?;
    tracing::info!(
        plugin_id = %plugin_id,
        device_id = %id,
        removed = removed,
        "host_auth_trusted_device_revoke: trust revoked"
    );
    Ok(removed)
}

/// 设备连接历史原始记录（JSON 数组；`device-id` = `pairings.id`）
pub(crate) fn auth_connection_history_list(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    device_id: &str,
) -> Result<String, String> {
    if !super::check_permission(
        host_ctx,
        plugin_id,
        PERMISSION_AUTH,
        "host_auth_connection_history_list",
    ) {
        return Err("permission denied".to_string());
    }
    let db = host_ctx.db.clone();
    let target = device_id.to_string();
    let history = block_on_async(async move {
        let db = db.lock().await;
        db.get_connection_history(&target)
            .map_err(|e| format!("database error: {}", e))
    })?;
    tracing::debug!(
        plugin_id = %plugin_id,
        device_id = %device_id,
        count = history.len(),
        "host_auth_connection_history_list: raw history returned"
    );
    serde_json::to_string(&history).map_err(|e| format!("connection history serialize: {}", e))
}

/// 清空某设备的连接历史（票 14）→ 是否命中了至少一条记录
///
/// 与 [`auth_trusted_device_revoke`] 的连带删除同语义：后者是撤销配对时的隐式清理，
/// 本函数是设备页「清空历史」的显式动作，**不影响配对状态**（不软删 `pairings`）。
/// 空历史幂等 `false`，不报错（重复点击不该成为错误路径）。
pub(crate) fn auth_connection_history_clear(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    device_id: &str,
) -> Result<bool, String> {
    if !super::check_permission(
        host_ctx,
        plugin_id,
        PERMISSION_AUTH,
        "host_auth_connection_history_clear",
    ) {
        return Err("permission denied".to_string());
    }
    let db = host_ctx.db.clone();
    let target = device_id.to_string();
    let removed = block_on_async(async move {
        let db = db.lock().await;
        // 先计数再删除：`delete_connection_history` 不返回行数，命中与否必须由
        // 「删除前是否有记录」判定（否则重复清空与从未有过历史不可区分）
        let existing = db
            .get_connection_history(&target)
            .map_err(|e| format!("database error: {}", e))?
            .len();
        if existing == 0 {
            return Ok::<usize, String>(0);
        }
        db.delete_connection_history(&target)
            .map_err(|e| format!("delete connection history {}: {}", target, e))?;
        Ok(existing)
    })?;
    tracing::info!(
        plugin_id = %plugin_id,
        device_id = %device_id,
        removed = removed,
        "host_auth_connection_history_clear: history cleared"
    );
    Ok(removed > 0)
}

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

// ==================== v19 函数级追加（票 07：认证链 HTTP 面下沉） ====================
//
// 认证执行编排移插件后的回调面：记录写入 / 查询与 P-256 验签，全部引擎级；
// `pairings` / `connection_history` 表与生物凭证公钥仍留宿主（凭据红线）。

/// 信任记录写入（内核 `add_pairing` 语义）→ 配对记录 id
///
/// `publicKey` 缺省 = 保留既有记录值（传空串会清掉生物凭证——缺省语义专防
/// 此坑；显式空串 = 显式解绑）；`uidHash` 命中存量设备时由内核复用原记录 id。
pub(crate) fn auth_trusted_device_upsert(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    record_json: &str,
) -> Result<String, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_AUTH, "host_auth_trusted_device_upsert") {
        return Err("permission denied".to_string());
    }
    let record: serde_json::Value =
        serde_json::from_str(record_json).map_err(|e| format!("auth error: invalid record json: {}", e))?;
    let device_name = record
        .get("deviceName")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "auth error: deviceName required".to_string())?
        .to_string();
    let fingerprint = record
        .get("fingerprint")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "auth error: fingerprint required".to_string())?
        .to_string();
    let address = record.get("address").and_then(|v| v.as_str()).map(str::to_string);
    let uid_hash = record.get("uidHash").and_then(|v| v.as_str()).map(str::to_string);

    let db = host_ctx.db.clone();
    let id = block_on_async(async move {
        let db = db.lock().await;
        // publicKey 缺省 = 保留既有值（无既有记录则空串——新配对本就无凭证）
        let public_key = match record.get("publicKey") {
            Some(serde_json::Value::String(s)) => s.clone(),
            _ => db
                .get_pairing_by_fingerprint(&fingerprint)
                .map_err(|e| format!("database error: {}", e))?
                .map(|p| p.public_key)
                .unwrap_or_default(),
        };
        db.add_pairing(
            &device_name,
            &fingerprint,
            &public_key,
            address.as_deref(),
            uid_hash.as_deref(),
        )
        .map_err(|e| format!("database error: {}", e))
    })?;
    tracing::info!(
        plugin_id = %plugin_id,
        pairing_id = %id,
        "host_auth_trusted_device_upsert: pairing record written"
    );
    Ok(id)
}

/// 连接计数 / last_seen 刷新（内核 `update_pairing_last_seen` 语义，不改设备名）
pub(crate) fn auth_trusted_device_touch(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    fingerprint: &str,
) -> Result<(), String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_AUTH, "host_auth_trusted_device_touch") {
        return Err("permission denied".to_string());
    }
    let db = host_ctx.db.clone();
    let fp = fingerprint.to_string();
    block_on_async(async move {
        let db = db.lock().await;
        db.update_pairing_last_seen(&fp, None)
            .map_err(|e| format!("database error: {}", e))
    })?;
    Ok(())
}

/// 连接历史追加（内核 `record_connection_event_by_fingerprint` 语义：指纹不存在
/// 时静默跳过——挑战签发失败等「未配对设备」路径依赖此语义，不报错不落库）
pub(crate) fn auth_connection_history_record(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    record_json: &str,
) -> Result<(), String> {
    if !super::check_permission(
        host_ctx,
        plugin_id,
        PERMISSION_AUTH,
        "host_auth_connection_history_record",
    ) {
        return Err("permission denied".to_string());
    }
    let record: serde_json::Value =
        serde_json::from_str(record_json).map_err(|e| format!("auth error: invalid record json: {}", e))?;
    let fingerprint = record
        .get("fingerprint")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "auth error: fingerprint required".to_string())?
        .to_string();
    let method = record
        .get("method")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "auth error: method required".to_string())?
        .to_string();
    let result = record
        .get("result")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "auth error: result required".to_string())?
        .to_string();
    let address = record.get("address").and_then(|v| v.as_str()).map(str::to_string);

    let db = host_ctx.db.clone();
    block_on_async(async move {
        let db = db.lock().await;
        db.record_connection_event_by_fingerprint(&fingerprint, &method, &result, address.as_deref())
            .map_err(|e| format!("database error: {}", e))
    })?;
    Ok(())
}

/// 「已配对且绑定生物凭证公钥」查询（挑战签发闸门）；公钥不出口（凭据红线）
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
    let db = host_ctx.db.clone();
    let fp = fingerprint.to_string();
    let bound = block_on_async(async move {
        let db = db.lock().await;
        db.get_pairing_by_fingerprint(&fp)
            .map_err(|e| format!("database error: {}", e))
            .map(|p| p.map(|p| p.is_active && !p.public_key.is_empty()).unwrap_or(false))
    })?;
    Ok(bound)
}

/// 生物认证签名验证（P-256 ECDSA，SPKI DER base64 公钥 + r||s base64 签名）：
/// 用宿主托管的绑定公钥验 `message`；未配对 / 未绑定 → `Ok(false)`。
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
    let db = host_ctx.db.clone();
    let fp = fingerprint.to_string();
    let msg = message.to_string();
    let sig = signature.to_string();
    let valid: bool = block_on_async(async move {
        let db = db.lock().await;
        let pairing = db
            .get_pairing_by_fingerprint(&fp)
            .map_err(|e| format!("database error: {}", e))?;
        match pairing {
            Some(p) if p.is_active && !p.public_key.is_empty() => {
                let verified: std::result::Result<(), crate::AppError> =
                    crate::utils::auth::biometric::verify_biometric_signature(&p.public_key, &msg, &sig);
                Ok::<bool, String>(verified.is_ok())
            }
            _ => Ok::<bool, String>(false),
        }
    })?;
    Ok(valid)
}

/// 绑定/解绑生物凭证公钥（内核 `update_pairing_public_key` 语义：只改凭证
/// 不动 connect_count / last_seen）；未配对 → `Ok(false)`
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
    let db = host_ctx.db.clone();
    let fp = fingerprint.to_string();
    let key = public_key.to_string();
    let updated = block_on_async(async move {
        let db = db.lock().await;
        let pairing = db
            .get_pairing_by_fingerprint(&fp)
            .map_err(|e| format!("database error: {}", e))?;
        let Some(pairing) = pairing else {
            return Ok(false);
        };
        db.update_pairing_public_key(&pairing.id, &key)
            .map_err(|e| format!("database error: {}", e))?;
        Ok::<bool, String>(true)
    })?;
    tracing::info!(plugin_id = %plugin_id, binding = %updated, "host_auth_biometric_credential_bind");
    Ok(updated)
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
    match crate::server::link_crypto::identity_parts() {
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
    use crate::plugin::manager::wasm_runtime::host_impl::tests::{build_host_ctx, grant_permissions};
    use crate::session::{SessionConfigManager, SessionManager};
    use std::sync::Arc;

    /// 文件后备库宿主上下文（重启持久化测试用；构造路径与 host_impl::tests::build_host_ctx 同构）
    fn file_host_ctx(db_path: &std::path::Path) -> Arc<WasmHostContext> {
        let db = crate::db::Database::new(db_path).expect("open db");
        db.init_schema().expect("init schema（含 plugin_secrets）");
        let db = Arc::new(tokio::sync::Mutex::new(db));
        let storage = Arc::new(crate::plugin::manager::storage::PluginStorage::new(db.clone()));
        let fs_auth = Arc::new(crate::plugin::security::fs_auth::FsAuthChecker::new(
            storage.clone(),
            None,
        ));
        let session_manager = Arc::new(SessionManager::default());
        let config_manager = Arc::new(SessionConfigManager::new(Arc::new(tokio::sync::Mutex::new({
            let db = crate::db::Database::new(&db_path.with_extension("config.db")).expect("config db");
            db.init_schema().expect("init schema");
            db
        }))));
        Arc::new(WasmHostContext::new(
            db,
            Arc::new(tokio::sync::Mutex::new(Default::default())),
            storage,
            session_manager,
            config_manager,
            None,
            Arc::new(crate::plugin::permission::PermissionManager::new()),
            fs_auth,
            Arc::new(crate::plugin::bus::MessageBus::new()),
        ))
    }

    #[test]
    fn test_set_get_roundtrip_and_overwrite() {
        let host_ctx = build_host_ctx();
        grant_permissions(
            &host_ctx,
            "com.bedcode.test-a",
            &[crate::plugin::permission::PERMISSION_AUTH],
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
            &[crate::plugin::permission::PERMISSION_AUTH],
        );
        grant_permissions(
            &host_ctx,
            "com.bedcode.test-b",
            &[crate::plugin::permission::PERMISSION_AUTH],
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
            &[crate::plugin::permission::PERMISSION_AUTH],
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
                &[crate::plugin::permission::PERMISSION_AUTH],
            );
            auth_secret_set(&host_ctx, "com.bedcode.test-a", "jwt.key", "persisted-secret").unwrap();
        }
        // 第二代上下文（全新内存缓存 + 全新连接）：重启后密钥稳定
        {
            let host_ctx = file_host_ctx(&db_path);
            grant_permissions(
                &host_ctx,
                "com.bedcode.test-a",
                &[crate::plugin::permission::PERMISSION_AUTH],
            );
            assert_eq!(
                auth_secret_get(&host_ctx, "com.bedcode.test-a", "jwt.key")
                    .unwrap()
                    .as_deref(),
                Some("persisted-secret")
            );
        }
    }

    // ==================== v18 认证记录面 ====================

    /// 造一条配对记录（直写内核表，模拟配对完成流；凭据列给值以证「不出内核」）
    fn seed_pairing(host_ctx: &WasmHostContext, id: &str, name: &str, fp: &str, paired_at: &str) {
        let db = host_ctx.db.clone();
        block_on_async(async move {
            let db = db.lock().await;
            db.conn()
                .execute(
                    "INSERT INTO pairings (id, device_name, device_fingerprint, public_key, address, \
                     paired_at, connect_count, is_active) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, 1)",
                    rusqlite::params![id, name, fp, "PUBLIC-KEY-SENTINEL", "192.168.1.9:8765", paired_at],
                )
                .expect("seed pairing");
        });
    }

    /// 记录面四函数的权限门：未授权 `auth` 一律拒绝（Rust 端最终仲裁）
    #[test]
    fn test_record_face_requires_auth_permission() {
        let host_ctx = build_host_ctx();
        assert_eq!(
            auth_trusted_devices_list(&host_ctx, "com.bedcode.no-auth").unwrap_err(),
            "permission denied"
        );
        assert_eq!(
            auth_trusted_device_revoke(&host_ctx, "com.bedcode.no-auth", "p-1").unwrap_err(),
            "permission denied"
        );
        assert_eq!(
            auth_connection_history_list(&host_ctx, "com.bedcode.no-auth", "p-1").unwrap_err(),
            "permission denied"
        );
        assert_eq!(
            auth_setting_set(&host_ctx, "com.bedcode.no-auth", "pairing_code_ttl", "300").unwrap_err(),
            "permission denied"
        );
    }

    /// 列表返回内核原始记录：含软删行（撤销检测依赖）、无凭据列（§8 红线）
    #[test]
    fn test_trusted_devices_list_returns_raw_records_without_credentials() {
        let host_ctx = build_host_ctx();
        grant_permissions(
            &host_ctx,
            "com.bedcode.session",
            &[crate::plugin::permission::PERMISSION_AUTH],
        );
        assert_eq!(
            auth_trusted_devices_list(&host_ctx, "com.bedcode.session").unwrap(),
            "[]",
            "空表 → 空数组"
        );

        seed_pairing(&host_ctx, "p-1", "Phone", "fp-1", "2026-09-19T00:00:00Z");
        seed_pairing(&host_ctx, "p-2", "Tablet", "fp-2", "2026-09-18T00:00:00Z");
        // 撤销一条：软删行必须仍可见（policy 的撤销检测依据）
        assert!(auth_trusted_device_revoke(&host_ctx, "com.bedcode.session", "p-2").unwrap());

        let raw = auth_trusted_devices_list(&host_ctx, "com.bedcode.session").unwrap();
        let rows: Vec<serde_json::Value> = serde_json::from_str(&raw).expect("JSON 数组");
        assert_eq!(rows.len(), 2, "软删行仍在原始记录里（active 过滤归插件）: {raw}");
        let revoked = rows.iter().find(|r| r["id"] == "p-2").expect("p-2 仍在");
        assert_eq!(revoked["isActive"], false, "软删行 isActive=false");
        let active = rows.iter().find(|r| r["id"] == "p-1").expect("p-1");
        assert_eq!(active["isActive"], true);
        assert_eq!(active["deviceName"], "Phone");
        assert_eq!(active["deviceFingerprint"], "fp-1");
        assert_eq!(active["pairedAt"], "2026-09-19T00:00:00Z");
        assert_eq!(active["connectCount"], 1);
        assert!(
            !raw.contains("PUBLIC-KEY-SENTINEL") && !raw.contains("publicKey") && !raw.contains("sessionToken"),
            "凭据列不得出口（§8 红线）: {raw}"
        );
    }

    /// 撤销：命中软删 + 连带删连接历史；未知 id 幂等 false；重复撤销仍报命中
    #[test]
    fn test_trusted_device_revoke_soft_deletes_and_is_idempotent() {
        let host_ctx = build_host_ctx();
        grant_permissions(
            &host_ctx,
            "com.bedcode.session",
            &[crate::plugin::permission::PERMISSION_AUTH],
        );
        seed_pairing(&host_ctx, "p-1", "Phone", "fp-1", "2026-09-19T00:00:00Z");
        {
            let db = host_ctx.db.clone();
            block_on_async(async move {
                let db = db.lock().await;
                db.conn()
                    .execute(
                        "INSERT INTO connection_history (device_id, auth_method, result, connected_at) \
                         VALUES ('p-1', 'jwt', 'success', '2026-09-19T01:00:00Z')",
                        [],
                    )
                    .expect("seed history");
            });
        }

        assert_eq!(
            auth_trusted_device_revoke(&host_ctx, "com.bedcode.session", "ghost").unwrap(),
            false,
            "未知 id 幂等 false（宿主 remove_pairing 影响 0 行语义）"
        );
        assert_eq!(
            auth_trusted_device_revoke(&host_ctx, "com.bedcode.session", "p-1").unwrap(),
            true
        );
        assert_eq!(
            auth_trusted_device_revoke(&host_ctx, "com.bedcode.session", "p-1").unwrap(),
            true,
            "已软删记录再次撤销：命中即 true（不重复写）"
        );
        // 软删记录保留 + 连接历史连带删除（宿主 remove_pairing 同语义）
        let raw = auth_trusted_devices_list(&host_ctx, "com.bedcode.session").unwrap();
        let rows: Vec<serde_json::Value> = serde_json::from_str(&raw).unwrap();
        assert_eq!(rows.len(), 1, "软删保留记录: {raw}");
        assert_eq!(rows[0]["isActive"], false);
        assert_eq!(
            auth_connection_history_list(&host_ctx, "com.bedcode.session", "p-1").unwrap(),
            "[]",
            "撤销连带删除连接历史（宿主 remove_pairing 语义）"
        );
    }

    /// 连接历史：按 device_id 寻址返回原始记录（camelCase 形状 + 断开时间可空）
    #[test]
    fn test_connection_history_list_returns_raw_records() {
        let host_ctx = build_host_ctx();
        grant_permissions(
            &host_ctx,
            "com.bedcode.session",
            &[crate::plugin::permission::PERMISSION_AUTH],
        );
        seed_pairing(&host_ctx, "p-1", "Phone", "fp-1", "2026-09-19T00:00:00Z");
        {
            let db = host_ctx.db.clone();
            block_on_async(async move {
                let db = db.lock().await;
                db.conn()
                    .execute(
                        "INSERT INTO connection_history (device_id, auth_method, result, address, connected_at) \
                         VALUES ('p-1', 'qr', 'success', '192.168.1.9:8765', '2026-09-19T01:00:00Z')",
                        [],
                    )
                    .expect("seed history");
            });
        }

        let raw = auth_connection_history_list(&host_ctx, "com.bedcode.session", "p-1").unwrap();
        let rows: Vec<serde_json::Value> = serde_json::from_str(&raw).expect("JSON 数组");
        assert_eq!(rows.len(), 1, "按 device_id 寻址: {raw}");
        assert_eq!(rows[0]["deviceId"], "p-1");
        assert_eq!(rows[0]["authMethod"], "qr");
        assert_eq!(rows[0]["result"], "success");
        assert_eq!(rows[0]["connectedAt"], "2026-09-19T01:00:00Z");
        assert!(rows[0]["disconnectedAt"].is_null(), "未断开 → null");
        // 未知设备：空数组（不报错——「这台设备没有历史」是合法查询结果）
        assert_eq!(
            auth_connection_history_list(&host_ctx, "com.bedcode.session", "ghost").unwrap(),
            "[]"
        );
    }

    /// 设置写入：白名单 + 正整数校验；合法值落内核 settings 表（宿主命令面据此取 TTL）
    #[test]
    fn test_auth_setting_set_whitelist_and_persist() {
        let host_ctx = build_host_ctx();
        grant_permissions(
            &host_ctx,
            "com.bedcode.session",
            &[crate::plugin::permission::PERMISSION_AUTH],
        );

        // 白名单外键拒绝（settings 表是宿主真源，不接受任意键写入）
        let err = auth_setting_set(&host_ctx, "com.bedcode.session", "network.port", "1").unwrap_err();
        assert!(err.contains("not in auth domain whitelist"), "got: {err}");
        // 非正整数拒绝（0 / 负数 / 非数字均不写入）
        for bad in ["0", "-1", "abc", ""] {
            let err = auth_setting_set(&host_ctx, "com.bedcode.session", "pairing_code_ttl", bad).unwrap_err();
            assert!(err.contains("positive integer"), "值 {bad:?} 必须拒绝, got: {err}");
        }

        auth_setting_set(&host_ctx, "com.bedcode.session", "pairing_code_ttl", "600").unwrap();
        auth_setting_set(&host_ctx, "com.bedcode.session", "qr_token_ttl", "120").unwrap();
        let db = host_ctx.db.blocking_lock();
        assert_eq!(db.get_setting("pairing_code_ttl").unwrap().as_deref(), Some("600"));
        assert_eq!(db.get_setting("qr_token_ttl").unwrap().as_deref(), Some("120"));
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
                "com.bedcode.session",
                &[crate::plugin::permission::PERMISSION_AUTH],
            );
            auth_setting_set(&host_ctx, "com.bedcode.session", "pairing_code_ttl", "900").unwrap();
        }
        {
            let host_ctx = file_host_ctx(&db_path);
            let db = host_ctx.db.blocking_lock();
            assert_eq!(
                db.get_setting("pairing_code_ttl").unwrap().as_deref(),
                Some("900"),
                "TTL 写入内核 settings 表且重启后稳定"
            );
        }
    }
}
