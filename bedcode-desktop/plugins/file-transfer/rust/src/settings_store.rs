//! 接收策略设置自持（issue 13 Phase 3 步骤 4，A1 裁决）
//!
//! 产品真源 = 插件 storage 键 `transfer_settings`（SettingsPanel wire 形状，
//! 迁移零成本）；变更后经保留的引擎配置原语 `set-receive-policy` /
//! `set-download-dir` 推送宿主闸门与落盘（ADR 0022 v3：二者是引擎安全闸门/
//! 落盘配置，非业务编排）。加密开关同时作为 send 批参数默认值（新通路）。
//!
//! 引擎旧读接口 `get-receive-settings` 仅作惰性迁移对账源（Phase 4 退役）。

use bedcode_plugin_api::host::{HostPeer, HostStorage};
use serde::{Deserialize, Serialize};

/// storage 键（双端一致）
pub(crate) const SETTINGS_KEY: &str = "transfer_settings";

/// 设置 wire 形状（沿用 SettingsPanel 契约；桌面 downloadDir 可选）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TransferSettings {
    /// `ask` | `accept` | `reject`（UI 词表；推送宿主时映射 always_*）
    pub receiving_policy: String,
    /// 同意超时秒（10–600，仅 ask 生效）
    pub approval_timeout_sec: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub download_dir: Option<String>,
    /// 发送加密默认值（send 批参数携带；同时推送引擎全局开关兜底）
    #[serde(default)]
    pub encryption: bool,
}

impl Default for TransferSettings {
    fn default() -> Self {
        TransferSettings {
            receiving_policy: "ask".to_string(),
            approval_timeout_sec: 60,
            download_dir: None,
            encryption: false,
        }
    }
}

/// UI 词表 → 宿主策略词表
pub(crate) fn policy_to_host(ui_policy: &str) -> &'static str {
    match ui_policy {
        "accept" => "always_accept",
        "reject" => "always_deny",
        _ => "ask",
    }
}

/// 钳制同意超时到合法区间
pub(crate) fn clamp_timeout(secs: u64) -> u64 {
    secs.clamp(10, 600)
}

// ==================== 存取（host I/O 薄层） ====================

/// 读取设置；storage 为空时返回默认值（不回填——迁移由 get-settings 命令的
/// 惰性迁移路径负责，保持本函数纯读语义）
pub(crate) fn load(h: &impl HostStorage) -> anyhow::Result<TransferSettings> {
    match h.storage_get(SETTINGS_KEY)? {
        Some(v) => Ok(serde_json::from_value(v)
            .map_err(|e| anyhow::anyhow!("transfer_settings corrupt: {e}"))?),
        None => Ok(TransferSettings::default()),
    }
}

/// 保存设置并推送宿主配置原语（闸门 + 落盘 + 加密全局兜底）。
/// 任一宿主推送失败如实上抛（storage 已写入，下次 set 重推即可收敛）。
pub(crate) fn save_and_push(
    h: &(impl HostStorage + HostPeer),
    s: &TransferSettings,
) -> anyhow::Result<()> {
    h.storage_set(SETTINGS_KEY, &serde_json::to_value(s)?)?;
    h.peer_set_receive_policy(policy_to_host(&s.receiving_policy), clamp_timeout(s.approval_timeout_sec))?;
    if let Some(dir) = &s.download_dir {
        if !dir.is_empty() {
            h.peer_set_download_dir(dir)?;
        }
    }
    Ok(())
}

/// 惰性迁移：storage 为空且引擎旧读接口可达时，把引擎当前值导入插件 storage。
/// 返回生效设置（迁移后或既有值或默认值）。
/// 读取设置（Phase 4 起 get-receive-settings 已退役，引擎侧历史值由宿主
/// 一次性迁移导出至本键，见宿主 migrate_legacy_peer_settings；此处空值即默认）
pub(crate) fn load_or_migrate(h: &impl HostStorage) -> anyhow::Result<TransferSettings> {
    load(h)
}

/// 接收待应答批的自动应答裁决（auto 分支编排核心）：accept/reject 策略下
/// 返回 Some(bool)，ask 返回 None（弹窗编排在 UI 层）
pub(crate) fn auto_answer(s: &TransferSettings) -> Option<bool> {
    match s.receiving_policy.as_str() {
        "accept" => Some(true),
        "reject" => Some(false),
        _ => None,
    }
}

