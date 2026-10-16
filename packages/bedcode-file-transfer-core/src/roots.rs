//! 共享目录注册表：纯函数核心 + 存取/推送编排。
//!
//! 任何增删后调引擎广播面 `set-shared-roots(dirs)` 全量推送，推送失败如实报错并**回滚**
//! 本地变更（引擎拒绝 = 注册表无效，如路径已不存在）。
//!
//! `id` = 路径/URI 的 FNV-1a hex：免随机源（wasm32 无 getrandom），同路径天然去重。
//!
//! 差异面全部外置：持久化实现（SQL 表 vs KV 键）走 `RootsStore`，推送载荷字段名
//! （`path` vs `safTreeUri`）走 `RootWireCodec`。

use crate::domain::SharedRoot;
use crate::ports::{PeerPort, PortError, PortResult, RootWireCodec, RootsStore};

/// FNV-1a 64-bit（无依赖内容哈希）
pub fn fnv1a(data: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in data.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// 路径/URI → 稳定条目 id（同根去重的唯一依据）
pub fn root_id(path_or_uri: &str) -> String {
    format!("root-{:016x}", fnv1a(path_or_uri.trim()))
}

// ==================== 纯函数核心 ====================

/// 插入或更新条目（按 id 去重：同路径重复添加视为更新名称）。返回是否变更。
pub fn upsert(list: &mut Vec<SharedRoot>, entry: SharedRoot) -> bool {
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
pub fn remove(list: &mut Vec<SharedRoot>, id: &str) -> bool {
    let before = list.len();
    list.retain(|r| r.id != id);
    list.len() != before
}

// ==================== 存取 + 推送编排 ====================

/// 变更应用 + 全量推送 + 失败回滚：`mutate` 在快照副本上执行增删；推送成功才落存储，
/// 失败恢复原内容并上抛。返回变更后的全量注册表。
pub fn apply_and_push<H>(
    h: &H,
    mutate: impl FnOnce(&mut Vec<SharedRoot>),
) -> PortResult<Vec<SharedRoot>>
where
    H: RootsStore + RootWireCodec + PeerPort,
{
    let snapshot = h.load_roots()?;
    let mut next = snapshot.clone();
    mutate(&mut next);

    if let Err(push_err) = h.peer_set_shared_roots(&h.roots_to_push_payload(&next)) {
        // 回滚失败不掩盖原始错误（但也不能静默：回滚失败意味着本地与引擎分叉）
        if let Err(rollback_err) = h.save_roots(&snapshot) {
            return Err(PortError::new(format!(
                "set-shared-roots rejected: {push_err}; rollback also failed: {rollback_err}"
            )));
        }
        return Err(PortError::new(format!(
            "set-shared-roots rejected: {push_err}"
        )));
    }
    h.save_roots(&next)?;
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{PeerPort, PortError, PortResult};
    use serde_json::Value;
    use std::cell::RefCell;

    /// 夹具：可切换「推送被引擎拒绝」与「回滚也失败」两种故障态
    #[derive(Default)]
    struct MockBackend {
        stored: RefCell<Vec<SharedRoot>>,
        pushed: RefCell<Vec<Vec<Value>>>,
        reject_push: bool,
        fail_rollback: bool,
    }

    impl RootsStore for MockBackend {
        fn load_roots(&self) -> PortResult<Vec<SharedRoot>> {
            Ok(self.stored.borrow().clone())
        }
        fn save_roots(&self, roots: &[SharedRoot]) -> PortResult<()> {
            if self.fail_rollback {
                return Err(PortError::new("storage write failed"));
            }
            *self.stored.borrow_mut() = roots.to_vec();
            Ok(())
        }
    }

    impl RootWireCodec for MockBackend {
        fn roots_to_push_payload(&self, roots: &[SharedRoot]) -> Vec<Value> {
            roots
                .iter()
                .map(|r| serde_json::json!({ "id": r.id, "name": r.name, "path": r.path }))
                .collect()
        }
    }

    impl PeerPort for MockBackend {
        fn peer_set_shared_roots(&self, dirs: &[Value]) -> PortResult<()> {
            if self.reject_push {
                return Err(PortError::new("engine rejected path"));
            }
            self.pushed.borrow_mut().push(dirs.to_vec());
            Ok(())
        }
        fn peer_dial(&self, _e: &Value) -> PortResult<String> {
            unreachable!("未接线")
        }
        fn peer_close(&self, _h: &str) -> PortResult<bool> {
            unreachable!()
        }
        fn peer_respond_consent(&self, _r: &str, _a: bool) -> PortResult<bool> {
            unreachable!()
        }
        fn peer_list_trusted(&self) -> PortResult<Value> {
            unreachable!()
        }
        fn peer_revoke_trusted(&self, _n: &str) -> PortResult<bool> {
            unreachable!()
        }
        fn peer_send_files(&self, _s: &str, _p: &[Value]) -> PortResult<String> {
            unreachable!()
        }
        fn peer_respond_transfer(&self, _b: &str, _a: bool) -> PortResult<()> {
            unreachable!()
        }
        fn peer_set_receive_policy(&self, _m: &str, _t: u64) -> PortResult<()> {
            unreachable!()
        }
        fn peer_pause_transfer(&self, _b: &str) -> PortResult<()> {
            unreachable!()
        }
        fn peer_resume_transfer(&self, _b: &str) -> PortResult<()> {
            unreachable!()
        }
        fn peer_list_shared_roots(&self, _s: &str) -> PortResult<Value> {
            unreachable!()
        }
        fn peer_browse_directory(&self, _s: &str, _d: &str, _r: &str) -> PortResult<Value> {
            unreachable!()
        }
        fn peer_pull_files(&self, _s: &str, _d: &str, _f: &[Value]) -> PortResult<u32> {
            unreachable!()
        }
        fn peer_set_download_dir(&self, _p: &str) -> PortResult<()> {
            unreachable!()
        }
        fn peer_active_transfers(&self) -> PortResult<Value> {
            unreachable!()
        }
        fn peer_collect_outgoing(&self, _p: &[Value]) -> PortResult<Value> {
            unreachable!()
        }
    }

    fn root(path: &str, name: &str) -> SharedRoot {
        SharedRoot {
            id: root_id(path),
            name: name.into(),
            path: path.into(),
        }
    }

    #[test]
    fn root_id_is_deterministic_and_normalizes_trim() {
        assert_eq!(root_id("content://x/tree/a"), root_id("content://x/tree/a"));
        assert_eq!(root_id(" C:/a/b "), root_id("C:/a/b"));
        assert_ne!(root_id("C:/a/b"), root_id("C:/a/b2"));
        assert!(root_id("x").starts_with("root-"));
        assert_eq!(
            root_id("x").len(),
            "root-".len() + 16,
            "id 形态必须稳定（16 位 hex）"
        );
    }

    #[test]
    fn upsert_dedupes_by_id_and_updates_name() {
        let mut list = vec![];
        let e1 = root("C:/a", "a");
        let e2 = root("C:/b", "b");
        assert!(upsert(&mut list, e1.clone()));
        assert!(!upsert(&mut list, e1.clone()), "同内容重复插入不算变更");
        assert!(upsert(&mut list, e2));
        let renamed = SharedRoot {
            name: "a2".into(),
            ..e1.clone()
        };
        assert!(upsert(&mut list, renamed), "同路径改名 = 更新");
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "a2");
    }

    #[test]
    fn remove_hits_only_matching_id() {
        let mut list = vec![root("p", "n")];
        let id = list[0].id.clone();
        assert!(remove(&mut list, &id));
        assert!(!remove(&mut list, &id));
        assert!(list.is_empty());
    }

    #[test]
    fn apply_and_push_persists_only_after_engine_accepts() {
        let h = MockBackend::default();
        let next = apply_and_push(&h, |list| {
            upsert(list, root("C:/docs", "Docs"));
        })
        .unwrap();
        assert_eq!(next.len(), 1);
        assert_eq!(h.stored.borrow().len(), 1, "推送成功后必须落存储");
        assert_eq!(
            h.pushed.borrow()[0],
            vec![
                serde_json::json!({ "id": root_id("C:/docs"), "name": "Docs", "path": "C:/docs" })
            ]
        );
    }

    #[test]
    fn rejected_push_rolls_back_and_reports_original_error() {
        let h = MockBackend {
            reject_push: true,
            ..Default::default()
        };
        // 预置一条既有条目：回滚必须把它原样恢复
        h.stored.borrow_mut().push(root("C:/keep", "Keep"));
        let err = apply_and_push(&h, |list| {
            upsert(list, root("C:/bad", "Bad"));
        })
        .unwrap_err();
        assert!(err.message().contains("engine rejected path"), "实得 {err}");
        let stored = h.stored.borrow().clone();
        assert_eq!(stored.len(), 1, "推送被拒后本地变更必须回滚");
        assert_eq!(stored[0].path, "C:/keep");
    }

    #[test]
    fn rollback_failure_is_reported_alongside_original_error() {
        // 静默吞掉回滚失败会让「本地已改、引擎没收」的分叉长期存活
        let h = MockBackend {
            reject_push: true,
            fail_rollback: true,
            ..Default::default()
        };
        let err = apply_and_push(&h, |list| {
            upsert(list, root("C:/bad", "Bad"));
        })
        .unwrap_err();
        assert!(err.message().contains("engine rejected path"), "实得 {err}");
        assert!(
            err.message().contains("rollback also failed"),
            "回滚失败必须如实上报：{err}"
        );
    }

    #[test]
    fn codec_controls_wire_field_name_per_end() {
        // 差异面③的可验证性：同一份注册表，两种 codec 产出两种字段名
        struct SafCodec;
        impl RootWireCodec for SafCodec {
            fn roots_to_push_payload(&self, roots: &[SharedRoot]) -> Vec<Value> {
                roots
                    .iter()
                    .map(
                        |r| serde_json::json!({ "id": r.id, "name": r.name, "safTreeUri": r.path }),
                    )
                    .collect()
            }
        }
        let list = vec![SharedRoot {
            id: "r1".into(),
            name: "相册".into(),
            path: "content://tree/1".into(),
        }];
        assert_eq!(
            SafCodec.roots_to_push_payload(&list),
            vec![
                serde_json::json!({ "id": "r1", "name": "相册", "safTreeUri": "content://tree/1" })
            ]
        );
        assert_eq!(
            MockBackend::default().roots_to_push_payload(&list),
            vec![serde_json::json!({ "id": "r1", "name": "相册", "path": "content://tree/1" })]
        );
    }
}
