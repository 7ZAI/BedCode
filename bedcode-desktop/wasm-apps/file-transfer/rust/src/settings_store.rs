//! 接收策略与落点设置 —— **实现已迁双端共享核**（`bedcode-file-transfer-core::settings`，
//! ADR 0044），本文件保留桌面端宿主 I/O 包装（薄适配，零判据）。
//!
//! 桌面形态：`downloadDir` 由 UI 选择后推送 `set-download-dir`（核内由形态位
//! `PluginProfile::supports_custom_download_dir` 闸门放行）。

use crate::adapters::DesktopPorts;
use bedcode_file_transfer_core::settings as core_settings;
use bedcode_plugin_api::host::{HostPeer, HostStorage};

pub(crate) use bedcode_file_transfer_core::settings::*;

/// 读取设置（存储空/缺失回默认；损坏值显性报错——静默重置用户设置比报错更糟）
pub(crate) fn load<H: HostStorage + ?Sized>(h: &H) -> anyhow::Result<TransferSettings> {
    core_settings::load(&DesktopPorts(h)).map_err(anyhow::Error::new)
}

/// 读取设置（惰性迁移语义与退役前一致：引擎旧读接口已退役，空值即默认）
pub(crate) fn load_or_migrate<H: HostStorage + ?Sized>(h: &H) -> anyhow::Result<TransferSettings> {
    core_settings::load_or_migrate(&DesktopPorts(h)).map_err(anyhow::Error::new)
}

/// 保存设置并推送宿主配置原语（接收策略闸门 + 落点）
pub(crate) fn save_and_push<H: HostStorage + HostPeer + ?Sized>(
    h: &H,
    s: &TransferSettings,
) -> anyhow::Result<()> {
    core_settings::save_and_push(&DesktopPorts(h), s).map_err(anyhow::Error::new)
}

// ==================== Tests ====================

// 这些用例现在的身份是**适配器接线测试**：判据已被核内用例覆盖，此处验证
// 「桌面 SDK host trait → 核端口」的委派真的把调用送到了宿主原语上。
#[cfg(test)]
mod tests {
    use super::*;

    /// 内存版 HostStorage+HostPeer mock：记录推送调用供断言
    struct MockHost {
        kv: std::cell::RefCell<std::collections::HashMap<String, serde_json::Value>>,
        pushed_policy: std::cell::RefCell<Vec<(String, u64)>>,
        pushed_dirs: std::cell::RefCell<Vec<String>>,
    }

    impl MockHost {
        fn new() -> Self {
            MockHost {
                kv: std::cell::RefCell::new(std::collections::HashMap::new()),
                pushed_policy: std::cell::RefCell::new(vec![]),
                pushed_dirs: std::cell::RefCell::new(vec![]),
            }
        }
    }

    impl HostStorage for MockHost {
        fn storage_get(
            &self,
            key: &str,
        ) -> Result<Option<serde_json::Value>, bedcode_plugin_api::host::HostError> {
            Ok(self.kv.borrow().get(key).cloned())
        }
        fn storage_set(
            &self,
            key: &str,
            value: &serde_json::Value,
        ) -> Result<(), bedcode_plugin_api::host::HostError> {
            self.kv.borrow_mut().insert(key.to_string(), value.clone());
            Ok(())
        }
        fn storage_delete(&self, key: &str) -> Result<(), bedcode_plugin_api::host::HostError> {
            self.kv.borrow_mut().remove(key);
            Ok(())
        }
    }

