//! 宿主能力：插件间消息总线（Topic 发布/订阅，JSON + 二进制双载荷）
//!
//! ## topic 命名空间（审计票 05）
//!
//! topic 分两类，由串本身形态区分（宿主按形态仲裁，不依赖任何注册表）：
//!
//! - **私有命名空间** `<plugin-id>::<name>`：某插件的收件箱。宿主按属主定向投递
//!   （`pty:exit` / `ws:*` / `mdns:found|lost` 等生命周期事件），**只有属主能订阅、
//!   只有宿主能投递**——他人既窃听不到也伪投递不进。
//! - **公开 topic**（不含 `::`）：插件间广播道（`task:*` / `peer:*` /
//!   `bedcode.api.*` 等），任何已激活插件可订阅、可发布。
//!
//! 分隔符取 `::` 而非 `.`：插件 id 字符集为 `[a-z0-9-]` 分段（`validate_plugin_id`），
//! `:` 不可能出现在 id 里，故「哪一段是属主」零歧义，也不存在前缀抢占。
//! 构造私有 topic 一律用 [`owned_topic`]（或 `pty_event_topic` / `ws_event_topic` 等
//! 域助手），禁止手拼。

use super::HostError;

/// 私有 topic 命名空间分隔符（插件 id 字符集不含 `:`，故无歧义）
pub const TOPIC_NS_SEP: &str = "::";

/// 互调请求道前缀：`bedcode.api.<api>`（服务方订阅此道接请求）
pub const API_TOPIC_PREFIX: &str = "bedcode.api.";

/// 互调回复道前缀：`bedcode.api.reply.<caller>.<request-id>`
///
/// 回复道是 caller 的收件箱，但**不**走私有命名空间：响应者是别的插件，
/// 必须能发布进 caller 的回复道。窃听由「订阅面 host-only」堵（回复订阅由
/// 宿主在 `host-api-call` 内静态注册，插件从不订阅自己被投的路径），
/// 抢答由「宿主校验回复 `sender` == api 声明属主」堵。
pub const REPLY_TOPIC_PREFIX: &str = "bedcode.api.reply.";

/// 构造属主私有 topic：`<owner>::<name>`
pub fn owned_topic(owner: &str, name: &str) -> String {
    format!("{owner}{TOPIC_NS_SEP}{name}")
}

/// 解析 topic 的属主（第一个 `::` 之前）；`None` = 公开 topic
pub fn topic_owner(topic: &str) -> Option<&str> {
    topic.split_once(TOPIC_NS_SEP).map(|(owner, _)| owner)
}

/// 是否互调回复道 topic
pub fn is_reply_topic(topic: &str) -> bool {
    topic.starts_with(REPLY_TOPIC_PREFIX)
}

/// 是否旧版定向事件形态（`<base>.<own-plugin-id>`，无命名空间）
///
/// 票 05 迁移前的 SDK 助手产出的串（如 `pty:exit.com.x`）。宿主订阅面对其
/// **显式拒绝**而非放行：放行会让旧产物静默断流（订阅得到、永远收不到），
/// 而该形态公开后又可能被他人伪发布打进旧订阅者。
pub fn is_legacy_owner_suffix(topic: &str, plugin_id: &str) -> bool {
    if topic.contains(TOPIC_NS_SEP) {
        return false;
    }
    let suffix = format!(".{plugin_id}");
    topic.len() > suffix.len() && topic.ends_with(&suffix)
}

/// 插件间消息总线
///
/// 订阅关系可在 manifest `contributes.subscribes` 声明（激活时自动订阅），
/// 也可运行时动态订阅。JSON 消息通过
/// [`WasmPlugin::on_message`](crate::wasm::WasmPlugin::on_message) 回调接收，
/// 二进制消息经 [`WasmPlugin::on_message_binary`](crate::wasm::WasmPlugin::on_message_binary)
/// 回调接收；均不投递给发送者自身。
pub trait HostBus {
    /// 发布消息到 topic
    fn bus_publish(&self, topic: &str, payload: &serde_json::Value) -> Result<(), HostError>;