// ==================== Tests ====================

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
        fn storage_get(&self, key: &str) -> Result<Option<serde_json::Value>, bedcode_plugin_api::host::HostError> {
            Ok(self.kv.borrow().get(key).cloned())
        }
        fn storage_set(&self, key: &str, value: &serde_json::Value) -> Result<(), bedcode_plugin_api::host::HostError> {
            self.kv.borrow_mut().insert(key.to_string(), value.clone());
            Ok(())
        }
        fn storage_delete(&self, key: &str) -> Result<(), bedcode_plugin_api::host::HostError> {
            self.kv.borrow_mut().remove(key);
            Ok(())
        }
    }

    impl HostPeer for MockHost {
        fn peer_dial(&self, _endpoint: &serde_json::Value) -> Result<String, bedcode_plugin_api::host::HostError> { unimplemented!() }
        fn peer_close(&self, _handle: &str) -> Result<bool, bedcode_plugin_api::host::HostError> { unimplemented!() }
        fn peer_respond_consent(&self, _request_id: &str, _accepted: bool) -> Result<bool, bedcode_plugin_api::host::HostError> { unimplemented!() }
        fn peer_list_trusted(&self) -> Result<serde_json::Value, bedcode_plugin_api::host::HostError> { unimplemented!() }
        fn peer_revoke_trusted(&self, _node_id: &str) -> Result<bool, bedcode_plugin_api::host::HostError> { unimplemented!() }
        fn peer_send_files(&self, _session: &str, _paths: &[serde_json::Value]) -> Result<String, bedcode_plugin_api::host::HostError> { unimplemented!() }
        fn peer_respond_transfer(&self, _batch_id: &str, _accept: bool) -> Result<(), bedcode_plugin_api::host::HostError> { unimplemented!() }
        fn peer_set_receive_policy(&self, mode: &str, timeout_secs: u64) -> Result<(), bedcode_plugin_api::host::HostError> {
            self.pushed_policy.borrow_mut().push((mode.to_string(), timeout_secs));
            Ok(())
        }
        fn peer_set_shared_roots(&self, _dirs: &[serde_json::Value]) -> Result<(), bedcode_plugin_api::host::HostError> { unimplemented!() }
        fn peer_list_shared_roots(&self, _session: &str) -> Result<serde_json::Value, bedcode_plugin_api::host::HostError> { unimplemented!() }
        fn peer_browse_directory(&self, _session: &str, _dir_id: &str, _rel_path: &str) -> Result<serde_json::Value, bedcode_plugin_api::host::HostError> { unimplemented!() }
        fn peer_pull_files(&self, _session: &str, _dir_id: &str, _files: &[serde_json::Value]) -> Result<u32, bedcode_plugin_api::host::HostError> { unimplemented!() }
        fn peer_set_download_dir(&self, path: &str) -> Result<(), bedcode_plugin_api::host::HostError> {
            self.pushed_dirs.borrow_mut().push(path.to_string());
            Ok(())
        }
    }

    #[test]
    fn save_and_push_writes_storage_and_host_primitives() {
        let mut h = MockHost::new();
        let s = TransferSettings {
            receiving_policy: "accept".into(),
            approval_timeout_sec: 999,
            download_dir: Some("D:/dl".into()),
            encryption: true,
        };
        save_and_push(&mut h, &s).unwrap();
        assert_eq!(h.pushed_policy.borrow().as_slice(), &[("always_accept".to_string(), 600u64)][..]);
        assert_eq!(h.pushed_dirs.borrow().as_slice(), &["D:/dl".to_string()][..]);
        // 回读一致
        let loaded = load(&h).unwrap();
        assert_eq!(loaded, s);
    }

    #[test]
    fn load_or_migrate_returns_default_when_storage_empty() {
        // Phase 4：引擎读接口退役，历史值由宿主一次性迁移导出；插件侧空即默认
        let mut h = MockHost::new();
        let s = load_or_migrate(&mut h).unwrap();
        assert_eq!(s, TransferSettings::default());
        // 预置 storage 后读回一致
        h.storage_set(SETTINGS_KEY, &serde_json::to_value(TransferSettings {
            receiving_policy: "reject".into(),
            approval_timeout_sec: 30,
            download_dir: Some("D:/dl".into()),
            encryption: true,
        }).unwrap()).unwrap();
        assert_eq!(load_or_migrate(&h).unwrap().receiving_policy, "reject");
    }

    #[test]
    fn auto_answer_maps_three_branches() {
        let base = TransferSettings::default();
        assert_eq!(auto_answer(&base), None);
        let accept = TransferSettings { receiving_policy: "accept".into(), ..Default::default() };
        assert_eq!(auto_answer(&accept), Some(true));
        let reject = TransferSettings { receiving_policy: "reject".into(), ..Default::default() };
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
