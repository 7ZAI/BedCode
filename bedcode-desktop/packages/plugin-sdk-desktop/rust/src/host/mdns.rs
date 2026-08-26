//! mDNS 浏览纯能力（WIT `host-mdns`，ADR 0022 v2）
//!
//! browse-only：发现/离开经消息总线 `mdns:found` / `mdns:lost` 原样透传，
//! 宿主不做任何加工；设备列表等派生视图由消费插件自建缓存。

use crate::host::HostError;

/// mDNS 浏览能力 trait —— 函数签名与 WIT `host-mdns` 一一对应
pub trait HostMdns {
    /// 浏览某服务类型，返回 browser 句柄（发现/离开经总线 `mdns:*` topic 推送）
    fn mdns_browse(&self, service_type: &str) -> Result<String, HostError>;
    /// 停止浏览并回收句柄（返回是否存在该句柄）
    fn mdns_stop_browse(&self, browser_id: &str) -> Result<bool, HostError>;
}
