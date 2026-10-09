//! 接收策略与落点设置 —— **实现已迁双端共享核**（`bedcode-file-transfer-core::settings`，
//! ADR 0044），本文件保留移动端宿主 I/O 包装（薄适配，零判据）。
//!
//! 移动端形态差异：接收落点固定 `MediaStore.Downloads`，页面无落点设置项，故
//! `download_dir` 恒 `None`——核内推送分支天然不走，无需端内分支。

use crate::adapters::MobilePorts;
use bedcode_file_transfer_core::settings as core_settings;
use bedcode_plugin_api_mobile::host::{HostPeer, HostStorage};

pub(crate) use bedcode_file_transfer_core::settings::*;

/// 读取设置（存储空/缺失回默认；损坏值显性报错——静默重置用户设置比报错更糟）
pub(crate) fn load<H: HostStorage + ?Sized>(h: &H) -> anyhow::Result<TransferSettings> {
    core_settings::load(&MobilePorts(h)).map_err(anyhow::Error::new)
}

/// 读取设置（惰性迁移语义与退役前一致：引擎旧读接口已退役，空值即默认）
pub(crate) fn load_or_migrate<H: HostStorage + ?Sized>(h: &H) -> anyhow::Result<TransferSettings> {
    core_settings::load_or_migrate(&MobilePorts(h)).map_err(anyhow::Error::new)
}

/// 保存设置并推送宿主配置原语（接收策略闸门）
pub(crate) fn save_and_push<H: HostStorage + HostPeer + ?Sized>(
    h: &H,
    s: &TransferSettings,
) -> anyhow::Result<()> {
    core_settings::save_and_push(&MobilePorts(h), s).map_err(anyhow::Error::new)
}

// ==================== Tests ====================

// 用例按功能拆至 `settings_store/tests/`（本内联模块的子模块路径由 rustc
// 自动解析到该目录；模块树 `settings_store::tests::<文件>` 与内联形态等价，私有项可见性不受影响）。
// 这些用例现在的身份是**适配器接线测试**：判据已被核内用例覆盖，此处验证
// 「移动 SDK host trait → 核端口」的委派真的把调用送到了宿主原语上。
#[cfg(test)]
mod tests {
    use super::*;
    // 跨分组共享的测试脚手架（子模块经 `use super::*` 可见）

    /// 内存版 HostStorage+HostPeer mock：记录推送调用供断言
    struct MockHost {
        kv: std::cell::RefCell<std::collections::HashMap<String, serde_json::Value>>,
        pushed_policy: std::cell::RefCell<Vec<(String, u64)>>,
    }
    impl MockHost {
        fn new() -> Self {
            MockHost {
                kv: std::cell::RefCell::new(std::collections::HashMap::new()),
                pushed_policy: std::cell::RefCell::new(vec![]),
            }
        }
    }
    impl HostStorage for MockHost {
        fn storage_get(
            &self,
            key: &str,
        ) -> Result<Option<serde_json::Value>, bedcode_plugin_api_mobile::host::HostError> {
            Ok(self.kv.borrow().get(key).cloned())
        }
        fn storage_set(
            &self,
            key: &str,
            value: &serde_json::Value,
        ) -> Result<(), bedcode_plugin_api_mobile::host::HostError> {
            self.kv.borrow_mut().insert(key.to_string(), value.clone());
            Ok(())
        }
        fn storage_delete(
            &self,
            key: &str,
        ) -> Result<(), bedcode_plugin_api_mobile::host::HostError> {
            self.kv.borrow_mut().remove(key);
            Ok(())
        }
    }
    impl HostPeer for MockHost {
        fn peer_dial(
            &self,
            _endpoint: &serde_json::Value,
        ) -> Result<String, bedcode_plugin_api_mobile::host::HostError> {
            unimplemented!()
        }
        fn peer_close(
            &self,
            _handle: &str,
        ) -> Result<bool, bedcode_plugin_api_mobile::host::HostError> {
            unimplemented!()
        }
        fn peer_respond_consent(
            &self,
            _request_id: &str,
            _accepted: bool,
        ) -> Result<bool, bedcode_plugin_api_mobile::host::HostError> {
            unimplemented!()
        }
        fn peer_list_trusted(
            &self,
        ) -> Result<serde_json::Value, bedcode_plugin_api_mobile::host::HostError> {
            unimplemented!()
        }
        fn peer_revoke_trusted(
            &self,
            _node_id: &str,
        ) -> Result<bool, bedcode_plugin_api_mobile::host::HostError> {
            unimplemented!()
        }
        fn peer_send_files(
            &self,
            _session: &str,
            _paths: &[serde_json::Value],
        ) -> Result<String, bedcode_plugin_api_mobile::host::HostError> {
            unimplemented!()
        }
        fn peer_respond_transfer(
            &self,
            _batch_id: &str,
            _accept: bool,
        ) -> Result<(), bedcode_plugin_api_mobile::host::HostError> {
            unimplemented!()
        }
        fn peer_set_receive_policy(
            &self,
            mode: &str,
            timeout_secs: u64,
        ) -> Result<(), bedcode_plugin_api_mobile::host::HostError> {
            self.pushed_policy
                .borrow_mut()
                .push((mode.to_string(), timeout_secs));
            Ok(())
        }
        fn peer_pause_transfer(
            &self,
            _batch_id: &str,
        ) -> Result<(), bedcode_plugin_api_mobile::host::HostError> {
            unimplemented!()
        }
        fn peer_resume_transfer(
            &self,
            _batch_id: &str,
        ) -> Result<(), bedcode_plugin_api_mobile::host::HostError> {
            unimplemented!()
        }
        fn peer_set_shared_roots(
            &self,
            _dirs: &[serde_json::Value],
        ) -> Result<(), bedcode_plugin_api_mobile::host::HostError> {
            unimplemented!()
        }
        fn peer_list_shared_roots(
            &self,
            _session: &str,
        ) -> Result<serde_json::Value, bedcode_plugin_api_mobile::host::HostError> {
            unimplemented!()
        }
        fn peer_browse_directory(
            &self,
            _session: &str,
            _dir_id: &str,
            _rel_path: &str,
        ) -> Result<serde_json::Value, bedcode_plugin_api_mobile::host::HostError> {
            unimplemented!()
        }
        fn peer_pull_files(
            &self,
            _session: &str,
            _dir_id: &str,
            _files: &[serde_json::Value],
        ) -> Result<u32, bedcode_plugin_api_mobile::host::HostError> {
            unimplemented!()
        }
        // 票 04 新增 5 原语（票 06 补齐 mock：trait 扩容后 mock 必须同步）
        fn peer_set_download_dir(
            &self,
            _path: &str,
        ) -> Result<(), bedcode_plugin_api_mobile::host::HostError> {
            unimplemented!()
        }
        fn peer_start_node(&self) -> Result<bool, bedcode_plugin_api_mobile::host::HostError> {
            unimplemented!()
        }
        fn peer_stop_node(&self) -> Result<bool, bedcode_plugin_api_mobile::host::HostError> {
            unimplemented!()
        }
        fn peer_active_transfers(
            &self,
        ) -> Result<serde_json::Value, bedcode_plugin_api_mobile::host::HostError> {
            unimplemented!()
        }
        fn peer_collect_outgoing(
            &self,
            _paths: &[serde_json::Value],
        ) -> Result<serde_json::Value, bedcode_plugin_api_mobile::host::HostError> {
            unimplemented!()
        }
    }
    mod save_and_push_writes;
}
