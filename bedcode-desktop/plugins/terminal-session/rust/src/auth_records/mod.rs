//! 认证记录域（2026-09-22 认证记录下沉）：配对设备 + 连接历史的**唯一真源**
//!
//! **为什么需要它**：用户裁定（2026-09-22）`pairings` / `connection_history`
//! 不再留宿主主库，认证记录归认证中心（本插件）自持——宿主 host-auth 记录面
//! 原语（v18/v19 七函数）退役。本域接管：
//!
//! - **真源存储**：私有库 `auth_pairings` / `auth_connection_history`（[`store`]）
//! - **业务编排**：归并（uid_hash 复用 id）、软删 + 连带删历史、指纹解析
//!   device_id、每设备历史上限（[`ops`] 纯函数，native 单测完整覆盖）
//! - **迁移**：宿主启动时经互调 api [`import_via_host`] 推存量（宿主主库旧行 →
//!   私有库，marker 幂等，与 `quick_actions_migration` 同模式）
//! - **查询面**：`devices-list` / `history-list` 互调 api（其他插件经 ADR 0017
//!   查询认证中心获取记录）；trust / policy / devices / auth_http 域改为本域自读
//!
//! **凭据边界（§8 不可违反）**：`public_key` 不落本域——生物凭证公钥留宿主
//! `plugin_secrets`（验签执行点在宿主）；`session_token` 是宿主旧表死列直接丢弃。
//! 本域只持公开记录，凭据零复制。

pub mod model;
pub mod ops;
pub mod store;

use model::PairingRecord;

/// 迁移 marker 键（store 同值暴露，避免跨模块字面量漂移）
pub const MIGRATION_MARKER: &str = store::MIGRATION_MARKER;

/// 建表（幂等）——activate 阶段调用；失败只降级认证记录域（配对 / 信任 /
/// 认证链按「无记录」降级，不阻断插件激活，D7 故障隔离）
pub fn ensure_schema_via_host() -> Result<(), String> {
    #[cfg(target_arch = "wasm32")]
    {
        use bedcode_plugin_api::wasm_host::WasmHost;
        use store::AuthRecordsStore;
        WasmHost.ensure_schema()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Err("auth records schema unavailable outside wasm runtime".to_string())
    }
}

/// 迁移导入（互调 api `auth-records-import`）：宿主 handoff 推送的 legacy 主库行
/// → 私有库，marker 幂等（一次性语义，否则插件侧删除会被 legacy 行复活）。
///
/// 入参形状（宿主 `auth_records_migration` 推送）：
/// `{ "pairings": [legacy Pairing 公开行…], "history": [legacy ConnectionHistory 行…] }`
/// （camelCase 字段；`publicKey` / `sessionToken` 凭据列**不推送**——凭据零复制）
pub fn import_via_host(rows: serde_json::Value) -> Result<serde_json::Value, String> {
    #[cfg(target_arch = "wasm32")]
    {
        use bedcode_plugin_api::wasm_host::WasmHost;
        use store::AuthRecordsStore;
        let report = ops::migrate(&WasmHost, &rows)?;
        serde_json::to_value(report).map_err(|e| format!("migration report serialize failed: {e}"))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = rows;
        Err("auth records import unavailable outside wasm runtime".to_string())
    }
}

/// 全部配对记录（含软删行，不排序）——policy 撤销检查 / devices 派生视图 /
/// trust 统一视图的数据源
pub fn records() -> Result<Vec<PairingRecord>, String> {
    #[cfg(target_arch = "wasm32")]
    {
        use bedcode_plugin_api::wasm_host::WasmHost;
        use store::AuthRecordsStore;
        WasmHost.pairings_all()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Err("auth records unavailable outside wasm runtime".to_string())
    }
}

/// 撤销信任（软删 + 连带删除该设备连接历史）；返回是否命中活跃记录
///
/// 语义 = 宿主旧 `remove_pairing`：`is_active = 0` 软删 + 删连接历史。
/// 未知 id / 已软删幂等返回 `false`；**不做**断开在线连接（与宿主现状一致）。
pub fn revoke(id: &str) -> Result<bool, String> {
    #[cfg(target_arch = "wasm32")]
    {
        use bedcode_plugin_api::wasm_host::WasmHost;
        use store::AuthRecordsStore;
        let removed = WasmHost.pairing_revoke(id)?;
        if removed {
            let _ = WasmHost.history_clear(id)?;
        }
        Ok(removed)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = id;
        Err("auth records revoke unavailable outside wasm runtime".to_string())
    }
}

