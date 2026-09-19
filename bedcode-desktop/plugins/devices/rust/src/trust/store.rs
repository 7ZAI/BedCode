//! trust 持久化 —— pairing 记录经 host-storage 存取（票 08 B3）
//!
//! 数据真源 = 宿主 `plugin_storage` 表（host-storage 原语，键 `trust.pairings`，
//! 按插件属主隔离），重启一致。语义对齐宿主 `pairings` 表：
//! - 写入即持久化（`save` 全量替换，宿主无事务级合并需求——设备数量级小）
//! - 撤销软删（`revoke` 置 `active=false`）即时写回，重启后仍不可见
//! - 读取容错：存储缺失/损坏时显性失败（不静默重建为空列表——与宿主
//!   `get_pairings` 的「查询失败上抛」一致，避免误删信任数据）
//!
//! 宿主交互经 [`HostAccess`] trait 抽象：wasm 运行时走 `WasmHost`
//! （host-storage 原语）；native 单测注入内存 mock（与 pairing/keys.rs 同模式）。

use super::model::PairingRecord;
#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::host::HostStorage;
#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::wasm_host::WasmHost;

/// host-storage 存取键（双端一致；宿主按 plugin_id 隔离）
pub(crate) const TRUST_STORAGE_KEY: &str = "trust.pairings";

/// 宿主存储抽象（native 单测注入 mock）
pub trait HostAccess {
    fn storage_get(&self, key: &str) -> Result<Option<serde_json::Value>, String>;
    fn storage_set(&self, key: &str, value: &serde_json::Value) -> Result<(), String>;
}

/// wasm 运行时：WasmHost（host-storage 原语）
#[cfg(target_arch = "wasm32")]
impl HostAccess for WasmHost {
    fn storage_get(&self, key: &str) -> Result<Option<serde_json::Value>, String> {
        HostStorage::storage_get(self, key).map_err(|e| e.message)
    }

    fn storage_set(&self, key: &str, value: &serde_json::Value) -> Result<(), String> {
        HostStorage::storage_set(self, key, value).map_err(|e| e.message)
    }
}

/// 读取全部 pairing 记录；键缺失返回空列表，损坏显性失败
pub(crate) fn load(h: &impl HostAccess) -> Result<Vec<PairingRecord>, String> {
    match h.storage_get(TRUST_STORAGE_KEY)? {
        Some(v) => serde_json::from_value(v)
            .map_err(|e| format!("trust.pairings corrupt: {}", e)),
        None => Ok(Vec::new()),
    }
}

/// 全量持久化（写入即生效，重启一致）
pub(crate) fn save(h: &impl HostAccess, records: &[PairingRecord]) -> Result<(), String> {
    let value = serde_json::to_value(records).map_err(|e| format!("trust.pairings serialize: {}", e))?;
    h.storage_set(TRUST_STORAGE_KEY, &value)
}

/// 新增 pairing 记录（配对完成流写入入口；`active=true`）
pub(crate) fn add_record(
    h: &impl HostAccess,
    id: &str,
    device_name: &str,
    device_fingerprint: &str,
    address: Option<&str>,
    paired_at: &str,
) -> Result<(), String> {
    let mut records = load(h)?;
    records.push(PairingRecord {
        id: id.to_string(),
        device_name: device_name.to_string(),
        device_fingerprint: device_fingerprint.to_string(),
        address: address.map(String::from),
        paired_at: paired_at.to_string(),
        last_seen: None,
        connect_count: 1,
        active: true,
    });
    save(h, &records)
}

/// 撤销 pairing 记录（软删：置 `active=false`，保留记录——宿主 `remove_pairing`
/// 的 `is_active=0` 语义）。返回是否命中了该记录。
///
/// 宿主对照：`UPDATE pairings SET is_active = 0 WHERE id = ?1` —— 未命中时
/// 影响 0 行不报错（幂等）。本函数同样对未知 id 返回 `Ok(false)`。
pub(crate) fn revoke_record(h: &impl HostAccess, id: &str) -> Result<bool, String> {
    let mut records = load(h)?;
    match records.iter_mut().find(|r| r.id == id) {
        Some(record) => {
            if record.active {
                record.active = false;
                save(h, &records)?;
            }
            Ok(true)
        }
        None => Ok(false),
    }
}

