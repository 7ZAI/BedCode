//! trust 记录来源 —— 本插件私有库 `auth_pairings` 表（2026-09-22 认证记录下沉）
//!
//! 真源 = 认证中心（本插件）私有库 [`crate::auth_records`]，经 \[`AuthRecordsStore`]
//! 端口读取 / 撤销。**宿主主库不再持有配对表**（host-auth 记录面原语退役），
//! 撤销的权威事实是私有库的软删（`is_active = 0`）。
//!
//! 宿主交互经 [`TrustRecords`] trait 抽象：wasm 运行时走私有库（`auth_records`
//! 域）；native 单测注入内存 mock（与 pairing/keys.rs 同模式——native 链接不
//! 引用 wasm 专属 import 符号）。

#[cfg(target_arch = "wasm32")]
use crate::auth_records;

use super::model::PairingRecord;

/// 配对记录存取面（native 单测注入 mock）
pub trait TrustRecords {
    /// 全部原始记录（含软删行；排序与过滤归调用方）
    fn records(&self) -> Result<Vec<PairingRecord>, String>;
    /// 撤销（软删 + 连带删除连接历史）；返回是否命中记录
    fn revoke(&self, id: &str) -> Result<bool, String>;
}

/// wasm 运行时：认证中心私有库（2026-09-22 下沉后真源在本插件）
#[cfg(target_arch = "wasm32")]
impl TrustRecords for bedcode_plugin_api::wasm_host::WasmHost {
    fn records(&self) -> Result<Vec<PairingRecord>, String> {
        // 认证记录域模型（含 uidHash 锚点）→ 信任视图模型（TrustedDeviceDto
        // 的 pairing 来源）逐字段搬运
        Ok(auth_records::records()?
            .into_iter()
            .map(PairingRecord::from)
            .collect())
    }

    fn revoke(&self, id: &str) -> Result<bool, String> {
        auth_records::revoke(id)
    }
}

/// 认证记录域 → 信任视图模型的逐字段转换（wasm 运行时 records 链路；
/// 视图模型无 uidHash 字段，丢弃）
#[cfg(target_arch = "wasm32")]
impl From<crate::auth_records::model::PairingRecord> for PairingRecord {
    fn from(r: crate::auth_records::model::PairingRecord) -> Self {
        Self {
            id: r.id,
            device_name: r.device_name,
            device_fingerprint: r.device_fingerprint,
            address: r.address,
            paired_at: r.paired_at,
            last_seen: r.last_seen,
            connect_count: r.connect_count,
            is_active: r.is_active,
        }
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
