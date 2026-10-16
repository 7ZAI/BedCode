//! 共享核端口适配器（移动行为侧）——file-transfer 单产物的第二套端口实现。
//!
//! 与 [`crate::adapters::DesktopPorts`]（桌面行为侧）并列，由
//! [`crate::platform`] 的运行期探测选择。纪律同桌面侧：**1:1 委派、零判据**；
//! 错误只做 `HostError` → `PortError` 类型转换（`Display` 透传）。
//!
//! ## 为什么「移动行为」也经桌面 SDK trait
//!
//! 共享产物只链桌面 SDK（`bedcode-plugin-api`）——按桌面 `world plugin` 构建。
//! 移动侧行为差异**不是另一套 SDK**：持久化走 `host-storage` KV、wire 字段名
//! `safTreeUri`、电源不接线、信任直答原语——这些全部是 core interface（双端宿主
//! 都提供）上的**不同编排**，用桌面 SDK 的 trait 就能完整表达。
//!
//! | 面 | 桌面（`adapters.rs`） | 移动（本文件） |
//! | --- | --- | --- |
//! | 共享根持久化 | plugin-db 表 `shared_roots` | storage 单键（[`ROOTS_KEY`]） |
//! | 任务台账持久化 | plugin-db 表 `transfer_entries` | storage 单键（peer.rs `ENTRIES_KEY`） |
//! | 推送 wire 字段名 | `path` | `safTreeUri` |
//! | 节点电源 | 插件显式请求起停 | **显性 unsupported**（宿主外壳驱动） |
//! | 目录多选 / 打开所在目录 | 支持 | 不支持（核内默认 unsupported） |
//! | 信任决策 | 互调认证中心（auth_center） | 直答 `peer-respond-consent` |
//! | 旧快照对账 | 仍订阅（双写期） | 已整条退役 |

use bedcode_file_transfer_core::domain::SharedRoot;
use bedcode_file_transfer_core::ports::{
    BusPort, ConsentGate, EventPort, KvStore, LogPort, MdnsPort, NodePower, PeerPort, PlatformPort,
    PluginProfile, PortError, PortResult, RootWireCodec, RootsStore,
};
use bedcode_plugin_api::host::{
    HostBus, HostError, HostEvents, HostLog, HostMdns, HostPeer, HostPlatform, HostStorage,
};

/// 移动端端口包：`&H`（`H` = 任一满足桌面 SDK 对应 trait 的宿主句柄；共享产物
/// 两端都是 `WasmHost`——import 经桌面 SDK 绑定发出，移动宿主由 core_binding 提供）
pub(crate) struct MobilePorts<'a, H: ?Sized>(pub(crate) &'a H);

/// `HostError` → `PortError`：消息逐字保留（适配层不得吞掉宿主给的真实原因）
fn map_err<T>(r: Result<T, HostError>) -> PortResult<T> {
    r.map_err(|e| PortError::new(e.to_string()))
}

// ==================== 观测面 ====================

impl<H: HostLog + ?Sized> LogPort for MobilePorts<'_, H> {
    fn log_info(&self, msg: &str) {
        self.0.log_info(msg);
    }

    fn log_error(&self, msg: &str) {
        self.0.log_error(msg);
    }
}

impl<H: HostEvents + ?Sized> EventPort for MobilePorts<'_, H> {
    fn emit_event(&self, name: &str, payload: &serde_json::Value) {
        self.0.emit_event(name, payload);
    }
}

impl<H: HostBus + ?Sized> BusPort for MobilePorts<'_, H> {
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

impl<H: HostStorage + ?Sized> KvStore for MobilePorts<'_, H> {
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

/// 移动端共享根存储键（与退役前逐字一致：换键 = 用户已配置的共享目录凭空消失）
pub(crate) const ROOTS_KEY: &str = "shared_roots";

/// 差异面①：移动 = `host-storage` 单键 JSON 数组（桌面 = plugin-db 表）。
///
/// 损坏值回空表（与退役前逐字一致：整读整写的 KV 形态下，坏值只可能是外力损坏）。
impl<H: HostStorage + ?Sized> RootsStore for MobilePorts<'_, H> {
    fn load_roots(&self) -> PortResult<Vec<SharedRoot>> {
        let Some(v) = self.storage_get(ROOTS_KEY)? else {
            return Ok(vec![]);
        };
        Ok(serde_json::from_value(v).unwrap_or_default())
    }

    fn save_roots(&self, roots: &[SharedRoot]) -> PortResult<()> {
        let value = serde_json::to_value(roots)
            .map_err(|e| PortError::new(format!("shared_roots serialize failed: {e}")))?;
        self.storage_set(ROOTS_KEY, &value)
    }
}

