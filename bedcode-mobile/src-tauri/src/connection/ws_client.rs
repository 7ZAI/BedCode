//! WebSocket Client - Main Implementation
//!
//! 整合所有子模块的主客户端，提供统一的 API
//! 使用 RequestResponseManager 实现请求-响应模式

use crate::connection::{
    ws_connection::WsConnectionManager, heartbeat::HeartbeatManager, io::IoManager,
    lifecycle::LifecycleManager, reconnect::ReconnectManager,
    ConnectionStatus, IoEvent, WsClientConfig, WsClientEvent, RequestResponseManager,
};
use crate::model::message::Message;
use crate::connection::MessageHandler;
use crate::Result;
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, Mutex, RwLock};
use tokio_tungstenite::tungstenite::protocol::Message as WsMsg;
use tracing::{debug, error, info, warn};

use crate::system::constants::connection::{
    BROADCAST_CHANNEL_CAPACITY, DISCONNECT_TASK_TIMEOUT_SECS,
    EVENT_FORWARDER_POLL_INTERVAL_MS, LOG_PREVIEW_MAX_LEN, PLACEHOLDER_CLIENT_ADDR,
    RECEIVER_POLL_INTERVAL_MS, SENDER_POLL_INTERVAL_MS,
};

/// WebSocket 客户端
pub struct WsClient {
    config: WsClientConfig,
    connection: Arc<WsConnectionManager>,
    io: Arc<IoManager>,
    heartbeat: Arc<HeartbeatManager>,
    lifecycle: Arc<LifecycleManager>,
    reconnect: Arc<ReconnectManager>,
    /// 请求-响应管理器
    request_manager: Arc<RequestResponseManager>,
    /// 推送消息处理器
    handler: RwLock<Option<Arc<dyn MessageHandler>>>,
    /// WebSocket 发送通道
    ws_sender: RwLock<Option<mpsc::Sender<WsMsg>>>,
    /// 运行标记
    running: Arc<std::sync::atomic::AtomicBool>,
    /// 任务句柄
    tasks: RwLock<ClientTasks>,
    /// 事件广播器（推送消息、连接状态等）
    event_tx: broadcast::Sender<WsClientEvent>,
}

#[derive(Debug, Default)]
struct ClientTasks {
    receiver: Option<Arc<tokio::task::JoinHandle<()>>>,
    sender: Option<Arc<tokio::task::JoinHandle<()>>>,
    event_forwarder: Option<Arc<tokio::task::JoinHandle<()>>>,
    heartbeat: Option<Arc<tokio::task::JoinHandle<()>>>,
}

impl WsClient {
    pub fn new(config: WsClientConfig) -> Arc<Self> {
        let lifecycle = LifecycleManager::new();
        let connection = WsConnectionManager::new(config.clone(), lifecycle.clone());
        let io = IoManager::new();
        let heartbeat = HeartbeatManager::from_client_config(config.heartbeat_interval_secs);
        let reconnect = ReconnectManager::from_client_config(config.heartbeat_interval_secs);
        let request_manager = RequestResponseManager::new();

        let (event_tx, _) = broadcast::channel(BROADCAST_CHANNEL_CAPACITY);

        Arc::new(Self {
            config: config.clone(),
            connection,
            io,
            heartbeat,
            lifecycle,
            reconnect,
            request_manager,
            handler: RwLock::new(None),
            ws_sender: RwLock::new(None),
            running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            tasks: RwLock::new(ClientTasks::default()),
            event_tx,
        })
    }

    pub fn config(&self) -> &WsClientConfig {
        &self.config
    }

    /// 订阅客户端事件（推送消息、连接状态等）
    pub fn subscribe(&self) -> broadcast::Receiver<WsClientEvent> {
        self.event_tx.subscribe()
    }

    /// 获取事件发送器
    pub fn event_tx(&self) -> broadcast::Sender<WsClientEvent> {
        self.event_tx.clone()
    }

    pub async fn get_status(&self) -> ConnectionStatus {
        self.lifecycle.get_status().await
    }

    pub async fn set_status(&self, status: ConnectionStatus) {
        self.lifecycle.set_status(status).await;
    }

    pub fn set_client_id(&self, client_id: impl Into<String>) {
        let client_id = client_id.into();
        let lifecycle = self.lifecycle.clone();
        tokio::spawn(async move {
            lifecycle.set_client_id(client_id).await;
        });
    }

