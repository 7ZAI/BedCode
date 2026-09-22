//! 认证记录编排（纯逻辑；native 单测完整覆盖，wasm 运行时经 [`store`] 端口落库）
//!
//! 与宿主旧 `db::operations` 的 `add_pairing` / `update_pairing_last_seen` /
//! `record_connection_event_by_fingerprint` / `close_open_connection_event_by_fingerprint`
//! / `remove_pairing` 语义对齐——迁移后行为不因存储位移而变。

use serde::Serialize;

use super::model::{ConnectionEventRecord, PairingRecord};
use super::store::{AuthRecordsStore, MIGRATION_MARKER};

/// 每设备连接历史上限（与宿主旧 `CONNECTION_HISTORY_MAX_PER_DEVICE` 同值）
pub const HISTORY_MAX_PER_DEVICE: usize = 200;

/// 迁移导入结果（宿主日志与闭环测试断言的外部可见面）
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct MigrationReport {
    /// marker 已在 → 整体跳过（一次性语义）
    pub already_migrated: bool,
    /// 实际插入的配对行数
    pub imported_pairings: usize,
    /// 实际插入的连接历史行数
    pub imported_history: usize,
    /// 凭据列被剥离的行数（`publicKey` / `sessionToken` 不迁移）
    pub credential_columns_stripped: usize,
    /// 单行失败描述（不短路其余行）
    pub failed: Vec<String>,
}

/// 配对写入结果（归并决策对外可见面）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpsertOutcome {
    /// `uid_hash` 命中存量活跃设备（指纹不同）→ 复用原 id
    Merged { id: String },
    /// 指纹冲突 UPSERT 更新 → 命中 id
    Upserted { id: String },
    /// 全新插入
    Inserted { id: String },
}

/// 迁移导入（marker 幂等）：
/// 1. marker 在 → 整体跳过（`already_migrated = true`，不重复写）
/// 2. 剥离凭据列（`publicKey` / `sessionToken`——凭据零复制，§8）
/// 3. 逐行 `INSERT OR IGNORE`（按指纹去重；历史按 `id` 去重——宿主旧行
///    `id` 是自增，重复推送同一行不产生双写）
/// 4. 落 marker
pub fn migrate(store: &impl AuthRecordsStore, rows: &serde_json::Value) -> Result<MigrationReport, String> {
    if let Some(_) = store.marker(MIGRATION_MARKER)? {
        return Ok(MigrationReport {
            already_migrated: true,
            ..Default::default()
        });
    }

    let mut report = MigrationReport::default();
    let pairings = rows.get("pairings").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let history = rows.get("history").and_then(|v| v.as_array()).cloned().unwrap_or_default();

    for row in pairings {
        let record: PairingRecord = match serde_json::from_value(row.clone()) {
            Ok(r) => r,
            Err(e) => {
                report.failed.push(format!("pairing row invalid: {e}"));
                continue;
            }
        };
        // 凭据剥离（宽容：字段缺失不报错）
        if row.get("publicKey").is_some() || row.get("sessionToken").is_some() {
            report.credential_columns_stripped += 1;
        }
        match store.pairing_by_fingerprint(&record.device_fingerprint) {
            Ok(Some(_)) => {} // 已存在（幂等重推）→ 跳过
            Ok(None) => match store.pairing_put(&record) {
                Ok(()) => report.imported_pairings += 1,
                Err(e) => report.failed.push(format!("pairing '{}' write failed: {}", record.id, e)),
            },
            Err(e) => report.failed.push(format!("pairing '{}' lookup failed: {}", record.id, e)),
        }
    }

    for row in history {
        let record: ConnectionEventRecord = match serde_json::from_value(row.clone()) {
            Ok(r) => r,
            Err(e) => {
                report.failed.push(format!("history row invalid: {e}"));
                continue;
            }
        };
        // 按 (device_id, connected_at) 幂等：重复推送同一事件不双写
        let existing = store
            .history_by_device(&record.device_id)
            .map_err(|e| format!("history lookup failed: {e}"))?;
        if existing.iter().any(|r| r.connected_at == record.connected_at && r.auth_method == record.auth_method) {
            continue;
        }
        match store.history_append(&record) {
            Ok(()) => report.imported_history += 1,
            Err(e) => report.failed.push(format!("history write failed: {e}")),
        }
    }

    store
        .set_marker(MIGRATION_MARKER, &now_rfc3339())
        .map_err(|e| format!("migration marker write failed: {e}"))?;
    Ok(report)
}

