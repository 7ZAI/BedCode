//! trust 数据模型 —— 统一视图 DTO 与内核原始记录（票 05 搬迁）
//!
//! 数据真源自票 05 起是**内核表**（`pairings`，经 host-auth `trusted-devices-list`
//! 原语读取；spec D3「不动」表），插件不再自持镜像——消除「宿主与插件各记一账」
//! 的口径漂移（Problem Statement 5）。
//!
//! - [`PairingRecord`] 字段与宿主原语返回的**原始记录**逐字段对齐（camelCase）；
//!   含软删行（`isActive = false`）——撤销检测依赖「已撤销记录仍可见」
//! - [`TrustedDeviceDto`] 为统一视图 wire 形状（消费方按 `kind` 判别两条来源）

use serde::{Deserialize, Serialize};

/// 信任条目来源判别（统一视图第一判别字段）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrustKind {
    /// 已配对设备（内核 `pairings` 表语义）
    Pairing,
    /// 可信对端（宿主 peer trust_store 语义）
    Peer,
}

/// 已配对设备原始记录（host-auth `trusted-devices-list` 元素）
///
/// 字段与宿主原语返回的记录 JSON 逐字段对齐（camelCase）。**不做任何解读**：
/// `is_active = false` 是软删行（撤销），过滤与排序归本模块。
/// 凭据列（session token / public key）不在记录面内（宿主 §8 红线）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingRecord {
    /// 配对记录 ID（撤销按此寻址）
    pub id: String,
    pub device_name: String,
    pub device_fingerprint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    /// 配对时刻（RFC3339，UTC）
    pub paired_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_seen: Option<String>,
    #[serde(default)]
    pub connect_count: u32,
    /// 活跃标记：撤销置 false（宿主 `remove_pairing` 的软删语义）
    #[serde(default = "default_active")]
    pub is_active: bool,
}

fn default_active() -> bool {
    true
}

/// 统一视图条目（wire 形状，camelCase）
///
/// pairing 来源字段与宿主 `list_paired_devices` 返回对齐；
/// peer 来源字段与宿主 `list_trusted_peers` 返回（`TrustedPeerDto`）对齐
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrustedDeviceDto {
    /// 来源判别：`pairing` | `peer`
    pub kind: TrustKind,
    /// 统一寻址 ID：pairing = 配对记录 id；peer = 节点 ID（64 位小写 hex）
    pub id: String,
    /// 展示名：pairing = device_name；peer = display_name（可缺，前端以短指纹兜底）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// 指纹：pairing = device_fingerprint；peer = 节点完整 ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    /// 短指纹（前 8 位；仅 peer 来源，pairing 不展示短指纹）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint_short: Option<String>,
    /// 加入信任时刻（RFC3339）：pairing = paired_at；peer = added_at
    pub added_at: String,
    /// 最近一次连接（RFC3339，可缺；仅 pairing 来源）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_seen: Option<String>,
    /// 连接次数（仅 pairing 来源）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connect_count: Option<u32>,
    /// 是否仍受信任（列表恒为 true——列表即信任集合；撤销即从列表消失）
    pub active: bool,
}

impl TrustedDeviceDto {
    /// 从内核原始记录构造统一条目（kind=pairing）
    pub(crate) fn from_pairing(record: &PairingRecord) -> Self {
        Self {
            kind: TrustKind::Pairing,
            id: record.id.clone(),
            name: Some(record.device_name.clone()),
            fingerprint: Some(record.device_fingerprint.clone()),
            fingerprint_short: None,
            added_at: record.paired_at.clone(),
            last_seen: record.last_seen.clone(),
            connect_count: Some(record.connect_count),
            active: record.is_active,
        }
    }

