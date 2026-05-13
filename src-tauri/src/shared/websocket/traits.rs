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

/// WebSocket 服务器事件
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
    /// 客户端认证成功
    ClientAuthenticated {
        addr: SocketAddr,
        client_id: String,
    },
    /// 心跳超时
    HeartbeatTimeout {
        addr: SocketAddr,
        client_id: Option<String>,
    },
    /// 服务器关闭
    ServerClosed {
        reason: String,
    },
}