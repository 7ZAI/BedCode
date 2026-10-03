//! trust 编排 —— 统一视图 list / 撤销 revoke（票 05：内核真源 + 插件编排）
//!
//! 行为等价对照（宿主实现基准，逐条见模块文档）：
//! - `list`：pairing 段只含活跃记录、按 `paired_at` DESC（= 宿主 `get_pairings`
//!   的 `WHERE is_active = 1 ORDER BY paired_at DESC`）；peer 段透传宿主
//!   `list_trusted_peers` 条目序；两段合并为统一数组（pairing 在前，各带 `kind`）
//! - `revoke`：pairing 走 host-auth `trusted-device-revoke`（宿主 `remove_pairing`
//!   的 `is_active = 0` 软删 + 删连接历史）；peer 走 host-peer `revoke-trusted`
//!   （宿主 `revoke_trusted_peer` 语义）
//!
//! 移除了票 05 前的 `add_pairing` 写入入口：配对完成流写内核 `pairings` 表（宿主
//! 命令面 / 认证端点），插件只读——两套账本合并为一（Problem Statement 5）。

use super::model::TrustedDeviceDto;
use super::source::TrustRecords;
use bedcode_plugin_api::host::HostPeer;
#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::wasm_host::WasmHost;

/// 统一视图响应：pairing 段 + peer 段合并。
///
/// pairing 段按 `paired_at` DESC（宿主 `get_pairings` 语义），peer 段保留宿主
/// `list-trusted` 的条目序（两侧各自的时序语义在各自段内成立）。
///
/// peer 侧不可用（无头上下文 / peer-net 未启动）时：pairing 段照常返回，
/// `peerError` 透出宿主错误（不静默降级为空列表——避免「看似成功实则缺失」）。
pub fn list(h: &impl TrustRecords, peer: &impl HostPeer) -> Result<serde_json::Value, String> {
    // pairing 段：活跃过滤 + paired_at DESC（宿主 get_pairings 语义）
    let mut records = h.records()?;
    records.retain(|r| r.is_active);
    records.sort_by(|a, b| b.paired_at.cmp(&a.paired_at));

    // peer 段：透传宿主 list-trusted（TrustedPeerDto JSON 数组，条目序保留）。
    // T-G02：非数组 Ok 当 Err 处理（宿主契约破坏伪装成空列表会破坏信任身份）；
    // 单条 DTO 缺 nodeId/addedAt 也按 Err（不静默变空条目）
    let mut peers: Vec<serde_json::Value> = Vec::new();
    let peer_error = match peer.peer_list_trusted() {
        Ok(v) => match v.as_array() {
            Some(arr) => {
                peers = arr.clone();
                None
            }
            None => Some(format!(
                "trusted peers list did not return an array: {v}"
            )),
        },
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
        // 单条畸形 → 整体报错（不静默跳过——信任列表缺条目会破坏撤销寻址）
        let dto = TrustedDeviceDto::from_peer(&peer)
            .map_err(|e| format!("trusted peer dto invalid: {e}"))?;
        devices.push(
            serde_json::to_value(dto).map_err(|e| format!("serialize trust dto: {}", e))?,
        );
    }

    Ok(serde_json::json!({
        "devices": devices,
        "peerError": peer_error,
    }))
}

/// 撤销统一条目：pairing 目标软删（内核 `remove_pairing` 语义，经 host-auth
/// 记录面）；peer 目标经 host-peer `revoke-trusted`（返回是否删除）。
/// 返回 `{ removed, kind }`。
///
/// 未命中：pairing 幂等 `removed=false` 不报错（宿主 UPDATE 影响 0 行）；
/// peer 目标无权限/引擎不可用时上抛错误（不做「看似成功」的假撤销）。
///
/// **T-G01（三态裁决）**：`h.revoke` 的 bool 无法区分「未命中 / 已非活跃 /
/// 根本不是 pairing」——陈旧 pairing id 撞上 peer node id 会把无关的活跃 peer
/// 撤销。故先按 records() 判定目标身份：
/// - records 里找得到该 id 且活跃 → pairing 软删
/// - records 里找得到该 id 但已非活跃 → pairing 幂等成功（removed=false，不落 peer）
/// - records 里找不到 → 才尝试 peer 路径（此时 id 明确不是 pairing）
pub fn revoke(
    h: &impl TrustRecords,
    peer: &impl HostPeer,
    id: &str,
) -> Result<serde_json::Value, String> {
    // 三态预判：目标 id 是否在 pairing 记录里（含软删行——撤销检测依赖可见性）
    let pairing_state = h.records()?.iter().find(|r| r.id == id).map(|r| r.is_active);
    match pairing_state {
        Some(true) => {
            // 活跃 pairing：软删（records 里的 is_active 是该记录的活跃位）
            let removed = h.revoke(id)?;
            Ok(serde_json::json!({ "removed": removed, "kind": "pairing" }))
        }
        Some(false) => {
            // 已非活跃 pairing：幂等成功（不落 peer 路径——id 属于 pairing 域，
            // 即使它撞上一个 peer node id 也不能撤销那个无关 peer）
            Ok(serde_json::json!({ "removed": false, "kind": "pairing" }))
        }
        None => {
            // 明确不是 pairing → peer 目标（host-peer revoke-trusted，node_id 寻址）
            let removed = peer.peer_revoke_trusted(id).map_err(|e| e.message)?;
            Ok(serde_json::json!({ "removed": removed, "kind": "peer" }))
        }
    }
}

