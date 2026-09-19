//! consent 模块（票 09 B4）— peer 首连确认决策
//!
//! 收敛 peer-net 首连确认语义（宿主 `peer:consent` 事件桥 + `respond_peer_consent`
//! 命令）的决策层：给定 peer 信息与当前信任状态，判定「放行 / 拒绝 / 需确认」。
//! 决策经互调 api `auth.decide-consent` 暴露（ADR 0017：manifest `api` 声明即
//! 契约），消费方 file-transfer 在票 10 迁移接入。
//!
//! ## 两阶段消费流（lib.rs 互调 api）
//!
//! 1. 消费方收到 `peer:consent` 事件 → 调 `auth.decide-consent(peer-info)`
//!    （无用户意向）：已信任 → accept（免确认，不弹窗）；未知 → ask（弹窗）
//! 2. 用户在弹窗做出选择 → 回传 `userDecision`（accept / deny / one_time）
//!    再调 → 最终 accept / deny（one_time = 本请求放行、不改变信任状态）
//!
//! 决策优先级：**已信任免确认 > 用户显式意向 > ask**。信任状态不可验证时
//! fail-closed 按未知处理（要求确认而非假设放行/阻断），见 [`ops`] 模块文档。
//!
//! 行为等价约束（对照锚点）：判定基准 = 宿主 peer trust_store（`peer-list-trusted`
//! node_id 集合）——与宿主 peer-net 的 consent 闸门（只拦未信任首连）同源。

pub mod model;
pub mod ops;
