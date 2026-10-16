//! revoke：撤销分派 — crate 内单元测试（自 bedcode-desktop/wasm-apps/terminal-session/rust/src/trust/ops.rs 迁出）

use super::*;

use crate::trust::source::tests::MockRecords;

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
