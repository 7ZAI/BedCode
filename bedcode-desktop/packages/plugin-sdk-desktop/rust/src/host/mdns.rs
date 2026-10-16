//! mDNS 浏览纯能力（WIT `host-mdns` v2，ADR 0022）
//!
//! 发现/离开经消息总线**属主私有 topic**（[`mdns_event_topic`] 生成：
//! `<owner>::mdns:found` / `<owner>::mdns:lost`）原样透传，宿主不做任何加工；
//! 设备列表等派生视图由消费插件自建缓存。v2 新增广播原语：config-json 为纯引擎参数，宿主零业务拼装。

use crate::host::HostError;

// ==================== 发现事件 topic（属主私有命名空间，勿手拼） ====================

/// 发现到新实例（唯一的「新增/更新」事件）
pub const MDNS_FOUND: &str = "mdns:found";
/// 实例离开（TTL 过期 / byebye）
pub const MDNS_LOST: &str = "mdns:lost";

/// 生成属主私有发现事件 topic：`<owner>::mdns:<event>`
///
/// 宿主只向 browse 发起方（属主）的命名空间投递，他人订阅被宿主拒绝。
pub fn mdns_event_topic(event: &str, plugin_id: &str) -> String {
    super::bus::owned_topic(plugin_id, event)
}

/// mDNS 浏览能力 trait —— 函数签名与 WIT `host-mdns` 一一对应
pub trait HostMdns {
    /// 浏览某服务类型，返回 browser 句柄（事件定向投递到属主私有 topic
    /// `<plugin-id>::mdns:found` / `<plugin-id>::mdns:lost`，见 [`mdns_event_topic`]）
    fn mdns_browse(&self, service_type: &str) -> Result<String, HostError>;
    /// 停止浏览并回收句柄（返回是否存在该句柄）
    fn mdns_stop_browse(&self, browser_id: &str) -> Result<bool, HostError>;
    /// 广播某服务类型（config-json 为纯引擎参数：服务类型 / 实例名 / 端口 / TXT 键值），返回 advertise 句柄
    fn mdns_advertise(&self, config_json: &str) -> Result<String, HostError>;
    /// 停止广播并回收句柄（返回是否存在该句柄）
    fn mdns_stop_advertise(&self, advertise_id: &str) -> Result<bool, HostError>;
    /// 查询广播状态（返回是否存在该句柄）
    fn mdns_is_advertising(&self, advertise_id: &str) -> Result<bool, HostError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 发现事件 topic 形状：属主私有命名空间 + 事件名常量
    #[test]
    fn mdns_event_topic_uses_owner_namespace() {
        assert_eq!(mdns_event_topic(MDNS_FOUND, "com.x"), "com.x::mdns:found");
        assert_eq!(mdns_event_topic(MDNS_LOST, "com.x"), "com.x::mdns:lost");
        assert_ne!(mdns_event_topic(MDNS_FOUND, "a"), mdns_event_topic(MDNS_FOUND, "b"));
    }
}
