//! 共享目录注册表真源自持（issue 13 Phase 3 步骤 2）
//!
//! 注册表落插件数据层（桌面 plugin-database 独立 SQLite 表）；任何增删后调
//! `set-shared-roots(dirs)` 全量推送引擎广播面，推送失败如实报错并回滚本地
//! 变更（引擎拒绝 = 注册表无效）。内置条目 `local-downloads` 不进注册表
//! （移动端引擎自行注入，UI 只读展示）。
//!
//! id = 路径/URI 的 FNV-1a hex：免随机源（wasm32 无 getrandom）、同路径天然
//! 去重。纯函数核心与宿主 I/O 分离，cargo test 直测。

use bedcode_plugin_api::host::{HostPeer, HostPluginDatabase};
use serde::{Deserialize, Serialize};

/// 单条共享根（桌面 path = 本地绝对路径；移动端序列化为 safTreeUri 由调用方
/// 决定字段名，本结构统一用 path 承载）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SharedRoot {
    pub id: String,
    pub name: String,
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

/// 路径/URI → 稳定条目 id（同根去重的唯一依据）
pub(crate) fn root_id(path_or_uri: &str) -> String {
    format!("root-{:016x}", fnv1a(path_or_uri.trim()))
}

// ==================== 纯函数核心 ====================

/// 插入或更新条目（按 id 去重：同路径重复添加视为更新名称）。返回是否变更。
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

/// 桌面推送载荷映射：`{ id, name, path }`（移动端在各自 crate 内覆写为 safTreeUri）
pub(crate) fn to_push_payload(list: &[SharedRoot]) -> Vec<serde_json::Value> {
    list.iter()
        .map(|r| serde_json::json!({ "id": r.id, "name": r.name, "path": r.path }))
        .collect()
}

// ==================== 宿主 I/O 薄层 ====================

/// 建表（幂等；activate 时调用一次）
pub(crate) fn ensure_table(h: &impl HostPluginDatabase) -> anyhow::Result<()> {
    h.plugin_db_execute(
        "CREATE TABLE IF NOT EXISTS shared_roots (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            path TEXT NOT NULL,
            created_at INTEGER NOT NULL DEFAULT 0
        )",
    )?;
    Ok(())
}

/// 读取全部条目（created_at 升序 = 加入顺序）
pub(crate) fn load_all(h: &impl HostPluginDatabase) -> anyhow::Result<Vec<SharedRoot>> {
    ensure_table(h)?;
    let rows = h
        .plugin_db_query("SELECT id, name, path FROM shared_roots ORDER BY created_at, rowid")?
        .unwrap_or(serde_json::Value::Array(vec![]));
    let arr = rows.as_array().cloned().unwrap_or_default();
    Ok(arr
        .iter()
        .filter_map(|r| {
            Some(SharedRoot {
                id: r.get("id")?.as_str()?.to_string(),
                name: r.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                path: r.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            })
        })
        .collect())
}

/// 变更应用 + 全量推送 + 失败回滚：
///
/// `mutate` 在快照副本上执行增删；推送成功才落库，失败恢复原库内容并上抛
/// （引擎拒绝 = 注册表无效，如路径已不存在）。返回变更后的全量注册表。
pub(crate) fn apply_and_push(
    h: &(impl HostPluginDatabase + HostPeer),
    mutate: impl FnOnce(&mut Vec<SharedRoot>),
) -> anyhow::Result<Vec<SharedRoot>> {
    let snapshot = load_all(h)?;
    let mut next = snapshot.clone();
    mutate(&mut next);

    // 推送失败先回滚数据库再上抛（回滚失败记日志不掩盖原始错误）
    if let Err(push_err) = h.peer_set_shared_roots(&to_push_payload(&next)) {
        let _ = rewrite_all(h, &snapshot);
        return Err(anyhow::anyhow!("set-shared-roots rejected: {push_err}"));
    }
    rewrite_all(h, &next)?;
    Ok(next)
}

/// 全量重写表内容（delete + 按序 insert；created_at 用序号保持加入顺序语义）
fn rewrite_all(h: &impl HostPluginDatabase, list: &[SharedRoot]) -> anyhow::Result<()> {
    ensure_table(h)?;
    h.plugin_db_execute("DELETE FROM shared_roots")?;
    for (i, r) in list.iter().enumerate() {
        h.plugin_db_execute_params(
            "INSERT INTO shared_roots (id, name, path, created_at) VALUES (?1, ?2, ?3, ?4)",
            &[
                serde_json::Value::String(r.id.clone()),
                serde_json::Value::String(r.name.clone()),
                serde_json::Value::String(r.path.clone()),
                serde_json::json!(i as u64),
            ],
        )?;
    }
    Ok(())
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_id_is_deterministic_and_normalizes_trim() {
        assert_eq!(root_id("C:/a/b"), root_id("C:/a/b"));
        assert_eq!(root_id(" C:/a/b "), root_id("C:/a/b"));
        assert_ne!(root_id("C:/a/b"), root_id("C:/a/b2"));
        assert!(root_id("x").starts_with("root-"));
    }

    #[test]
    fn upsert_dedupes_by_id() {
        let mut list = vec![];
        let e1 = SharedRoot { id: root_id("C:/a"), name: "a".into(), path: "C:/a".into() };
        let e2 = SharedRoot { id: root_id("C:/b"), name: "b".into(), path: "C:/b".into() };
        assert!(upsert(&mut list, e1.clone()));
        assert!(!upsert(&mut list, e1.clone()));
        assert!(upsert(&mut list, e2));
        // 同路径改名 = 更新
        let renamed = SharedRoot { id: e1.id.clone(), name: "a2".into(), path: e1.path.clone() };
        assert!(upsert(&mut list, renamed));
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "a2");
    }

    #[test]
    fn remove_hits_only_matching_id() {
        let mut list = vec![SharedRoot { id: "r1".into(), name: "n".into(), path: "p".into() }];
        assert!(remove(&mut list, "r1"));
        assert!(!remove(&mut list, "r1"));
        assert!(list.is_empty());
    }

    #[test]
    fn push_payload_maps_camel_case() {
        let list = vec![SharedRoot { id: "r1".into(), name: "Docs".into(), path: "E:/Docs".into() }];
        let payload = to_push_payload(&list);
        assert_eq!(
            payload,
            vec![serde_json::json!({ "id": "r1", "name": "Docs", "path": "E:/Docs" })]
        );
    }
}