    impl HostPeer for MockHost {
        fn peer_dial(
            &self,
            _endpoint: &serde_json::Value,
        ) -> Result<String, bedcode_plugin_api::host::HostError> {
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
        fn peer_list_trusted(
            &self,
        ) -> Result<serde_json::Value, bedcode_plugin_api::host::HostError> {
            unimplemented!()
        }
        fn peer_revoke_trusted(
            &self,
            _node_id: &str,
        ) -> Result<bool, bedcode_plugin_api::host::HostError> {
            unimplemented!()
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
            mode: &str,
            timeout_secs: u64,
        ) -> Result<(), bedcode_plugin_api::host::HostError> {
            self.pushed_policy
                .borrow_mut()
                .push((mode.to_string(), timeout_secs));
            Ok(())
        }
        fn peer_pause_transfer(
            &self,
            _batch_id: &str,
        ) -> Result<(), bedcode_plugin_api::host::HostError> {
            unimplemented!()
        }
        fn peer_resume_transfer(
            &self,
            _batch_id: &str,
        ) -> Result<(), bedcode_plugin_api::host::HostError> {
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
        fn peer_set_download_dir(
            &self,
            path: &str,
        ) -> Result<(), bedcode_plugin_api::host::HostError> {
            self.pushed_dirs.borrow_mut().push(path.to_string());
            Ok(())
        }
        fn peer_start_node(&self) -> Result<bool, bedcode_plugin_api::host::HostError> {
            unimplemented!()
        }
        fn peer_stop_node(&self) -> Result<bool, bedcode_plugin_api::host::HostError> {
            unimplemented!()
        }
        fn peer_active_transfers(
            &self,
        ) -> Result<serde_json::Value, bedcode_plugin_api::host::HostError> {
            unimplemented!()
        }
        fn peer_collect_outgoing(
            &self,
            _paths: &[serde_json::Value],
        ) -> Result<serde_json::Value, bedcode_plugin_api::host::HostError> {
            unimplemented!()
        }
    }

    #[test]
    fn save_and_push_writes_storage_and_host_primitives() {
        let h = MockHost::new();
        let s = TransferSettings {
            receiving_policy: "accept".into(),
            approval_timeout_sec: 999,
            download_dir: Some("D:/dl".into()),
            encryption: true,
            concurrency: 5,
        };
        save_and_push(&h, &s).unwrap();
        assert_eq!(
            h.pushed_policy.borrow().as_slice(),
            &[("always_accept".to_string(), 600u64)][..]
        );
        // 桌面形态位 = 接受自定义落点 ⇒ 落点必须真的推给宿主
        assert_eq!(
            h.pushed_dirs.borrow().as_slice(),
            &["D:/dl".to_string()][..]
        );
        // 回读一致
        let loaded = load(&h).unwrap();
        assert_eq!(loaded, s);
    }

    #[test]
    fn concurrency_defaults_to_3_and_clamps_into_range() {
        // 旧 storage 无 concurrency 字段：缺省 3（向后兼容）
        let h = MockHost::new();
        h.storage_set(
            SETTINGS_KEY,
            &serde_json::json!({"receivingPolicy": "ask", "approvalTimeoutSec": 60, "encryption": false}),
        )
        .unwrap();
        assert_eq!(load(&h).unwrap().concurrency, 3);
        assert_eq!(clamp_concurrency(0), 1);
        assert_eq!(clamp_concurrency(9), 8);
        assert_eq!(clamp_concurrency(5), 5);
    }

    #[test]
    fn load_or_migrate_returns_default_when_storage_empty() {
        // Phase 4：引擎读接口退役，历史值由宿主一次性迁移导出；插件侧空即默认
        let h = MockHost::new();
        let s = load_or_migrate(&h).unwrap();
        assert_eq!(s, TransferSettings::default());
        // 预置 storage 后读回一致
        h.storage_set(
            SETTINGS_KEY,
            &serde_json::to_value(TransferSettings {
                receiving_policy: "reject".into(),
                approval_timeout_sec: 30,
                download_dir: Some("D:/dl".into()),
                encryption: true,
                concurrency: 3,
            })
            .unwrap(),
        )
        .unwrap();
        assert_eq!(load_or_migrate(&h).unwrap().receiving_policy, "reject");
    }

    #[test]
    fn auto_answer_maps_three_branches() {
        let base = TransferSettings::default();
        assert_eq!(auto_answer(&base), None);
        let accept = TransferSettings {
            receiving_policy: "accept".into(),
            ..Default::default()
        };
        assert_eq!(auto_answer(&accept), Some(true));
        let reject = TransferSettings {
            receiving_policy: "reject".into(),
            ..Default::default()
        };
        assert_eq!(auto_answer(&reject), Some(false));
    }

    #[test]
    fn policy_word_mapping() {
        assert_eq!(policy_to_host("accept"), "always_accept");
        assert_eq!(policy_to_host("reject"), "always_deny");
        assert_eq!(policy_to_host("ask"), "ask");
        assert_eq!(clamp_timeout(5), 10);
        assert_eq!(clamp_timeout(700), 600);
    }
}