    /// 发布二进制消息到 topic（v11）：字节列原样透传，零 JSON 编解码，
    /// 可传非 UTF-8 与大载荷（MB 级）
    ///
    /// 仅以 [`Self::bus_subscribe_binary`] 订阅该 topic 的插件接收；
    /// JSON 偏好订阅者被宿主按格式不匹配拒绝（反之亦然）。
    fn bus_publish_binary(&self, topic: &str, payload: &[u8]) -> Result<(), HostError>;

    /// 订阅 topic
    fn bus_subscribe(&self, topic: &str) -> Result<(), HostError>;

    /// 以二进制格式偏好订阅 topic（v11）：只接收 [`Self::bus_publish_binary`]
    /// 投递，JSON 消息对该订阅按格式不匹配拒绝；投递经
    /// [`WasmPlugin::on_message_binary`](crate::wasm::WasmPlugin::on_message_binary) 回调
    fn bus_subscribe_binary(&self, topic: &str) -> Result<(), HostError>;

    /// 取消订阅
    fn bus_unsubscribe(&self, topic: &str) -> Result<(), HostError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 私有 topic 构造 ↔ 解析闭环，公开 topic 判为无属主
    #[test]
    fn owned_topic_roundtrip_and_public_topic_unowned() {
        let t = owned_topic("com.x", "pty:exit");
        assert_eq!(t, "com.x::pty:exit");
        assert_eq!(topic_owner(&t), Some("com.x"));
        assert_eq!(topic_owner("task:status-changed"), None);
        assert_eq!(topic_owner("peer:consent"), None);
    }

    /// 多个 `::` 时属主取第一段（事件名里再出现 `::` 不改归属判定）
    #[test]
    fn topic_owner_takes_first_separator() {
        assert_eq!(topic_owner("com.x::a::b"), Some("com.x"));
    }

    /// 空属主段仍判为「有属主命名空间」：不匹配任何插件 id → 门禁一律拒（fail-closed）
    #[test]
    fn empty_owner_segment_is_still_a_namespace() {
        assert_eq!(topic_owner("::x"), Some(""));
        assert_ne!(topic_owner("::x"), Some("com.x"));
    }

    /// 回复道判定：仅 `bedcode.api.reply.` 前缀命中，请求道不命中
    #[test]
    fn reply_topic_prefix_recognised() {
        assert!(is_reply_topic("bedcode.api.reply.com.x.req-1"));
        assert!(!is_reply_topic("bedcode.api.com.x.echo"));
        assert!(!is_reply_topic("task:status-changed"));
    }

    /// legacy 形态识别：只认「无命名空间 + 以 `.<自身 id>` 结尾 + base 非空」
    #[test]
    fn legacy_owner_suffix_matches_only_own_directed_form() {
        assert!(is_legacy_owner_suffix("pty:exit.com.x", "com.x"));
        assert!(is_legacy_owner_suffix("ws:client-connect.com.x", "com.x"));
        // 他人属主 / 新命名空间形态 / 公开 topic 都不算
        assert!(!is_legacy_owner_suffix("pty:exit.com.y", "com.x"));
        assert!(!is_legacy_owner_suffix("com.x::pty:exit", "com.x"));
        assert!(!is_legacy_owner_suffix("task:status-changed", "com.x"));
        // 边界：整串就是 `.com.x`（无 base）或本身即 id，不构成定向形态
        assert!(!is_legacy_owner_suffix(".com.x", "com.x"));
        assert!(!is_legacy_owner_suffix("com.x", "com.x"));
        // 后缀必须落在 `.` 边界上，否则 `xcom.x` 之类会误判
        assert!(!is_legacy_owner_suffix("pty:exitnot-com.x", "com.x"));
    }
}
