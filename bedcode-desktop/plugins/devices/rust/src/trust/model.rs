//! trust 数据模型 —— 统一视图 DTO 与持久化记录（票 08 B3）
//!
//! 字段命名/语义对齐宿主侧对应实现：
//! - [`PairingRecord`] 字段对齐宿主 `src/db/models.rs::Pairing`
//!   （公开视图字段；会话令牌/密钥等敏感字段不随迁——spec §3 密钥留宿主）
//! - [`TrustedDeviceDto`] 为统一视图 wire 形状（camelCase JSON），
//!   消费方（设置面/未来插件）按 `kind` 判别两条来源

use serde::{Deserialize, Serialize};

/// 信任条目来源判别（统一视图第一判别字段）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrustKind {
    /// 已配对设备（宿主 `pairings` 表语义）
    Pairing,
    /// 可信对端（宿主 peer trust_store 语义）
    Peer,
}

/// 已配对设备持久化记录（插件自持，host-storage 键 `trust.pairings`）
///
/// 字段对齐宿主 `Pairing`（公开视图子集）：
/// `id` / `device_name` / `device_fingerprint` / `address` /
/// `paired_at`（RFC3339）/ `last_seen` / `connect_count` / `active`
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
    /// 软删标记：撤销置 false，列表只含 true（宿主 `is_active` 语义）
    #[serde(default = "default_active")]
    pub active: bool,
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
    /// 从持久化 pairing 记录构造统一条目（kind=pairing）
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
            active: record.active,
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
            name: peer.get("displayName").and_then(|v| v.as_str()).map(String::from),
            fingerprint: peer.get("nodeId").and_then(|v| v.as_str()).map(String::from),
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
            active: true,
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

    /// PairingRecord 序列化：camelCase + 可选字段缺省 + active 缺省为 true
    /// （旧数据无 active 字段读回默认受信任，向后兼容）
    #[test]
    fn pairing_record_serde_roundtrip_with_defaults() {
        let json = r#"{
            "id": "p-1",
            "deviceName": "Phone",
            "deviceFingerprint": "fp-1",
            "pairedAt": "2026-09-19T00:00:00Z"
        }"#;
        let record: PairingRecord = serde_json::from_str(json).expect("parse with defaults");
        assert_eq!(record.connect_count, 0);
        assert_eq!(record.active, true, "active 缺省受信任");
        assert!(record.address.is_none());
        assert!(record.last_seen.is_none());

        let serialized = serde_json::to_string(&record).unwrap();
        let parsed: PairingRecord = serde_json::from_str(&serialized).expect("roundtrip");
        assert_eq!(parsed, record);
    }
}
