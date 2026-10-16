//! 快捷指令模型（票 02：session 插件第 4 域——私有库持久化）
//!
//! **wire 形状是兼容红线**：HTTP 面字段与宿主 `server::dtos::config_dto::QuickActionItem`
//! 逐字一致（camelCase，`icon` / `color` 为 **显式 null** 而非省略——移动端
//! `?? 空串` 依赖这一格）。插件私有库行比 wire 多三个内部字段（`category` /
//! `sort_order` / `created_at`），与 legacy 主库 `quick_actions` 表同形，
//! 保证迁移逐字段零丢失；wire 映射（去内部字段）只存在于 [`super::ops`] 的
//! HTTP 装配处，插件侧不引入第二套字段名。

use serde::{Deserialize, Serialize};

/// 快捷指令（插件私有库真源行；wire 形状 camelCase）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickAction {
    pub id: String,
    pub name: String,
    pub content: String,
    /// wire 上为显式 null（与宿主 DTO 同口径，移动端依赖）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// 内部字段（legacy 主库同形，迁移零丢失；不出现在 HTTP wire）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// 排序权重：与 legacy `ORDER BY sort_order` 同语义
    #[serde(default)]
    pub sort_order: i64,
    #[serde(default)]
    pub created_at: String,
}

/// 迁移入参（宿主 handoff 推送的行；字段全部来自 legacy 主库行）
///
/// 与 [`QuickAction`] 同形，但 `category` / `sort_order` / `created_at` 为必填
/// （宿主侧主库行恒有值）；`icon` / `color` 缺省为 None（显式空与缺省等价）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickActionRow {
    pub id: String,
    pub name: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default)]
    pub sort_order: i64,
    #[serde(default)]
    pub created_at: String,
}

impl From<QuickActionRow> for QuickAction {
    fn from(row: QuickActionRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            content: row.content,
            icon: row.icon,
            color: row.color,
            category: row.category,
            sort_order: row.sort_order,
            created_at: row.created_at,
        }
    }
}

// ==================== HTTP wire 映射（纯函数，native 可测） ====================

/// 行 → HTTP wire 条目（`QuickActionItem` 同形：只留 5 个 wire 字段，
/// `icon` / `color` 显式 null；顺序与宿主 DTO 字段序一致）
pub fn to_http_item(action: &QuickAction) -> serde_json::Value {
    serde_json::json!({
        "id": action.id,
        "name": action.name,
        "content": action.content,
        "icon": action.icon,
        "color": action.color,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_http_item_shape_matches_host_dto() {
        let item = QuickAction {
            id: "qa-1".into(),
            name: "部署".into(),
            content: "pnpm run deploy".into(),
            icon: None,
            color: None,
            category: Some("dev".into()),
            sort_order: 2,
            created_at: "2026-09-20T00:00:00Z".into(),
        };
        // 宿主 QuickActionItem 序列化：id/name/content/icon/color（camelCase，
        // icon/color 显式 null，无 skip_serializing_if）——逐键比对防漂移
        assert_eq!(
            to_http_item(&item),
            serde_json::json!({
                "id": "qa-1",
                "name": "部署",
                "content": "pnpm run deploy",
                "icon": null,
                "color": null,
            })
        );
        // 内部字段不得泄漏到 wire
        let s = serde_json::to_string(&to_http_item(&item)).unwrap();
        assert!(!s.contains("category"), "category 是内部字段: {s}");
        assert!(!s.contains("sortOrder"), "sortOrder 是内部字段: {s}");
        assert!(!s.contains("createdAt"), "createdAt 是内部字段: {s}");
    }

    #[test]
    fn row_to_action_preserves_all_fields() {
        let row = QuickActionRow {
            id: "qa-1".into(),
            name: "部署".into(),
            content: "pnpm run deploy".into(),
            icon: Some("rocket".into()),
            color: Some("#f00".into()),
            category: Some("dev".into()),
            sort_order: 3,
            created_at: "2026-09-20T01:02:03Z".into(),
        };
        let action: QuickAction = row.into();
        assert_eq!(action.icon.as_deref(), Some("rocket"));
        assert_eq!(action.color.as_deref(), Some("#f00"));
        assert_eq!(action.category.as_deref(), Some("dev"));
        assert_eq!(action.sort_order, 3);
        assert_eq!(action.created_at, "2026-09-20T01:02:03Z");
    }
}
