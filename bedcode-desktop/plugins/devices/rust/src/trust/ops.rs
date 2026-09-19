//! trust 编排 —— 统一视图 list / 撤销 revoke / 新增 add_pairing（票 08 B3）
//!
//! 行为等价对照（宿主实现基准，逐条见模块文档）：
//! - `list`：pairing 段只含 active 记录、按 `paired_at` DESC（= 宿主
//!   `get_pairings` 的 `WHERE is_active=1 ORDER BY paired_at DESC`）；
//!   peer 段透传宿主 `list_trusted_peers` 条目序；两段合并统一数组
//! - `revoke`：pairing 软删（宿主 `remove_pairing` 的 `is_active=0`）；
//!   peer 经 host-peer `revoke-trusted`（宿主 `revoke_trusted_peer` 语义）
//! - `add_pairing`：新增记录（`connect_count=1`、`active=true`、RFC3339
//!   配对时刻）——宿主 `add_pairing` 的插入语义（对照测试注入用）
//!
//! 宿主交互经 [`store::HostAccess`] + host-peer 原语；wasm 运行时走
//! `WasmHost`，native 单测注入 mock。lib.rs 命令面经 `*_via_host`
//! 入口（cfg 分流，与 pairing/keys.rs 同模式——native 链接不引用
//! wasm 专属 import 符号）。

use super::model::TrustedDeviceDto;
use super::store::{self, HostAccess};
use bedcode_plugin_api::host::HostPeer;
#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::wasm_host::WasmHost;

/// 统一视图响应：pairing 段 + peer 段合并，按 `added_at` DESC 稳定排序
/// （宿主两侧各自的时序语义：pairings 按 paired_at DESC；peers 按
/// trust_store 条目序——统一数组按加入时刻降序，最新加入在前）。
///
/// peer 侧不可用（无头上下文 / peer-net 未启动）时：pairing 段照常返回，
/// `peerError` 透出宿主错误（不静默降级为空列表——避免「看似成功实则缺失」）。
pub fn list(h: &impl HostAccess, peer: &impl HostPeer) -> Result<serde_json::Value, String> {
    // pairing 段：active 过滤 + paired_at DESC（宿主 get_pairings 语义）
    let mut records = store::load(h)?;
    records.retain(|r| r.active);
    records.sort_by(|a, b| b.paired_at.cmp(&a.paired_at));

    // peer 段：透传宿主 list-trusted（TrustedPeerDto JSON 数组，条目序保留）
    let mut peers: Vec<serde_json::Value> = Vec::new();
    let peer_error = match peer.peer_list_trusted() {
        Ok(v) => {
            peers = v
                .as_array()
                .cloned()
                .unwrap_or_default();
            None
        }
        Err(e) => Some(e.message),
    };

    let mut devices: Vec<serde_json::Value> = Vec::with_capacity(records.len() + peers.len());
    for record in records {
        devices.push(
            serde_json::to_value(TrustedDeviceDto::from_pairing(&record))
                .map_err(|e| format!("serialize trust dto: {}", e))?,
        );
    }
    for peer in peers {
        devices.push(
            serde_json::to_value(TrustedDeviceDto::from_peer(&peer))
                .map_err(|e| format!("serialize peer dto: {}", e))?,
        );
    }

    Ok(serde_json::json!({
        "devices": devices,
        "peerError": peer_error,
    }))
}

/// 撤销统一条目：pairing 目标软删（宿主 `remove_pairing` 语义）；peer 目标
/// 经 host-peer `revoke-trusted`（宿主 `revoke_trusted_peer` 语义，返回是否
/// 删除）。返回 `{ removed, kind }`。
///
/// 未命中：pairing 幂等 `removed=false` 不报错（宿主 UPDATE 影响 0 行）；
/// peer 目标无权限/引擎不可用时上抛错误（不做「看似成功」的假撤销）。
pub fn revoke(h: &impl HostAccess, peer: &impl HostPeer, id: &str) -> Result<serde_json::Value, String> {
    // pairing 优先：命中即软删
    if store::revoke_record(h, id)? {
        return Ok(serde_json::json!({ "removed": true, "kind": "pairing" }));
    }
    // peer 目标：host-peer revoke-trusted（node_id 寻址）
    let removed = peer.peer_revoke_trusted(id).map_err(|e| e.message)?;
    Ok(serde_json::json!({ "removed": removed, "kind": "peer" }))
}