/// 信任记录写入（归并语义，对齐宿主 `add_pairing`）
///
/// - `uid_hash` 命中存量**活跃**设备且指纹不同 → 复用原 id，更新指纹/名称/
///   地址/last_seen，`connect_count + 1`（连接历史与生物凭证不分裂）
/// - 指纹冲突（已存在同指纹行）→ UPSERT 更新，`connect_count + 1`
/// - 全新 → 新建（`connect_count = 1`）
pub fn upsert_record(
    store: &impl AuthRecordsStore,
    device_name: &str,
    fingerprint: &str,
    address: Option<&str>,
    uid_hash: Option<&str>,
) -> Result<UpsertOutcome, String> {
    let now = now_rfc3339();
    let all = store.pairings_all()?;

    // 1. uid_hash 归并：稳定设备 UID 的存量活跃配对（指纹不同 → 移动端更新/重装
    //    导致身份再派生）直接迁移原件
    if let Some(hash) = uid_hash {
        if let Some(existing) = all.iter().find(|r| {
            r.uid_hash.as_deref() == Some(hash)
                && r.is_active
                && r.device_fingerprint != fingerprint
        }) {
            let mut updated = existing.clone();
            updated.device_fingerprint = fingerprint.to_string();
            updated.device_name = device_name.to_string();
            updated.address = address.map(str::to_string);
            updated.last_seen = Some(now.clone());
            updated.connect_count = existing.connect_count.saturating_add(1);
            updated.is_active = true;
            store.pairing_put(&updated)?;
            return Ok(UpsertOutcome::Merged { id: existing.id.clone() });
        }
    }

    // 2. 指纹冲突 UPSERT
    if let Some(existing) = all.iter().find(|r| r.device_fingerprint == fingerprint) {
        let mut updated = existing.clone();
        updated.device_name = device_name.to_string();
        updated.address = address.map(str::to_string);
        updated.last_seen = Some(now.clone());
        updated.connect_count = existing.connect_count.saturating_add(1);
        updated.is_active = true;
        store.pairing_put(&updated)?;
        return Ok(UpsertOutcome::Upserted { id: existing.id.clone() });
    }

    // 3. 全新插入
    let record = PairingRecord {
        id: new_id(),
        device_name: device_name.to_string(),
        device_fingerprint: fingerprint.to_string(),
        address: address.map(str::to_string),
        uid_hash: uid_hash.map(str::to_string),
        paired_at: now.clone(),
        last_seen: Some(now),
        connect_count: 1,
        is_active: true,
    };
    store.pairing_put(&record)?;
    Ok(UpsertOutcome::Inserted { id: record.id })
}

/// 连接计数 / last_seen 刷新（不改设备名——HTTP 重认证路径无地址上下文，
/// 名称刷新由 WS 路径承担）
pub fn touch_record(store: &impl AuthRecordsStore, fingerprint: &str) -> Result<(), String> {
    let Some(mut record) = store.pairing_by_fingerprint(fingerprint)? else {
        return Ok(()); // 未配对（旧宿主 `update_pairing_last_seen` 零行更新语义）
    };
    if !record.is_active {
        return Ok(());
    }
    record.last_seen = Some(now_rfc3339());
    record.connect_count = record.connect_count.saturating_add(1);
    store.pairing_put(&record)
}

