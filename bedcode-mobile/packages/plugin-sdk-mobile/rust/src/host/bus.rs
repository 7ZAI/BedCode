//! 宿主能力：插件间消息总线（Topic 发布/订阅，JSON + 二进制双载荷）

use super::HostError;

/// 插件间消息总线
///
/// 订阅关系可在运行时动态订阅。JSON 消息通过
/// [`WasmPlugin::on_bus_message`](crate::wasm::WasmPlugin::on_bus_message) 回调接收，
/// 二进制消息经
/// [`WasmPlugin::on_message_binary`](crate::wasm::WasmPlugin::on_message_binary)
/// 回调接收；均不投递给发送者自身。
pub trait HostBus {
    /// 发布消息到 topic
    fn bus_publish(&self, topic: &str, payload: &serde_json::Value) -> Result<(), HostError>;

    /// 发布二进制消息到 topic（v9）：字节列原样透传，零 JSON 编解码，
    /// 可传非 UTF-8 与大载荷（MB 级）
    ///
    /// 仅以 [`Self::bus_subscribe_binary`] 订阅该 topic 的插件接收；
    /// JSON 偏好订阅者被宿主按格式不匹配拒绝（反之亦然）。
    fn bus_publish_binary(&self, topic: &str, payload: &[u8]) -> Result<(), HostError>;

    /// 订阅 topic
    fn bus_subscribe(&self, topic: &str) -> Result<(), HostError>;

    /// 以二进制格式偏好订阅 topic（v9）：只接收 [`Self::bus_publish_binary`]
    /// 投递，JSON 消息对该订阅按格式不匹配拒绝；投递经
    /// [`WasmPlugin::on_message_binary`](crate::wasm::WasmPlugin::on_message_binary) 回调
    fn bus_subscribe_binary(&self, topic: &str) -> Result<(), HostError>;

    /// 取消订阅
    fn bus_unsubscribe(&self, topic: &str) -> Result<(), HostError>;
}
