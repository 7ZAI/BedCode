//! 共享目录注册表 —— 纯函数与存取编排在双端共享核
//! （`bedcode-file-transfer-core::roots`，ADR 0044），本文件是单产物后的宿主 I/O
//! 包装（平台分支选择桌面 / 移动端口实现，规格
//! `.scratch/2026-10-13-file-transfer-single-wasm/spec.md`）。
//!
//! 平台差异全部落在适配器：桌面持久化走 `host-plugin-database` 独立表
//! `shared_roots`（差异面①）、推送字段名 `path`（差异面③）；移动走
//! `host-storage` 单键（`adapters_mobile::ROOTS_KEY`）、`safTreeUri`。
//!
//! 内置条目 `local-downloads` 不进注册表（引擎自行注入，UI 只读展示；
//! 移动端 get-settings 以只读形状合并展示）。

use crate::adapters::DesktopPorts;
use crate::adapters_mobile::MobilePorts;
use crate::platform::{self, Platform};
use bedcode_file_transfer_core::ports::RootsStore as _;
use bedcode_file_transfer_core::roots as core_roots;
use bedcode_plugin_api::host::{HostPeer, HostPluginDatabase, HostStorage};

pub(crate) use bedcode_file_transfer_core::domain::SharedRoot;
pub(crate) use bedcode_file_transfer_core::roots::{remove, root_id, upsert};

/// 建表（幂等；桌面 activate 时调用一次以尽早暴露持久化故障）。
/// 移动端走 storage KV 无表可建——按平台短路（不白跑一条 SQL import）。
pub(crate) fn ensure_table<H: HostPluginDatabase + ?Sized>(h: &H) -> anyhow::Result<()> {
    if platform::current() == Platform::Mobile {
        return Ok(());
    }
    DesktopPorts(h)
        .ensure_roots_table()
        .map_err(anyhow::Error::new)
}

/// 读取全部条目（桌面 created_at 升序 = 加入顺序；移动存储数组序）
pub(crate) fn load_all<H: HostStorage + HostPluginDatabase + ?Sized>(
    h: &H,
) -> anyhow::Result<Vec<SharedRoot>> {
    match platform::current() {
        Platform::Desktop => DesktopPorts(h).load_roots().map_err(anyhow::Error::new),
        Platform::Mobile => MobilePorts(h).load_roots().map_err(anyhow::Error::new),
    }
}

