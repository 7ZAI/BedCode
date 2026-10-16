//! 插件身份运行期注入。
//!
//! 业务核**不得硬编码任何产品身份**（插件 id / 事件命名空间）：核里出现具体插件 id 就等于
//! 「核知道了有哪些产品」，直接损害它对任何宿主可复用的属性（宿主侧语义锁
//! `capability_crates_no_product_ids` 的 C-4 判据同源）。故 id 与短名由双端在装配期注入。
//!
//! - `owned_topic(suffix)`：属主私有总线 topic（`<id>::<suffix>`），双端规则逐字一致
//!   （桌面 `mdns_event_topic` / 移动 `format!` 同值）。
//! - `event_name(suffix)`：向前端广播的事件名（`plugin:<短名>:<suffix>`）。短名与插件 id 的
//!   产品段同值但不是同一字符串，故分开注入而非从 id 里切。

/// 运行期注入的插件身份（借用，核内不持有所有权）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PluginIdentity<'a> {
    /// 完整插件 id（`<反向域名>.<产品段>`），用于属主私有 bus topic
    pub id: &'a str,
    /// 前端可见的事件命名空间短名（与 id 的产品段同值，如 `file-transfer`）
    pub namespace: &'a str,
}

impl<'a> PluginIdentity<'a> {
    pub const fn new(id: &'a str, namespace: &'a str) -> Self {
        PluginIdentity { id, namespace }
    }

    /// 属主私有总线 topic：`<id>::<suffix>`
    pub fn owned_topic(&self, suffix: &str) -> String {
        format!("{}::{suffix}", self.id)
    }

    /// 前端事件名：`plugin:<短名>:<suffix>`
    pub fn event_name(&self, suffix: &str) -> String {
        format!("plugin:{}:{suffix}", self.namespace)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owned_topic_shape_is_end_agnostic() {
        let id = PluginIdentity::new("com.bedcode.example", "example");
        assert_eq!(
            id.owned_topic("mdns:found"),
            "com.bedcode.example::mdns:found"
        );
        assert_eq!(
            id.owned_topic("mdns:lost"),
            "com.bedcode.example::mdns:lost"
        );
    }

    #[test]
    fn event_name_uses_short_namespace_not_full_id() {
        let id = PluginIdentity::new("com.bedcode.example", "example");
        assert_eq!(id.event_name("mdns-found"), "plugin:example:mdns-found");
        // 事件名不得把完整 id 当命名空间（前端订阅名与退役前逐字一致）
        assert!(!id.event_name("x").contains("com.bedcode"));
    }
}
