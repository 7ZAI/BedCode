//! WebSocket Traits Definition
//!
//! 定义泛型 trait，支持不同业务场景扩展

use std::fmt::Debug;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::time::{Duration, Instant};

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

impl Default for DefaultClientInfo {
    fn default() -> Self {
        Self {
            addr: std::net::SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)), 0),
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

/// 消息处理器 trait（泛型版本，用于���务器端）
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

// ==================== ClientMessageHandler (for WsClient) ====================

/// 客户端消息处理器 trait - 用于 WsClient 处理接收到的消息
pub trait ClientMessageHandler: Send + Sync {
    /// 处理接收到的消息
    fn handle(
        &self,
        message: WsMessage,
    ) -> Pin<Box<dyn Future<Output = HandlerResult> + Send + '_>>;

    /// 处理器名称
    fn name(&self) -> &str;
}

/// 空处理器 - 不处理任何消息
#[derive(Debug, Clone, Default)]
pub struct NoopHandler;

impl ClientMessageHandler for NoopHandler {
    fn handle(
        &self,
        _message: WsMessage,
    ) -> Pin<Box<dyn Future<Output = HandlerResult> + Send + '_>> {
        Box::pin(async { Ok(None) })
    }

    fn name(&self) -> &str {
        "NoopHandler"
    }
}

// ==================== SendStrategy ====================

/// 发送策略 trait - 定义消息发送的具体逻辑
pub trait SendStrategy: Send + Sync {
    /// 发送消息（异步，不等待响应）
    fn send<'a>(
        &'a self,
        client: &'a crate::shared::websocket::WsClient,
        message: &'a WsMessage,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

    /// 发送消息并等待响应
    fn send_and_wait<'a>(
        &'a self,
        client: &'a crate::shared::websocket::WsClient,
        message: &'a WsMessage,
        timeout: Duration,
    ) -> Pin<Box<dyn Future<Output = Result<WsMessage>> + Send + 'a>>;

    /// 策略名称
    fn name(&self) -> &str;
}

/// 默认发送策略 - 直接发送
#[derive(Debug, Clone, Default)]
pub struct DefaultSendStrategy;

impl SendStrategy for DefaultSendStrategy {
    fn send<'a>(
        &'a self,
        client: &'a crate::shared::websocket::WsClient,
        message: &'a WsMessage,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(client.send(message))
    }

    fn send_and_wait<'a>(
        &'a self,
        client: &'a crate::shared::websocket::WsClient,
        message: &'a WsMessage,
        timeout: Duration,
    ) -> Pin<Box<dyn Future<Output = Result<WsMessage>> + Send + 'a>> {
        Box::pin(async move {
            client.send_and_wait(message, timeout).await
        })
    }

    fn name(&self) -> &str {
        "DefaultSendStrategy"
    }
}

/// 重试发送策略
#[derive(Debug, Clone)]
pub struct RetrySendStrategy {
    pub max_retries: u32,
    pub delay: Duration,
}

impl Default for RetrySendStrategy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            delay: Duration::from_secs(1),
        }
    }
}

impl SendStrategy for RetrySendStrategy {
    fn send<'a>(
        &'a self,
        client: &'a crate::shared::websocket::WsClient,
        message: &'a WsMessage,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        let client = client.clone();
        let message = message.clone();
        let max_retries = self.max_retries;
        let delay = self.delay;

