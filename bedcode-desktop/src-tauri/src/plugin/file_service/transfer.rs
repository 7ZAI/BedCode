//! 传输批状态机（v2 异步批量批准，spec 14.2）
//!
//! 接收端宿主内存态：`POST /transfer-request` 经批钩子三路分流后
//! （allow → approved / ask → pending / deny → 403）建批记录；
//! 用户应答命令把 pending 迁到 approved / rejected；pending 超时由
//! 宿主 sweeper 扫描自动拒绝（timeout）。批准状态随批保留至批内全部
//! session 终态（24h TTL 兜底，复用 session TTL 语义）。
//!
//! 批不持久化（接收端重启后 pending 批自然消失，发送方超时 rejected）。

use bedcode_plugin_api::UploadRequestMeta;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// 批默认批准超时（秒）：等待用户应答的最长时间
pub const DEFAULT_APPROVAL_TIMEOUT_SECS: u64 = 60;
/// 批准超时下限（秒）
pub const MIN_APPROVAL_TIMEOUT_SECS: u64 = 10;
/// 批准超时上限（秒）
pub const MAX_APPROVAL_TIMEOUT_SECS: u64 = 600;
/// approved 批 24h 无活动 TTL（兜底清理，复用 session TTL 语义）
pub const APPROVED_BATCH_TTL: Duration = Duration::from_secs(24 * 3600);

/// 批状态（spec 14.2）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BatchState {
    /// ask 后等待用户应答（宿主 TTL 扫描超时自动拒绝）
    Pending,
    /// 用户接受 / 钩子 allow（批内 session 创建免钩子）
    Approved,
    /// 用户拒绝 / 超时
    Rejected { reason: RejectReason },
}

/// 拒绝原因枚举（wire kebab-case，发送方据此映射文案）
///
/// 注意：不用 `rename_all = "snake_case"`——方案 §5.2 线协议明确定义
/// reason 取值 `user-rejected`（kebab），与 snake_case 的 `user_rejected` 不符；
/// 显式 rename 锁死 wire 值，与移动端逐字一致
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RejectReason {
    /// 用户点了拒绝
    #[serde(rename = "user-rejected")]
    UserRejected,
    /// 等待超时（宿主 TTL / 断线）
    #[serde(rename = "timeout")]
    Timeout,
}

/// 批错误（命令/HTTP 层据此映射错误语义，spec §3.3）
#[derive(Debug, thiserror::Error)]
pub enum BatchError {
    /// 批不存在（命令路径 → AppError::NotFound，不泄露存在性）
    #[error("transfer batch not found: {0}")]
    NotFound(String),
    /// 批非 pending（重复应答 / 已超时 → AppError::InvalidInput）
    #[error("transfer batch {0} not pending")]
    NotPending(String),
    /// session 创建 gating 拒绝（消息即 wire 值：batch-not-approved /
    /// batch-rejected / batch-not-found，发送方据此解析）
    #[error("{0}")]
    GatingDenied(String),
    /// 批钩子 deny（消息为钩子 reason，如 policy-denied）→ HTTP 403
    #[error("transfer request denied: {0}")]
    PolicyDenied(String),
    /// 超时值越界（10–600）
    #[error("approval timeout {0}s out of range 10-600")]
    InvalidTimeout(u64),
}

/// 传输批记录（宿主内存态，不持久化）
#[derive(Debug, Clone)]
pub struct TransferBatch {
    /// 批 ID（发送方生成，UUID）
    pub batch_id: String,
    /// 所属插件（应答命令与 session gating 校验归属）
    pub plugin_id: String,
    /// 所属挂载点
    pub mount_path: String,
    /// 批内文件清单（相对路径 + 大小）
    pub files: Vec<UploadRequestMeta>,
    /// 批总大小（字节）
    pub total_size: u64,
    /// 当前状态
    pub state: BatchState,
    /// 批创建时间（pending 超时计时起点）
    pub created_at: Instant,
    /// 批内 session 活动刷新时间（approved 批 24h TTL 依据）
    pub last_active: Instant,
    /// 批准超时（建批时从 per-mount 配置快照，默认 60s）
    pub approval_timeout: Duration,
}

/// 批量传输请求 DTO（POST /transfer-request 请求体，camelCase wire）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferRequestDto {
    /// 批 ID（发送方生成，UUID）
    pub batch_id: String,
    /// 批内文件清单（相对路径 + 大小）
    pub files: Vec<UploadRequestMeta>,
    /// 批总大小（字节）
    pub total_size: u64,
}