    pub async fn get_client_id(&self) -> Option<String> {
        self.lifecycle.get_client_id().await
    }

    pub async fn is_connected(&self) -> bool {
        self.lifecycle.is_connected().await
    }

    /// 设置推送消息处理器
    pub async fn set_handler(&self, handler: Arc<dyn MessageHandler>) {
        *self.handler.write().await = Some(handler);
    }

    /// 获取请求-响应管理器
    pub fn request_manager(&self) -> Arc<RequestResponseManager> {
        self.request_manager.clone()
    }

    pub async fn connect(self: &Arc<Self>) -> Result<()> {
        info!("[WsClient] Starting connection to {}", self.config.url());

        let (stream, sender) = self.connection.connect().await?;

        *self.ws_sender.write().await = Some(sender.clone());
        self.running.store(true, std::sync::atomic::Ordering::SeqCst);

        self.spawn_io_tasks(stream, sender).await;
        self.start_event_forwarder().await;
        self.start_heartbeat_task().await;

        let _ = self.event_tx.send(WsClientEvent::Connected);

        info!("[WsClient] Connection established");
        Ok(())
    }

    async fn spawn_io_tasks(
        &self,
        stream: tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
        _sender: mpsc::Sender<WsMsg>,
    ) {
        let running = self.running.clone();

        let (tx, rx) = mpsc::channel::<WsMsg>(self.config.message_queue_size);
        *self.ws_sender.write().await = Some(tx);

        // 获取 handler 和 request_manager
        let handler = self.handler.read().await.clone();
        tracing::debug!("[WsClient] Handler status: is_some={}", handler.is_some());
        let request_manager = self.request_manager.clone();
        let event_tx = self.event_tx.clone();
        let heartbeat = self.heartbeat.clone();

        let (write, read) = stream.split();
        // write（SplitSink）仅由 sender 任务独占访问：receiver 的 Ping/Pong 回复
        // 经 ws_sender channel 转发，避免两个任务竞争同一把锁并在 send().await
        // 期间持有它（对端停止读时背压会让另一任务无限等锁）
        let write = Arc::new(Mutex::new(write));

        // receiver 任务经此 channel 回 Pong（与公开 send 同队列，串行写）
        let ws_sender = self.ws_sender.read().await.clone();
        let receiver_handle = {
            let running = running.clone();

            tokio::spawn(async move {
                use futures_util::StreamExt;
                let mut rx = read.fuse();

                info!("[WsClient] Receiver task started, waiting for messages...");

                loop {
                    if !running.load(std::sync::atomic::Ordering::SeqCst) {
                        break;
                    }

                    tokio::select! {
                        msg = rx.next() => {
                            match msg {
                                Some(Ok(WsMsg::Text(text))) => {
                                    debug!("[WsClient] <<< RECV: {}...", &text[..text.len().min(LOG_PREVIEW_MAX_LEN)]);

                                    // 1. 尝试匹配 pending 请求
                                    match request_manager.try_match(WsMsg::Text(text.clone())).await {
                                        Some(_) => {
                                            // 未匹配，是推送消息，交给 handler 处理
                                            debug!("[WsClient] Push message, handler is_some: {}", handler.is_some());
                                            if let Err(e) = event_tx.send(WsClientEvent::PushMessage {
                                                content: text.clone(),
                                            }) {
                                                // 广播满时静默丢弃会丢帧（移动端游标连续性破坏）——必须可观测
                                                let dropped = match e.0 {
                                                    WsClientEvent::PushMessage { content } => content,
                                                    _ => String::new(),
                                                };
                                                error!(
                                                    "[WsClient] Push message dropped (broadcast full): {}...",
                                                    &dropped[..dropped.len().min(LOG_PREVIEW_MAX_LEN)]
                                                );
                                            }

                                            if let Some(h) = &handler {
                                                h.handle(
                                                    WsMsg::Text(text),
                                                    PLACEHOLDER_CLIENT_ADDR.parse().unwrap(),
                                                    None,
                                                    None,
                                                );
                                            } else {
                                                warn!("[WsClient] No handler for push message!");
                                            }
                                        }
                                        None => {
                                            // 已匹配 pending 请求，无需处理
                                            debug!("[WsClient] Matched pending request");
                                        }
                                    }
                                }
                                Some(Ok(WsMsg::Binary(data))) => {
                                    debug!("[WsClient] <<< RECV Binary: {} bytes", data.len());
                                    // Binary 消息交给 handler 处理
                                    if let Some(h) = &handler {
                                        h.handle(
                                            WsMsg::Binary(data),
                                            "0.0.0.0:0".parse().unwrap(),
                                            None,
                                            None,
                                        );
                                    }
                                }
                                Some(Ok(WsMsg::Close(reason))) => {
                                    let reason_str = reason.map(|r| r.to_string()).unwrap_or_default();
                                    info!("[WsClient] Server closed: {}", reason_str);

                                    // 通知所有 pending 请求
                                    request_manager.on_error("Server closed").await;

                                    let _ = event_tx.send(WsClientEvent::ServerClosed { reason: reason_str });
                                    break;
                                }
                                Some(Ok(WsMsg::Ping(data))) => {
                                    // 经发送 channel 回复 Pong（write 由 sender 任务独占）
                                    let Some(sender) = ws_sender.as_ref() else { break };
                                    if let Err(e) = sender.send(WsMsg::Pong(data)).await {
                                        error!("[WsClient] Failed to send pong: {}", e);
                                        break;
                                    }
                                }
                                Some(Ok(WsMsg::Pong(_))) => {
                                    debug!("[WsClient] Received pong");
                                    heartbeat.on_pong_received().await;
                                    let _ = event_tx.send(WsClientEvent::HeartbeatResponse);
                                }
                                Some(Err(e)) => {
                                    error!("[WsClient] WebSocket error: {}", e);

                                    // 通知所有 pending 请求
                                    request_manager.on_error(&e.to_string()).await;

                                    let _ = event_tx.send(WsClientEvent::Error { message: e.to_string() });
                                    break;
                                }
                                None => break,
                                _ => {}
                            }
                        }
                        _ = tokio::time::sleep(std::time::Duration::from_millis(RECEIVER_POLL_INTERVAL_MS)) => {}
                    }
                }
            })
        };

        let write_for_sender = write.clone();
        let sender_handle = {
            let running = running.clone();

            tokio::spawn(async move {
                let mut rx = rx;

                loop {
                    tokio::select! {
                        msg = rx.recv() => {
                            match msg {
                                Some(WsMsg::Text(text)) => {
                                    debug!("[WsClient] >>> SEND: {}...", &text[..text.len().min(LOG_PREVIEW_MAX_LEN)]);
                                    let mut write = write_for_sender.lock().await;
                                    if let Err(e) = write.send(WsMsg::Text(text)).await {
                                        error!("[WsClient] Send error: {}", e);
                                        break;
                                    }
                                }
                                Some(WsMsg::Binary(data)) => {
                                    let mut write = write_for_sender.lock().await;
                                    if let Err(e) = write.send(WsMsg::Binary(data)).await {
                                        error!("[WsClient] Send binary error: {}", e);
                                        break;
                                    }
                                }
                                Some(WsMsg::Close(_)) => {
                                    break;
                                }
                                // 所有发送方已关闭：队列排空完毕，优雅退出
                                None => break,
                                _ => {}
                            }
                        }
                        _ = tokio::time::sleep(std::time::Duration::from_millis(SENDER_POLL_INTERVAL_MS)) => {
                            // 停止标记后（disconnect）继续排空队列；
                            // 队列为空且不再收新消息时才退出，避免丢弃已确认入队的消息
                            if !running.load(std::sync::atomic::Ordering::SeqCst) && rx.is_empty() {
                                break;
                            }
                        }
                    }
                }
            })
        };

        let mut tasks = self.tasks.write().await;
        tasks.receiver = Some(Arc::new(receiver_handle));
        tasks.sender = Some(Arc::new(sender_handle));
    }

