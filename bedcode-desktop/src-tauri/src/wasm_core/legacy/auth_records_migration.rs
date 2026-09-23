//! 认证记录 legacy 主库 → 认证中心（terminal-session 插件私有库）的一次性搬运
//!
//! **为什么需要它**（2026-09-22 认证记录下沉，用户裁定）：宿主主库
//! `pairings` / `connection_history` 两表退役（schema.sql 不再创建），但存量旧库
//! 里还有历史配对 / 连接记录。本模块在启动装配期读取 legacy 行，经互调 api
//! `com.bedcode.terminal-session.auth-records-import`（JSON-RPC 2.0 over host-bus，
//! 与 [`crate::wasm_core::quick_actions_migration`] 同通道）推给认证中心插件；
//! 插件按 marker 幂等落库（重复推送整体跳过，插件侧删除不会被 legacy 行复活）。
//!
//! **凭据处理（方案 A，用户裁定）**：
//! - `public_key`（生物凭证公钥）→ 宿主 `plugin_secrets`（key = `biometric:<fp>`，
//!   §8 凭据红线指定存储位；验签执行点留宿主不变）。由本模块**直接写主库表**
//!   ——宿主内核初始化路径不经 guest 原语（无权限仲裁语义），缓存无该键，
//!   后续 guest `auth_secret_get` miss 后读库回填，逻辑一致。只记条数不落明文。
//! - `session_token` 死列（`update_pairing_token` / `verify_session_token` 零调用方
//!   随表一并删除）→ **丢弃不迁移**（凭据零复制）。
//!
//! **触发时机**：`lib.rs` setup 阶段 `PluginHost::new()` 之后（与
//! `quick_actions_migration` 同一位置）——认证中心插件已按持久化状态自动激活、
//! 互调面已登记。插件未激活 → 本轮跳过（legacy 表**不 DROP**，下次启动重试，
//! 数据零丢失）；表不存在 → 无历史（全新安装 / 已清理）。
//!
//! **与 `quick_actions_migration` 的区别**：那边 legacy 表在双轨期继续被宿主服务，
//! 只能在契约退役后清理；这边两表已随 schema 退役（无任何新写入方），迁移成功
//! （api 推送成功，含插件 already_migrated 回执）即 DROP 清表——本模块是
//! legacy 表存续的唯一意图方。
//!
//! **已知边界**：插件在运行中途被启用（非启动时激活）时本模块不感知——legacy
//! 表保留（零丢失），待下次启动补齐。

use crate::db::{Database, LegacyAuthRows};
use crate::wasm_core::manager::runtime::WasmHostContext;
use crate::system::app_context::AppContext;
use crate::utils::auth::auth_center::{call_api, session_active};
use crate::{AppError, Result};
use chrono::Utc;

/// 插件互调 api（短名由 `#[plugin_api]` 宏按 manifest.api 比对防漂移）
pub const API_AUTH_RECORDS_IMPORT: &str = "com.bedcode.terminal-session.auth-records-import";

/// 生物凭证公钥的属主插件（与 terminal-session 插件激活互调面同 id）
pub const CREDENTIAL_PLUGIN_ID: &str = "com.bedcode.terminal-session";

/// 生物凭证在 `plugin_secrets` 的键前缀（与 host_impl/auth.rs::biometric_secret_key 一致）
const BIOMETRIC_KEY_PREFIX: &str = "biometric:";

/// 搬运结果（宿主日志与测试断言的外部可见面）
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuthRecordsMigrationReport {
    /// 整体未执行搬运的原因（插件未激活 / legacy 表不存在）
    pub skipped: Option<String>,
    /// 插件侧回执：`{alreadyMigrated, importedPairings, importedHistory, failed}`
    /// （camelCase，插件 auth_records::MigrationReport 序列化）
    pub plugin_report: Option<serde_json::Value>,
    /// 寄主到 `plugin_secrets` 的生物凭证公钥条数（仅数量——§8 不落明文）
    pub biometric_public_key_count: usize,
}

impl AuthRecordsMigrationReport {
    pub fn skipped(reason: impl Into<String>) -> Self {
        Self {
            skipped: Some(reason.into()),
            plugin_report: None,
            biometric_public_key_count: 0,
        }
    }
}