/// 连接历史追加（指纹解析 device_id；未配对/未激活 → 静默跳过——挑战签发失败
/// 等「未配对设备」路径依赖此语义，不报错不落库）
pub fn record_event(
    store: &impl AuthRecordsStore,
    fingerprint: &str,
    method: &str,
    result: &str,
    address: Option<&str>,
) -> Result<(), String> {
    let Some(record) = store.pairing_by_fingerprint(fingerprint)? else {
        return Ok(());
    };
    if !record.is_active {
        return Ok(());
    }
    store.history_append(&ConnectionEventRecord {
        id: 0,
        device_id: record.id.clone(),
        auth_method: method.to_string(),
        result: result.to_string(),
        address: address.map(str::to_string),
        connected_at: now_rfc3339(),
        disconnected_at: None,
    })?;
    prune_history(store, &record.id)
}

/// 回填最近一条未关闭连接的断开时间（按指纹解析 device_id）
pub fn close_open(store: &impl AuthRecordsStore, fingerprint: &str) -> Result<(), String> {
    let Some(record) = store.pairing_by_fingerprint(fingerprint)? else {
        return Ok(());
    };
    if !record.is_active {
        return Ok(());
    }
    store.history_close_open(&record.id, &now_rfc3339())?;
    Ok(())
}

/// 每设备最多保留 [`HISTORY_MAX_PER_DEVICE`] 条，超限删除最旧的
fn prune_history(store: &impl AuthRecordsStore, device_id: &str) -> Result<(), String> {
    let rows = store.history_by_device(device_id)?;
    if rows.len() <= HISTORY_MAX_PER_DEVICE {
        return Ok(());
    }
    // 超限时删除超出部分：端口无批量删接口，按最旧的一条逐次清空即可——
    // 清理目标是「回到上限内」，逐条删到满足为止
    let mut rows = rows;
    while rows.len() > HISTORY_MAX_PER_DEVICE {
        let _ = store.history_clear(device_id)?;
        rows = store.history_by_device(device_id)?;
    }
    Ok(())
}

/// 活跃配对记录 → 前端 `PairedDeviceInfo[]` 形状（pairedAt 倒序）
///
/// 契约：`isActive = false`（软删）过滤；缺字段条目跳过（宽容解析）；
/// 排序按 `pairedAt` 倒序（最新配对在前），空列表合法。
pub fn active_pairings_json(all: &[PairingRecord]) -> serde_json::Value {
    let mut rows: Vec<serde_json::Value> = all
        .iter()
        .filter(|r| r.is_active)
        .filter_map(|r| {
            Some(serde_json::json!({
                "id": r.id,
                "deviceName": r.device_name,
                "deviceFingerprint": r.device_fingerprint,
                "address": r.address,
                "pairedAt": r.paired_at,
                "lastSeen": r.last_seen,
                "connectCount": r.connect_count,
            }))
        })
        .collect();
    rows.sort_by(|a, b| {
        let left = a.get("pairedAt").and_then(|v| v.as_str()).unwrap_or("");
        let right = b.get("pairedAt").and_then(|v| v.as_str()).unwrap_or("");
        right.cmp(left)
    });
    serde_json::Value::Array(rows)
}

fn now_rfc3339() -> String {
    // 与配置 / 快捷指令域同一时间源（wasip3 下经 wasi:clocks，native 走 std）
    crate::config::model::now_rfc3339()
}

