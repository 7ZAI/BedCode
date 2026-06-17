//! WsServerConfig - WebSocket 服务器配置

use crate::shared::websocket::traits::ResponseHandler;
use std::net::IpAddr;
use std::sync::Arc;

/// IP 过滤规则
#[derive(Debug, Clone, Default)]
pub struct IpFilter {
    /// IP 白名单（为空表示允许所有）
    pub whitelist: Vec<IpAddr>,
    /// IP 黑名单（为空表示不屏蔽任何 IP）
    pub blacklist: Vec<IpAddr>,
}

impl IpFilter {
    /// 检查 IP 是否允许连接
    pub fn is_allowed(&self, ip: &IpAddr) -> bool {
        // 白名单优先：如果有白名单，只允许白名单中的 IP
        if !self.whitelist.is_empty() {
            return self.whitelist.contains(ip);
        }
        // 否则检查黑名单：如果在黑名单中则拒绝
        return !self.blacklist.contains(ip);
    }
}

/// WebSocket 服务器配置
#[derive(Clone)]
pub struct WsServerConfig {
    /// 监听端口
    pub port: u16,
    /// 最大连接数（0 表示不限制）
    pub max_connections: usize,
    /// 心跳间隔（秒）
    pub heartbeat_interval_secs: u64,
    /// 心跳超时（秒）
    pub heartbeat_timeout_secs: u64,
    /// 消息队列大小
    pub message_queue_size: usize,
    /// 业务处理线程池大小（0 表示使用 tokio 默认的阻塞线程池）
    pub business_thread_pool_size: usize,
    /// IP 过滤规则
    pub ip_filter: IpFilter,
    /// 响应处理器（处理需要响应的消息）
    pub response_handler: Option<Arc<dyn ResponseHandler>>,
    /// HTTP 路由器（处理非 WebSocket 的 HTTP 请求）
    pub http_router: Option<Arc<crate::shared::websocket::server::http_router::HttpRouter>>,
}

impl Default for WsServerConfig {
    fn default() -> Self {
        Self {
            port: 8765,
            max_connections: 0, // 0 表示不限制
            heartbeat_interval_secs: 20,  // 每 20 秒检查一次
            heartbeat_timeout_secs: 60,   // 超时 60 秒判定为僵尸连接
            message_queue_size: 512,
            business_thread_pool_size: 0, // 使用 tokio 默认
            ip_filter: IpFilter::default(),
            response_handler: None,
            http_router: None,
        }
    }
}

impl std::fmt::Debug for WsServerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WsServerConfig")
            .field("port", &self.port)
            .field("max_connections", &self.max_connections)
            .field("heartbeat_interval_secs", &self.heartbeat_interval_secs)
            .field("heartbeat_timeout_secs", &self.heartbeat_timeout_secs)
            .field("message_queue_size", &self.message_queue_size)
            .field("business_thread_pool_size", &self.business_thread_pool_size)
            .field("ip_filter", &self.ip_filter)
            .field("response_handler", &"...")
            .field("http_router", &if self.http_router.is_some() { "Some" } else { "None" })
            .finish()
    }
}