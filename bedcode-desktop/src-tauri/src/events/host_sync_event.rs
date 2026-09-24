//! Host Sync Event
//!
//! 插件事件进入宿主广播面的**唯一薄适配**（会话事件下沉专项票 02）
//!
//! 此前这里是「SDK `SyncEvent` → 宿主镜像枚举 `DesktopSyncEvent` → 处理器按 11 个
//! 变体业务 match → 重建 `SyncPayload`」的三段转换，宿主持有会话事件解释权。
//! 现在只剩一步：`SyncEvent` 与 `SyncPayload` 同 wire（票 02 已对齐），适配器做的
//! 是**信封级**工作——取出源设备、把事件折成出站载荷、失败即显性留痕。
//!
//! 红线：本文件**禁止**出现按会话/任务变体的业务分支（校验必填、折算状态、
//! 编造缺省值一律在生产者侧）。判据见 `events/sync_handler.rs` 的防回接锁。

use bedcode_plugin_api::events::SyncEvent;
use bedcode_plugin_api::wire::SyncPayload;

use super::app_event::AppEvent;

/// 宿主侧的同步事件（插件 → 宿主 → 移动端）
///
/// newtype 而非裸 `SyncEvent`：`AppEvent` 定义在本 crate，给外域类型实现 trait 会撞
/// orphan rule；同时它标出「已进入宿主广播面」这一边界——边界之前是插件的线协议，
/// 之后是宿主的传输信封。
#[derive(Debug, Clone)]
pub struct HostSyncEvent(pub SyncEvent);

impl From<SyncEvent> for HostSyncEvent {
    fn from(event: SyncEvent) -> Self {
        Self(event)
    }
}

impl AppEvent for HostSyncEvent {
    /// 源设备只从带该字段的三个会话变体取（空串 = 桌面本地操作 → 不排除任何设备）
    ///
    /// 与 [`Self::to_sync_payload`] 的差别就是该字段的用途：它服务宿主的「排除发起者」
    /// 传输语义，不在出站 `data` 里（移动端形状不变）。带字段变体的清单由
    /// `source_device_variants_are_fully_handled` 钉住，新增变体漏分支不会静默。
    fn source_device(&self) -> Option<&str> {
        let device = match &self.0 {
            SyncEvent::SessionCreated { source_device, .. }
            | SyncEvent::SessionStopped { source_device, .. }
            | SyncEvent::SessionRemoved { source_device, .. } => source_device.as_str(),
            _ => "",
        };
        (!device.is_empty()).then_some(device)
    }

    /// 信封级校验：能不能折成出站形状
    ///
    /// 业务必填（会话概要、会话名）由插件产出口保证；这里兜的是「事件与
    /// `SyncPayload` 不同构」这类协议断链——必须在发布侧显性失败回给生产者，
    /// 而不是投出去后在处理器里静默丢掉。
    fn validate(&self) -> Result<(), String> {
        match self.sync_payload_result() {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("sync event does not map to SyncPayload: {e}")),
        }
    }

    fn to_sync_payload(&self) -> Option<SyncPayload> {
        match self.sync_payload_result() {
            Ok(payload) => Some(payload),
            // validate 已在 publish 入口挡在前面；走到这里说明调用方绕过了 publish，
            // 只留痕不伪造（宁可少发一条，也不发一条字段是编出来的推送）
            Err(e) => {
                tracing::error!(error = %e, "[HostSyncEvent] 事件折不成出站载荷，跳过广播");
                None
            }
        }
    }
}

impl HostSyncEvent {
    /// `SyncEvent` → `SyncPayload`：同一 wire 的机械折算（无变体分支、无取值兜底）
    fn sync_payload_result(&self) -> Result<SyncPayload, serde_json::Error> {
        let value = serde_json::to_value(&self.0)?;
        serde_json::from_value(value)
    }
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    fn summary() -> bedcode_plugin_api::wire::SessionSummary {
        bedcode_plugin_api::wire::SessionSummary {
            id: "s1".to_string(),
            name: "dev".to_string(),
            status: "running".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            started_at: Some("2026-01-01T00:00:05Z".to_string()),
            session_type: Some("pty".to_string()),
            config_id: Some("cfg-1".to_string()),
            task_status: None,
            task_reason: None,
        }
    }