/// 差异面③：移动端推送载荷字段名 = `safTreeUri`（桌面 = `path`）——引擎按 SAF
/// 树 URI 解析目录，真实路径会被整批拒绝
impl<H: ?Sized> RootWireCodec for MobilePorts<'_, H> {
    fn roots_to_push_payload(&self, roots: &[SharedRoot]) -> Vec<serde_json::Value> {
        roots
            .iter()
            .map(|r| serde_json::json!({ "id": r.id, "name": r.name, "safTreeUri": r.path }))
            .collect()
    }
}

// ==================== 对等网络面 ====================

impl<H: HostPeer + ?Sized> PeerPort for MobilePorts<'_, H> {
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

/// 差异面④：移动端**不接管节点电源**（宿主外壳驱动 activate/deactivate 时起停）。
///
/// 显性 `Err` 而非「不实现 trait」：共享产物的 activate/deactivate 编排是同一份
/// 代码（`platform` 分支只决定**调不调**），trait 必须可编译；误调用（= 平台探测
/// 失效）时 fail-visible 报错，不会静默无操作。无需 `HostPeer` 约束——本实现不
/// 委派任何宿主原语（委派才需要约束，桌面侧同理见 `adapters.rs`）。
impl<H: ?Sized> NodePower for MobilePorts<'_, H> {
    fn peer_start_node(&self) -> PortResult<bool> {
        Err(PortError::new(
            "unsupported on mobile: peer node power is driven by the host shell \
             (plugin does not own node lifecycle)",
        ))
    }

    fn peer_stop_node(&self) -> PortResult<bool> {
        Err(PortError::new(
            "unsupported on mobile: peer node power is driven by the host shell \
             (plugin does not own node lifecycle)",
        ))
    }
}

// ==================== 平台交互面 ====================

/// 差异面⑥：移动端无「打开所在目录」（不实现 ⇒ 核内默认显性 unsupported）；
/// `pick_folders` 双端均有（2026-10-13 功能级统一后走 core `host-platform`，
/// SAF 单选语义由宿主返回 0/1 项）
impl<H: HostPlatform + ?Sized> PlatformPort for MobilePorts<'_, H> {
    fn platform_pick_files(&self) -> PortResult<Vec<String>> {
        map_err(self.0.platform_pick_files())
    }

    fn platform_pick_folders(&self) -> PortResult<Vec<String>> {
        map_err(self.0.platform_pick_folders())
    }
}

// ==================== mDNS ====================

impl<H: HostMdns + ?Sized> MdnsPort for MobilePorts<'_, H> {
    fn mdns_browse(&self, service_type: &str) -> PortResult<String> {
        map_err(self.0.mdns_browse(service_type))
    }

    fn mdns_stop_browse(&self, browser_id: &str) -> PortResult<bool> {
        map_err(self.0.mdns_stop_browse(browser_id))
    }
}

// ==================== 形态能力位 ====================

/// 差异面⑤⑧：移动端落点固定（页面无落点设置项）、旧快照 topic 已整条退役
impl<H: ?Sized> PluginProfile for MobilePorts<'_, H> {
    fn uses_legacy_snapshot(&self) -> bool {
        false
    }

    fn supports_custom_download_dir(&self) -> bool {
        false
    }
}

// ==================== 信任决策面 ====================

