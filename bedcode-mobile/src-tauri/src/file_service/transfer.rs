//! 传输批状态机（v2 接收策略 / 异步批量批准）
//!
//! 与桌面端 `bedcode-desktop/src-tauri/src/plugin/file_service/transfer.rs` 同构
//! （两端各自实现、不建共享 crate）。批 = 一次「发送」动作的文件集合：
//! 发送方先 POST /transfer-request 询问，接收端钩子三路分流（allow/ask/deny），
//! ask 时批进入 pending 等待用户应答；批准后批内 session 创建免钩子。
//!
//! 核心安全规则（spec 14.2）：
//! - ask 模式强制批上下文：无已批准批 ID 的 session 创建一律 403（防绕过 /upload）
//! - pending 批 TTL 扫描在宿主，超时自动拒绝（默认 60s，可配 10–600）
//! - 批准状态随批保留至批内全部 session 终态（+24h TTL 兜底）
//! - 批记录为宿主内存态，不持久化（接收方任务不跨重启）
//!
//! 本模块只含数据模型与纯函数（可单测）；状态操作在 registry 内实现。

use bedcode_plugin_api_mobile::UploadRequestMeta;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// approved 批 24h 无活动清理 TTL（兜底；复用 session TTL 语义）
pub const APPROVED_BATCH_TTL: Duration = Duration::from_secs(24 * 3600);
/// 默认批准超时（秒，spec 14.1）
pub const DEFAULT_APPROVAL_TIMEOUT_SECS: u64 = 60;
/// 批准超时下限（秒）
pub const MIN_APPROVAL_TIMEOUT_SECS: u64 = 10;
/// 批准超时上限（秒）
pub const MAX_APPROVAL_TIMEOUT_SECS: u64 = 600;

/// 批状态（spec 14.2）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BatchState {
    /// ask 后等待用户应答（宿主 TTL 扫描超时自动拒绝）
    Pending,
    /// 用户接受 / 钩子 allow（批内 session 创建免钩子）
    Approved,
    /// 用户拒绝 / 超时（终态，不可再迁移）
    Rejected { reason: RejectReason },
}

/// 拒绝原因枚举（wire kebab-case，发送方据此映射文案）
///
/// 注意：不用 `rename_all = "snake_case"`——方案 §5.2 线协议明确定义
/// reason 取值 `user-rejected`（kebab），与 snake_case 的 `user_rejected` 不符；
/// 显式 rename 锁死 wire 值，两端逐字一致
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RejectReason {
    /// 用户点了拒绝
    #[serde(rename = "user-rejected")]
    UserRejected,
    /// 等待超时（宿主 TTL / 断线）
    #[serde(rename = "timeout")]
    Timeout,
}

impl RejectReason {
    /// wire 字符串（跨端推送 / resolved 事件载荷用）
    pub fn as_str(self) -> &'static str {
        match self {
            RejectReason::UserRejected => "user-rejected",
            RejectReason::Timeout => "timeout",
        }
    }
}

/// 传输批记录（宿主内存态，不持久化）
#[derive(Debug, Clone)]
pub struct TransferBatch {
    /// 批 ID（发送方生成，跨端唯一标识一次「发送」动作）
    pub batch_id: String,
    /// 归属插件（应答命令按此校验归属，防跨插件操作）
    pub plugin_id: String,
    /// 挂载点（批上下文与挂载绑定）
    pub mount_path: String,
    /// 批内文件清单（相对路径 + 大小）
    pub files: Vec<UploadRequestMeta>,
    /// 批内文件总大小（字节）
    pub total_size: u64,
    /// 当前状态
    pub state: BatchState,
    /// 创建时间（TTL 计时基线）
    pub created_at: Instant,
    /// 批内 session 活动刷新时间：仅 approved 批 24h TTL 清理依据
    /// （pending 超时按 created_at 计，pending 期间不 touch）
    pub last_active: Instant,
    /// 批准超时（pending 状态 TTL 扫描依据；per-mount 可配）
    pub approval_timeout: Duration,
}

/// 批量传输请求 DTO（POST /transfer-request 请求体，camelCase 线协议）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferRequestDto {
    /// 批 ID
    pub batch_id: String,
    /// 批内文件清单
    pub files: Vec<UploadRequestMeta>,
    /// 批内文件总大小（字节）
    pub total_size: u64,
}

/// 批钩子分流结果（registry.create_transfer_request 返回）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchDecision {
    /// 钩子 allow：批直接 approved（200）
    Approved,
    /// 钩子 ask：批进入 pending 等待用户应答（202）
    Pending,
}

/// 纯函数：校验批状态迁移（spec 14.2）
///
/// 仅 pending → approved / rejected 合法（应答命令与 TTL 扫描共用）；
/// 已终态批重复应答、approved 后再拒绝等一律拒绝。
pub fn validate_batch_transition(from: &BatchState, to: &BatchState) -> Result<(), &'static str> {
    match (from, to) {
        (BatchState::Pending, BatchState::Approved) => Ok(()),
        (BatchState::Pending, BatchState::Rejected { .. }) => Ok(()),
        _ => Err("invalid batch state transition"),
    }
}

/// 纯函数：批是否处于已批准状态（session 创建 gating 用）
pub fn is_approved(batch: &TransferBatch) -> bool {
    matches!(batch.state, BatchState::Approved)
}

/// 纯函数：批是否已超时（pending 超时 → 拒绝；approved 24h 无活动 → 清理）
pub fn is_batch_expired(batch: &TransferBatch) -> bool {
    let now = Instant::now();
    match &batch.state {
        BatchState::Pending => now.saturating_duration_since(batch.last_active) >= batch.approval_timeout,
        BatchState::Approved => now.saturating_duration_since(batch.last_active) >= APPROVED_BATCH_TTL,
        BatchState::Rejected { .. } => false,
    }
}