    /// 八个变体各一条，`source_device` 统一填非空值（便于同时验排除面与出站面）
    fn samples() -> Vec<SyncEvent> {
        vec![
            SyncEvent::SessionCreated {
                session: summary(),
                source_device: "d1".to_string(),
            },
            SyncEvent::SessionStatusChanged {
                session_id: "s1".to_string(),
                old_status: "running".to_string(),
                new_status: "stopped".to_string(),
                session_name: "dev".to_string(),
            },
            SyncEvent::SessionStopped {
                session_id: "s1".to_string(),
                session_name: "dev".to_string(),
                source_device: "d1".to_string(),
            },
            SyncEvent::SessionRemoved {
                session_id: "s1".to_string(),
                session_name: "dev".to_string(),
                source_device: "d1".to_string(),
            },
            SyncEvent::TaskStatusChanged {
                session_id: "s1".to_string(),
                task_status: "asking".to_string(),
                task_reason: None,
                task_questions: None,
            },
            SyncEvent::SessionModeChanged {
                session_id: "s1".to_string(),
                auto_approve: true,
            },
            SyncEvent::TaskQueueChanged {
                session_id: "s1".to_string(),
                queue_count: 2,
                action: "add".to_string(),
                task_id: None,
                status: None,
            },
            SyncEvent::TaskScheduledChanged {
                job_id: "job-1".to_string(),
                status: "pending".to_string(),
                action: "create".to_string(),
            },
        ]
    }

    /// 八个变体都要能折成出站载荷（缺一个即该变体的推送静默丢失）
    #[test]
    fn every_variant_maps_to_sync_payload() {
        for event in samples() {
            let label = serde_json::to_value(&event).unwrap()["type"]
                .as_str()
                .unwrap()
                .to_string();
            let host = HostSyncEvent(event);
            assert!(
                host.to_sync_payload().is_some(),
                "{label} 折不成 SyncPayload：插件事件面与出站载荷面已漂移"
            );
            assert_eq!(host.validate(), Ok(()), "{label} 发布前校验应通过");
        }
    }

    /// 源设备面锁：`source_device()` 的分支集合必须与「wire 上带 source_device 的变体」
    /// 完全一致——新增带源设备的变体却漏掉分支时，本用例转红（否则该变体的排除语义
    /// 静默失效：发起端会收到自己刚做的那条推送）。
    #[test]
    fn source_device_variants_are_fully_handled() {
        for event in samples() {
            let value = serde_json::to_value(&event).unwrap();
            let label = value["type"].as_str().unwrap().to_string();
            let wire_has_device = value["data"].get("source_device").is_some();
            let host = HostSyncEvent(event);
            let got = host.source_device();
            assert_eq!(
                wire_has_device,
                got.is_some(),
                "{label}: wire 带 source_device={wire_has_device} 与 source_device()={got:?} 不一致"
            );
            if wire_has_device {
                assert_eq!(got, Some("d1"), "{label} 应原样透出源设备");
            }
        }
    }

    /// 桌面本地操作（空串）不得被当成设备名去排除
    #[test]
    fn empty_source_device_is_not_an_exclusion() {
        let event = SyncEvent::SessionRemoved {
            session_id: "s1".to_string(),
            session_name: "dev".to_string(),
            source_device: String::new(),
        };
        assert_eq!(HostSyncEvent(event).source_device(), None);
    }

    /// 「近恒等」的那一处例外必须**被走到**：Stopped / Removed 的 `source_device`
    /// 只服务宿主排除语义，折算后不得出现在出站 `data` 里（移动端形状不变）。
    #[test]
    fn source_device_is_stripped_from_outbound_data() {
        for event in samples() {
            let value = serde_json::to_value(&event).unwrap();
            let label = value["type"].as_str().unwrap().to_string();
            if !matches!(label.as_str(), "session_stopped" | "session_removed") {
                continue;
            }
            assert!(value["data"].get("source_device").is_some(), "{label} 样本应带源设备");
            let payload = serde_json::to_value(HostSyncEvent(event).to_sync_payload().unwrap()).unwrap();
            assert!(
                payload["data"].get("source_device").is_none(),
                "{label} 出站 data 泄漏了 source_device: {payload}"
            );
            // 除该键外其余字段逐字相同（机械折算不得改值）
            let mut expected = value["data"].as_object().unwrap().clone();
            expected.remove("source_device");
            assert_eq!(
                payload["data"].as_object().unwrap().clone(),
                expected,
                "{label} 折算改了字段值"
            );
        }
    }
}