        Box::pin(async move {
            let mut last_error = None;
            for attempt in 0..max_retries {
                if attempt > 0 {
                    tokio::time::sleep(delay).await;
                }
                match client.send(&message).await {
                    Ok(()) => return Ok(()),
                    Err(e) => {
                        tracing::warn!("Send attempt {} failed: {}", attempt + 1, e);
                        last_error = Some(e);
                    }
                }
            }
            Err(last_error.unwrap_or_else(|| {
                crate::AppError::WebSocket("Max retries exceeded".to_string())
            }))
        })
    }

    fn send_and_wait<'a>(
        &'a self,
        client: &'a crate::shared::websocket::WsClient,
        message: &'a WsMessage,
        timeout: Duration,
    ) -> Pin<Box<dyn Future<Output = Result<WsMessage>> + Send + 'a>> {
        let client = client.clone();
        let message = message.clone();
        let max_retries = self.max_retries;
        let delay = self.delay;

        Box::pin(async move {
            let mut last_error = None;
            for attempt in 0..max_retries {
                if attempt > 0 {
                    tokio::time::sleep(delay).await;
                }
                match client.send_and_wait(&message, timeout).await {
                    Ok(msg) => return Ok(msg),
                    Err(e) => {
                        tracing::warn!("SendAndWait attempt {} failed: {}", attempt + 1, e);
                        last_error = Some(e);
                    }
                }
            }
            Err(last_error.unwrap_or_else(|| {
                crate::AppError::WebSocket("Max retries exceeded".to_string())
            }))
        })
    }

    fn name(&self) -> &str {
        "RetrySendStrategy"
    }
}

// ==================== SendInterceptor ====================

/// 发送拦截器 trait - 在发送前后执行自定义逻辑
pub trait SendInterceptor: Send + Sync {
    /// 发送前调用
    fn on_before_send(&self, message: &WsMessage) -> Result<()>;

    /// 发送后调用
    fn on_after_send(&self, message: &WsMessage, result: &Result<()>);

    /// 拦截器名称
    fn name(&self) -> &str;
}

/// 日志拦截器
#[derive(Debug, Clone, Default)]
pub struct LoggingInterceptor;

impl SendInterceptor for LoggingInterceptor {
    fn on_before_send(&self, message: &WsMessage) -> Result<()> {
        tracing::debug!(
            "[LoggingInterceptor] Sending message: type={}",
            message.message_type()
        );
        Ok(())
    }

    fn on_after_send(&self, _message: &WsMessage, result: &Result<()>) {
        match result {
            Ok(()) => tracing::debug!("[LoggingInterceptor] Message sent successfully"),
            Err(e) => tracing::error!("[LoggingInterceptor] Send failed: {}", e),
        }
    }

    fn name(&self) -> &str {
        "LoggingInterceptor"
    }
}

/// 监控拦截器
#[derive(Debug, Default)]
pub struct MetricsInterceptor {
    sent_total: std::sync::atomic::AtomicU64,
    sent_success: std::sync::atomic::AtomicU64,
    sent_failure: std::sync::atomic::AtomicU64,
}

impl MetricsInterceptor {
    pub fn sent_total(&self) -> u64 {
        self.sent_total
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn sent_success(&self) -> u64 {
        self.sent_success
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn sent_failure(&self) -> u64 {
        self.sent_failure
            .load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl SendInterceptor for MetricsInterceptor {
    fn on_before_send(&self, _message: &WsMessage) -> Result<()> {
        self.sent_total
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    fn on_after_send(&self, _message: &WsMessage, result: &Result<()>) {
        match result {
            Ok(()) => self
                .sent_success
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            Err(_) => self
                .sent_failure
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        };
    }

    fn name(&self) -> &str {
        "MetricsInterceptor"
    }
}

/// 响应处理器 trait
/// 用于处理需要响应的 WebSocket 消息
pub trait ResponseHandler: Send + Sync {
    /// 处理需要响应的消息
    /// - ws_message: 原始 WebSocket 消息
    /// - business_message: 解析后的业务消息
    /// 返回 None 表示使用默认响应，返回 Some(WsResponse) 使用自定义响应
    fn handle_response(
        &self,
        ws_message: &WsMessage,
        business_message: &crate::shared::enums::message::Message,
    ) -> Option<crate::shared::websocket::message::WsResponse>;
}

/// 默认响应处理器
/// 返回成功响应（code=0, message="OK"）
pub struct DefaultResponseHandler;

impl ResponseHandler for DefaultResponseHandler {
    fn handle_response(
        &self,
        _ws_message: &WsMessage,
        _business_message: &crate::shared::enums::message::Message,
    ) -> Option<crate::shared::websocket::message::WsResponse> {
        // 默认返回成功响应
        Some(crate::shared::websocket::message::WsResponse::success())
    }
}