//! WebSocket Traits Definition
//!
//! 定义泛型 trait，支持不同业务场景扩展

use std::fmt::Debug;
use std::net::SocketAddr;
use std::time::Instant;

/// 客户端信息 trait（泛型基础）
/// 让不同业务场景可以定义自己的客户端信息结构
pub trait ClientInfoTrait: Send + Sync + Debug + Clone {
    /// 获取客户端地址
    fn addr(&self) -> SocketAddr;

    /// 获取客户端 ID
    fn client_id(&self) -> Option<&str>;

    /// 设置客户端 ID
    fn set_client_id(&mut self, id: Option<String>);

    /// 是否已认证
    fn is_authenticated(&self) -> bool;

    /// 设置认证状态
    fn set_authenticated(&mut self, auth: bool);

    /// 获取最后心跳时间
    fn last_heartbeat(&self) -> Instant;

    /// 设置最后心跳时间
    fn set_last_heartbeat(&mut self, time: Instant);
}