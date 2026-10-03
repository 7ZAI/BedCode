//! 使用统计解析层（票据 06）—— 纯函数，无宿主调用
//!
//! 按适配器拆分的统一解析层：claude / pi / opencode / codex 各家把日志行
//! 归一为事件流 + 会话级使用记录（看板与会话日志共用，§4.6「归一事件是
//! 使用记录的超集」）。本模块不落库、不调宿主——扫描（usage.rs）与
//! SQLite 源（usage_sqlite.rs）只消费 [`ParsedSession`] / [`NormalizedEvent`]。
//!
//! # 模块结构
//! - [`types`]：归一数据结构（TokenUsage / NormalizedEvent / ModelUsage / ParsedSession）
//! - [`time`]：ISO8601 → epoch 毫秒（纯函数）
//! - [`common`]：值提取与事件流公共工具（纯函数）
//! - [`claude`] / [`pi`] / [`opencode`] / [`codex`]：各家适配器（纯函数）
//!
//! 实机格式事实（2026-09-13 核验，spec §9）：
//! - claude `~/.claude/projects/<cwd→->/<uuid>.jsonl`：同一 assistant
//!   message 按内容块拆多行且**每行携带完整 usage**（220 行 / 97 个
//!   message.id）→ 聚合必须按 `message.id` 去重；token 字段 snake_case
//!   （`message.usage.input_tokens` 等）；`type=cost-state` 行含真实
//!   `totalCostUSD`（最后一条为准，有则存不估算）。
//! - pi `~/.pi/agent/sessions/<cwd桶>/<ts>_<uuid>.jsonl`：首行
//!   `type=session` 携带 id/cwd/timestamp；`type=message` 行 role ∈
//!   user/assistant/toolResult，token 字段 camelCase
//!   （`message.usage.input/output/cacheRead/cacheWrite/reasoning`）+
//!   `usage.cost.total`。
//! - opencode `~/.local/share/opencode/opencode.db`（**SQLite**）：不逐行
//!   JSONL 解析，而是 `usage_sqlite.rs` 用 `sqlite3` 只读查表后，把行交给
//!   [`opencode::parse_opencode_session_row`] / [`opencode::parse_opencode_events`]
//!   归一。实机事实（2026-09-27，54 会话 / 22 表）：时间戳是 epoch 毫秒
//!   （不复用 [`time::parse_iso8601_ms`]）；`model` 列是 JSON 串；`cost` 全为
//!   0.0 → 按 spec §4.5 落 NULL（未上报 ≠ 已知为零）；零 token 会话照常入库。
//! - codex `~/.codex/sessions/YYYY/MM/DD/rollout-<ts>-<uuid>.jsonl`：
//!   官方 rollout 格式（见 [`codex`] 模块头）。
//!
//! 水位不可得 mtime（WIT 无 stat 原语）：JSONL 为 append-only 语义，
//! 以 size（字节长）为水位即可保证幂等（见 usage.rs）。

mod claude;
mod codex;
mod common;
mod opencode;
mod pi;
mod time;
mod types;

/// 适配器入口（lib.rs 路由 + usage_sqlite.rs 取数层共用）
pub(crate) use claude::parse_claude_session;
pub(crate) use codex::parse_codex_session;
pub(crate) use opencode::{parse_opencode_events, parse_opencode_session_row};
pub(crate) use pi::parse_pi_session;
#[cfg(test)]
pub(crate) use types::TokenUsage;
pub(crate) use types::{ModelUsage, NormalizedEvent, ParsedSession};

/// 事件角色（wire 形状小写，与前端 NormalizedEvent.role 对应）
pub(crate) const ROLE_USER: &str = "user";
pub(crate) const ROLE_ASSISTANT: &str = "assistant";
pub(crate) const ROLE_TOOL: &str = "tool";
pub(crate) const ROLE_SYSTEM: &str = "system";

/// 事件流上限：防御异常巨大的会话文件拖垮 WATM 边界序列化，超出截断
pub(crate) const MAX_EVENTS: usize = 5000;
/// 附件事件（读过/注入的文件、工具清单、提醒）的**独立**上限
///
/// 附件行在长会话里极频繁（实测单会话 115 条），且几乎不带信息量。若与
/// user / assistant / tool 共用同一个 5000 额度，接近上限的会话里附件噪音会把
/// **后续实质消息**挤出日志视图——而用户看不出丢了什么（日志完整性回退）。
/// 独立计数后实质消息始终能拿满 `MAX_EVENTS`，整体仍有界（两者之和）。
pub(crate) const MAX_ATTACHMENT_EVENTS: usize = 500;
/// 单文件解析上限（字节）：超过视为异常数据跳过（正常会话 < 30MB）
pub(crate) const MAX_FILE_BYTES: usize = 64 * 1024 * 1024;