/// 纯函数：校验批准超时值（10–600 秒，spec 14.1）
pub fn validate_approval_timeout(secs: u64) -> Result<u64, &'static str> {
    if (MIN_APPROVAL_TIMEOUT_SECS..=MAX_APPROVAL_TIMEOUT_SECS).contains(&secs) {
        Ok(secs)
    } else {
        Err("approval timeout must be in 10..=600 seconds")
    }
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    fn pending_batch() -> TransferBatch {
        TransferBatch {
            batch_id: "b1".to_string(),
            plugin_id: "p1".to_string(),
            mount_path: "files".to_string(),
            files: vec![],
            total_size: 0,
            state: BatchState::Pending,
            created_at: Instant::now(),
            last_active: Instant::now(),
            approval_timeout: Duration::from_secs(DEFAULT_APPROVAL_TIMEOUT_SECS),
        }
    }

    #[test]
    fn test_batch_transition_valid() {
        // pending → approved（用户接受）与 pending → rejected（用户拒绝/超时）合法
        assert!(validate_batch_transition(
            &BatchState::Pending,
            &BatchState::Approved
        )
        .is_ok());
        assert!(validate_batch_transition(
            &BatchState::Pending,
            &BatchState::Rejected { reason: RejectReason::UserRejected }
        )
        .is_ok());
        assert!(validate_batch_transition(
            &BatchState::Pending,
            &BatchState::Rejected { reason: RejectReason::Timeout }
        )
        .is_ok());
    }

    #[test]
    fn test_batch_transition_invalid() {
        // 已终态不可再迁移（重复应答 / 已拒绝后批准）
        assert!(validate_batch_transition(
            &BatchState::Approved,
            &BatchState::Rejected { reason: RejectReason::UserRejected }
        )
        .is_err());
        assert!(validate_batch_transition(
            &BatchState::Rejected { reason: RejectReason::Timeout },
            &BatchState::Approved
        )
        .is_err());
        // 自迁移无意义
        assert!(validate_batch_transition(&BatchState::Pending, &BatchState::Pending).is_err());
    }

    #[test]
    fn test_reject_reason_serde_snake_case() {
        // wire snake_case：跨端推送与 resolved 事件载荷逐字一致
        assert_eq!(
            serde_json::to_string(&RejectReason::UserRejected).unwrap(),
            "\"user-rejected\""
        );
        assert_eq!(
            serde_json::to_string(&RejectReason::Timeout).unwrap(),
            "\"timeout\""
        );
        assert_eq!(
            serde_json::from_str::<RejectReason>("\"user-rejected\"").unwrap(),
            RejectReason::UserRejected
        );
        assert!(serde_json::from_str::<RejectReason>("\"UserRejected\"").is_err());
        // as_str 与 serde 字面量一致
        assert_eq!(RejectReason::UserRejected.as_str(), "user-rejected");
        assert_eq!(RejectReason::Timeout.as_str(), "timeout");
    }

    #[test]
    fn test_is_approved() {
        let mut batch = pending_batch();
        assert!(!is_approved(&batch));
        batch.state = BatchState::Approved;
        assert!(is_approved(&batch));
    }

    #[test]
    fn test_validate_approval_timeout_bounds() {
        // 10–600 边界：9/10/600/601
        assert!(validate_approval_timeout(9).is_err());
        assert_eq!(validate_approval_timeout(10).unwrap(), 10);
        assert_eq!(validate_approval_timeout(600).unwrap(), 600);
        assert!(validate_approval_timeout(601).is_err());
        assert!(validate_approval_timeout(0).is_err());
    }

    #[test]
    fn test_is_batch_expired_pending_and_approved() {
        // pending 超时：超过 approval_timeout 即过期
        let mut batch = pending_batch();
        batch.approval_timeout = Duration::from_millis(10);
        batch.last_active = Instant::now() - Duration::from_millis(50);
        assert!(is_batch_expired(&batch));

        // approved 批 24h TTL：未超时不算过期
        //（checked_sub 兜底：短开机时间（测试环境）下不能构造 24h 前的 Instant）
        let mut approved = pending_batch();
        approved.state = BatchState::Approved;
        approved.last_active = Instant::now()
            .checked_sub(Duration::from_secs(3600))
            .unwrap_or_else(Instant::now);
        assert!(!is_batch_expired(&approved));

        // 终态永不过期（sweeper 不动已拒绝批）
        let mut rejected = pending_batch();
        rejected.state = BatchState::Rejected { reason: RejectReason::UserRejected };
        rejected.last_active = Instant::now()
            .checked_sub(Duration::from_secs(3600 * 48))
            .unwrap_or_else(Instant::now);
        assert!(!is_batch_expired(&rejected));
    }

    #[test]
    fn test_transfer_request_dto_wire_format() {
        let dto = TransferRequestDto {
            batch_id: "b1".to_string(),
            files: vec![UploadRequestMeta { relative_path: "dir/a.mp4".into(), size: 123456 }],
            total_size: 123456,
        };
        assert_eq!(
            serde_json::to_value(&dto).unwrap(),
            serde_json::json!({
                "batchId": "b1",
                "files": [{ "relativePath": "dir/a.mp4", "size": 123456 }],
                "totalSize": 123456
            })
        );
        // 缺省字段不携带时按默认值解析（serde default）
        let back: TransferRequestDto =
            serde_json::from_value(serde_json::json!({ "batchId": "b2", "files": [], "totalSize": 0 }))
                .unwrap();
        assert_eq!(back.batch_id, "b2");
    }
}