/// 差异面⑦：移动端**无认证中心接入**，直答宿主原语（桌面经互调认证中心）
impl<H: HostPeer + ?Sized> ConsentGate for MobilePorts<'_, H> {
    fn decide_consent(&self, request_id: &str, accepted: bool) -> PortResult<bool> {
        self.peer_respond_consent(request_id, accepted)
    }

    /// 移动端无「已信任则免确认自动放行」的预检路径：恒返回 false = 交由前端弹窗询问
    fn evaluate_consent(&self, _payload: &serde_json::Value) -> PortResult<bool> {
        Ok(false)
    }

    fn list_trusted(&self) -> PortResult<serde_json::Value> {
        self.peer_list_trusted()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    /// 只覆写被测面的 SDK 句柄夹具（其余方法 unreachable!，调用即 panic）
    #[derive(Default)]
    struct FakeHost {
        kv: RefCell<HashMap<String, serde_json::Value>>,
        fail_set: bool,
    }

    impl HostStorage for FakeHost {
        fn storage_get(&self, key: &str) -> Result<Option<serde_json::Value>, HostError> {
            Ok(self.kv.borrow().get(key).cloned())
        }

        fn storage_set(&self, key: &str, value: &serde_json::Value) -> Result<(), HostError> {
            if self.fail_set {
                return Err(HostError::custom(-1, "storage write rejected"));
            }
            self.kv.borrow_mut().insert(key.to_string(), value.clone());
            Ok(())
        }

        fn storage_delete(&self, key: &str) -> Result<(), HostError> {
            self.kv.borrow_mut().remove(key);
            Ok(())
        }
    }

    #[test]
    fn roots_store_round_trips_under_mobile_key() {
        let h = FakeHost::default();
        let ports = MobilePorts(&h);
        assert!(ports.load_roots().unwrap().is_empty(), "无值时回空表");
        let roots = vec![SharedRoot {
            id: "root-1".into(),
            name: "相册".into(),
            path: "content://tree/1".into(),
        }];
        ports.save_roots(&roots).unwrap();
        assert_eq!(ports.load_roots().unwrap(), roots);
        // 落点必须是移动端既有 storage 键（换键 = 用户已配置的共享目录凭空消失）
        assert!(h.kv.borrow().contains_key("shared_roots"));
    }

    #[test]
    fn roots_store_reports_host_error_instead_of_swallowing() {
        let h = FakeHost {
            fail_set: true,
            ..Default::default()
        };
        let err = MobilePorts(&h).save_roots(&[]).unwrap_err();
        assert!(
            err.message().contains("storage write rejected"),
            "宿主原因必须透传：{err}"
        );
    }

    #[test]
    fn corrupt_stored_roots_degrade_to_empty() {
        let h = FakeHost::default();
        h.kv
            .borrow_mut()
            .insert("shared_roots".into(), serde_json::json!("broken"));
        assert!(MobilePorts(&h).load_roots().unwrap().is_empty());
    }

    #[test]
    fn push_payload_uses_saf_tree_uri_field() {
        // 差异面③的移动一侧：字段名必须是 safTreeUri（引擎按 SAF 树 URI 解析目录）
        let h = FakeHost::default();
        let roots = vec![SharedRoot {
            id: "r1".into(),
            name: "相册".into(),
            path: "content://tree/1".into(),
        }];
        assert_eq!(
            MobilePorts(&h).roots_to_push_payload(&roots),
            vec![
                serde_json::json!({ "id": "r1", "name": "相册", "safTreeUri": "content://tree/1" })
            ]
        );
    }

    #[test]
    fn node_power_is_explicitly_unsupported_not_silent() {
        // 差异面④：移动不接管电源；误调用必须 fail-visible（静默无操作会让
        // 「节点没起」表现成「插件坏了」却无线索）
        let h = FakeHost::default();
        let err = MobilePorts(&h).peer_start_node().unwrap_err();
        assert!(err.message().contains("unsupported on mobile"), "{err}");
        assert!(err.message().contains("host shell"), "{err}");
    }

    #[test]
    fn profile_bits_are_mobile_shaped() {
        let h = FakeHost::default();
        let p = MobilePorts(&h);
        assert!(!p.supports_custom_download_dir(), "移动固定 MediaStore.Downloads");
        assert!(!p.uses_legacy_snapshot(), "移动旧快照通路已退役");
    }

    // ==================== 差异面⑦：ConsentGate ====================

    /// peer 面替身：只覆写 `ConsentGate` 实际用到的两个原语，其余 `unreachable!()`
    /// ——误调用当场 panic（严格模式），而不是悄悄返回默认值让断言失去意义
    #[derive(Default)]
    struct FakePeer {
        /// 记录 `peer_respond_consent` 收到的参数，供「逐字委派」断言
        consent_calls: RefCell<Vec<(String, bool)>>,
        /// 置位则两个原语都失败（反例：错误必须透传而非被适配层吞掉）
        fail: bool,
    }

    impl FakePeer {
        fn hit(&self, request_id: &str, accepted: bool) -> Result<bool, HostError> {
            self.consent_calls
                .borrow_mut()
                .push((request_id.to_string(), accepted));
            if self.fail {
                return Err(HostError::custom(-1, "peer consent rejected by host"));
            }
            Ok(true)
        }
    }

    impl HostPeer for FakePeer {
        fn peer_dial(&self, _endpoint: &serde_json::Value) -> Result<String, HostError> {
            unreachable!("ConsentGate 不发起拨号")
        }
        fn peer_close(&self, _handle: &str) -> Result<bool, HostError> {
            unreachable!("ConsentGate 不关闭会话")
        }
        fn peer_respond_consent(&self, request_id: &str, accepted: bool) -> Result<bool, HostError> {
            self.hit(request_id, accepted)
        }
        fn peer_list_trusted(&self) -> Result<serde_json::Value, HostError> {
            if self.fail {
                return Err(HostError::custom(-1, "trust store unavailable"));
            }
            Ok(serde_json::json!([{ "nodeId": "peer-1" }]))
        }
        fn peer_revoke_trusted(&self, _node_id: &str) -> Result<bool, HostError> {
            unreachable!("撤销不经本端口（两端都走宿主原语直通）")
        }
        fn peer_send_files(
            &self,
            _session: &str,
            _paths: &[serde_json::Value],
        ) -> Result<String, HostError> {
            unreachable!()
        }
        fn peer_respond_transfer(
            &self,
            _batch_id: &str,
            _accept: bool,
        ) -> Result<(), HostError> {
            unreachable!()
        }
        fn peer_set_receive_policy(&self, _mode: &str, _timeout_secs: u64) -> Result<(), HostError> {
            unreachable!()
        }
        fn peer_pause_transfer(&self, _batch_id: &str) -> Result<(), HostError> {
            unreachable!()
        }
        fn peer_resume_transfer(&self, _batch_id: &str) -> Result<(), HostError> {
            unreachable!()
        }
        fn peer_set_shared_roots(&self, _dirs: &[serde_json::Value]) -> Result<(), HostError> {
            unreachable!()
        }
        fn peer_list_shared_roots(&self, _session: &str) -> Result<serde_json::Value, HostError> {
            unreachable!()
        }
        fn peer_browse_directory(
            &self,
            _session: &str,
            _node_id: &str,
            _dir: &str,
        ) -> Result<serde_json::Value, HostError> {
            unreachable!()
        }
        fn peer_pull_files(
            &self,
            _session: &str,
            _dir_id: &str,
            _files: &[serde_json::Value],
        ) -> Result<u32, HostError> {
            unreachable!()
        }
        fn peer_set_download_dir(&self, _path: &str) -> Result<(), HostError> {
            unreachable!()
        }
        fn peer_start_node(&self) -> Result<bool, HostError> {
            unreachable!("移动端电源由宿主外壳驱动，插件不调（差异面④）")
        }
        fn peer_stop_node(&self) -> Result<bool, HostError> {
            unreachable!("移动端电源由宿主外壳驱动，插件不调（差异面④）")
        }
        fn peer_active_transfers(&self) -> Result<serde_json::Value, HostError> {
            unreachable!()
        }
        fn peer_collect_outgoing(
            &self,
            _paths: &[serde_json::Value],
        ) -> Result<serde_json::Value, HostError> {
            unreachable!()
        }
    }

    /// 差异面⑦移动侧正例：无认证中心 ⇒ 直答宿主原语，参数逐字、返回值即 hit
    #[test]
    fn mobile_consent_gate_answers_host_primitive_directly() {
        let h = FakePeer::default();
        let hit = MobilePorts(&h).decide_consent("req-7", true).unwrap();
        assert!(hit, "原语返回的命中位必须原样透出（编排层据此回前端 hit）");
        assert_eq!(
            h.consent_calls.borrow().as_slice(),
            &[("req-7".to_string(), true)],
            "request_id / accepted 必须逐字委派（改名即静默改写用户裁决对象）"
        );
    }

    /// 差异面⑦移动侧反例：宿主原语失败必须透传原因（吞掉 = 前端拿到 hit:false，
    /// 把「宿主拒绝」误报成「没有待应答项」）
    #[test]
    fn mobile_consent_gate_surfaces_host_error() {
        let h = FakePeer {
            fail: true,
            ..Default::default()
        };
        let err = MobilePorts(&h).decide_consent("req-8", false).unwrap_err();
        assert!(
            err.message().contains("peer consent rejected by host"),
            "宿主原因必须逐字透传：{err}"
        );
    }

    /// 差异面⑦移动侧：信任列表直答原语（不做认证中心视图映射——移动无该接入）
    #[test]
    fn mobile_consent_gate_lists_trusted_from_host() {
        let h = FakePeer::default();
        assert_eq!(
            MobilePorts(&h).list_trusted().unwrap(),
            serde_json::json!([{ "nodeId": "peer-1" }]),
            "移动端信任列表 = 宿主原语返回值，不经任何映射"
        );
    }

    /// 差异面⑦移动侧预检：恒 false = 交前端弹窗（移动无「已信任免确认」路径）。
    /// 覆盖畸形 payload：预检在任何输入下都不得 panic，也不得误判为已放行
    #[test]
    fn mobile_consent_gate_precheck_never_auto_approves() {
        let h = FakePeer::default();
        for payload in [
            serde_json::json!({}),
            serde_json::Value::Null,
            serde_json::json!({ "trusted": true }),
        ] {
            assert!(
                !MobilePorts(&h).evaluate_consent(&payload).unwrap(),
                "移动端预检必须恒 false（含畸形 payload {payload}）：\
                 误判 true 会让未信任对端免确认直连"
            );
        }
        assert!(
            h.consent_calls.borrow().is_empty(),
            "预检不得顺手应答宿主（应答只发生在 decide_consent）"
        );
    }
}
