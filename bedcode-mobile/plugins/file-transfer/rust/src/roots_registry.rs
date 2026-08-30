//! 共享目录注册表真源自持（issue 13 Phase 3 步骤 2，Mobile / storage KV 版）
//!
//! 移动端无 host-plugin-database，注册表落 host-storage 单键 JSON 数组
//! （`shared_roots`，整读改整写；条目规模 ≤ 数十，KV 形态足够）。任何增删后
//! 调 `set-shared-roots(dirs)` 全量推送引擎广播面，推送失败如实报错并回滚
//! 本地变更。内置条目 `local-downloads` 不进注册表（引擎自行注入，get-settings
//! 时以只读形状合并展示）。
//!
//! id = SAF 树 URI 的 FNV-1a hex：免随机源、同 URI 天然去重。

use bedcode_plugin_api_mobile::host::{HostPeer, HostStorage};
use serde::{Deserialize, Serialize};

/// 注册表 storage 键
pub(crate) const ROOTS_KEY: &str = "shared_roots";

/// 单条共享根（path 字段承载 SAF 树 URI）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SharedRoot {
    pub id: String,
    pub name: String,
    /// SAF 目录树 URI（content://...）
    pub path: String,
}

/// FNV-1a 64-bit（无依赖内容哈希）
pub(crate) fn fnv1a(data: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in data.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// URI → 稳定条目 id（同根去重的唯一依据）
pub(crate) fn root_id(path_or_uri: &str) -> String {
    format!("root-{:016x}", fnv1a(path_or_uri.trim()))
}

// ==================== 纯函数核心 ====================

/// 插入或更新条目（按 id 去重：同 URI 重复添加视为更新名称）。返回是否变更。
pub(crate) fn upsert(list: &mut Vec<SharedRoot>, entry: SharedRoot) -> bool {
    if let Some(existing) = list.iter_mut().find(|r| r.id == entry.id) {
        if *existing == entry {
            return false;
        }
        *existing = entry;
        return true;
    }
    list.push(entry);
    true
}

/// 按 id 移除。返回是否命中。
pub(crate) fn remove(list: &mut Vec<SharedRoot>, id: &str) -> bool {
    let before = list.len();
    list.retain(|r| r.id != id);
    list.len() != before
}

/// 推送载荷映射：移动端 SAF 条目 `{ id, name, safTreeUri }`
pub(crate) fn to_push_payload(list: &[SharedRoot]) -> Vec<serde_json::Value> {
    list.iter()
        .map(|r| serde_json::json!({ "id": r.id, "name": r.name, "safTreeUri": r.path }))
        .collect()
}

// ==================== 宿主 I/O 薄层 ====================

/// 读取全部条目（storage 无值/损坏回空表）
pub(crate) fn load_all(h: &impl HostStorage) -> anyhow::Result<Vec<SharedRoot>> {
    let Some(v) = h.storage_get(ROOTS_KEY)? else {
        return Ok(vec![]);
    };
    Ok(serde_json::from_value(v).unwrap_or_default())
}

fn save_all(h: &impl HostStorage, list: &[SharedRoot]) -> anyhow::Result<()> {
    h.storage_set(ROOTS_KEY, &serde_json::to_value(list)?)?;
    Ok(())
}

/// 变更应用 + 全量推送 + 失败回滚：
///
/// `mutate` 在快照副本上执行增删；推送成功才落 storage，失败恢复原内容并上抛
/// （引擎拒绝 = 注册表无效）。返回变更后的全量注册表。
pub(crate) fn apply_and_push(
    h: &(impl HostStorage + HostPeer),
    mutate: impl FnOnce(&mut Vec<SharedRoot>),
) -> anyhow::Result<Vec<SharedRoot>> {
    let snapshot = load_all(h)?;
    let mut next = snapshot.clone();
    mutate(&mut next);

    if let Err(push_err) = h.peer_set_shared_roots(&to_push_payload(&next)) {
        let _ = save_all(h, &snapshot);
        return Err(anyhow::anyhow!("set-shared-roots rejected: {push_err}"));
    }
    save_all(h, &next)?;
    Ok(next)
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_id_is_deterministic_and_normalizes_trim() {
        assert_eq!(root_id("content://x/tree/a"), root_id("content://x/tree/a"));
        assert_eq!(root_id(" content://x/tree/a "), root_id("content://x/tree/a"));
        assert_ne!(root_id("content://x/a"), root_id("content://x/b"));
    }

    #[test]
    fn upsert_dedupes_by_id() {
        let mut list = vec![];
        let e1 = SharedRoot { id: root_id("uri-a"), name: "a".into(), path: "uri-a".into() };
        assert!(upsert(&mut list, e1.clone()));
        assert!(!upsert(&mut list, e1));
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn push_payload_maps_saf_tree_uri() {
        let list = vec![SharedRoot { id: "r1".into(), name: "相册".into(), path: "content://tree/1".into() }];
        assert_eq!(
            to_push_payload(&list),
            vec![serde_json::json!({ "id": "r1", "name": "相册", "safTreeUri": "content://tree/1" })]
        );
    }

    #[test]
    fn remove_hits_only_matching_id() {
        let mut list = vec![SharedRoot { id: "r1".into(), name: "n".into(), path: "p".into() }];
        assert!(remove(&mut list, "r1"));
        assert!(!remove(&mut list, "r1"));
        assert!(list.is_empty());
    }
}