    async fn start_event_forwarder(&self) {
        let io_subscription = self.io.subscribe();
        let lifecycle_subscription = self.lifecycle.subscribe();
        let event_tx = self.event_tx.clone();

        let handle = tokio::spawn(async move {
            let mut io_rx = io_subscription;
            let mut lifecycle_rx = lifecycle_subscription;

            loop {
                tokio::select! {
                    event = io_rx.recv() => {
                        match event {
                            Ok(IoEvent::HeartbeatResponse) => {
                                let _ = event_tx.send(WsClientEvent::HeartbeatResponse);
                            }
                            Ok(IoEvent::ConnectionClosed { reason }) => {
                                let _ = event_tx.send(WsClientEvent::ServerClosed { reason });
                            }
                            Ok(IoEvent::Error { message }) => {
                                let _ = event_tx.send(WsClientEvent::Error { message });
                            }
                            _ => {}
                        }
                    }
                    event = lifecycle_rx.recv() => {
                        match event {
                            Ok(crate::connection::lifecycle::LifecycleEvent::Disconnected) => {
                                let _ = event_tx.send(WsClientEvent::Disconnected);
                            }
                            _ => {}
                        }
                    }
                    _ = tokio::time::sleep(std::time::Duration::from_millis(EVENT_FORWARDER_POLL_INTERVAL_MS)) => {}
                }
            }
        });

        let mut tasks = self.tasks.write().await;
        tasks.event_forwarder = Some(Arc::new(handle));
    }