/// 新增 pairing 记录（配对完成流写入入口）。`paired_at` 缺省取当前时刻
/// （RFC3339，UTC；注入测试可显式传参）。
///
/// 返回新记录 ID（撤销/列表寻址）。
pub fn add_pairing(
    h: &impl HostAccess,
    device_name: &str,
    device_fingerprint: &str,
    address: Option<&str>,
) -> Result<String, String> {
    let now = crate::pairing::jwt::now_secs();
    let id = now.to_string();
    let paired_at = crate::pairing::code::format_rfc3339_utc(now);
    store::add_record(h, &id, device_name, device_fingerprint, address, &paired_at)?;
    Ok(id)
}

// ==================== 命令面入口（cfg 分流，native 显性失败） ====================
//
// lib.rs 命令面只调这些入口；native（cargo test）下 WasmHost 无
// HostAccess/HostPeer impl（wasm 专属 import 符号不在 native 链接），
// 返回显性错误——命令面是 wasm 运行时才可用的路径（同 keys.rs）。

/// 统一视图列表（wasm 运行时）
#[cfg(target_arch = "wasm32")]
pub fn list_via_host() -> Result<serde_json::Value, String> {
    list(&WasmHost, &WasmHost)
}

/// 统一视图列表（native 无宿主环境）
#[cfg(not(target_arch = "wasm32"))]
pub fn list_via_host() -> Result<serde_json::Value, String> {
    Err("trust list unavailable outside wasm runtime".to_string())
}

/// 撤销统一条目（wasm 运行时）
#[cfg(target_arch = "wasm32")]
pub fn revoke_via_host(id: &str) -> Result<serde_json::Value, String> {
    revoke(&WasmHost, &WasmHost, id)
}

/// 撤销统一条目（native 无宿主环境）
#[cfg(not(target_arch = "wasm32"))]
pub fn revoke_via_host(_id: &str) -> Result<serde_json::Value, String> {
    Err("trust revoke unavailable outside wasm runtime".to_string())
}

/// 新增 pairing 记录（wasm 运行时）
#[cfg(target_arch = "wasm32")]
pub fn add_pairing_via_host(
    device_name: &str,
    device_fingerprint: &str,
    address: Option<&str>,
) -> Result<String, String> {
    add_pairing(&WasmHost, device_name, device_fingerprint, address)
}