/// 信任记录写入（归并语义）→ 配对记录 id（幂等归并后命中的 id）
///
/// 与宿主旧 `add_pairing` 语义对齐：`uidHash` 命中存量活跃设备（指纹不同）时
/// **复用原记录 id**（连接历史 / connect_count 不分裂）；同名指纹走 UPSERT
/// 更新 last_seen + connect_count+1。
pub fn upsert(
    device_name: &str,
    fingerprint: &str,
    address: Option<&str>,
    uid_hash: Option<&str>,
) -> Result<String, String> {
    #[cfg(target_arch = "wasm32")]
    {
        use bedcode_plugin_api::wasm_host::WasmHost;
        use store::AuthRecordsStore;
        let outcome = ops::upsert_record(&WasmHost, device_name, fingerprint, address, uid_hash)?;
        let id = match outcome {
            ops::UpsertOutcome::Merged { id }
            | ops::UpsertOutcome::Upserted { id }
            | ops::UpsertOutcome::Inserted { id } => id,
        };
        Ok(id)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (device_name, fingerprint, address, uid_hash);
        Err("auth records upsert unavailable outside wasm runtime".to_string())
    }
}

/// 连接计数 / last_seen 刷新（不改设备名）
pub fn touch(fingerprint: &str) -> Result<(), String> {
    #[cfg(target_arch = "wasm32")]
    {
        use bedcode_plugin_api::wasm_host::WasmHost;
        use store::AuthRecordsStore;
        ops::touch_record(&WasmHost, fingerprint)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = fingerprint;
        Err("auth records touch unavailable outside wasm runtime".to_string())
    }
}

/// 连接历史追加（指纹解析 device_id；未配对/未激活 → 静默跳过——挑战签发失败
/// 等「未配对设备」路径依赖此语义，不报错不落库）
pub fn record_connection_event(
    fingerprint: &str,
    method: &str,
    result: &str,
    address: Option<&str>,
) -> Result<(), String> {
    #[cfg(target_arch = "wasm32")]
    {
        use bedcode_plugin_api::wasm_host::WasmHost;
        use store::AuthRecordsStore;
        ops::record_event(&WasmHost, fingerprint, method, result, address)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (fingerprint, method, result, address);
        Err("auth records record unavailable outside wasm runtime".to_string())
    }
}

/// 回填最近一条未关闭连接的断开时间（按指纹解析 device_id）
pub fn close_open_connection(fingerprint: &str) -> Result<(), String> {
    #[cfg(target_arch = "wasm32")]
    {
        use bedcode_plugin_api::wasm_host::WasmHost;
        use store::AuthRecordsStore;
        ops::close_open(&WasmHost, fingerprint)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = fingerprint;
        Err("auth records close unavailable outside wasm runtime".to_string())
    }
}

/// 已配对且活跃的设备列表（`PairedDeviceInfo[]` 形状——与前端 DTO 同形，
/// `pairedAt` 倒序）。区别于 [`records`]：过滤软删 + 展示组织，供
/// `session.devices.paired-list` 命令面与 `devices-list` 互调 api 共用。
pub fn paired_list() -> Result<serde_json::Value, String> {
    #[cfg(target_arch = "wasm32")]
    {
        use bedcode_plugin_api::wasm_host::WasmHost;
        use store::AuthRecordsStore;
        let all = WasmHost.pairings_all()?;
        Ok(ops::active_pairings_json(&all))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Err("auth records paired list unavailable outside wasm runtime".to_string())
    }
}

/// 设备连接历史（`device-id` = 配对记录 id；倒序由存储查询给出）
pub fn history_list(device_id: &str) -> Result<serde_json::Value, String> {
    #[cfg(target_arch = "wasm32")]
    {
        use bedcode_plugin_api::wasm_host::WasmHost;
        use store::AuthRecordsStore;
        let rows = WasmHost.history_by_device(device_id)?;
        serde_json::to_value(rows)
            .map_err(|e| format!("auth history serialize: {e}"))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = device_id;
        Err("auth records history unavailable outside wasm runtime".to_string())
    }
}

/// 清空某设备的连接历史 → `{cleared}`（是否命中了至少一条；空历史幂等 false）
///
/// 不影响配对状态（与撤销配对的连带删除区分开：那是隐式清理，本函数是显式动作）。
pub fn history_clear(device_id: &str) -> Result<serde_json::Value, String> {
    #[cfg(target_arch = "wasm32")]
    {
        use bedcode_plugin_api::wasm_host::WasmHost;
        use store::AuthRecordsStore;
        let cleared = WasmHost.history_clear(device_id)?;
        Ok(serde_json::json!({ "cleared": cleared }))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = device_id;
        Err("auth records history clear unavailable outside wasm runtime".to_string())
    }
}

