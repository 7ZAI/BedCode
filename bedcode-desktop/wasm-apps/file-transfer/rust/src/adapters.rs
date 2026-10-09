//! 共享核端口适配器（桌面端）——**双端差异的唯一落点**。
//!
//! 与移动端 `adapters.rs` 同构（本文件是差异的镜像面，逐条对照读最有价值）：
//!
//! | 面 | 桌面 | 移动 |
//! | --- | --- | --- |
//! | 共享根持久化 | **plugin-db 表 `shared_roots`** | `host-storage` 单键 |
//! | 推送 wire 字段名 | **`path`** | `safTreeUri` |
//! | 节点电源 | **插件显式请求起停** | 宿主外壳驱动（不接线） |
//! | 目录多选 / 打开所在目录 | **支持** | 不支持 |
//! | 接收落点可否自定义 | **可** | 固定 MediaStore.Downloads |
//! | 旧快照 topic 对账 | **仍订阅（双写期）** | 已整条退役 |
//!
//! 纪律同移动端：**1:1 委派、零判据**；错误只做 `HostError` → `PortError` 类型转换
//! （`Display` 透传，不吞宿主给的真实原因）。
//!
//! 这里把「能 1:1 委派的端口」全部实现，**本身就是一道编译期校验**：端口签名与桌面 SDK
//! 不一致会直接编不过（移动端 T4 就是靠这步查出三处签名出入）。
//! 唯一不实现的是 `ConsentGate`——它要包住具体 `WasmHost` 并调 `auth_center` 互调，
//! 属「编排层共享」票据的落点，不在本轮。

use bedcode_file_transfer_core::domain::SharedRoot;
use bedcode_file_transfer_core::ports::{
    BusPort, EventPort, KvStore, LogPort, MdnsPort, NodePower, PeerPort, PlatformPort,
    PluginProfile, PortError, PortResult, RootWireCodec, RootsStore,
};
use bedcode_plugin_api::host::{
    HostBus, HostError, HostEvents, HostLog, HostMdns, HostPeer, HostPlatform, HostPluginDatabase,
    HostStorage,
};

/// 桌面端端口包：`&H`（`H` = 任一满足对应 SDK trait 的宿主句柄，通常是 `WasmHost`）
pub(crate) struct DesktopPorts<'a, H: ?Sized>(pub(crate) &'a H);

/// `HostError` → `PortError`：消息逐字保留
fn map_err<T>(r: Result<T, HostError>) -> PortResult<T> {
    r.map_err(|e| PortError::new(e.to_string()))
}

// ==================== 观测面 ====================

impl<H: HostLog + ?Sized> LogPort for DesktopPorts<'_, H> {
    fn log_info(&self, msg: &str) {
        self.0.log_info(msg);
    }

    fn log_error(&self, msg: &str) {
        self.0.log_error(msg);
    }
}

impl<H: HostEvents + ?Sized> EventPort for DesktopPorts<'_, H> {
    fn emit_event(&self, name: &str, payload: &serde_json::Value) {
        self.0.emit_event(name, payload);
    }
}

impl<H: HostBus + ?Sized> BusPort for DesktopPorts<'_, H> {
    fn bus_publish(&self, topic: &str, payload: &serde_json::Value) -> PortResult<()> {
        map_err(self.0.bus_publish(topic, payload))
    }

    fn bus_subscribe(&self, topic: &str) -> PortResult<()> {
        map_err(self.0.bus_subscribe(topic))
    }

    fn bus_unsubscribe(&self, topic: &str) -> PortResult<()> {
        map_err(self.0.bus_unsubscribe(topic))
    }
}

// ==================== 存储面 ====================

impl<H: HostStorage + ?Sized> KvStore for DesktopPorts<'_, H> {
    fn storage_get(&self, key: &str) -> PortResult<Option<serde_json::Value>> {
        map_err(self.0.storage_get(key))
    }

    fn storage_set(&self, key: &str, value: &serde_json::Value) -> PortResult<()> {
        map_err(self.0.storage_set(key, value))
    }

    fn storage_delete(&self, key: &str) -> PortResult<()> {
        map_err(self.0.storage_delete(key))
    }
}

