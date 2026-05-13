//! WebSocket Server Events
//!
//! 泛型事件系统，支持自定义事件类型

use std::fmt::Debug;
use std::net::SocketAddr;

/// 服务器事件 trait - 泛型事件基础
/// 让业务可以定义自己的事件类型
pub trait ServerEvent: Clone + Send + Sync + Debug {}

/// 服务器事件构建器 trait
/// 用于服务器内部创建各种事件
pub trait ServerEventBuilder<E: ServerEvent>: Send + Sync {
    /// 创建文本消息事件
    fn text_message(&self, addr: SocketAddr, client_id: Option<String>, message_id: Option<String>, content: String) -> E;

    /// 创建二进制消息事件
    fn binary_message(&self, addr: SocketAddr, client_id: Option<String>, message_id: Option<String>, data: Vec<u8>) -> E;

    /// 创建客户端断开事件
    fn client_disconnected(&self, addr: SocketAddr, client_id: Option<String>) -> E;

    /// 创建消息错误事件
    fn message_error(&self, addr: SocketAddr, error: String) -> E;

    /// 创建服务器关闭事件
    fn server_closed(&self, reason: String) -> E;
}

/// 默认服务器事件实现
#[derive(Debug, Clone)]
pub enum WsServerEvent {
    /// 新客户端连接
    ClientConnected {
        addr: SocketAddr,
        client_id: Option<String>,
    },
    /// 客户端断开
    ClientDisconnected {
        addr: SocketAddr,
        client_id: Option<String>,
    },
    /// 收到文本消息
    TextMessage {
        addr: SocketAddr,
        client_id: Option<String>,
        message_id: Option<String>,
        content: String,
    },
    /// 收到二进制消息
    BinaryMessage {
        addr: SocketAddr,
        client_id: Option<String>,
        message_id: Option<String>,
        data: Vec<u8>,
    },
    /// 收到心跳
    Heartbeat {
        addr: SocketAddr,
    },
    /// 服务器关闭
    ServerClosed {
        reason: String,
    },
    /// 消息处理错误
    MessageError {
        addr: SocketAddr,
        error: String,
    },
    /// 认证成功
    AuthSuccess {
        addr: SocketAddr,
        client_id: String,
    },
}

impl ServerEvent for WsServerEvent {}

/// WsServerEvent 的构建器实现
impl ServerEventBuilder<WsServerEvent> for WsServerEventBuilder {
    fn text_message(
        &self,
        addr: SocketAddr,
        client_id: Option<String>,
        message_id: Option<String>,
        content: String,
    ) -> WsServerEvent {
        WsServerEvent::TextMessage {
            addr,
            client_id,
            message_id,
            content,
        }
    }

    fn binary_message(
        &self,
        addr: SocketAddr,
        client_id: Option<String>,
        message_id: Option<String>,
        data: Vec<u8>,
    ) -> WsServerEvent {
        WsServerEvent::BinaryMessage {
            addr,
            client_id,
            message_id,
            data,
        }
    }

    fn client_disconnected(&self, addr: SocketAddr, client_id: Option<String>) -> WsServerEvent {
        WsServerEvent::ClientDisconnected { addr, client_id }
    }

    fn message_error(&self, addr: SocketAddr, error: String) -> WsServerEvent {
        WsServerEvent::MessageError { addr, error }
    }

    fn server_closed(&self, reason: String) -> WsServerEvent {
        WsServerEvent::ServerClosed { reason }
    }
}

/// 服务器事件构建器
#[derive(Debug, Clone, Default)]
pub struct WsServerEventBuilder {
    addr: Option<SocketAddr>,
    client_id: Option<Option<String>>,
    message_id: Option<Option<String>>,
}

impl WsServerEventBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn addr(mut self, addr: SocketAddr) -> Self {
        self.addr = Some(addr);
        self
    }

    pub fn client_id(mut self, client_id: Option<String>) -> Self {
        self.client_id = Some(client_id);
        self
    }

    pub fn message_id(mut self, message_id: Option<String>) -> Self {
        self.message_id = Some(message_id);
        self
    }

    pub fn text_message(self, content: String) -> WsServerEvent {
        WsServerEvent::TextMessage {
            addr: self.addr.unwrap(),
            client_id: self.client_id.flatten(),
            message_id: self.message_id.flatten(),
            content,
        }
    }

    pub fn binary_message(self, data: Vec<u8>) -> WsServerEvent {
        WsServerEvent::BinaryMessage {
            addr: self.addr.unwrap(),
            client_id: self.client_id.flatten(),
            message_id: self.message_id.flatten(),
            data,
        }
    }

    pub fn connected(self) -> WsServerEvent {
        WsServerEvent::ClientConnected {
            addr: self.addr.unwrap(),
            client_id: self.client_id.flatten(),
        }
    }

    pub fn disconnected(self) -> WsServerEvent {
        WsServerEvent::ClientDisconnected {
            addr: self.addr.unwrap(),
            client_id: self.client_id.flatten(),
        }
    }
}