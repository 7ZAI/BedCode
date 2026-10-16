//! list：统一视图组装 — crate 内单元测试（自 bedcode-desktop/wasm-apps/terminal-session/rust/src/trust/ops.rs 迁出）

use super::*;

use crate::trust::source::tests::MockRecords;
use bedcode_plugin_api::host::HostError;
use std::sync::Mutex;

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
    let mut revoked = MockRecords::record("revoked", "已撤销", "fp-rev", "2026-09-10T00:00:00Z");
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
        fn peer_send_files(&self, _s: &str, _p: &[serde_json::Value]) -> Result<String, HostError> {
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
        fn peer_collect_outgoing(
            &self,
            _paths: &[serde_json::Value],
        ) -> Result<serde_json::Value, HostError> {
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