    /// 启动心跳保活任务
    ///
    /// 定期发送 WebSocket Ping 帧，检测连接是否仍然活跃。
    /// 连续超时 max_timeouts 次后发送 Error 事件，触发断连通知。
    async fn start_heartbeat_task(&self) {
        let running = self.running.clone();
        let ws_sender = self.ws_sender.read().await.clone();
        let heartbeat = self.heartbeat.clone();
        let event_tx = self.event_tx.clone();

        let interval = heartbeat.config().interval;
        let max_timeouts = heartbeat.config().max_timeouts;

        let handle = tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(interval);
            interval_timer.tick().await;

            loop {
                if !running.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }

                interval_timer.tick().await;

                if !running.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }

                // 发送 Ping
                if let Some(sender) = ws_sender.as_ref() {
                    match sender.send(WsMsg::Ping(vec![])).await {
                        Ok(_) => {
                            debug!("[Heartbeat] Ping sent");
                        }
                        Err(e) => {
                            warn!("[Heartbeat] Failed to send ping: {}", e);
                            let consecutive = heartbeat.increment_timeout().await;
                            if consecutive >= max_timeouts {
                                warn!("[Heartbeat] Max timeouts reached ({}), connection lost", consecutive);
                                let _ = event_tx.send(WsClientEvent::Error {
                                    message: format!("Heartbeat timeout after {} consecutive misses", consecutive),
                                });
                                break;
                            }
                            continue;
                        }
                    }
                } else {
                    warn!("[Heartbeat] No ws_sender, stopping heartbeat");
                    break;
                }