/// 变更应用 + 全量推送 + 失败回滚：`mutate` 在快照副本上执行增删；推送成功才落
/// 持久层，失败恢复原内容并上抛（引擎拒绝 = 注册表无效，如路径已不存在）。
/// 返回变更后的全量注册表。
pub(crate) fn apply_and_push<H: HostStorage + HostPluginDatabase + HostPeer + ?Sized>(
    h: &H,
    mutate: impl FnOnce(&mut Vec<SharedRoot>),
) -> anyhow::Result<Vec<SharedRoot>> {
    match platform::current() {
        Platform::Desktop => {
            core_roots::apply_and_push(&DesktopPorts(h), mutate).map_err(anyhow::Error::new)
        }
        Platform::Mobile => {
            core_roots::apply_and_push(&MobilePorts(h), mutate).map_err(anyhow::Error::new)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform;
    use bedcode_plugin_api::host::{HostError, HostPeer};
    use std::cell::RefCell;
    use std::collections::HashMap;

    /// 双平台共用的内存 mock（桌面 SDK trait；移动分支的 KV 落点可断言）
    #[derive(Default)]
    struct MockHost {
        kv: RefCell<HashMap<String, serde_json::Value>>,
        rows: RefCell<Vec<serde_json::Value>>,
        pushed_roots: RefCell<Vec<serde_json::Value>>,
    }

    impl HostStorage for MockHost {
        fn storage_get(&self, key: &str) -> Result<Option<serde_json::Value>, HostError> {
            Ok(self.kv.borrow().get(key).cloned())
        }
        fn storage_set(&self, key: &str, value: &serde_json::Value) -> Result<(), HostError> {
            self.kv.borrow_mut().insert(key.to_string(), value.clone());
            Ok(())
        }
        fn storage_delete(&self, key: &str) -> Result<(), HostError> {
            self.kv.borrow_mut().remove(key);
            Ok(())
        }
    }

    impl HostPluginDatabase for MockHost {
        fn plugin_db_execute(&self, sql: &str) -> Result<i32, HostError> {
            if sql.contains("CREATE TABLE IF NOT EXISTS shared_roots") {
                return Ok(0);
            }
            if sql == "DELETE FROM shared_roots" {
                self.rows.borrow_mut().clear();
                return Ok(0);
            }
            Ok(0)
        }
        fn plugin_db_query(&self, sql: &str) -> Result<Option<serde_json::Value>, HostError> {
            if sql.contains("SELECT id, name, path FROM shared_roots") {
                return Ok(Some(serde_json::Value::Array(self.rows.borrow().clone())));
            }
            Ok(None)
        }
        fn plugin_db_execute_params(&self, sql: &str, params: &[serde_json::Value]) -> Result<i32, HostError> {
            if sql.contains("INSERT INTO shared_roots") {
                self.rows.borrow_mut().push(serde_json::json!({
                    "id": params[0], "name": params[1], "path": params[2],
                }));
                return Ok(1);
            }
            Ok(0)
        }
        fn plugin_db_query_params(&self, _sql: &str, _params: &[serde_json::Value]) -> Result<Option<serde_json::Value>, HostError> {
            Ok(None)
        }
        fn plugin_db_execute_batch(&self, _sqls: &[String]) -> Result<i32, HostError> {
            Ok(0)
        }
    }

    impl HostPeer for MockHost {
        fn peer_dial(&self, _endpoint: &serde_json::Value) -> Result<String, HostError> {
            unimplemented!()
        }
        fn peer_close(&self, _handle: &str) -> Result<bool, HostError> {
            unimplemented!()
        }
        fn peer_respond_consent(&self, _request_id: &str, _accepted: bool) -> Result<bool, HostError> {
            unimplemented!()
        }
        fn peer_list_trusted(&self) -> Result<serde_json::Value, HostError> {
            unimplemented!()
        }
        fn peer_revoke_trusted(&self, _node_id: &str) -> Result<bool, HostError> {
            unimplemented!()
        }
        fn peer_send_files(&self, _session: &str, _paths: &[serde_json::Value]) -> Result<String, HostError> {
            unimplemented!()
        }
        fn peer_respond_transfer(&self, _batch_id: &str, _accept: bool) -> Result<(), HostError> {
            unimplemented!()
        }
        fn peer_pause_transfer(&self, _batch_id: &str) -> Result<(), HostError> {
            unimplemented!()
        }
        fn peer_resume_transfer(&self, _batch_id: &str) -> Result<(), HostError> {
            unimplemented!()
        }
        fn peer_set_receive_policy(&self, _mode: &str, _timeout_secs: u64) -> Result<(), HostError> {
            unimplemented!()
        }
        fn peer_set_shared_roots(&self, dirs: &[serde_json::Value]) -> Result<(), HostError> {
            self.pushed_roots.borrow_mut().clear();
            self.pushed_roots.borrow_mut().extend_from_slice(dirs);
            Ok(())
        }
        fn peer_list_shared_roots(&self, _session: &str) -> Result<serde_json::Value, HostError> {
            unimplemented!()
        }
        fn peer_browse_directory(&self, _session: &str, _dir_id: &str, _rel_path: &str) -> Result<serde_json::Value, HostError> {
            unimplemented!()
        }
        fn peer_pull_files(&self, _session: &str, _dir_id: &str, _files: &[serde_json::Value]) -> Result<u32, HostError> {
            unimplemented!()
        }
        fn peer_set_download_dir(&self, _path: &str) -> Result<(), HostError> {
            unimplemented!()
        }
        fn peer_start_node(&self) -> Result<bool, HostError> {
            unimplemented!()
        }
        fn peer_stop_node(&self) -> Result<bool, HostError> {
            unimplemented!()
        }
        fn peer_active_transfers(&self) -> Result<serde_json::Value, HostError> {
            unimplemented!()
        }
        fn peer_collect_outgoing(&self, _paths: &[serde_json::Value]) -> Result<serde_json::Value, HostError> {
            unimplemented!()
        }
    }

    fn sample() -> Vec<SharedRoot> {
        vec![SharedRoot {
            id: "root-1".into(),
            name: "Docs".into(),
            path: "E:/Docs".into(),
        }]
    }

    #[test]
    fn desktop_branch_round_trips_through_plugin_db() {
        let _g = platform::test_lock().lock().unwrap();
        platform::set_for_test(platform::Platform::Desktop);
        let h = MockHost::default();
        assert!(load_all(&h).unwrap().is_empty(), "空表回空数组");
        let next = apply_and_push(&h, |list| list.extend(sample())).unwrap();
        assert_eq!(next.len(), 1);
        assert_eq!(load_all(&h).unwrap(), sample(), "加入顺序必须保序");
        // 桌面落点是 plugin-db 行（不是 storage 键）
        assert_eq!(h.rows.borrow().len(), 1);
        assert!(!h.kv.borrow().contains_key("shared_roots"), "桌面不得落 KV 键");
        // 推送 wire 用 path 字段（差异面③）
        assert_eq!(h.pushed_roots.borrow()[0]["path"], "E:/Docs");
    }

    #[test]
    fn mobile_branch_round_trips_through_storage_key() {
        let _g = platform::test_lock().lock().unwrap();
        platform::set_for_test(platform::Platform::Mobile);
        let h = MockHost::default();
        assert!(load_all(&h).unwrap().is_empty(), "无键回空表");
        let next = apply_and_push(&h, |list| list.extend(sample())).unwrap();
        assert_eq!(next.len(), 1);
        assert_eq!(load_all(&h).unwrap(), sample());
        // 移动落点是 storage 单键（换键 = 用户已配置的共享目录凭空消失）
        assert!(h.kv.borrow().contains_key("shared_roots"));
        assert!(h.rows.borrow().is_empty(), "移动不得落 plugin-db 表");
        // 推送 wire 用 safTreeUri 字段（差异面③）
        assert_eq!(h.pushed_roots.borrow()[0]["safTreeUri"], "E:/Docs");
    }

    #[test]
    fn ensure_table_is_noop_on_mobile() {
        let _g = platform::test_lock().lock().unwrap();
        platform::set_for_test(platform::Platform::Mobile);
        let h = MockHost::default();
        ensure_table(&h).unwrap();
        assert!(h.rows.borrow().is_empty(), "移动端不建桌面表");
    }
}
