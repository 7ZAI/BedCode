//! WebSocket Traits Definition
//!
//! 定义泛型 trait，支持不同业务场景扩展

use std::fmt::Debug;
use std::net::SocketAddr;
use std::time::Instant;

use crate::shared::websocket::message::WsMessage;
use crate::Result;

/// 消息处理结果类型
pub type HandlerResult = Result<Option<WsMessage>>;

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

/// 默认基础实现（无业务扩展字段）
#[derive(Debug, Clone)]
pub struct DefaultClientInfo {
    pub addr: SocketAddr,
    pub client_id: Option<String>,
    pub authenticated: bool,
    pub last_heartbeat: Instant,
}

impl DefaultClientInfo {
    pub fn new(addr: SocketAddr) -> Self {
        Self {
            addr,
            client_id: None,
            authenticated: false,
            last_heartbeat: Instant::now(),
        }
    }
}

impl ClientInfoTrait for DefaultClientInfo {
    fn addr(&self) -> SocketAddr {
        self.addr
    }

    fn client_id(&self) -> Option<&str> {
        self.client_id.as_deref()
    }

    fn set_client_id(&mut self, id: Option<String>) {
        self.client_id = id;
    }

    fn is_authenticated(&self) -> bool {
        self.authenticated
    }

    fn set_authenticated(&mut self, auth: bool) {
        self.authenticated = auth;
    }

    fn last_heartbeat(&self) -> Instant {
        self.last_heartbeat
    }

    fn set_last_heartbeat(&mut self, time: Instant) {
        self.last_heartbeat = time;
    }
}

/// 消息处理器 trait（泛型版本）
pub trait MessageHandler<C: ClientInfoTrait>: Send + Sync {
    /// 处理文本消息（核心方法）
    fn handle_text(
        &self,
        message: &WsMessage,
        addr: SocketAddr,
        client_info: &C,
    ) -> HandlerResult {
        let _ = (message, addr, client_info);
        Ok(None)
    }

    /// 处理二进制消息
    fn handle_binary(
        &self,
        message: &WsMessage,
        addr: SocketAddr,
        client_info: &C,
    ) -> HandlerResult {
        let _ = (message, addr, client_info);
        Ok(None)
    }

    /// 连接建立时（WebSocket 握手后，还未注册到 clients）
    fn on_connecting(&self, _addr: SocketAddr) {}

    /// 客户端认证成功回调
    fn on_authenticated(&self, _addr: SocketAddr, _client_id: &str) {}

    /// 客户端断开连接回调
    fn on_disconnected(&self, _addr: SocketAddr, _client_id: Option<&str>) {}

    /// 心跳超时回调
    fn on_heartbeat_timeout(&self, _addr: SocketAddr, _client_id: Option<&str>) {}
}