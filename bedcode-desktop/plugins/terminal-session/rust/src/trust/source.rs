//! trust 记录来源 —— 内核 `pairings` 表经 host-auth 记录面读取/撤销（票 05）
//!
//! 真源 = 宿主 `pairings` 表（host-auth `trusted-devices-list` /
//! `trusted-device-revoke` 原语，按 `auth` 权限授权）。**插件不再自持镜像**：
//! 票 05 前的 host-storage `trust.pairings` 镜像已被删除——两套账本是
//! Problem Statement 5 的口径漂移来源，且内核软删（`is_active = 0`）才是撤销的
//! 权威事实。
//!
//! 宿主交互经 [`TrustRecords`] trait 抽象：wasm 运行时走 `WasmHost`（host-auth
//! 原语）；native 单测注入内存 mock（与 pairing/keys.rs 同模式——native 链接不
//! 引用 wasm 专属 import 符号）。

#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::host::HostAuth;
#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::wasm_host::WasmHost;

use super::model::PairingRecord;

/// 内核配对记录存取面（native 单测注入 mock）
pub trait TrustRecords {
    /// 全部原始记录（含软删行；排序与过滤归调用方）
    fn records(&self) -> Result<Vec<PairingRecord>, String>;
    /// 撤销（软删 + 连带删除连接历史）；返回是否命中记录
    fn revoke(&self, id: &str) -> Result<bool, String>;
}

/// wasm 运行时：WasmHost（host-auth v18 记录面）
#[cfg(target_arch = "wasm32")]
impl TrustRecords for WasmHost {
    fn records(&self) -> Result<Vec<PairingRecord>, String> {
        let raw = self
            .auth_trusted_devices_list()
            .map_err(|e| format!("host-auth trusted-devices-list failed: {}", e.message))?;
        serde_json::from_value(raw).map_err(|e| format!("trusted devices decode failed: {}", e))
    }

    fn revoke(&self, id: &str) -> Result<bool, String> {
        self.auth_trusted_device_revoke(id)
            .map_err(|e| format!("host-auth trusted-device-revoke failed: {}", e.message))
    }
}

// ==================== Tests ====================

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::Mutex;

    /// 内存 mock 记录面（native 单测注入；语义对齐宿主 `pairings` 表：
    /// 全量返回原始记录、撤销置 `is_active = false` 且记录保留）
    pub struct MockRecords {
        inner: Mutex<Vec<PairingRecord>>,
    }

    impl MockRecords {
        pub fn new(records: Vec<PairingRecord>) -> Self {
            Self {
                inner: Mutex::new(records),
            }
        }

        /// 造一条活跃记录（测试构造用）
        pub fn record(id: &str, name: &str, fp: &str, paired_at: &str) -> PairingRecord {
            PairingRecord {
                id: id.to_string(),
                device_name: name.to_string(),
                device_fingerprint: fp.to_string(),
                address: None,
                paired_at: paired_at.to_string(),
                last_seen: None,
                connect_count: 1,
                is_active: true,
            }
        }
    }

    impl TrustRecords for MockRecords {
        fn records(&self) -> Result<Vec<PairingRecord>, String> {
            Ok(self.inner.lock().unwrap().clone())
        }

        fn revoke(&self, id: &str) -> Result<bool, String> {
            let mut records = self.inner.lock().unwrap();
            match records.iter_mut().find(|r| r.id == id) {
                Some(record) => {
                    record.is_active = false;
                    Ok(true)
                }
                None => Ok(false),
            }
        }
    }

    /// 原始记录全量透传（含软删行）——过滤是调用方的职责，mock 不替它决定
    #[test]
    fn mock_surfaces_raw_records_including_revoked() {
        let mut revoked = MockRecords::record("p-2", "Tablet", "fp-2", "2026-09-18T00:00:00Z");
        revoked.is_active = false;
        let records = MockRecords::new(vec![
            MockRecords::record("p-1", "Phone", "fp-1", "2026-09-19T00:00:00Z"),
            revoked,
        ]);
        let raw = TrustRecords::records(&records).expect("records");
        assert_eq!(raw.len(), 2);
        assert!(raw.iter().any(|r| !r.is_active), "软删行必须可见");
    }

    /// 撤销命中即软删且记录保留；未知 id 幂等 false（宿主原语同语义）
    #[test]
    fn mock_revoke_soft_deletes_and_reports_hit() {
        let records = MockRecords::new(vec![MockRecords::record(
            "p-1",
            "Phone",
            "fp-1",
            "2026-09-19T00:00:00Z",
        )]);
        assert_eq!(
            TrustRecords::revoke(&records, "ghost").expect("revoke ghost"),
            false
        );
        assert_eq!(TrustRecords::revoke(&records, "p-1").expect("revoke"), true);
        let after = TrustRecords::records(&records).expect("records");
        assert_eq!(after.len(), 1, "软删保留记录");
        assert!(!after[0].is_active);
    }
}
