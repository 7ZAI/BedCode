//! consent 决策编排 —— 首连确认决策（票 05 自认证中心搬入会话中心）
//!
//! 决策规则（优先级从高到低）：
//! 1. **已信任免确认**：`node_id ∈ 宿主 peer 可信列表` → accept（reason=trusted），
//!    不再询问——宿主 peer-net 对受信对端直连数据面、consent 只拦未信任首连
//!    （peer_net.rs 同语义）
//! 2. **用户显式意向**：accept → accept（explicit）；deny → deny（explicit）；
//!    one_time → accept（one_time，本请求放行但不改变信任状态）
//! 3. **未知 peer 且无意向** → ask（消费方弹窗取得意向后再调，两阶段流见
//!    [`crate::consent`] 模块文档）
//!
//! ## 信任状态不可验证（fail-closed）
//!
//! 无头上下文 / peer-net 未启动时宿主 `peer-list-trusted` 报错（require_app
//! 失败）。本模块不静默放行也不假设可信：**按未知处理 → ask**（无法证明信任
//! 就要求确认），同时 log_warn 透出原因——与 trust.list 的 peerError 显性
//! 降级同一哲学（不吞错；决策结果可见地收紧为需确认）。

use super::model::{ConsentDecision, ConsentRequest, DecisionReason, UserDecision};
#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::host::HostLog;
#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::host::HostPeer;
#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::wasm_host::WasmHost;

/// 决策核心（纯函数，native 单测直测）：给定请求 + 当前可信节点集 → 决策
///
/// 校验：`request_id` / `node_id` 必填（空则拒绝——无身份/无寻址的请求没有
/// 可裁决的语义，显性报错而非猜测）。
pub fn decide(
    request: &ConsentRequest,
    trusted_node_ids: &[String],
) -> Result<ConsentDecision, String> {
    if request.request_id.is_empty() {
        return Err("requestId required".to_string());
    }
    if request.node_id.is_empty() {
        return Err("nodeId required".to_string());
    }

    // 1. 已信任免确认（优先级最高：信任状态一经建立，单次连接不重复询问）
    if trusted_node_ids.iter().any(|id| id == &request.node_id) {
        return Ok(ConsentDecision::accept(
            DecisionReason::Trusted,
            &request.request_id,
        ));
    }

    // 2. 用户显式意向 / 3. 未知 peer 无意向 → ask
    match request.user_decision {
        Some(UserDecision::Accept) => Ok(ConsentDecision::accept(
            DecisionReason::Explicit,
            &request.request_id,
        )),
        Some(UserDecision::Deny) => Ok(ConsentDecision::deny(&request.request_id)),
        Some(UserDecision::OneTimeAccept) => Ok(ConsentDecision::accept(
            DecisionReason::OneTime,
            &request.request_id,
        )),
        None => Ok(ConsentDecision::ask(&request.request_id)),
    }
}