// ==================== Tests ====================

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::Mutex;

    /// 内存 mock 宿主存储（native 单测注入；与 pairing/keys.rs MockSecretStore 同模式）
    pub struct MockHost {
        inner: Mutex<Vec<(String, serde_json::Value)>>,
    }

    impl MockHost {
        pub fn new() -> Self {
            Self {
                inner: Mutex::new(Vec::new()),
            }
        }
    }

    impl Default for MockHost {
        fn default() -> Self {
            Self::new()
        }
    }

    impl HostAccess for MockHost {
        fn storage_get(&self, key: &str) -> Result<Option<serde_json::Value>, String> {
            let inner = self.inner.lock().unwrap();
            Ok(inner.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone()))
        }

        fn storage_set(&self, key: &str, value: &serde_json::Value) -> Result<(), String> {
            let mut inner = self.inner.lock().unwrap();
            if let Some(entry) = inner.iter_mut().find(|(k, _)| k == key) {
                entry.1 = value.clone();
            } else {
                inner.push((key.to_string(), value.clone()));
            }
            Ok(())
        }
    }

    /// 空存储 → 空列表（键缺失语义）
    #[test]
    fn load_missing_key_returns_empty() {
        let host = MockHost::new();
        assert_eq!(load(&host).expect("load"), Vec::<PairingRecord>::new());
    }

    /// 新增 → 落库 → 读回一致（写入即持久化；含 connect_count=1、active=true）
    #[test]
    fn add_then_load_roundtrip_persists() {
        let host = MockHost::new();
        add_record(
            &host,
            "p-1",
            "Pixel 9",
            "fp-1",
            Some("192.168.1.5:9000"),
            "2026-09-19T00:00:00Z",
        )
        .expect("add");

        // 模拟重启：新实例读同一存储（同一 mock 即同一底层）
        let records = load(&host).expect("reload");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].id, "p-1");
        assert_eq!(records[0].device_name, "Pixel 9");
        assert_eq!(records[0].connect_count, 1, "宿主 add_pairing 初始 connect_count=1");
        assert_eq!(records[0].active, true);
    }

    /// 撤销软删：active 置 false 且持久化；再次加载（重启）仍不可见
    #[test]
    fn revoke_soft_deletes_and_persists_across_restart() {
        let host = MockHost::new();
        add_record(&host, "p-1", "Phone", "fp-1", None, "2026-09-19T00:00:00Z").expect("add");

        assert_eq!(revoke_record(&host, "p-1").expect("revoke"), true);
        let records = load(&host).expect("after revoke");
        assert_eq!(records[0].active, false, "撤销=软删（宿主 is_active=0 语义）");
        assert_eq!(records.len(), 1, "记录保留（软删不物理删除）");

        // 重启后加载：撤销状态保持（列表过滤逻辑在 ops::list，此处验证持久化）
        let reloaded = load(&host).expect("restart reload");
        assert_eq!(reloaded[0].active, false);
    }

    /// 撤销未知 id：幂等返回 false 不报错（宿主 UPDATE 影响 0 行语义）
    #[test]
    fn revoke_unknown_id_is_idempotent() {
        let host = MockHost::new();
        assert_eq!(revoke_record(&host, "ghost").expect("revoke"), false);
        add_record(&host, "p-1", "Phone", "fp-1", None, "2026-09-19T00:00:00Z").expect("add");
        assert_eq!(revoke_record(&host, "p-2").expect("revoke missing"), false);
        assert_eq!(load(&host).expect("load").len(), 1, "未命中不得影响存量");
    }

    /// 重复撤销同一记录：命中但幂等（已 active=false 不再写回，仍返回 true）
    #[test]
    fn revoke_twice_is_idempotent() {
        let host = MockHost::new();
        add_record(&host, "p-1", "Phone", "fp-1", None, "2026-09-19T00:00:00Z").expect("add");
        assert_eq!(revoke_record(&host, "p-1").expect("first"), true);
        assert_eq!(revoke_record(&host, "p-1").expect("second"), true);
        let records = load(&host).expect("load");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].active, false);
    }

    /// 损坏存储：显性失败，不静默重建为空列表（避免误删信任数据）
    #[test]
    fn corrupt_storage_fails_loudly() {
        let host = MockHost::new();
        host.storage_set(TRUST_STORAGE_KEY, &serde_json::json!("not-an-array")).unwrap();
        let err = load(&host).unwrap_err();
        assert!(err.contains("corrupt"), "损坏存储必须显性失败: {}", err);
    }
}