/// 差异面①：桌面 = `host-plugin-database` 独立表 `shared_roots`（移动 = `host-storage` 单键）。
///
/// 表结构与读写语句与退役前**逐字一致**（含 `created_at` 用序号保加入顺序的语义）——
/// 换表结构等于用户已配置的共享根凭空消失，属迁移事故。
impl<H: HostPluginDatabase + ?Sized> DesktopPorts<'_, H> {
    /// 建表（幂等）；桌面 activate 期显式调用以尽早暴露持久化故障
    pub(crate) fn ensure_roots_table(&self) -> PortResult<()> {
        map_err(self.0.plugin_db_execute(
            "CREATE TABLE IF NOT EXISTS shared_roots (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            path TEXT NOT NULL,
            created_at INTEGER NOT NULL DEFAULT 0
        )",
        ))
        .map(|_| ())
    }
}

impl<H: HostPluginDatabase + ?Sized> RootsStore for DesktopPorts<'_, H> {
    fn load_roots(&self) -> PortResult<Vec<SharedRoot>> {
        self.ensure_roots_table()?;
        let rows = map_err(self.0.plugin_db_query(
            "SELECT id, name, path FROM shared_roots ORDER BY created_at, rowid",
        ))?
        .unwrap_or(serde_json::Value::Array(vec![]));
        let arr = rows.as_array().cloned().unwrap_or_default();
        Ok(arr
            .iter()
            .filter_map(|r| {
                Some(SharedRoot {
                    id: r.get("id")?.as_str()?.to_string(),
                    name: r
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    path: r
                        .get("path")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                })
            })
            .collect())
    }

    fn save_roots(&self, roots: &[SharedRoot]) -> PortResult<()> {
        self.ensure_roots_table()?;
        self.0
            .plugin_db_execute("DELETE FROM shared_roots")
            .map_err(|e| PortError::new(format!("shared_roots clear failed: {e}")))?;
        for (i, r) in roots.iter().enumerate() {
            self.0
                .plugin_db_execute_params(
                    "INSERT INTO shared_roots (id, name, path, created_at) VALUES (?1, ?2, ?3, ?4)",
                    &[
                        serde_json::Value::String(r.id.clone()),
                        serde_json::Value::String(r.name.clone()),
                        serde_json::Value::String(r.path.clone()),
                        serde_json::json!(i as u64),
                    ],
                )
                .map_err(|e| PortError::new(format!("shared_roots insert {} failed: {e}", r.id)))?;
        }
        Ok(())
    }
}

/// 差异面③：桌面推送载荷字段名 = `path`（移动 = `safTreeUri`）
impl<H: ?Sized> RootWireCodec for DesktopPorts<'_, H> {
    fn roots_to_push_payload(&self, roots: &[SharedRoot]) -> Vec<serde_json::Value> {
        roots
            .iter()
            .map(|r| serde_json::json!({ "id": r.id, "name": r.name, "path": r.path }))
            .collect()
    }
}

// ==================== 对等网络面 ====================

impl<H: HostPeer + ?Sized> PeerPort for DesktopPorts<'_, H> {
    fn peer_dial(&self, endpoint: &serde_json::Value) -> PortResult<String> {
        map_err(self.0.peer_dial(endpoint))
    }

    fn peer_close(&self, handle: &str) -> PortResult<bool> {
        map_err(self.0.peer_close(handle))
    }

    fn peer_respond_consent(&self, request_id: &str, accepted: bool) -> PortResult<bool> {
        map_err(self.0.peer_respond_consent(request_id, accepted))
    }

    fn peer_list_trusted(&self) -> PortResult<serde_json::Value> {
        map_err(self.0.peer_list_trusted())
    }

    fn peer_revoke_trusted(&self, node_id: &str) -> PortResult<bool> {
        map_err(self.0.peer_revoke_trusted(node_id))
    }

    fn peer_send_files(&self, session: &str, paths: &[serde_json::Value]) -> PortResult<String> {
        map_err(self.0.peer_send_files(session, paths))
    }

    fn peer_respond_transfer(&self, batch_id: &str, accept: bool) -> PortResult<()> {
        map_err(self.0.peer_respond_transfer(batch_id, accept))
    }

    fn peer_set_receive_policy(&self, mode: &str, timeout_secs: u64) -> PortResult<()> {
        map_err(self.0.peer_set_receive_policy(mode, timeout_secs))
    }

    fn peer_pause_transfer(&self, batch_id: &str) -> PortResult<()> {
        map_err(self.0.peer_pause_transfer(batch_id))
    }

    fn peer_resume_transfer(&self, batch_id: &str) -> PortResult<()> {
        map_err(self.0.peer_resume_transfer(batch_id))
    }

    fn peer_set_shared_roots(&self, dirs: &[serde_json::Value]) -> PortResult<()> {
        map_err(self.0.peer_set_shared_roots(dirs))
    }

    fn peer_list_shared_roots(&self, session: &str) -> PortResult<serde_json::Value> {
        map_err(self.0.peer_list_shared_roots(session))
    }

    fn peer_browse_directory(
        &self,
        session: &str,
        dir_id: &str,
        rel_path: &str,
    ) -> PortResult<serde_json::Value> {
        map_err(self.0.peer_browse_directory(session, dir_id, rel_path))
    }

    fn peer_pull_files(
        &self,
        session: &str,
        dir_id: &str,
        files: &[serde_json::Value],
    ) -> PortResult<u32> {
        map_err(self.0.peer_pull_files(session, dir_id, files))
    }

    fn peer_set_download_dir(&self, path: &str) -> PortResult<()> {
        map_err(self.0.peer_set_download_dir(path))
    }

    fn peer_active_transfers(&self) -> PortResult<serde_json::Value> {
        map_err(self.0.peer_active_transfers())
    }

    fn peer_collect_outgoing(&self, paths: &[serde_json::Value]) -> PortResult<serde_json::Value> {
        map_err(self.0.peer_collect_outgoing(paths))
    }
}