/// 新增 pairing 记录（native 无宿主环境）
#[cfg(not(target_arch = "wasm32"))]
pub fn add_pairing_via_host(
    _device_name: &str,
    _device_fingerprint: &str,
    _address: Option<&str>,
) -> Result<String, String> {
    Err("trust add-pairing unavailable outside wasm runtime".to_string())
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trust::store::tests::MockHost;
    use std::sync::Mutex;

    /// mock host-peer（native 单测；peer_list_trusted 返回 TrustedPeerDto
    /// JSON 数组，与宿主 list_trusted_peers wire 形状一致）
    struct MockPeer {
        trusted: Mutex<Vec<serde_json::Value>>,
    }

    impl MockPeer {
        fn new(trusted: Vec<serde_json::Value>) -> Self {
            Self {
                trusted: Mutex::new(trusted),
            }
        }
    }

    impl HostPeer for MockPeer {
        fn peer_dial(&self, _endpoint: &serde_json::Value) -> Result<String, bedcode_plugin_api::host::HostError> {
            unimplemented!()
        }
        fn peer_close(&self, _handle: &str) -> Result<bool, bedcode_plugin_api::host::HostError> {
            unimplemented!()
        }
        fn peer_respond_consent(
            &self,
            _request_id: &str,
            _accepted: bool,
        ) -> Result<bool, bedcode_plugin_api::host::HostError> {
            unimplemented!()
        }
        fn peer_list_trusted(&self) -> Result<serde_json::Value, bedcode_plugin_api::host::HostError> {
            Ok(serde_json::Value::Array(self.trusted.lock().unwrap().clone()))
        }
        fn peer_revoke_trusted(&self, node_id: &str) -> Result<bool, bedcode_plugin_api::host::HostError> {
            let mut trusted = self.trusted.lock().unwrap();
            let before = trusted.len();
            trusted.retain(|t| t["nodeId"] != node_id);
            Ok(trusted.len() < before)
        }
        fn peer_send_files(
            &self,
            _session: &str,
            _paths: &[serde_json::Value],
        ) -> Result<String, bedcode_plugin_api::host::HostError> {
            unimplemented!()
        }
        fn peer_respond_transfer(
            &self,
            _batch_id: &str,
            _accept: bool,
        ) -> Result<(), bedcode_plugin_api::host::HostError> {
            unimplemented!()
        }
        fn peer_set_receive_policy(
            &self,
            _mode: &str,
            _timeout_secs: u64,
        ) -> Result<(), bedcode_plugin_api::host::HostError> {
            unimplemented!()
        }
        fn peer_pause_transfer(&self, _batch_id: &str) -> Result<(), bedcode_plugin_api::host::HostError> {
            unimplemented!()
        }
        fn peer_resume_transfer(&self, _batch_id: &str) -> Result<(), bedcode_plugin_api::host::HostError> {
            unimplemented!()
        }
        fn peer_resume_all_transfers(&self) -> Result<u32, bedcode_plugin_api::host::HostError> {
            unimplemented!()
        }
        fn peer_set_shared_roots(
            &self,
            _dirs: &[serde_json::Value],
        ) -> Result<(), bedcode_plugin_api::host::HostError> {
            unimplemented!()
        }
        fn peer_list_shared_roots(
            &self,
            _session: &str,
        ) -> Result<serde_json::Value, bedcode_plugin_api::host::HostError> {
            unimplemented!()
        }
        fn peer_browse_directory(
            &self,
            _session: &str,
            _dir_id: &str,
            _rel_path: &str,
        ) -> Result<serde_json::Value, bedcode_plugin_api::host::HostError> {
            unimplemented!()
        }
        fn peer_pull_files(
            &self,
            _session: &str,
            _dir_id: &str,
            _files: &[serde_json::Value],
        ) -> Result<u32, bedcode_plugin_api::host::HostError> {
            unimplemented!()
        }
        fn peer_set_download_dir(&self, _path: &str) -> Result<(), bedcode_plugin_api::host::HostError> {
            unimplemented!()
        }
    }

    fn sample_peer(node_id: &str, name: &str) -> serde_json::Value {
        serde_json::json!({
            "nodeId": node_id,
            "displayName": name,
            "fingerprintShort": &node_id[..8],
            "addedAt": "2026-09-18T12:00:00Z",
        })
    }

    // ==================== list：统一视图组装 ====================

    /// 空信任列表 → 空 devices（pairing/peer 均空）
    #[test]
    fn list_empty_trust_returns_no_devices() {
        let host = MockHost::new();
        let peer = MockPeer::new(vec![]);
        let r = list(&host, &peer).expect("list");
        assert_eq!(r["devices"].as_array().unwrap().len(), 0);
        assert!(r["peerError"].is_null());
    }

    /// pairing 段：active 过滤 + paired_at DESC 排序（宿主 get_pairings 语义），
    /// 字段与宿主 Pairing 公开视图对齐
    #[test]
    fn list_pairings_only_active_sorted_by_paired_at_desc() {
        let host = MockHost::new();
        // 按插入序为乱序：新/旧/已撤销
        store::add_record(&host, "old", "旧设备", "fp-old", None, "2026-09-01T00:00:00Z").expect("old");
        store::add_record(&host, "new", "新设备", "fp-new", None, "2026-09-19T00:00:00Z").expect("new");
        store::add_record(&host, "revoked", "已撤销", "fp-rev", None, "2026-09-10T00:00:00Z").expect("revoked");
        store::revoke_record(&host, "revoked").expect("revoke revoked");

        let peer = MockPeer::new(vec![]);
        let r = list(&host, &peer).expect("list");
        let devices = r["devices"].as_array().unwrap();

        assert_eq!(devices.len(), 2, "已撤销记录不进入列表（is_active=1 过滤）");
        assert_eq!(devices[0]["kind"], "pairing");
        assert_eq!(devices[0]["id"], "new", "最新配对在前（paired_at DESC）");
        assert_eq!(devices[1]["id"], "old");
        assert_eq!(devices[0]["name"], "新设备");
        assert_eq!(devices[0]["fingerprint"], "fp-new");
        assert_eq!(devices[0]["addedAt"], "2026-09-19T00:00:00Z");
        assert_eq!(devices[0]["active"], true);
    }

    /// peer 段：透传宿主 list-trusted 条目（kind=peer，条目序保留），
    /// 与 pairing 段合并统一数组
    #[test]
    fn list_merges_peers_after_pairings() {
        let host = MockHost::new();
        store::add_record(&host, "p-1", "Phone", "fp-1", None, "2026-09-19T00:00:00Z").expect("add");

        let peer = MockPeer::new(vec![sample_peer("aabbccdd", "书房台式机")]);
        let r = list(&host, &peer).expect("list");
        let devices = r["devices"].as_array().unwrap();

        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0]["kind"], "pairing");
        assert_eq!(devices[1]["kind"], "peer");
        assert_eq!(devices[1]["id"], "aabbccdd");
        assert_eq!(devices[1]["name"], "书房台式机");
        assert_eq!(devices[1]["fingerprintShort"], "aabbccdd");
        assert_eq!(devices[1]["addedAt"], "2026-09-18T12:00:00Z");
    }

    /// peer 侧不可用（无头上下文/引擎未启动）：pairing 段照常返回，
    /// peerError 透出错误——不静默降级为空列表
    #[test]
    fn list_surfaces_peer_error_without_dropping_pairings() {
        struct PeerDown;
        impl HostPeer for PeerDown {
            fn peer_dial(&self, _e: &serde_json::Value) -> Result<String, bedcode_plugin_api::host::HostError> {
                unimplemented!()
            }
            fn peer_close(&self, _h: &str) -> Result<bool, bedcode_plugin_api::host::HostError> {
                unimplemented!()
            }
            fn peer_respond_consent(
                &self,
                _r: &str,
                _a: bool,
            ) -> Result<bool, bedcode_plugin_api::host::HostError> {
                unimplemented!()
            }
            fn peer_list_trusted(&self) -> Result<serde_json::Value, bedcode_plugin_api::host::HostError> {
                Err(bedcode_plugin_api::host::HostError::custom(
                    -1,
                    "peer-net unavailable in headless context".to_string(),
                ))
            }
            fn peer_revoke_trusted(&self, _n: &str) -> Result<bool, bedcode_plugin_api::host::HostError> {
                unimplemented!()
            }
            fn peer_send_files(
                &self,
                _s: &str,
                _p: &[serde_json::Value],
            ) -> Result<String, bedcode_plugin_api::host::HostError> {
                unimplemented!()
            }
            fn peer_respond_transfer(
                &self,
                _b: &str,
                _a: bool,
            ) -> Result<(), bedcode_plugin_api::host::HostError> {
                unimplemented!()
            }
            fn peer_set_receive_policy(
                &self,
                _m: &str,
                _t: u64,
            ) -> Result<(), bedcode_plugin_api::host::HostError> {
                unimplemented!()
            }
            fn peer_pause_transfer(&self, _b: &str) -> Result<(), bedcode_plugin_api::host::HostError> {
                unimplemented!()
            }
            fn peer_resume_transfer(&self, _b: &str) -> Result<(), bedcode_plugin_api::host::HostError> {
                unimplemented!()
            }
            fn peer_resume_all_transfers(&self) -> Result<u32, bedcode_plugin_api::host::HostError> {
                unimplemented!()
            }
            fn peer_set_shared_roots(
                &self,
                _d: &[serde_json::Value],
            ) -> Result<(), bedcode_plugin_api::host::HostError> {
                unimplemented!()
            }
            fn peer_list_shared_roots(
                &self,
                _s: &str,
            ) -> Result<serde_json::Value, bedcode_plugin_api::host::HostError> {
                unimplemented!()
            }
            fn peer_browse_directory(
                &self,
                _s: &str,
                _d: &str,
                _r: &str,
            ) -> Result<serde_json::Value, bedcode_plugin_api::host::HostError> {
                unimplemented!()
            }
            fn peer_pull_files(
                &self,
                _s: &str,
                _d: &str,
                _f: &[serde_json::Value],
            ) -> Result<u32, bedcode_plugin_api::host::HostError> {
                unimplemented!()
            }
            fn peer_set_download_dir(&self, _p: &str) -> Result<(), bedcode_plugin_api::host::HostError> {
                unimplemented!()
            }
        }

        let host = MockHost::new();
        store::add_record(&host, "p-1", "Phone", "fp-1", None, "2026-09-19T00:00:00Z").expect("add");
        let r = list(&host, &PeerDown).expect("list");
        let devices = r["devices"].as_array().unwrap();
        assert_eq!(devices.len(), 1, "pairing 段照常返回");
        assert_eq!(devices[0]["id"], "p-1");
        let err = r["peerError"].as_str().expect("peerError 必须透出");
        assert!(err.contains("unavailable"), "peerError 内容透出: {}", err);
    }

    // ==================== revoke：撤销分派 ====================

    /// 撤销 pairing：软删后列表立即消失；返回值 kind=pairing
    #[test]
    fn revoke_pairing_removes_from_list_immediately() {
        let host = MockHost::new();
        let peer = MockPeer::new(vec![]);
        let id = add_pairing(&host, "Phone", "fp-1", None).expect("add");
        assert_eq!(list(&host, &peer).expect("list")["devices"].as_array().unwrap().len(), 1);

        let r = revoke(&host, &peer, &id).expect("revoke");
        assert_eq!(r["removed"], true);
        assert_eq!(r["kind"], "pairing");

        // 撤销后立即生效：列表不再包含
        let devices = list(&host, &peer).expect("list after revoke")["devices"].as_array().unwrap().clone();
        assert_eq!(devices.len(), 0, "撤销后立即从统一视图消失");
    }

    /// 撤销未命中的 id：pairing 幂等 false；peer 侧未命中也 false（宿主
    /// revoke_trusted_peer 返回是否删除），整体不报错
    #[test]
    fn revoke_unknown_id_returns_removed_false() {
        let host = MockHost::new();
        let peer = MockPeer::new(vec![]);
        let r = revoke(&host, &peer, "ghost-id").expect("revoke unknown");
        assert_eq!(r["removed"], false);
    }

    /// 撤销 peer 目标：转发 host-peer revoke-trusted（kind=peer、removed 透传）
    #[test]
    fn revoke_peer_forwards_to_host_peer() {
        let host = MockHost::new();
        let node_id = "aabbccdd";
        let peer = MockPeer::new(vec![sample_peer(node_id, "书房台式机")]);

        let r = revoke(&host, &peer, node_id).expect("revoke peer");
        assert_eq!(r["removed"], true);
        assert_eq!(r["kind"], "peer");

        // 二次撤销同一 node_id：宿主 revoke_trusted_peer 语义 removed=false
        let r2 = revoke(&host, &peer, node_id).expect("revoke peer again");
        assert_eq!(r2["removed"], false);
    }

    // ==================== add_pairing：写入入口 ====================

    /// 新增记录：connect_count=1、active=true、RFC3339 配对时刻（宿主
    /// add_pairing 插入语义）
    #[test]
    fn add_pairing_records_with_host_insert_semantics() {
        let host = MockHost::new();
        let peer = MockPeer::new(vec![]);
        let id = add_pairing(&host, "Pixel 9", "fp-9", Some("192.168.1.5:9000")).expect("add");
        let devices = list(&host, &peer).expect("list")["devices"].as_array().unwrap().clone();

        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0]["id"], id);
        assert_eq!(devices[0]["name"], "Pixel 9");
        assert_eq!(devices[0]["fingerprint"], "fp-9");
        assert_eq!(devices[0]["connectCount"], 1, "宿主 add_pairing 初始 connect_count=1");
        // addedAt 必须是 RFC3339 UTC（格式合法可解析）
        let added = devices[0]["addedAt"].as_str().expect("addedAt");
        assert!(
            crate::pairing::code::parse_rfc3339_utc(added).is_some(),
            "addedAt 必须 RFC3339: {}",
            added
        );
    }
}
