//! Connection Module - WebSocket Connection Management
//!
//! 职责：管理 WebSocket 连接建立、断开、重连逻辑
//! 与 IO 模块分离：只负责 TCP/TLS 握手，不处理消息收发

use crate::connection::lifecycle::{ConnectionStatus, LifecycleManager};
use crate::Result;
use std::sync::Arc;
use tokio_tungstenite::tungstenite::protocol::Message as WsMsg;
use tracing::{debug, error, info, warn};

use crate::system::constants::connection::{
    DEFAULT_CONNECT_TIMEOUT_MS, DEFAULT_HEARTBEAT_INTERVAL_SECS,
    DEFAULT_MESSAGE_QUEUE_SIZE, WS_DEFAULT_PATH,
};

/// WebSocket 客户端配置
#[derive(Debug, Clone)]
pub struct WsClientConfig {
    /// 服务器地址
    pub address: String,
    /// 服务器端口
    pub port: u16,
    /// WebSocket 路径（默认 "/"）
    pub path: String,
    /// 心跳间隔（秒）
    pub heartbeat_interval_secs: u64,
    /// 消息队列大小
    pub message_queue_size: usize,
    /// 连接超时（毫秒）
    pub connect_timeout_ms: u64,
}

impl WsClientConfig {
    /// 创建新配置
    pub fn new(address: impl Into<String>, port: u16) -> Self {
        Self {
            address: address.into(),
            port,
            path: WS_DEFAULT_PATH.to_string(),
            heartbeat_interval_secs: DEFAULT_HEARTBEAT_INTERVAL_SECS,
            message_queue_size: DEFAULT_MESSAGE_QUEUE_SIZE,
            connect_timeout_ms: DEFAULT_CONNECT_TIMEOUT_MS,
        }
    }

    /// 设置 WS 路径
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = path.into();
        self
    }

    /// 获取 WebSocket URL
    pub fn url(&self) -> String {
        format!("ws://{}:{}{}", self.address, self.port, self.path)
    }
}

impl Default for WsClientConfig {
    fn default() -> Self {
        Self {
            address: "127.0.0.1".to_string(),
            port: 8765,
            path: WS_DEFAULT_PATH.to_string(),
            heartbeat_interval_secs: DEFAULT_HEARTBEAT_INTERVAL_SECS,
            message_queue_size: DEFAULT_MESSAGE_QUEUE_SIZE,
            connect_timeout_ms: DEFAULT_CONNECT_TIMEOUT_MS,
        }
    }
}

/// WS 层连接管理器 - 负责底层连接的建立和断开
///
/// 内部类型，仅由 WsClient 使用
pub struct WsConnectionManager {
    /// 配置
    config: WsClientConfig,
    /// 生命周期管理器
    lifecycle: Arc<LifecycleManager>,
    /// 运行中标记（原子操作，确保只有一个连接任务）
    running: Arc<std::sync::atomic::AtomicBool>,
}

impl WsConnectionManager {
    /// 创建新的连接管理器
    pub fn new(config: WsClientConfig, lifecycle: Arc<LifecycleManager>) -> Arc<Self> {
        Arc::new(Self {
            config,
            lifecycle,
            running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        })
    }

    /// 获取配置
    pub fn config(&self) -> &WsClientConfig {
        &self.config
    }

    /// 获取生命周期管理器
    pub fn lifecycle(&self) -> &Arc<LifecycleManager> {
        &self.lifecycle
    }

    /// 检查是否正在运行
    pub fn is_running(&self) -> bool {
        self.running.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// 建立 WebSocket 连接
    pub async fn connect(
        self: &Arc<Self>,
    ) -> Result<(
        tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
        tokio::sync::mpsc::Sender<WsMsg>,
    )> {
        // 使用原子操作确保只有一个连接任务在运行
        if self.running.compare_exchange(
            false,
            true,
            std::sync::atomic::Ordering::SeqCst,
            std::sync::atomic::Ordering::SeqCst,
        ).is_err() {
            warn!("Already connected or connecting");
            return Err(crate::AppError::WebSocket("Already connected or connecting".to_string()));
        }

        // 检查当前状态
        {
            let status = self.lifecycle.get_status().await;
            if status == ConnectionStatus::Connected || status == ConnectionStatus::Paired {
                self.running.store(false, std::sync::atomic::Ordering::SeqCst);
                warn!("Already connected");
                return Err(crate::AppError::WebSocket("Already connected".to_string()));
            }
        }

        let url = self.config.url();
        info!("[WsConnectionManager] Connecting to {}", url);

        // 设置为连接中状态
        self.lifecycle.set_status(ConnectionStatus::Connecting).await;

        // 建立 WebSocket 连接（带超时）
        let connect_start = std::time::Instant::now();

        let ws_result = tokio::time::timeout(
            std::time::Duration::from_millis(self.config.connect_timeout_ms),
            tokio_tungstenite::connect_async(&url),
        )
        .await;

        // 在 async 上下文中处理连接错误，避免 map_err 闭包中调用 blocking 方法导致 panic
        let ws_stream = match ws_result {
            Ok(Ok(stream)) => stream,
            Ok(Err(e)) => {
                self.running.store(false, std::sync::atomic::Ordering::SeqCst);
                let error_msg = format!("Failed to connect: {}", e);
                self.lifecycle.set_status(ConnectionStatus::Error(error_msg.clone())).await;
                error!("[WsConnectionManager] Failed to connect to {}: {:#}", url, e);
                return Err(crate::AppError::WebSocket(error_msg));
            }
            Err(_) => {
                self.running.store(false, std::sync::atomic::Ordering::SeqCst);
                self.lifecycle.set_status(ConnectionStatus::Error("Connection timeout".to_string())).await;
                error!("[WsConnectionManager] Connection timeout after {}ms", self.config.connect_timeout_ms);
                return Err(crate::AppError::WebSocket("Connection timeout".to_string()));
            }
        };

        let connect_duration = connect_start.elapsed();
        tracing::info!("WebSocket handshake completed in {}ms", connect_duration.as_millis());

        // 获取读写流
        let (ws_stream, _) = ws_stream;

        // 创建消息通道（用于发送）
        let (tx, _rx) = tokio::sync::mpsc::channel::<WsMsg>(self.config.message_queue_size);

        // 更新连接状态为已连接
        self.lifecycle.set_status(ConnectionStatus::Connected).await;

        debug!("[WsConnectionManager] Connection established successfully");

        Ok((ws_stream, tx))
    }

    /// 断开连接
    pub async fn disconnect(&self) {
        info!("[WsConnectionManager] Disconnecting...");

        // 停止运行标记
        self.running.store(false, std::sync::atomic::Ordering::SeqCst);

        // 更新状态
        self.lifecycle.set_status(ConnectionStatus::Disconnected).await;

        info!("[WsConnectionManager] Disconnected");
    }

    /// 标记连接失败（内部使用）
    pub async fn mark_failed(&self, error: String) {
        self.running.store(false, std::sync::atomic::Ordering::SeqCst);
        self.lifecycle.set_status(ConnectionStatus::Error(error)).await;
    }

    /// 重置运行标记（用于允许重新连接）
    pub fn reset_running(&self) {
        self.running.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}