/// 批钩子分流结果（create_transfer_request 返回值）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchDecision {
    /// 钩子 allow：批已批准（HTTP 200）
    Approved,
    /// 钩子 ask：批置 pending 等待用户应答（HTTP 202）
    Pending,
}

/// sweeper 返回的过期批（调用方逐批发布 resolved 事件 + 跨端推送）
#[derive(Debug, Clone)]
pub struct ExpiredBatch {
    /// 批 ID
    pub batch_id: String,
    /// 决策（pending 超时固定为 "rejected"）
    pub decision: String,
    /// 原因（pending 超时固定为 "timeout"）
    pub reason: String,
}

/// 校验批状态迁移合法性（纯函数，可单测）
///
/// 返回 `Ok(())` 表示迁移合法，`Err(reason)` 表示非法迁移。
/// 合法迁移（spec 14.2）：
/// - Pending → Approved（用户接受）
/// - Pending → Rejected（用户拒绝 / 超时）
pub fn validate_batch_transition(from: &BatchState, to: &BatchState) -> Result<(), &'static str> {
    match (from, to) {
        (BatchState::Pending, BatchState::Approved) => Ok(()),
        (BatchState::Pending, BatchState::Rejected { .. }) => Ok(()),
        // 终态不可迁出（approved 批只可被 TTL 清理，不经状态迁移）
        (BatchState::Approved, _) => Err("cannot transition from approved"),
        (BatchState::Rejected { .. }, _) => Err("cannot transition from rejected"),
        (from, to) if from == to => Err("self-transition"),
        _ => Err("invalid batch state transition"),
    }
}

/// 批是否为已批准（session 创建 gating 依据）
pub fn is_approved(batch: &TransferBatch) -> bool {
    batch.state == BatchState::Approved
}

/// 尝试状态迁移（合法性校验 + 更新；失败保持原状态）
pub fn transition_batch(batch: &mut TransferBatch, to: BatchState) -> Result<(), BatchError> {
    validate_batch_transition(&batch.state, &to)
        .map_err(|e| BatchError::NotPending(format!("transfer batch {}: {}", batch.batch_id, e)))?;
    batch.state = to;
    batch.last_active = Instant::now();
    Ok(())
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_batch() -> TransferBatch {
        TransferBatch {
            batch_id: "b1".into(),
            plugin_id: "p1".into(),
            mount_path: "files".into(),
            files: Vec::new(),
            total_size: 0,
            state: BatchState::Pending,
            created_at: Instant::now(),
            last_active: Instant::now(),
            approval_timeout: Duration::from_secs(DEFAULT_APPROVAL_TIMEOUT_SECS),
        }
    }

    #[test]
    fn test_valid_transitions() {
        // pending → approved（用户接受）
        assert!(validate_batch_transition(
            &BatchState::Pending,
            &BatchState::Approved
        )
        .is_ok());
        // pending → rejected（用户拒绝 / 超时）
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
    fn test_invalid_transitions() {
        // 终态不可迁出
        assert!(validate_batch_transition(&BatchState::Approved, &BatchState::Pending).is_err());
        assert!(validate_batch_transition(
            &BatchState::Rejected { reason: RejectReason::UserRejected },
            &BatchState::Approved
        )
        .is_err());
        // 非 pending 不可批准/拒绝（重复应答场景）
        assert!(validate_batch_transition(&BatchState::Approved, &BatchState::Approved).is_err());
        // 自迁移无意义
        assert!(validate_batch_transition(&BatchState::Pending, &BatchState::Pending).is_err());
    }

    #[test]
    fn test_transition_batch_updates_state_and_active() {
        let mut batch = make_batch();
        assert!(!is_approved(&batch));
        transition_batch(&mut batch, BatchState::Approved).expect("approve ok");
        assert!(is_approved(&batch));
        // 重复批准：非 pending → 错误，状态不被破坏
        assert!(transition_batch(&mut batch, BatchState::Approved).is_err());
        assert!(is_approved(&batch));
    }

    #[test]
    fn test_reject_reason_wire_kebab() {
        // wire kebab-case（方案 §5.2）：发送方按字面量映射文案，与移动端逐字一致
        assert_eq!(
            serde_json::to_value(RejectReason::UserRejected).unwrap(),
            serde_json::json!("user-rejected")
        );
        assert_eq!(
            serde_json::to_value(RejectReason::Timeout).unwrap(),
            serde_json::json!("timeout")
        );
        let back: RejectReason =
            serde_json::from_value(serde_json::json!("user-rejected")).unwrap();
        assert_eq!(back, RejectReason::UserRejected);
    }
}