/// 迁移入口（setup 阶段调用一次；失败只记日志，绝不阻断启动）
pub fn run() {
    let Some(ctx) = AppContext::try_global() else {
        tracing::warn!("auth records migration skipped (AppContext not ready)");
        return;
    };
    let host_ctx = ctx.plugin_host().wasm_host_ctx().clone();
    let db = ctx.db().clone();
    match tauri::async_runtime::block_on(async move {
        let db_guard = db.lock().await;
        migrate(&host_ctx, &db_guard).await
    }) {
        Ok(report) => {
            if let Some(reason) = &report.skipped {
                tracing::info!(
                    plugin_id = "com.bedcode.terminal-session",
                    "认证记录搬运跳过: {reason}"
                );
            } else {
                tracing::info!(
                    plugin_id = "com.bedcode.terminal-session",
                    biometric_count = report.biometric_public_key_count,
                    plugin_report = ?report.plugin_report,
                    "legacy 主库认证记录已迁入认证中心私有库，legacy 表已清理"
                );
            }
        }
        Err(e) => tracing::warn!(
            error = %e,
            "认证记录搬运整体失败（不阻断启动；legacy 表保留，下次启动重试）"
        ),
    }
}

/// 搬运主体（host_ctx + legacy 主库注入，便于无头/闭环测试）
pub async fn migrate(host_ctx: &WasmHostContext, db: &Database) -> Result<AuthRecordsMigrationReport> {
    if !session_active(host_ctx) {
        return Ok(AuthRecordsMigrationReport::skipped(
            "认证中心插件未激活（互调面未登记）；legacy 数据留在主库，下次启动重试",
        ));
    }

    // 读 legacy 主库（表不存在 → 已清理/全新安装，无历史可搬）
    let Some(rows) = db.list_legacy_auth_rows()? else {
        return Ok(AuthRecordsMigrationReport::skipped(
            "legacy pairings/connection_history 表不存在（契约已退役或从未有历史数据）",
        ));
    };

    // 1. 生物凭证公钥寄主 `plugin_secrets`（幂等 upsert，先于行推送：任一步失败
    //    都不 DROP，下次启动整体重试；重复 upsert 无害）。明文不落日志，只记条数。
    let biometric_public_key_count = rows.biometric_public_keys.len();
    for (fingerprint, public_key) in &rows.biometric_public_keys {
        put_biometric_secret(db, fingerprint, public_key)?;
    }

    // 2. 推送公开行给认证中心（插件 marker 幂等落库；即使空表也调，插件侧落
    //    marker 后后续启动整体跳过）
    let payload = serde_json::to_value(LegacyAuthRowsPayload::from(&rows)).map_err(AppError::Serialization)?;
    let reply = call_api(host_ctx, API_AUTH_RECORDS_IMPORT, payload)?;

    // 3. 推送成功（含 already_migrated 回执）→ legacy 表已无服务方，清理
    db.drop_legacy_auth_tables()?;

    Ok(AuthRecordsMigrationReport {
        skipped: None,
        plugin_report: Some(reply),
        biometric_public_key_count,
    })
}

/// wire 载荷：只含公开行（无凭据列）——`publicKey` / `sessionToken` 不出任何 JSON 面
#[derive(serde::Serialize)]
struct LegacyAuthRowsPayload<'a> {
    pairings: &'a [crate::db::LegacyPairingRow],
    history: &'a [crate::db::LegacyConnectionRow],
}

impl<'a> From<&'a LegacyAuthRows> for LegacyAuthRowsPayload<'a> {
    fn from(rows: &'a LegacyAuthRows) -> Self {
        Self {
            pairings: &rows.pairings,
            history: &rows.history,
        }
    }
}