                // 检查心跳超时
                if heartbeat.is_connection_lost().await {
                    let consecutive = heartbeat.increment_timeout().await;
                    warn!("[Heartbeat] Heartbeat timeout (consecutive: {})", consecutive);
                    if consecutive >= max_timeouts {
                        warn!("[Heartbeat] Max timeouts reached, connection lost");
                        let _ = event_tx.send(WsClientEvent::Error {
                            message: format!("Heartbeat timeout after {} consecutive misses", consecutive),
                        });
                        break;
                    }
                }
            }

            heartbeat.stop().await;
            debug!("[Heartbeat] Task stopped");
        });

        let mut tasks = self.tasks.write().await;
        tasks.heartbeat = Some(Arc::new(handle));
    }

    pub async fn disconnect(&self) {
        info!("[WsClient] Disconnecting...");

        self.running.store(false, std::sync::atomic::Ordering::SeqCst);
        self.heartbeat.stop().await;
        *self.ws_sender.write().await = None;

        // 复位底层连接运行标记，允许同实例再次 connect（否则 reconnect 必报
        // "Already connected or connecting"）
        self.connection.reset_running();

        // 通知所有 pending 请求
        self.request_manager.on_error("Disconnected").await;

        self.await_tasks(DISCONNECT_TASK_TIMEOUT_SECS).await;

        self.lifecycle.set_status(ConnectionStatus::Disconnected).await;

        let _ = self.event_tx.send(WsClientEvent::Disconnected);

        info!("[WsClient] Disconnected");
    }

    async fn await_tasks(&self, _timeout_secs: u64) {
        // Note: We cannot directly await JoinHandle wrapped in Arc.
        // The tasks will be aborted when running flag is set to false above.
    }

    /// 发送消息（不等待响应）
    pub async fn send(&self, message: &Message) -> Result<()> {
        tracing::debug!("[WsClient] send() called, checking ws_sender...");
        if let Some(sender) = self.ws_sender.read().await.as_ref() {
            let json = message.to_json()?;
            tracing::debug!("[WsClient] >>> SEND to mpsc queue: {}...", &json[..json.len().min(LOG_PREVIEW_MAX_LEN)]);
            sender
                .send(WsMsg::Text(json))
                .await
                .map_err(|e| crate::AppError::WebSocket(format!("Failed to send: {}", e)))?;
            tracing::debug!("[WsClient] send() completed - message queued");
            Ok(())
        } else {
            tracing::error!("[WsClient] send() failed - ws_sender is None!");
            Err(crate::AppError::WebSocket("Not connected".to_string()))
        }
    }

    /// 发送原始文本
    pub async fn send_text(&self, content: &str) -> Result<()> {
        if let Some(sender) = self.ws_sender.read().await.as_ref() {
            let ws_msg = WsMsg::Text(content.to_string());
            sender
                .send(ws_msg)
                .await
                .map_err(|e| crate::AppError::WebSocket(format!("Failed to send: {}", e)))?;
            Ok(())
        } else {
            Err(crate::AppError::WebSocket("Not connected".to_string()))
        }
    }

    /// 发送消息并等待响应
    ///
    /// 使用 RequestResponseManager 实现精准投递：
    /// 1. 发送消息前注册 pending 请求
    /// 2. 收到响应时根据 message_id 匹配
    /// 3. 通过 oneshot 通道通知等待者
    pub async fn send_and_wait(
        &self,
        message: &Message,
        timeout: std::time::Duration,
    ) -> Result<Message> {
        let message_id = message.message_id()
            .ok_or_else(|| crate::AppError::WebSocket("Message has no message_id".to_string()))?
            .to_string();

        // 1. 注册 pending 请求
        let rx = self.request_manager.register(message_id.clone()).await;

        // 2. 发送消息
        if let Err(e) = self.send(message).await {
            // 发送失败，清理 pending
            self.request_manager.remove(&message_id).await;
            return Err(e);
        }

        // 3. 等待响应
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => {
                // oneshot 通道关闭
                self.request_manager.remove(&message_id).await;
                Err(crate::AppError::WebSocket("Response channel closed".to_string()))
            }
            Err(_) => {
                // 超时，清理 pending
                self.request_manager.remove(&message_id).await;
                Err(crate::AppError::WebSocket("Response timeout".to_string()))
            }
        }
    }

    pub async fn reconnect(self: &Arc<Self>) -> Result<()> {
        if !self.reconnect.should_retry().await {
            return Err(crate::AppError::WebSocket("Max retries exceeded".to_string()));
        }

        if let Some(delay) = self.reconnect.start().await {
            info!("[WsClient] Reconnecting in {:?}...", delay);
            tokio::time::sleep(delay).await;

            match self.connect().await {
                Ok(_) => {
                    self.reconnect.on_success().await;
                    Ok(())
                }
                Err(e) => {
                    self.reconnect.on_failure(e.to_string()).await;
                    Err(crate::AppError::WebSocket(format!("Reconnect failed: {}", e)))
                }
            }
        } else {
            Err(crate::AppError::WebSocket("Reconnect abandoned".to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{SinkExt, StreamExt};
    use tokio::net::TcpListener;
    use tokio_tungstenite::accept_async;
    use tokio_tungstenite::tungstenite::protocol::Message as ServerMsg;
    use std::time::{Duration, Instant};

    /// 本地 WS 服务端：accept 一次连接，收集文本消息直到连接关闭，Ping 回 Pong
    async fn spawn_local_server() -> (std::net::SocketAddr, tokio::task::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(stream).await.unwrap();
            let mut received = Vec::new();
            loop {
                match ws.next().await {
                    Some(Ok(ServerMsg::Text(t))) => received.push(t.to_string()),
                    Some(Ok(ServerMsg::Ping(d))) => {
                        let _ = ws.send(ServerMsg::Pong(d)).await;
                    }
                    Some(Ok(ServerMsg::Close(_))) | None => break,
                    _ => {}
                }
            }
            received
        });
        (addr, handle)
    }

    fn test_config(addr: std::net::SocketAddr) -> WsClientConfig {
        WsClientConfig::new("127.0.0.1", addr.port())
    }

    /// 高频发送不丢不乱：2000 条消息顺序与服务端收到顺序一致
    #[tokio::test]
    async fn send_high_frequency_preserves_order() {
        let (addr, server) = spawn_local_server().await;
        let client = WsClient::new(test_config(addr));
        client.connect().await.unwrap();

        const N: usize = 2000;
        for i in 0..N {
            client.send_text(&format!("msg-{i}")).await.unwrap();
        }

        // disconnect 触发连接关闭，服务端收满后返回
        client.disconnect().await;
        let received = tokio::time::timeout(Duration::from_secs(10), server)
            .await
            .expect("server should finish")
            .unwrap();

        assert_eq!(received.len(), N, "all messages must arrive");
        for (i, msg) in received.iter().enumerate() {
            assert_eq!(msg, &format!("msg-{i}"), "order must be preserved at index {}", i);
        }
    }

    /// 对端关闭后 send 快速失败（channel 关闭），不永久挂起
    #[tokio::test]
    async fn send_fails_fast_after_peer_close() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        // 服务端完成握手后（通知主测试）再关闭连接
        let (handshake_done_tx, mut handshake_done_rx) = tokio::sync::mpsc::channel::<()>(1);
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let _ws = accept_async(stream).await.unwrap();
            let _ = handshake_done_tx.send(()).await;
            // 等主测试确认已连接后 drop：模拟对端掉线
            tokio::time::sleep(Duration::from_secs(1)).await;
        });

        let client = WsClient::new(test_config(addr));
        client.connect().await.unwrap();
        // 确认服务端握手完成，随后服务端关闭连接
        let _ = tokio::time::timeout(Duration::from_secs(3), handshake_done_rx.recv())
            .await
            .expect("server handshake must complete");
        server.await.unwrap();

        // 等待 receiver/sender task 感知连接关闭
        tokio::time::sleep(Duration::from_millis(300)).await;

        // 发送应最终失败且不挂起（单条 3s 硬超时兜底断言）
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut last_result = Ok(());
        while Instant::now() < deadline {
            last_result = client.send_text("after-close").await;
            if last_result.is_err() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(
            last_result.is_err(),
            "send must fail after peer close, got: {:?}",
            last_result
        );
    }

    /// disconnect 后 send 返回错误（ws_sender 已清空），不静默成功
    #[tokio::test]
    async fn send_after_disconnect_returns_error() {
        let (addr, _server) = spawn_local_server().await;
        let client = WsClient::new(test_config(addr));
        client.connect().await.unwrap();
        client.disconnect().await;

        let result = client.send_text("after-disconnect").await;
        assert!(result.is_err(), "send after disconnect must fail, got: {:?}", result);
    }

    /// 重连重建通道：disconnect 后再次 connect，新 channel 可正常收发
    #[tokio::test]
    async fn disconnect_then_reconnect_rebuilds_channel() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        // 服务端接受两次连接，分别收集各自的消息
        let server = tokio::spawn(async move {
            let mut rounds: Vec<Vec<String>> = Vec::new();
            for _ in 0..2 {
                let (stream, _) = listener.accept().await.unwrap();
                let mut ws = accept_async(stream).await.unwrap();
                let mut received = Vec::new();
                loop {
                    match ws.next().await {
                        Some(Ok(ServerMsg::Text(t))) => received.push(t.to_string()),
                        Some(Ok(ServerMsg::Ping(d))) => {
                            let _ = ws.send(ServerMsg::Pong(d)).await;
                        }
                        Some(Ok(ServerMsg::Close(_))) | None => break,
                        _ => {}
                    }
                }
                rounds.push(received);
            }
            rounds
        });

        let client = WsClient::new(test_config(addr));
        client.connect().await.unwrap();
        client.send_text("round-1").await.unwrap();
        client.disconnect().await;

        // 直接再次 connect：应成功（而非 "Already connected"）
        let reconnect_result = tokio::time::timeout(Duration::from_secs(5), client.connect()).await;
        assert!(
            reconnect_result.is_ok(),
            "reconnect must not hang, got: {:?}",
            reconnect_result
        );
        assert!(
            reconnect_result.unwrap().is_ok(),
            "reconnect must succeed, running flag should be reset by disconnect"
        );

        client.send_text("round-2").await.unwrap();
        client.disconnect().await;

        let rounds = tokio::time::timeout(Duration::from_secs(10), server)
            .await
            .expect("server should finish")
            .unwrap();
        assert_eq!(rounds.len(), 2, "server must see two connections");
        assert_eq!(rounds[0], vec!["round-1".to_string()]);
        assert_eq!(rounds[1], vec!["round-2".to_string()]);
    }
}
