//! 宿主能力：插件间消息总线（Topic 发布/订阅）

use super::HostError;

/// 插件间消息总线
///
/// 订阅关系可在 manifest `contributes.subscribes` 声明（激活时自动订阅），
/// 也可运行时动态订阅。消息通过
/// [`WasmPlugin::on_message`](crate::wasm::WasmPlugin::on_message) 回调接收，
/// 不投递给发送者自身。
pub trait HostBus {
    /// 发布消息到 topic
    fn bus_publish(&self, topic: &str, payload: &serde_json::Value) -> Result<(), HostError>;

    /// 订阅 topic
    fn bus_subscribe(&self, topic: &str) -> Result<(), HostError>;

    /// 取消订阅
    fn bus_unsubscribe(&self, topic: &str) -> Result<(), HostError>;
}
