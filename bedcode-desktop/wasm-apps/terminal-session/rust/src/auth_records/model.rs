//! 认证记录数据模型（2026-09-22 认证记录下沉）
//!
//! 真源 = 本插件私有库（`auth_records::store`）。字段与宿主旧 `pairings` /
//! `connection_history` 表的**公开列**对齐（camelCase wire 形状供前端与互调面），
//! 凭据列（`public_key` / `session_token`）**不在此模型**——§8 凭据红线：
//! 公钥留宿主 `plugin_secrets`（验签执行点在宿主），session_token 是宿主旧表
//! 死列（零生产消费）直接丢弃，凭据零复制。

use serde::{Deserialize, Serialize};

/// 配对设备公开记录（与宿主旧 `Pairing` 公开视图对齐）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingRecord {
    pub id: String,
    pub device_name: String,
    pub device_fingerprint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    /// 设备唯一 ID（ANDROID_ID 等）哈希：跨指纹合并配对记录的锚点
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uid_hash: Option<String>,
    /// 配对时刻（RFC3339，UTC）
    pub paired_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_seen: Option<String>,
    #[serde(default)]
    pub connect_count: u32,
    /// 活跃标记：撤销置 false（软删保留行——撤销检测依赖「已撤销记录仍可见」）
    #[serde(default = "default_active")]
    pub is_active: bool,
}

fn default_active() -> bool {
    true
}

/// 连接事件记录（与宿主旧 `ConnectionHistory` 公开视图对齐）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionEventRecord {
    /// 自增 id（仅内部排序参考；wire 面通常不消费）
    #[serde(default, skip_serializing_if = "is_zero")]
    pub id: i64,
    /// 设备 id（= `auth_pairings.id`，撤销 / 清空按此寻址）
    pub device_id: String,
    /// 认证方式（内核常量字符串：`pairing_code` / `qr` / `jwt` / `biometric`）
    pub auth_method: String,
    /// 结果（`success` / `failed`）
    pub result: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    /// 连接时刻（RFC3339，UTC）
    pub connected_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disconnected_at: Option<String>,
}

fn is_zero(v: &i64) -> bool {
    *v == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// camelCase wire 形状：前端 / 互调面消费的字段名与宿主旧公开视图一致
    #[test]
    fn pairing_record_wire_shape_camel_case() {
        let raw = r#"{
            "id": "p-1",
            "deviceName": "Pixel 9",
            "deviceFingerprint": "fp-abc",
            "address": "192.168.1.5:9000",
            "uidHash": "uid-1",
            "pairedAt": "2026-09-19T00:00:00Z",
            "lastSeen": "2026-09-19T01:00:00Z",
            "connectCount": 3,
            "isActive": true
        }"#;
        let record: PairingRecord = serde_json::from_str(raw).expect("解析");
        assert_eq!(record.id, "p-1");
        assert_eq!(record.device_name, "Pixel 9");
        assert_eq!(record.device_fingerprint, "fp-abc");
        assert_eq!(record.address.as_deref(), Some("192.168.1.5:9000"));
        assert_eq!(record.uid_hash.as_deref(), Some("uid-1"));
        assert_eq!(record.connect_count, 3);
        assert!(record.is_active);
    }

    /// 可选字段缺省从宽：旧记录无 uid_hash / last_seen / address 时不误判
    #[test]
    fn pairing_record_defaults_optional_fields() {
        let raw = r#"{ "id": "p-2", "deviceName": "Phone", "deviceFingerprint": "fp-1", "pairedAt": "2026-09-19T00:00:00Z" }"#;
        let record: PairingRecord = serde_json::from_str(raw).expect("缺省解析");
        assert!(record.uid_hash.is_none());
        assert!(record.last_seen.is_none());
        assert!(record.is_active, "isActive 缺省 = 活跃");
        assert_eq!(record.connect_count, 0);
    }

    /// 连接历史 wire：authMethod/result 小写常量（消费侧 i18n 映射依赖）
    #[test]
    fn connection_event_wire_shape() {
        let raw = r#"{
            "id": 12,
            "deviceId": "p-1",
            "authMethod": "qr",
            "result": "success",
            "address": "192.168.1.5:9000",
            "connectedAt": "2026-09-19T00:00:00Z",
            "disconnectedAt": "2026-09-19T01:00:00Z"
        }"#;
        let record: ConnectionEventRecord = serde_json::from_str(raw).expect("解析");
        assert_eq!(record.device_id, "p-1");
        assert_eq!(record.auth_method, "qr");
        assert_eq!(record.result, "success");
        assert_eq!(record.disconnected_at.as_deref(), Some("2026-09-19T01:00:00Z"));
    }

    /// 连接历史 wire：未断开时 disconnectedAt 缺省为 None（null 也不报错）
    #[test]
    fn connection_event_open_optional_disconnect() {
        let raw = r#"{
            "deviceId": "p-1",
            "authMethod": "jwt",
            "result": "failed",
            "connectedAt": "2026-09-19T00:00:00Z"
        }"#;
        let record: ConnectionEventRecord = serde_json::from_str(raw).expect("解析");
        assert!(record.disconnected_at.is_none());
    }
}