/// 从宿主 peer `list-trusted` 响应提取 node_id 集合（TrustedPeerDto 数组；
/// 非数组/缺 nodeId 条目忽略——信任判定只认完整的节点标识）
#[cfg(target_arch = "wasm32")]
fn trusted_node_ids(value: &serde_json::Value) -> Vec<String> {
    value
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|e| e.get("nodeId").and_then(|n| n.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// 互调 api `consent-decide` 实现（wasm 运行时）：收集可信集 → 决策
///
/// 可信集取宿主 peer `list-trusted`（node_id 集合）。不可验证（无头上下文 /
/// peer-net 未启动）时按空集处理 → 未知 peer → ask（fail-closed，见模块文档），
/// 错误透出到日志（决策结果仍可见地收紧为需确认，不静默放行）。
#[cfg(target_arch = "wasm32")]
pub fn decide_consent_via_host(request: ConsentRequest) -> Result<ConsentDecision, String> {
    let trusted = match WasmHost.peer_list_trusted() {
        Ok(v) => trusted_node_ids(&v),
        Err(e) => {
            WasmHost.log_warn(&format!(
                "consent: trusted list unavailable, fail-closed to ask: {}",
                e.message
            ));
            Vec::new()
        }
    };
    decide(&request, &trusted)
}

/// native（cargo test）：无宿主环境显性失败（单测直测 [`decide`] 纯函数，
/// 经 mock 注入可信集；与 trust/keys.rs 同模式——native 链接不引用
/// wasm 专属 import 符号）
#[cfg(not(target_arch = "wasm32"))]
pub fn decide_consent_via_host(_request: ConsentRequest) -> Result<ConsentDecision, String> {
    Err("consent decide unavailable outside wasm runtime".to_string())
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consent::model::DecisionKind;

    const NODE_A: &str = "aabbccdd11223344aabbccdd11223344aabbccdd11223344aabbccdd11223344";
    const NODE_B: &str = "11223344aabbccdd11223344aabbccdd11223344aabbccdd11223344aabbccdd";

    fn request(
        request_id: &str,
        node_id: &str,
        user_decision: Option<UserDecision>,
    ) -> ConsentRequest {
        ConsentRequest {
            request_id: request_id.to_string(),
            node_id: node_id.to_string(),
            fingerprint_short: Some(node_id[..8].to_string()),
            device_name: Some("模拟对端".to_string()),
            user_decision,
        }
    }

    /// 已信任免确认：node_id 在可信集 → accept（reason=trusted），无需用户意向
    #[test]
    fn trusted_peer_auto_accepts_without_asking() {
        let r = request("req-1", NODE_A, None);
        let d = decide(&r, &[NODE_A.to_string()]).expect("decide");
        assert_eq!(d.decision, DecisionKind::Accept);
        assert_eq!(d.reason, Some(DecisionReason::Trusted));
        assert_eq!(d.request_id, "req-1");
    }

    /// 已信任免确认优先于用户意向：即使传入 deny，可信 peer 仍放行
    /// （信任状态是持久承诺，单次连接的否定应走 trust.revoke 而非每次确认）
    #[test]
    fn trusted_peer_wins_over_user_decision() {
        let r = request("req-1", NODE_A, Some(UserDecision::Deny));
        let d = decide(&r, &[NODE_A.to_string()]).expect("decide");
        assert_eq!(d.decision, DecisionKind::Accept, "信任优先于单次意向");
        assert_eq!(d.reason, Some(DecisionReason::Trusted));
    }

    /// 允许：未知 peer + 用户接受 → accept（reason=explicit）
    #[test]
    fn explicit_accept_allows_unknown_peer() {
        let r = request("req-2", NODE_B, Some(UserDecision::Accept));
        let d = decide(&r, &[]).expect("decide");
        assert_eq!(d.decision, DecisionKind::Accept);
        assert_eq!(d.reason, Some(DecisionReason::Explicit));
    }

    /// 拒绝：未知 peer + 用户拒绝 → deny（reason=explicit）
    #[test]
    fn explicit_deny_rejects_unknown_peer() {
        let r = request("req-3", NODE_B, Some(UserDecision::Deny));
        let d = decide(&r, &[]).expect("decide");
        assert_eq!(d.decision, DecisionKind::Deny);
        assert_eq!(d.reason, Some(DecisionReason::Explicit));
    }

    /// 一次性确认：本请求放行（reason=one_time），不改变信任状态——
    /// 与「允许」的判别在于 reason：消费方据此不落信任，下次首连仍需确认
    #[test]
    fn one_time_accept_grants_request_without_trust() {
        let r = request("req-4", NODE_B, Some(UserDecision::OneTimeAccept));
        let d = decide(&r, &[]).expect("decide");
        assert_eq!(d.decision, DecisionKind::Accept);
        assert_eq!(
            d.reason,
            Some(DecisionReason::OneTime),
            "one_time 判别消费方不落信任"
        );
    }

    /// 未知 peer 且无用户意向 → ask（消费方弹窗；两阶段流的阶段 1）
    #[test]
    fn unknown_peer_without_decision_asks_for_confirmation() {
        let r = request("req-5", NODE_B, None);
        let d = decide(&r, &[]).expect("decide");
        assert_eq!(d.decision, DecisionKind::Ask);
        assert_eq!(d.reason, None, "ask 无理由字段（JSON 缺省不输出）");
    }

    /// 可信集中存在其它节点不影响判定（按 node_id 精确匹配）
    #[test]
    fn trust_match_is_exact_node_id() {
        let r = request("req-6", NODE_B, Some(UserDecision::Accept));
        let d = decide(&r, &[NODE_A.to_string()]).expect("decide");
        assert_eq!(
            d.decision,
            DecisionKind::Accept,
            "NODE_B 不在可信集 → 走用户意向"
        );
        assert_eq!(d.reason, Some(DecisionReason::Explicit));

        // 反向：NODE_B 在可信集时免确认
        let d2 = decide(&request("req-6", NODE_B, None), &[NODE_B.to_string()]).expect("decide");
        assert_eq!(d2.reason, Some(DecisionReason::Trusted));
    }

    /// 校验反例：requestId / nodeId 缺失显性报错（不猜测语义）
    #[test]
    fn missing_identity_fails_loudly() {
        let r = ConsentRequest {
            request_id: String::new(),
            node_id: NODE_A.to_string(),
            fingerprint_short: None,
            device_name: None,
            user_decision: Some(UserDecision::Accept),
        };
        let err = decide(&r, &[]).unwrap_err();
        assert!(err.contains("requestId"), "requestId 缺失必须报错: {}", err);

        let r2 = ConsentRequest {
            request_id: "req-x".to_string(),
            node_id: String::new(),
            fingerprint_short: None,
            device_name: None,
            user_decision: Some(UserDecision::Accept),
        };
        let err2 = decide(&r2, &[]).unwrap_err();
        assert!(err2.contains("nodeId"), "nodeId 缺失必须报错: {}", err2);
    }

    /// wire 形状：camelCase 序列化 + ask 无 reason + one_time 判别可解析
    #[test]
    fn decision_json_shape_is_contract() {
        // ask：无 reason 字段（JSON 缺省不输出）
        let json =
            serde_json::to_value(decide(&request("req-7", NODE_B, None), &[]).expect("decide"))
                .unwrap();
        assert_eq!(json["decision"], "ask");
        assert!(json.get("reason").is_none(), "ask 不得携带 reason 字段");
        assert_eq!(json["requestId"], "req-7");

        // accept（trusted）：reason 可解析回枚举
        let json2 = serde_json::to_value(
            decide(&request("req-8", NODE_A, None), &[NODE_A.to_string()]).expect("decide"),
        )
        .unwrap();
        assert_eq!(json2["decision"], "accept");
        assert_eq!(json2["reason"], "trusted");
        let parsed: ConsentDecision = serde_json::from_value(json2).expect("roundtrip");
        assert_eq!(parsed.decision, DecisionKind::Accept);
        assert_eq!(parsed.reason, Some(DecisionReason::Trusted));

        // one_time：wire 判别
        let json3 = serde_json::to_value(
            decide(
                &request("req-9", NODE_B, Some(UserDecision::OneTimeAccept)),
                &[],
            )
            .expect("decide"),
        )
        .unwrap();
        assert_eq!(json3["decision"], "accept");
        assert_eq!(json3["reason"], "one_time");
    }

    /// 请求 wire 反序列化：camelCase 入参 + 可选字段缺省（消费方只传必填）
    #[test]
    fn request_json_shape_is_contract() {
        let json = serde_json::json!({
            "requestId": "req-10",
            "nodeId": NODE_A,
            "userDecision": "deny",
        });
        let r: ConsentRequest = serde_json::from_value(json).expect("parse");
        assert_eq!(r.request_id, "req-10");
        assert_eq!(r.node_id, NODE_A);
        assert!(r.fingerprint_short.is_none(), "可选字段缺省为 None");
        assert!(r.device_name.is_none());
        assert_eq!(r.user_decision, Some(UserDecision::Deny));

        // one_time wire 值
        let json2 = serde_json::json!({
            "requestId": "req-11",
            "nodeId": NODE_A,
            "userDecision": "one_time",
        });
        let r2: ConsentRequest = serde_json::from_value(json2).expect("parse");
        assert_eq!(r2.user_decision, Some(UserDecision::OneTimeAccept));
    }

    /// native 路径显性失败（无宿主环境不得假放行）
    #[test]
    fn native_path_fails_without_host() {
        assert!(decide_consent_via_host(request("req-x", NODE_A, None)).is_err());
    }
}
