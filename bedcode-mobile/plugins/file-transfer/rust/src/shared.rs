//! 共享目录条目模型（SAF URI 存储）
//!
//! 共享目录（Shared Directory）条目存储改为 SAF URI（content://tree/...）
//! + 持久化授权（takePersistableUriPermission 由 Kotlin 侧完成）；旧真实路径
//! 条目直接废除（开发阶段无兼容负担）。app 私有下载目录保留为唯一免授权
//! 特殊条目（真实路径，直读直传）。
//!
//! 纯数据模型 + 纯函数（不依赖宿主），可独立单测（复用 state.rs 测试模式）。

use serde::{Deserialize, Serialize};

/// 条目类型：SAF 树授权目录（经系统目录选择器选择并持久化授权）
pub const KIND_SAF: &str = "saf";
/// 条目类型：app 私有下载目录（免授权特殊条目，唯一保留的真实路径条目）
pub const KIND_PRIVATE_DOWNLOADS: &str = "private_downloads";

/// 共享目录条目
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SharedRoot {
    /// 条目 id：SAF 树 URI（content://tree/...）；特殊条目为真实路径
    pub id: String,
    /// 条目类型（saf / private_downloads）
    pub kind: String,
    /// 展示名（SAF 目录名；特殊条目为下载目录名）
    pub name: String,
    /// SAF 根 document id（kind=saf 时有效；App 内遍历的起点）
    #[serde(default)]
    pub document_id: String,
    /// 授权有效性（check_authorized 结果回写；false = 已失效，需重新授权）
    #[serde(default = "default_authorized")]
    pub authorized: bool,
}

fn default_authorized() -> bool {
    true
}

impl SharedRoot {
    /// 是否为 SAF 树授权条目
    pub fn is_saf(&self) -> bool {
        self.kind == KIND_SAF
    }
}

/// 展开可挂载条目为路径/URI 列表（filesrv_mount / update_roots 用）
///
/// M2：SAF 树条目（content://tree/...）与真实路径条目均可挂载（宿主按
/// content:// 前缀分流）；返回值为 MountOptions.roots 字符串列表。
pub fn mountable_paths(roots: &[SharedRoot]) -> Vec<String> {
    roots.iter().map(|r| r.id.clone()).collect()
}

/// 是否已存在同 id 条目（添加去重）
pub fn contains(roots: &[SharedRoot], id: &str) -> bool {
    roots.iter().any(|r| r.id == id)
}

/// 解析 storage 中的 roots 值（容错：非法结构/旧格式返回空，即废除旧真实路径条目）
pub fn parse_roots(value: &serde_json::Value) -> Vec<SharedRoot> {
    serde_json::from_value(value.clone()).unwrap_or_default()
}

/// Settings.roots 字段级容错反序列化（serde deserialize_with）
///
/// 新格式 `Vec<SharedRoot>` 优先；旧格式（`Vec<String>` 真实路径条目，已废除）
/// 或任意失败返回空列表——避免旧数据导致整个 Settings 反序列化失败被
/// `unwrap_or_default` 抹掉全部设置（download_dir/concurrency 等）
pub fn deserialize_roots<'de, D>(deserializer: D) -> Result<Vec<SharedRoot>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    if let Ok(roots) = serde_json::from_value::<Vec<SharedRoot>>(value.clone()) {
        return Ok(roots);
    }
    Ok(Vec::new())
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    fn saf_root(id: &str) -> SharedRoot {
        SharedRoot {
            id: id.to_string(),
            kind: KIND_SAF.to_string(),
            name: "照片".to_string(),
            document_id: "primary%3APhoto".to_string(),
            authorized: true,
        }
    }

    fn private_downloads_root(path: &str) -> SharedRoot {
        SharedRoot {
            id: path.to_string(),
            kind: KIND_PRIVATE_DOWNLOADS.to_string(),
            name: "Download".to_string(),
            document_id: String::new(),
            authorized: true,
        }
    }

    #[test]
    fn mountable_paths_filters_saf_roots() {
        // M2：SAF 树条目与真实路径条目均可挂载（宿主按 content:// 前缀分流）
        let roots = vec![
            saf_root("content://tree/a"),
            private_downloads_root("/data/user/0/com.bedcode.mobile/files/Download"),
            saf_root("content://tree/b"),
        ];
        assert_eq!(
            mountable_paths(&roots),
            vec![
                "content://tree/a".to_string(),
                "/data/user/0/com.bedcode.mobile/files/Download".to_string(),
                "content://tree/b".to_string()
            ]
        );
    }

    #[test]
    fn contains_detects_duplicate_by_id() {
        let roots = vec![saf_root("content://tree/a")];
        assert!(contains(&roots, "content://tree/a"));
        assert!(!contains(&roots, "content://tree/b"));
    }

    #[test]
    fn parse_roots_accepts_structured_entries() {
        let json = serde_json::json!([
            {
                "id": "content://tree/a",
                "kind": "saf",
                "name": "照片",
                "document_id": "primary%3APhoto",
                "authorized": false
            }
        ]);
        let roots = parse_roots(&json);
        assert_eq!(roots.len(), 1);
        assert!(roots[0].is_saf());
        assert!(!roots[0].authorized);
        assert_eq!(roots[0].document_id, "primary%3APhoto");
    }

    #[test]
    fn parse_roots_abolishes_legacy_string_entries() {
        // 旧格式（真实路径字符串数组）直接解析失败 → 空列表（开发阶段无兼容负担）
        let json = serde_json::json!(["/storage/emulated/0/Download"]);
        assert!(parse_roots(&json).is_empty());
    }

    #[test]
    fn parse_roots_tolerates_missing_optional_fields() {
        // document_id / authorized 可缺省（旧条目或最小写入）
        let json = serde_json::json!([{ "id": "content://tree/a", "kind": "saf", "name": "照片" }]);
        let roots = parse_roots(&json);
        assert_eq!(roots.len(), 1);
        assert!(roots[0].authorized);
        assert_eq!(roots[0].document_id, "");
    }
}
