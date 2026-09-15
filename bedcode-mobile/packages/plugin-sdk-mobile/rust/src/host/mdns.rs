//! mDNS 浏览纯能力（WIT `host-mdns` v2，ADR 0022）
//!
//! 发现/离开经消息总线定向 topic（`mdns:found.<owner>` / `mdns:lost.<owner>`）
//! 原样透传，宿主不做任何加工；设备列表等派生视图由消费插件自建缓存。
//! v2 新增广播原语：config-json 为纯引擎参数，宿主零业务拼装。

use crate::host::HostError;

/// mDNS 浏览能力 trait —— 函数签名与 WIT `host-mdns` 一一对应
pub trait HostMdns {
    /// 浏览某服务类型，返回 browser 句柄（事件定向投递到 `mdns:found.<plugin-id>` / `mdns:lost.<plugin-id>`）
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