// ==================== 命令面入口（cfg 分流，native 显性失败） ====================
//
// lib.rs 命令面只调这些入口；native（cargo test）下 WasmHost 无 TrustRecords /
// HostPeer impl（wasm 专属 import 符号不在 native 链接），因此这里只是 wasm 运行时
// 路径的薄包装（与 pairing/keys.rs 同模式）。

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

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trust::source::tests::MockRecords;
    use bedcode_plugin_api::host::HostError;
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

    /// mock host-peer 的未实现方法统一 panic（本测试只消费 list / revoke 两函数）
    macro_rules! unimplemented_peer {
        () => {
            unimplemented!()
        };
    }

    impl HostPeer for MockPeer {
        fn peer_dial(&self, _endpoint: &serde_json::Value) -> Result<String, HostError> {
            unimplemented_peer!()
        }
        fn peer_close(&self, _handle: &str) -> Result<bool, HostError> {
            unimplemented_peer!()
        }
        fn peer_respond_consent(
            &self,
            _request_id: &str,
            _accepted: bool,
        ) -> Result<bool, HostError> {
            unimplemented_peer!()
        }
        fn peer_list_trusted(&self) -> Result<serde_json::Value, HostError> {
            Ok(serde_json::Value::Array(
                self.trusted.lock().unwrap().clone(),
            ))
        }
        fn peer_revoke_trusted(&self, node_id: &str) -> Result<bool, HostError> {
            let mut trusted = self.trusted.lock().unwrap();
            let before = trusted.len();
            trusted.retain(|t| t["nodeId"] != node_id);
            Ok(trusted.len() < before)
        }
        fn peer_send_files(
            &self,
            _session: &str,
            _paths: &[serde_json::Value],
        ) -> Result<String, HostError> {
            unimplemented_peer!()
        }
        fn peer_respond_transfer(&self, _batch_id: &str, _accept: bool) -> Result<(), HostError> {
            unimplemented_peer!()
        }
        fn peer_set_receive_policy(
            &self,
            _mode: &str,
            _timeout_secs: u64,
        ) -> Result<(), HostError> {
            unimplemented_peer!()
        }
        fn peer_pause_transfer(&self, _batch_id: &str) -> Result<(), HostError> {
            unimplemented_peer!()
        }
        fn peer_resume_transfer(&self, _batch_id: &str) -> Result<(), HostError> {
            unimplemented_peer!()
        }
        fn peer_set_shared_roots(&self, _dirs: &[serde_json::Value]) -> Result<(), HostError> {
            unimplemented_peer!()
        }
        fn peer_list_shared_roots(&self, _session: &str) -> Result<serde_json::Value, HostError> {
            unimplemented_peer!()
        }
        fn peer_browse_directory(
            &self,
            _session: &str,
            _dir_id: &str,
            _rel_path: &str,
        ) -> Result<serde_json::Value, HostError> {
            unimplemented_peer!()
        }
        fn peer_pull_files(
            &self,
            _session: &str,
            _dir_id: &str,
            _files: &[serde_json::Value],
        ) -> Result<u32, HostError> {
            unimplemented_peer!()
        }
        fn peer_set_download_dir(&self, _path: &str) -> Result<(), HostError> {
            unimplemented_peer!()
        }
        fn peer_start_node(&self) -> Result<bool, HostError> {
            unimplemented_peer!()
        }
        fn peer_stop_node(&self) -> Result<bool, HostError> {
            unimplemented_peer!()
        }
        fn peer_active_transfers(&self) -> Result<serde_json::Value, HostError> {
            unimplemented_peer!()
        }
        fn peer_collect_outgoing(&self, _paths: &[serde_json::Value]) -> Result<serde_json::Value, HostError> {
            unimplemented_peer!()
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
        let records = MockRecords::new(vec![]);
        let peer = MockPeer::new(vec![]);
        let r = list(&records, &peer).expect("list");
        assert_eq!(r["devices"].as_array().unwrap().len(), 0);
        assert!(r["peerError"].is_null());
    }

    /// pairing 段：活跃过滤 + paired_at DESC 排序（宿主 get_pairings 语义），
    /// 字段与宿主 Pairing 公开视图对齐；软删行必须被过滤掉
    #[test]
    fn list_pairings_only_active_sorted_by_paired_at_desc() {
        let mut revoked =
            MockRecords::record("revoked", "已撤销", "fp-rev", "2026-09-10T00:00:00Z");
        revoked.is_active = false;
        let records = MockRecords::new(vec![
            MockRecords::record("old", "旧设备", "fp-old", "2026-09-01T00:00:00Z"),
            MockRecords::record("new", "新设备", "fp-new", "2026-09-19T00:00:00Z"),
            revoked,
        ]);

        let peer = MockPeer::new(vec![]);
        let r = list(&records, &peer).expect("list");
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
        let records = MockRecords::new(vec![MockRecords::record(
            "p-1",
            "Phone",
            "fp-1",
            "2026-09-19T00:00:00Z",
        )]);

        let peer = MockPeer::new(vec![sample_peer("aabbccdd", "书房台式机")]);
        let r = list(&records, &peer).expect("list");
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
            fn peer_dial(&self, _e: &serde_json::Value) -> Result<String, HostError> {
                unimplemented_peer!()
            }
            fn peer_close(&self, _h: &str) -> Result<bool, HostError> {
                unimplemented_peer!()
            }
            fn peer_respond_consent(&self, _r: &str, _a: bool) -> Result<bool, HostError> {
                unimplemented_peer!()
            }
            fn peer_list_trusted(&self) -> Result<serde_json::Value, HostError> {
                Err(HostError::custom(
                    -1,
                    "peer-net unavailable in headless context".to_string(),
                ))
            }
            fn peer_revoke_trusted(&self, _n: &str) -> Result<bool, HostError> {
                unimplemented_peer!()
            }
            fn peer_send_files(
                &self,
                _s: &str,
                _p: &[serde_json::Value],
            ) -> Result<String, HostError> {
                unimplemented_peer!()
            }
            fn peer_respond_transfer(&self, _b: &str, _a: bool) -> Result<(), HostError> {
                unimplemented_peer!()
            }
            fn peer_set_receive_policy(&self, _m: &str, _t: u64) -> Result<(), HostError> {
                unimplemented_peer!()
            }
            fn peer_pause_transfer(&self, _b: &str) -> Result<(), HostError> {
                unimplemented_peer!()
            }
            fn peer_resume_transfer(&self, _b: &str) -> Result<(), HostError> {
                unimplemented_peer!()
            }
            fn peer_set_shared_roots(&self, _d: &[serde_json::Value]) -> Result<(), HostError> {
                unimplemented_peer!()
            }
            fn peer_list_shared_roots(&self, _s: &str) -> Result<serde_json::Value, HostError> {
                unimplemented_peer!()
            }
            fn peer_browse_directory(
                &self,
                _s: &str,
                _d: &str,
                _r: &str,
            ) -> Result<serde_json::Value, HostError> {
                unimplemented_peer!()
            }
            fn peer_pull_files(
                &self,
                _s: &str,
                _d: &str,
                _f: &[serde_json::Value],
            ) -> Result<u32, HostError> {
                unimplemented_peer!()
            }
            fn peer_set_download_dir(&self, _p: &str) -> Result<(), HostError> {
                unimplemented_peer!()
            }
            fn peer_start_node(&self) -> Result<bool, HostError> {
                unimplemented_peer!()
            }
            fn peer_stop_node(&self) -> Result<bool, HostError> {
                unimplemented_peer!()
            }
            fn peer_active_transfers(&self) -> Result<serde_json::Value, HostError> {
                unimplemented_peer!()
            }
            fn peer_collect_outgoing(&self, _paths: &[serde_json::Value]) -> Result<serde_json::Value, HostError> {
                unimplemented_peer!()
            }
        }

        let records = MockRecords::new(vec![MockRecords::record(
            "p-1",
            "Phone",
            "fp-1",
            "2026-09-19T00:00:00Z",
        )]);
        let r = list(&records, &PeerDown).expect("list");
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
        let records = MockRecords::new(vec![MockRecords::record(
            "p-1",
            "Phone",
            "fp-1",
            "2026-09-19T00:00:00Z",
        )]);
        let peer = MockPeer::new(vec![]);
        assert_eq!(
            list(&records, &peer).expect("list")["devices"]
                .as_array()
                .unwrap()
                .len(),
            1
        );

        let r = revoke(&records, &peer, "p-1").expect("revoke");
        assert_eq!(r["removed"], true);
        assert_eq!(r["kind"], "pairing");

        // 撤销后立即生效：列表不再包含
        let devices = list(&records, &peer).expect("list after revoke")["devices"]
            .as_array()
            .unwrap()
            .clone();
        assert_eq!(devices.len(), 0, "撤销后立即从统一视图消失");
    }

    /// 撤销未命中的 id：pairing 幂等 false；peer 侧未命中也 false（宿主
    /// revoke_trusted_peer 返回是否删除），整体不报错
    #[test]
    fn revoke_unknown_id_returns_removed_false() {
        let records = MockRecords::new(vec![]);
        let peer = MockPeer::new(vec![]);
        let r = revoke(&records, &peer, "ghost-id").expect("revoke unknown");
        assert_eq!(r["removed"], false);
        assert_eq!(r["kind"], "peer", "未命中 pairing 即按 peer 寻址");
    }

    /// T-G01：已非活跃 pairing 的 id 不得落 peer 路径——即使它撞上某个 peer
    /// node id，也不能撤销那个无关的活跃 peer（旧实现 bool false 无法区分
    /// 「已非活跃」与「不是 pairing」，会误撤销）
    #[test]
    fn revoke_inactive_pairing_id_does_not_touch_peer() {
        let mut inactive = MockRecords::record("p-stale-00", "Old", "fp-x", "2026-09-01T00:00:00Z");
        inactive.is_active = false;
        let records = MockRecords::new(vec![inactive]);
        // peer 列表里恰好有个 node id = 陈旧的 pairing id：
        // 撤销该 id 必须只报 pairing 幂等，绝不能删掉这个活跃 peer
        let peer = MockPeer::new(vec![sample_peer("p-stale-00", "书房台式机")]);
        let r = revoke(&records, &peer, "p-stale-00").expect("revoke stale pairing");
        assert_eq!(r["removed"], false);
        assert_eq!(r["kind"], "pairing", "id 在 pairing 域 → 不落 peer 路径");
        let after = peer.trusted.lock().unwrap();
        assert_eq!(after.len(), 1, "活跃 peer 必须原样保留");
    }

    /// 撤销 peer 目标：转发 host-peer revoke-trusted（kind=peer、removed 透传）
    #[test]
    fn revoke_peer_forwards_to_host_peer() {
        let records = MockRecords::new(vec![]);
        let node_id = "aabbccdd";
        let peer = MockPeer::new(vec![sample_peer(node_id, "书房台式机")]);

        let r = revoke(&records, &peer, node_id).expect("revoke peer");
        assert_eq!(r["removed"], true);
        assert_eq!(r["kind"], "peer");

        // 二次撤销同一 node_id：宿主 revoke_trusted_peer 语义 removed=false
        let r2 = revoke(&records, &peer, node_id).expect("revoke peer again");
        assert_eq!(r2["removed"], false);
    }

    /// 数据源不可用（host-auth 原语报错）：显性上抛，不静默空列表
    /// （空列表会被消费方读成「没有任何信任设备」，是危险的默认值）
    #[test]
    fn list_surfaces_record_source_failure() {
        struct BrokenRecords;
        impl TrustRecords for BrokenRecords {
            fn records(&self) -> Result<Vec<crate::trust::model::PairingRecord>, String> {
                Err("host-auth trusted-devices-list failed: permission denied".to_string())
            }
            fn revoke(&self, _id: &str) -> Result<bool, String> {
                Err("host-auth trusted-device-revoke failed: permission denied".to_string())
            }
        }

        let err = list(&BrokenRecords, &MockPeer::new(vec![])).unwrap_err();
        assert!(err.contains("permission denied"), "got: {err}");
        let err = revoke(&BrokenRecords, &MockPeer::new(vec![]), "p-1").unwrap_err();
        assert!(err.contains("permission denied"), "got: {err}");
    }
}
