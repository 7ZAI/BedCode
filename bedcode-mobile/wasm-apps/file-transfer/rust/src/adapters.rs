//! 共享核端口适配器（移动端）——**双端差异的唯一落点**。
//!
//! 共享核（`bedcode-file-transfer-core`，ADR 0044）只认 `core::ports` 的 trait；本模块把
//! 移动 SDK 的宿主 trait 适配成那些端口。纪律：
//!
//! 1. **1:1 委派，零判据**：这里不写任何 `if`、不补默认值、不做重试——任何「顺手加的逻辑」
//!    都会变成第三份判据（核一份、桌面一份、这里一份），漂移必然发生。
//! 2. **错误只做类型转换**：`HostError` → `PortError`，消息逐字保留（`Display` 透传），
//!    否则宿主日志里的真实原因会被适配层吃掉。
//! 3. **移动端形态差异写在本文件**（`RootWireCodec` 用 `safTreeUri`、`NodePower` 不接线、
//!    `PlatformPort` 只实现 pick 两项）——差异集中在适配器而非散落核内。
//!
//! 泛型而非具体 `WasmHost`：核的消费面（`peer.rs` 等）多数函数声明为
//! `h: &(impl HostStorage + HostPeer)`，适配器必须能包住这类抽象句柄。SDK trait 的方法签名
//! 都是 `&self`，故适配器持有 `&H` 即可，无需所有权。

use bedcode_file_transfer_core::domain::SharedRoot;
use bedcode_file_transfer_core::ports::{
    BusPort, ConsentGate, EventPort, KvStore, LogPort, MdnsPort, PeerPort, PlatformPort,
    PluginProfile, PortError, PortResult, RootWireCodec, RootsStore,
};
use bedcode_plugin_api_mobile::host::{
    HostBus, HostError, HostEvents, HostLog, HostMdns, HostPeer, HostPlatform, HostStorage,
};

/// 移动端端口包：`&H`（`H` = 任一满足对应 SDK trait 的宿主句柄，通常是 `WasmHost`）
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

/// 差异面①：移动端 = `host-storage` 单键 JSON 数组（桌面 = plugin-db 表）
impl<H: HostStorage + ?Sized> RootsStore for MobilePorts<'_, H> {
    fn load_roots(&self) -> PortResult<Vec<SharedRoot>> {
        let Some(v) = self.storage_get(crate::roots_registry::ROOTS_KEY)? else {
            return Ok(vec![]);
        };
        // 损坏值回空表（与退役前逐字一致：整读整写的 KV 形态下，坏值只可能是外力损坏）
        Ok(serde_json::from_value(v).unwrap_or_default())
    }

    fn save_roots(&self, roots: &[SharedRoot]) -> PortResult<()> {
        let value = serde_json::to_value(roots)
            .map_err(|e| PortError::new(format!("shared_roots serialize failed: {e}")))?;
        self.storage_set(crate::roots_registry::ROOTS_KEY, &value)
    }
}

/// 差异面③：移动端推送载荷字段名 = `safTreeUri`（桌面 = `path`）
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

// 差异面④：**不实现 `NodePower`** —— 移动端节点生命周期由宿主外壳驱动，插件不持有电源。
// 核内默认实现即「本端不支持」，误接线到电源原语会在调用点显性报错（fail-visible）。

// ==================== 平台交互面 ====================

/// 差异面⑤⑥：移动端只有 `pick_files` / `pick_folder`（SAF 选择器）；
/// 多选与「打开所在目录」桌面独有，不实现 ⇒ 核内默认显性 unsupported
impl<H: HostPlatform + ?Sized> PlatformPort for MobilePorts<'_, H> {
    fn platform_pick_files(&self) -> PortResult<Vec<String>> {
        map_err(self.0.platform_pick_files())
    }

    fn platform_pick_folder(&self) -> PortResult<String> {
        map_err(self.0.platform_pick_folder())
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

// ==================== 信任决策面 ====================

// ==================== 形态能力位 ====================

/// 差异面⑤⑧：移动端落点固定（页面无落点设置项）、旧快照 topic 已整条退役。
///
/// 两个方法都用默认值 `false` 是**刻意显式**的：形态位写在这里，评审时一眼能看到
/// 「移动端不做什么」，而不是散落在核内的 `if` 里。
impl<H: ?Sized> PluginProfile for MobilePorts<'_, H> {
    fn uses_legacy_snapshot(&self) -> bool {
        false
    }

    fn supports_custom_download_dir(&self) -> bool {
        false
    }
}

/// 差异面⑦：移动端**无认证中心接入**，直答宿主原语（桌面经互调认证中心）。
impl<H: HostPeer + ?Sized> ConsentGate for MobilePorts<'_, H> {
    fn decide_consent(&self, request_id: &str, accepted: bool) -> PortResult<bool> {
        self.peer_respond_consent(request_id, accepted)
    }

    /// 移动端无「已信任则免确认自动放行」的预检路径：恒返回 false = 交由前端弹窗询问。
    /// （桌面走认证中心 `auth.decide-consent` 预检后可能自动应答。）
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
        h.kv.borrow_mut()
            .insert("shared_roots".into(), serde_json::json!("broken"));
        assert!(MobilePorts(&h).load_roots().unwrap().is_empty());
    }

    #[test]
    fn push_payload_uses_saf_tree_uri_field() {
        // 差异面③的移动端一侧：字段名必须是 safTreeUri（引擎按 SAF 树 URI 解析目录）
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
}