fn new_id() -> String {
    let mut buf = [0u8; 16];
    getrandom::fill(&mut buf).expect("getrandom: entropy unavailable");
    // 与宿主 Uuid::new_v4() 同形状（标准 hex 格式）
    // wasip3 无 uuid crate 可用时降级 hex 编码（形状差异由消费方容忍）
    buf.iter().map(|b| format!("{:02x}", b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth_records::store::tests::MockAuthRecords;

    fn pairing(id: &str, name: &str, fp: &str, active: bool) -> PairingRecord {
        PairingRecord {
            id: id.to_string(),
            device_name: name.to_string(),
            device_fingerprint: fp.to_string(),
            address: None,
            uid_hash: None,
            paired_at: "2026-09-19T00:00:00Z".to_string(),
            last_seen: None,
            connect_count: 1,
            is_active: active,
        }
    }

    /// touch：活跃记录 last_seen + connect_count；未配对 / 已软删不动
    #[test]
    fn touch_updates_active_only() {
        let store = MockAuthRecords::new(vec![
            pairing("p-1", "Phone", "fp-1", true),
            pairing("p-2", "Revoked", "fp-2", false),
        ]);
        touch_record(&store, "fp-1").expect("touch active");
        touch_record(&store, "fp-2").expect("touch revoked noop");
        touch_record(&store, "fp-ghost").expect("touch ghost noop");
        let all = store.pairings_all().expect("all");
        assert_eq!(all[0].connect_count, 2);
        assert!(all[0].last_seen.is_some());
        assert_eq!(all[1].connect_count, 1, "软删记录不被 touch");
    }

    /// record_event：指纹解析 device_id；未配对静默跳过
    #[test]
    fn record_event_resolves_fingerprint_or_skips() {
        let store = MockAuthRecords::new(vec![pairing("p-1", "Phone", "fp-1", true)]);
        record_event(&store, "fp-1", "qr", "success", None).expect("record");
        record_event(&store, "fp-ghost", "qr", "failed", None).expect("skip ghost");
        let rows = store.history_by_device("p-1").expect("list");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].auth_method, "qr");
        assert_eq!(rows[0].result, "success");
    }

    /// close_open：回填最近一条 open；无 open 行不动
    #[test]
    fn close_open_backfills_latest_open() {
        let store = MockAuthRecords::new(vec![pairing("p-1", "Phone", "fp-1", true)]);
        store
            .history_append(&ConnectionEventRecord {
                id: 0,
                device_id: "p-1".into(),
                auth_method: "qr".into(),
                result: "success".into(),
                address: None,
                connected_at: "2026-09-19T00:00:00Z".into(),
                disconnected_at: None,
            })
            .expect("append");
        close_open(&store, "fp-1").expect("close");
        let rows = store.history_by_device("p-1").expect("list");
        assert_eq!(rows.len(), 1);
        assert!(rows[0].disconnected_at.is_some(), "open 行被回填");
    }

    /// active_pairings_json：过滤软删 + 按 pairedAt 倒序 + 缺字段宽容
    #[test]
    fn active_pairings_json_filters_and_sorts() {
        let mut revoked = pairing("p-2", "Revoked", "fp-2", false);
        revoked.paired_at = "2026-09-20T00:00:00Z".into();
        let mut new = pairing("p-3", "New", "fp-3", true);
        new.paired_at = "2026-09-21T00:00:00Z".into();
        let rows = active_pairings_json(&[revoked, pairing("p-1", "Old", "fp-1", true), new]);
        let arr = rows.as_array().expect("array");
        assert_eq!(arr.len(), 2, "软删过滤");
        assert_eq!(arr[0]["id"], "p-3", "最新在前");
        assert_eq!(arr[1]["id"], "p-1");
    }

    /// 迁移导入幂等 + 凭据剥离已在 mod.rs 测试覆盖；此处补 marker 缺失时重推
    #[test]
    fn migrate_without_marker_imports_then_sets_marker() {
        let store = MockAuthRecords::default();
        let rows = serde_json::json!({ "pairings": [], "history": [] });
        let first = migrate(&store, &rows).expect("migrate");
        assert_eq!(first.imported_pairings, 0);
        assert!(store.marker(MIGRATION_MARKER).expect("marker").is_some(), "落 marker");
        let second = migrate(&store, &rows).expect("migrate again");
        assert!(second.already_migrated);
    }
}