/// 公钥写 `plugin_secrets`（key = `biometric:<fingerprint>`，覆盖写幂等）
///
/// 不经 guest 原语 `auth_secret_set`：宿主内核初始化路径无权限仲裁语义；该函数
/// 的 secrets_cache 缺失无影响（缓存无此键，后续读库回填）。
fn put_biometric_secret(db: &Database, fingerprint: &str, public_key: &str) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    db.conn()
        .execute(
            "INSERT INTO plugin_secrets (plugin_id, key, value, updated_at) \
             VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT(plugin_id, key) DO UPDATE SET value = ?3, updated_at = ?4",
            rusqlite::params![
                CREDENTIAL_PLUGIN_ID,
                format!("{BIOMETRIC_KEY_PREFIX}{fingerprint}"),
                public_key,
                now
            ],
        )
        .map_err(|e| {
            AppError::Internal(format!(
                "auth records migration: biometric secret upsert failed (plugin_id={}, fingerprint=***, public_key_len={}): {e}",
                CREDENTIAL_PLUGIN_ID,
                public_key.len()
            ))
        })?;
    tracing::info!(
        plugin_id = %CREDENTIAL_PLUGIN_ID,
        public_key_len = public_key.len(),
        "auth records migration: biometric secret stored in plugin_secrets (length only)"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Database, LegacyConnectionRow, LegacyPairingRow};
    use std::path::Path;

    fn open_in_memory_and_init() -> Database {
        let db = Database::new(Path::new(":memory:")).expect("open in-memory db");
        db.init_schema().expect("init schema");
        db
    }

    /// legacy 行 → 插件 api 入参 JSON 的形状锁定（camelCase + 凭据列剥离）
    #[test]
    fn legacy_rows_serialize_camel_case_without_credentials() {
        let rows = LegacyAuthRows {
            pairings: vec![LegacyPairingRow {
                id: "p-1".into(),
                device_name: "Pixel 9".into(),
                device_fingerprint: "fp-1".into(),
                address: Some("192.168.1.5:9000".into()),
                uid_hash: Some("uid-1".into()),
                paired_at: "2026-09-19T00:00:00Z".into(),
                last_seen: Some("2026-09-19T01:00:00Z".into()),
                connect_count: 3,
                is_active: true,
            }],
            history: vec![LegacyConnectionRow {
                id: 7,
                device_id: "p-1".into(),
                auth_method: "qr".into(),
                result: "success".into(),
                address: Some("192.168.1.5:9000".into()),
                connected_at: "2026-09-19T00:00:00Z".into(),
                disconnected_at: Some("2026-09-19T01:00:00Z".into()),
            }],
            biometric_public_keys: vec![("fp-1".into(), "SPKI-SECRET".into())],
        };
        let v = serde_json::to_value(LegacyAuthRowsPayload::from(&rows)).expect("serialize");
        assert_eq!(
            v,
            serde_json::json!({
                "pairings": [{
                    "id": "p-1",
                    "deviceName": "Pixel 9",
                    "deviceFingerprint": "fp-1",
                    "address": "192.168.1.5:9000",
                    "uidHash": "uid-1",
                    "pairedAt": "2026-09-19T00:00:00Z",
                    "lastSeen": "2026-09-19T01:00:00Z",
                    "connectCount": 3,
                    "isActive": true
                }],
                "history": [{
                    "id": 7,
                    "deviceId": "p-1",
                    "authMethod": "qr",
                    "result": "success",
                    "address": "192.168.1.5:9000",
                    "connectedAt": "2026-09-19T00:00:00Z",
                    "disconnectedAt": "2026-09-19T01:00:00Z"
                }]
            }),
            "与插件 PairingRecord/ConnectionEventRecord 反序列化形状逐字一致"
        );
        // §8 凭据红线：wire 面不得出现公钥 / session_token
        let raw = serde_json::to_string(&v).expect("to string");
        assert!(!raw.contains("publicKey"), "公钥列不得出现在推送 JSON");
        assert!(!raw.contains("sessionToken"), "session_token 死列不得出现在推送 JSON");
        assert!(!raw.contains("SPKI-SECRET"), "公钥值不得出现在推送 JSON");
    }

    /// api 常量与插件 manifest 声明一致（防漂移的第一道闸，闭环测试兜底真实调用）
    #[test]
    fn import_api_matches_plugin_manifest() {
        let manifest_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../plugins/terminal-session/plugin.json");
        let raw = std::fs::read_to_string(&manifest_path).expect("session plugin.json 可读");
        let manifest: serde_json::Value = serde_json::from_str(&raw).expect("manifest JSON");
        assert!(
            manifest["api"]
                .as_array()
                .expect("api 数组")
                .iter()
                .any(|v| v.as_str() == Some(API_AUTH_RECORDS_IMPORT)),
            "宿主 handoff api 常量必须与 session 插件 manifest.api 一致"
        );
    }

    /// legacy 表读回：公开行 + 公钥元组正确；session_token 列不读（核验 SQL 列清单）
    #[test]
    fn list_legacy_auth_rows_reads_public_rows_and_biometric_keys() {
        let db = open_in_memory_and_init();
        let pairing = LegacyPairingRow {
            id: "p-1".into(),
            device_name: "Pixel 9".into(),
            device_fingerprint: "fp-1".into(),
            address: Some("192.168.1.5:9000".into()),
            uid_hash: Some("uid-1".into()),
            paired_at: "2026-09-19T00:00:00Z".into(),
            last_seen: None,
            connect_count: 2,
            is_active: true,
        };
        // 种子带 session_token（模拟存量旧库凭据列）——迁移读面不得触碰
        db.seed_legacy_auth_rows(
            &pairing,
            "SPKI-SECRET",
            Some("SEKRIT-TOKEN"),
            &[LegacyConnectionRow {
                id: 3,
                device_id: "p-1".into(),
                auth_method: "biometric".into(),
                result: "success".into(),
                address: None,
                connected_at: "2026-09-19T00:00:00Z".into(),
                disconnected_at: None,
            }],
        )
        .expect("seed");

        let rows = db.list_legacy_auth_rows().expect("list").expect("有表");
        assert_eq!(rows.pairings.len(), 1);
        assert_eq!(rows.pairings[0].id, "p-1");
        assert_eq!(rows.pairings[0].device_fingerprint, "fp-1");
        assert_eq!(rows.biometric_public_keys, vec![("fp-1".to_string(), "SPKI-SECRET".to_string())]);
        assert_eq!(rows.history.len(), 1);
        assert_eq!(rows.history[0].id, 3);
        assert_eq!(rows.history[0].auth_method, "biometric");
    }

    /// 全新安装（无 legacy 表）→ 读面返回 None；清理后（DROP）→ 亦返回 None
    #[test]
    fn list_legacy_auth_rows_none_when_tables_absent() {
        let db = open_in_memory_and_init();
        assert!(
            db.list_legacy_auth_rows().expect("list").is_none(),
            "全新安装无 legacy 表"
        );

        // 种子后表存在 → 读回 Some → drop → None（清理闭环）
        db.seed_legacy_auth_rows(
            &LegacyPairingRow {
                id: "p-1".into(),
                device_name: "Pixel 9".into(),
                device_fingerprint: "fp-1".into(),
                address: None,
                uid_hash: None,
                paired_at: "2026-09-19T00:00:00Z".into(),
                last_seen: None,
                connect_count: 1,
                is_active: true,
            },
            "SPKI",
            None,
            &[],
        )
        .expect("seed");
        assert!(db.list_legacy_auth_rows().expect("list").is_some());
        db.drop_legacy_auth_tables().expect("drop");
        assert!(db.list_legacy_auth_rows().expect("list").is_none(), "迁移后表已清");
    }

    /// 公钥寄主：写 plugin_secrets（key = biometric:<fp>），覆盖写幂等，只在指定存储位
    #[test]
    fn put_biometric_secret_upserts_plugin_secrets() {
        let db = open_in_memory_and_init();
        put_biometric_secret(&db, "fp-1", "SPKI-SECRET").expect("write");
        put_biometric_secret(&db, "fp-1", "SPKI-SECRET-V2").expect("overwrite");

        let (value, updated): (String, String) = db
            .conn()
            .query_row(
                "SELECT value, updated_at FROM plugin_secrets \
                 WHERE plugin_id = ?1 AND key = ?2",
                rusqlite::params![CREDENTIAL_PLUGIN_ID, "biometric:fp-1"],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("read back");
        assert_eq!(value, "SPKI-SECRET-V2", "覆盖写幂等");
        assert!(!updated.is_empty());
    }
}