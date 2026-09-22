//! Database models

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 连接历史的认证方式取值（审计/展示语义归消费方；真源随认证记录下沉
/// 认证中心插件私有库——宿主侧常量保留供既有 HTTP/日志面引用）
pub mod connection_method {
    /// 配对码
    pub const PAIRING_CODE: &str = "pairing_code";
    /// QR 令牌
    pub const QR: &str = "qr";
    /// 生物认证
    pub const BIOMETRIC: &str = "biometric";
    /// JWT 会话令牌（重连）
    pub const JWT: &str = "jwt";
}

/// 连接历史的结果取值
pub mod connection_result {
    /// 认证成功
    pub const SUCCESS: &str = "success";
    /// 认证失败
    pub const FAILED: &str = "failed";
}

/// legacy 主库 `quick_actions` 行（票 02 迁移只读视图）
///
/// 与插件侧 `quick_actions::model::QuickActionRow` 同形（camelCase，`created_at`
/// 保持 DB 原始字符串不重解析，保证逐字节搬运）；宿主侧 handoff
/// （`plugin/quick_actions_migration.rs`）经互调 api 推给 session 插件。
/// 宿主业务表契约退役（schema.sql 不再建表）后，仅存量旧库还持有该表，
/// 本类型与 `operations::list_legacy_quick_action_rows` 一并移除。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyQuickActionRow {
    pub id: String,
    pub name: String,
    pub content: String,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub category: Option<String>,
    pub sort_order: i64,
    pub created_at: String,
}

/// legacy 主库配对行（2026-09-22 认证记录下沉，迁移只读视图）
///
/// 与认证中心插件 `auth_records::model::PairingRecord` 同形（camelCase）；
/// **本结构不含凭据列**（`public_key` / `session_token` 不随任何 JSON/记录面
/// 流动，§8 凭据红线——公钥走 [`LegacyAuthRows::biometric_public_keys`]
/// 直达 `plugin_secrets`，session_token 死列直接丢弃）。`paired_at` 等时间戳
/// 保持 DB 原始字符串不重解析，保证逐字节搬运。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyPairingRow {
    pub id: String,
    pub device_name: String,
    pub device_fingerprint: String,
    pub address: Option<String>,
    pub uid_hash: Option<String>,
    pub paired_at: String,
    pub last_seen: Option<String>,
    pub connect_count: i64,
    pub is_active: bool,
}

/// legacy 主库连接历史行（2026-09-22 认证记录下沉，迁移只读视图）
///
/// 与认证中心插件 `auth_records::model::ConnectionEventRecord` 同形
/// （camelCase）；`connected_at` 等时间戳保持 DB 原始字符串逐字节搬运。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyConnectionRow {
    pub id: i64,
    pub device_id: String,
    pub auth_method: String,
    pub result: String,
    pub address: Option<String>,
    pub connected_at: String,
    pub disconnected_at: Option<String>,
}

/// legacy 主库认证记录整体（2026-09-22 认证记录下沉，迁移只读视图）
///
/// 宿主 `plugin/auth_records_migration.rs` 的输入：配对公开行 + 连接历史行
/// 经互调 api 推给认证中心；生物凭证公钥（`biometric_public_keys`）单独寄主
/// 到 `plugin_secrets`（key = `biometric:<fingerprint>`）。
pub struct LegacyAuthRows {
    pub pairings: Vec<LegacyPairingRow>,
    pub history: Vec<LegacyConnectionRow>,
    /// (device_fingerprint, public_key) 对——§8 凭据红线：只进
    /// `plugin_secrets`（可读回凭据指定存储位），禁止经任何序列化/日志面出现
    pub biometric_public_keys: Vec<(String, String)>,
}

/// App setting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Setting {
    pub key: String,
    pub value: String,
    pub updated_at: DateTime<Utc>,
}

impl Setting {
    pub fn new(key: String, value: String) -> Self {
        Self {
            key,
            value,
            updated_at: Utc::now(),
        }
    }
}