/// 生物凭证「已配对且绑定公钥」判定所需的两条事实之一：配对记录存在且活跃
/// （公钥绑定事实在宿主 plugin_secrets，见 host-auth `biometric-credential-bound`）
pub fn pairing_active(fingerprint: &str) -> Result<bool, String> {
    #[cfg(target_arch = "wasm32")]
    {
        use bedcode_plugin_api::wasm_host::WasmHost;
        use store::AuthRecordsStore;
        Ok(WasmHost
            .pairing_by_fingerprint(fingerprint)?
            .map(|p| p.is_active)
            .unwrap_or(false))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = fingerprint;
        Err("auth records pairing check unavailable outside wasm runtime".to_string())
    }
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth_records::ops::{self, MigrationReport, UpsertOutcome};
    use crate::auth_records::store::tests::MockAuthRecords;
    use crate::auth_records::store::AuthRecordsStore;

    /// 归并决策：uid_hash 命中存量活跃设备（指纹不同）→ 复用原 id；
    /// 指纹相同 → UPSERT 更新；全新 → 新建
    #[test]
    fn upsert_outcome_decides_merge_strategy() {
        let existing = PairingRecord {
            id: "legacy-id".into(),
            device_name: "Old Phone".into(),
            device_fingerprint: "fp-old".into(),
            address: None,
            uid_hash: Some("uid-1".into()),
            paired_at: "2026-09-01T00:00:00Z".into(),
            last_seen: None,
            connect_count: 3,
            is_active: true,
        };
        let store = MockAuthRecords::new(vec![existing]);

        // uid_hash 命中 → 复用原 id
        let outcome = ops::upsert_record(&store, "New Phone", "fp-new", None, Some("uid-1")).expect("merge");
        assert!(matches!(outcome, UpsertOutcome::Merged { id } if id == "legacy-id"));
        let all = store.pairings_all().expect("all");
        assert_eq!(all.len(), 1, "归并不新增行");
        assert_eq!(all[0].device_fingerprint, "fp-new");
        assert_eq!(all[0].connect_count, 4, "connect_count 继承 + 1");
    }

    /// 指纹相同 → UPSERT 更新（id 不变，connect_count + 1）
    #[test]
    fn upsert_same_fingerprint_updates() {
        let store = MockAuthRecords::new(vec![PairingRecord {
            id: "p-1".into(),
            device_name: "Phone".into(),
            device_fingerprint: "fp-1".into(),
            address: None,
            uid_hash: None,
            paired_at: "2026-09-01T00:00:00Z".into(),
            last_seen: None,
            connect_count: 2,
            is_active: true,
        }]);
        let outcome = ops::upsert_record(&store, "Pixel 9", "fp-1", Some("192.168.1.5:9000"), None).expect("upsert");
        assert!(matches!(outcome, UpsertOutcome::Upserted { id } if id == "p-1"));
        let all = store.pairings_all().expect("all");
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].device_name, "Pixel 9");
        assert_eq!(all[0].connect_count, 3);
        assert_eq!(all[0].address.as_deref(), Some("192.168.1.5:9000"));
    }

    /// 迁移导入：marker 幂等（第二次整体跳过）
    #[test]
    fn import_respects_marker_idempotent() {
        let store = MockAuthRecords::default();
        let rows = serde_json::json!({
            "pairings": [{
                "id": "p-1",
                "deviceName": "Phone",
                "deviceFingerprint": "fp-1",
                "pairedAt": "2026-09-19T00:00:00Z",
                "connectCount": 1,
                "isActive": true
            }],
            "history": []
        });
        let first = ops::migrate(&store, &rows).expect("first migrate");
        assert_eq!(first.imported_pairings, 1);
        let second = ops::migrate(&store, &rows).expect("second migrate");
        assert!(matches!(second, MigrationReport { already_migrated: true, .. }));
    }

    /// 迁移导入：凭据列不迁移（publicKey / sessionToken 即使被推送也剥离）
    #[test]
    fn import_strips_credential_columns() {
        let store = MockAuthRecords::default();
        let rows = serde_json::json!({
            "pairings": [{
                "id": "p-1",
                "deviceName": "Phone",
                "deviceFingerprint": "fp-1",
                "publicKey": "SPKI-BASE64-SECRET",
                "sessionToken": "JWT-SECRET",
                "pairedAt": "2026-09-19T00:00:00Z",
                "connectCount": 1,
                "isActive": true
            }],
            "history": []
        });
        let report = ops::migrate(&store, &rows).expect("migrate");
        assert_eq!(report.imported_pairings, 1);
        let all = store.pairings_all().expect("all");
        assert_eq!(all.len(), 1);
        assert!(!serde_json::to_string(&all[0]).expect("serialize").contains("SPKI"), "公钥不得入私有库");
        assert!(!serde_json::to_string(&all[0]).expect("serialize").contains("JWT-SECRET"), "session_token 不得入私有库");
    }
}
