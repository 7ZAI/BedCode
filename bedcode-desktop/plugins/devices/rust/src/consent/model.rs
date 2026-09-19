//! consent 数据模型 —— 首连确认请求 / 用户意向 / 决策结果（票 09 B4）
//!
//! 语义对齐宿主 peer-net 首连确认流：
//! - 宿主事件桥 `peer:consent` 载荷 `{ requestId, nodeId, fingerprintShort, deviceName }`
//!   → [`ConsentRequest`]（应答 `requestId` 透传寻址，宿主 `respond_peer_consent`
//!   消费）
//! - 决策经互调 api `auth.decide-consent` 暴露（ADR 0017：manifest `api` 声明
//!   即契约），消费方（file-transfer，票 10）按决策驱动 UI 与宿主应答

use serde::{Deserialize, Serialize};

/// 用户显式意向（未知 peer 由消费方弹窗取得后回传；wire 形状 snake_case）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserDecision {
    /// 允许：本连接放行（宿主 accept 路径带名落库信任）
    Accept,
    /// 拒绝：本连接拒绝
    Deny,
    /// 一次性确认：本请求放行但不改变信任状态（下次首连仍需确认）
    #[serde(rename = "one_time")]
    OneTimeAccept,
}

/// 决策请求（互调 api `auth.decide-consent` 载荷；字段对齐宿主
/// `peer:consent` 事件桥 payload：requestId / nodeId / fingerprintShort / deviceName）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsentRequest {
    /// 宿主 peer:consent 事件的 requestId（应答透传寻址）
    pub request_id: String,
    /// 对端节点 ID（64 位小写 hex，宿主 NodeId 语义）
    pub node_id: String,
    /// 短指纹（前 8 位；对端展示兜底）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint_short: Option<String>,
    /// 对端展示名（发现缓存；缺失时以短指纹兜底）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_name: Option<String>,
    /// 用户显式意向（缺省 = 仅做策略评估：已信任自动放行 / 未知 → ask）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_decision: Option<UserDecision>,
}

/// 决策种类（wire 形状小写）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionKind {
    /// 放行
    Accept,
    /// 拒绝
    Deny,
    /// 需要用户确认（消费方弹窗；取得意向后再调）
    Ask,
}

/// 决策理由（wire 形状 snake_case；ask 时无理由）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionReason {
    /// 已信任免确认（node_id 在可信列表）
    Trusted,
    /// 用户显式允许 / 拒绝
    Explicit,
    /// 一次性确认（本请求放行、不改变信任状态）
    OneTime,
}

/// 决策结果（互调 api `auth.decide-consent` 返回；wire 形状 camelCase）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsentDecision {
    pub decision: DecisionKind,
    /// 仅 accept/deny 携带；ask 为 None（JSON 缺省不输出）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<DecisionReason>,
    /// 对应请求 ID（应答寻址透传）
    pub request_id: String,
}

impl ConsentDecision {
    /// 放行（reason 区分 trusted / explicit / one_time）
    pub fn accept(reason: DecisionReason, request_id: &str) -> Self {
        Self {
            decision: DecisionKind::Accept,
            reason: Some(reason),
            request_id: request_id.to_string(),
        }
    }

    /// 拒绝（reason 恒为 explicit —— 拒绝必然来自用户显式选择）
    pub fn deny(request_id: &str) -> Self {
        Self {
            decision: DecisionKind::Deny,
            reason: Some(DecisionReason::Explicit),
            request_id: request_id.to_string(),
        }
    }

    /// 需用户确认（未知 peer 且无意向；无 reason）
    pub fn ask(request_id: &str) -> Self {
        Self {
            decision: DecisionKind::Ask,
            reason: None,
            request_id: request_id.to_string(),
        }
    }
}