/// 差异面④：桌面**由插件显式请求节点起停**（移动端不接线——宿主外壳驱动）
///
/// 与退役前 `lib.rs` 里那两处 `match h.peer_start_node()` 同语义：失败只记日志、
/// 不翻成激活/停用失败（一次引擎故障不该让插件整体不可用）。
impl<H: HostPeer + ?Sized> NodePower for DesktopPorts<'_, H> {
    fn peer_start_node(&self) -> PortResult<bool> {
        map_err(self.0.peer_start_node())
    }

    fn peer_stop_node(&self) -> PortResult<bool> {
        map_err(self.0.peer_stop_node())
    }
}

// ==================== 平台交互面 ====================

/// 差异面⑤⑥：桌面支持目录多选与「打开所在目录」（移动端不实现 ⇒ 核内默认显性 unsupported）
impl<H: HostPlatform + ?Sized> PlatformPort for DesktopPorts<'_, H> {
    fn platform_pick_files(&self) -> PortResult<Vec<String>> {
        map_err(self.0.platform_pick_files())
    }

    fn platform_pick_folder(&self) -> PortResult<String> {
        map_err(self.0.platform_pick_folder())
    }

    fn platform_pick_folders(&self) -> PortResult<Vec<String>> {
        map_err(self.0.platform_pick_folders())
    }

    fn platform_reveal_in_dir(&self, path: &str) -> PortResult<()> {
        map_err(self.0.platform_reveal_in_dir(path))
    }
}

// ==================== mDNS ====================

impl<H: HostMdns + ?Sized> MdnsPort for DesktopPorts<'_, H> {
    fn mdns_browse(&self, service_type: &str) -> PortResult<String> {
        map_err(self.0.mdns_browse(service_type))
    }

    fn mdns_stop_browse(&self, browser_id: &str) -> PortResult<bool> {
        map_err(self.0.mdns_stop_browse(browser_id))
    }
}

// ==================== 信任决策面 ====================

// `ConsentGate` 桌面侧实现留到「编排层共享」票据：它需要包住具体 `WasmHost` 并调
// `auth_center` 互调（差异面⑦），而本轮核内尚无消费方。**刻意不写**：未消费的适配器
// 会诱使后来者以为「已经接线了」。核内 trait 保留 = 差异面登记在册。

// ==================== 形态能力位 ====================