    /// 从宿主 `TrustedPeerDto` JSON（host-peer `list-trusted` 原样返回）构造
    /// 统一条目（kind=peer）。字段对齐宿主 `TrustedPeerDto`：
    /// `nodeId` / `displayName` / `fingerprintShort` / `addedAt`
    pub(crate) fn from_peer(peer: &serde_json::Value) -> Self {
        Self {
            kind: TrustKind::Peer,
            id: peer
                .get("nodeId")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            name: peer
                .get("displayName")
                .and_then(|v| v.as_str())
                .map(String::from),
            fingerprint: peer
                .get("nodeId")
                .and_then(|v| v.as_str())
                .map(String::from),
            fingerprint_short: peer
                .get("fingerprintShort")
                .and_then(|v| v.as_str())
                .map(String::from),
            added_at: peer
                .get("addedAt")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            last_seen: None,
            connect_count: None,
            active: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 原始记录 JSON（宿主原语 wire 形状）→ 模型：字段名/类型逐字段对齐
    #[test]
    fn pairing_record_deserializes_host_wire() {
        let raw = r#"[{
            "id": "p-1",
            "deviceName": "Pixel 9",
            "deviceFingerprint": "fp-abc",
            "address": "192.168.1.5:9000",
            "pairedAt": "2026-09-19T00:00:00Z",
            "lastSeen": "2026-09-19T01:00:00Z",
            "connectCount": 3,
            "isActive": true
        }]"#;
        let records: Vec<PairingRecord> = serde_json::from_str(raw).expect("宿主记录可解析");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].id, "p-1");
        assert_eq!(records[0].device_name, "Pixel 9");
        assert_eq!(records[0].device_fingerprint, "fp-abc");
        assert_eq!(records[0].address.as_deref(), Some("192.168.1.5:9000"));
        assert_eq!(
            records[0].last_seen.as_deref(),
            Some("2026-09-19T01:00:00Z")
        );
        assert_eq!(records[0].connect_count, 3);
        assert!(records[0].is_active);
    }

    /// 可选字段缺省：地址/最近连接缺失 + isActive 缺省从宽（旧记录无该列时不误判撤销）
    #[test]
    fn pairing_record_defaults_when_optional_fields_missing() {
        let raw = r#"{ "id": "p-2", "deviceName": "Phone", "deviceFingerprint": "fp-1", "pairedAt": "2026-09-19T00:00:00Z" }"#;
        let record: PairingRecord = serde_json::from_str(raw).expect("缺省解析");
        assert!(record.address.is_none());
        assert!(record.last_seen.is_none());
        assert_eq!(record.connect_count, 0);
        assert!(record.is_active, "isActive 缺省 = 活跃（不误判为撤销）");
    }

    /// pairing → DTO 字段映射（与宿主 `Pairing` 公开视图对齐）
    #[test]
    fn from_pairing_maps_host_pairing_fields() {
        let record = PairingRecord {
            id: "pairing-1".to_string(),
            device_name: "Pixel 9".to_string(),
            device_fingerprint: "fp-abc".to_string(),
            address: Some("192.168.1.5:9000".to_string()),
            paired_at: "2026-09-19T00:00:00Z".to_string(),
            last_seen: Some("2026-09-19T01:00:00Z".to_string()),
            connect_count: 3,
            is_active: true,
        };
        let dto = TrustedDeviceDto::from_pairing(&record);
        assert_eq!(dto.kind, TrustKind::Pairing);
        assert_eq!(dto.id, "pairing-1");
        assert_eq!(dto.name.as_deref(), Some("Pixel 9"));
        assert_eq!(dto.fingerprint.as_deref(), Some("fp-abc"));
        assert_eq!(dto.added_at, "2026-09-19T00:00:00Z");
        assert_eq!(dto.last_seen.as_deref(), Some("2026-09-19T01:00:00Z"));
        assert_eq!(dto.connect_count, Some(3));
        assert!(dto.fingerprint_short.is_none(), "pairing 不展示短指纹");
    }

    /// peer → DTO 字段映射（与宿主 `TrustedPeerDto` 对齐）；displayName
    /// 缺失时 name=None（前端以短指纹兜底，宿主同语义）
    #[test]
    fn from_peer_maps_host_trusted_peer_dto() {
        let peer = serde_json::json!({
            "nodeId": "aabbccdd" .repeat(8),
            "displayName": "书房台式机",
            "fingerprintShort": "aabbccdd",
            "addedAt": "2026-09-18T12:00:00Z",
        });
        let dto = TrustedDeviceDto::from_peer(&peer);
        assert_eq!(dto.kind, TrustKind::Peer);
        assert_eq!(dto.id, peer["nodeId"]);
        assert_eq!(dto.name.as_deref(), Some("书房台式机"));
        assert_eq!(dto.fingerprint_short.as_deref(), Some("aabbccdd"));
        assert_eq!(dto.added_at, "2026-09-18T12:00:00Z");
        assert!(dto.last_seen.is_none());
        assert!(dto.connect_count.is_none());

        let no_name = serde_json::json!({
            "nodeId": "11223344",
            "displayName": serde_json::Value::Null,
            "fingerprintShort": "11223344",
            "addedAt": "2026-09-18T12:00:00Z",
        });
        let dto = TrustedDeviceDto::from_peer(&no_name);
        assert_eq!(dto.name, None, "displayName 缺失 → None（宿主同语义）");
    }
}