/// 差异面⑤⑧：桌面落点可由 UI 选择、且仍处旧快照双写期
impl<H: ?Sized> PluginProfile for DesktopPorts<'_, H> {
    fn uses_legacy_snapshot(&self) -> bool {
        true
    }

    fn supports_custom_download_dir(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::cell::RefCell;

    /// 只覆写被测面的 SDK 句柄夹具（其余方法 unreachable!，调用即 panic）
    ///
    /// 桌面注册表走 plugin-db，故夹具实现的是一个**最小的内存 SQL 执行器**：只认
    /// 适配器实际发出的三条语句。SQL 解析写死在这里是刻意的——适配器改语句会让本夹具
    /// 立刻失败（等于「表结构/语句形状变动」被测试拦住）。
    #[derive(Default)]
    struct FakeDb {
        rows: RefCell<Vec<SharedRoot>>,
        /// 记录执行过的非查询语句，供断言幂等建表
        statements: RefCell<Vec<String>>,
    }

    impl HostPluginDatabase for FakeDb {
        fn plugin_db_execute(&self, sql: &str) -> Result<i32, HostError> {
            self.statements.borrow_mut().push(sql.to_string());
            if sql.contains("CREATE TABLE IF NOT EXISTS shared_roots") {
                return Ok(0);
            }
            if sql.trim() == "DELETE FROM shared_roots" {
                self.rows.borrow_mut().clear();
                return Ok(0);
            }
            Err(HostError::custom(-1, format!("unexpected execute: {sql}")))
        }

        fn plugin_db_query(&self, sql: &str) -> Result<Option<Value>, HostError> {
            assert!(
                sql.contains("FROM shared_roots ORDER BY created_at, rowid"),
                "语句形状须稳定：{sql}"
            );
            let rows: Vec<Value> = self
                .rows
                .borrow()
                .iter()
                .map(|r| serde_json::json!({ "id": r.id, "name": r.name, "path": r.path }))
                .collect();
            Ok(Some(Value::Array(rows)))
        }

        fn plugin_db_execute_params(&self, sql: &str, params: &[Value]) -> Result<i32, HostError> {
            assert!(
                sql.contains("INSERT INTO shared_roots"),
                "语句形状须稳定：{sql}"
            );
            self.rows.borrow_mut().push(SharedRoot {
                id: params[0].as_str().unwrap_or_default().to_string(),
                name: params[1].as_str().unwrap_or_default().to_string(),
                path: params[2].as_str().unwrap_or_default().to_string(),
            });
            Ok(1)
        }

        fn plugin_db_query_params(
            &self,
            _sql: &str,
            _params: &[Value],
        ) -> Result<Option<Value>, HostError> {
            unreachable!("适配器不使用参数化查询")
        }

        fn plugin_db_execute_batch(&self, _sqls: &[String]) -> Result<i32, HostError> {
            unreachable!("适配器不使用 execute-batch")
        }
    }

    fn roots() -> Vec<SharedRoot> {
        vec![
            SharedRoot {
                id: "root-1".into(),
                name: "Docs".into(),
                path: "E:/Docs".into(),
            },
            SharedRoot {
                id: "root-2".into(),
                name: "Pics".into(),
                path: "E:/Pics".into(),
            },
        ]
    }

    #[test]
    fn roots_store_round_trips_through_plugin_db() {
        let db = FakeDb::default();
        let ports = DesktopPorts(&db);
        assert!(ports.load_roots().unwrap().is_empty(), "空表回空数组");
        ports.save_roots(&roots()).unwrap();
        assert_eq!(ports.load_roots().unwrap(), roots(), "加入顺序必须保序");
        // 覆盖式写入（整删整插）：写短表后不得残留旧行
        ports.save_roots(&roots()[..1]).unwrap();
        assert_eq!(ports.load_roots().unwrap().len(), 1);
    }

    #[test]
    fn ensure_roots_table_is_idempotent_and_cheap() {
        let db = FakeDb::default();
        DesktopPorts(&db).ensure_roots_table().unwrap();
        DesktopPorts(&db).ensure_roots_table().unwrap();
        let stmts = db.statements.borrow();
        assert_eq!(
            stmts.len(),
            2,
            "两次调用各发一次 CREATE TABLE IF NOT EXISTS（幂等由 SQL 保证）"
        );
        assert!(stmts
            .iter()
            .all(|s| s.contains("CREATE TABLE IF NOT EXISTS")));
    }

    #[test]
    fn push_payload_uses_path_field() {
        // 差异面③的桌面一侧：字段名必须是 path（移动端是 safTreeUri）
        let db = FakeDb::default();
        assert_eq!(
            DesktopPorts(&db).roots_to_push_payload(&roots()),
            vec![
                serde_json::json!({ "id": "root-1", "name": "Docs", "path": "E:/Docs" }),
                serde_json::json!({ "id": "root-2", "name": "Pics", "path": "E:/Pics" }),
            ]
        );
    }

    #[test]
    fn profile_bits_are_desktop_shaped() {
        let db = FakeDb::default();
        let p = DesktopPorts(&db);
        assert!(p.supports_custom_download_dir(), "桌面可自定义接收落点");
        assert!(p.uses_legacy_snapshot(), "桌面仍处旧快照双写期");
    }
}
