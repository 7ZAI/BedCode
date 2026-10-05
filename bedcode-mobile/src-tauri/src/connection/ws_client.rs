//! WebSocket Client - Main Implementation
//!
//! 整合所有子模块的主客户端，提供统一的 API
//! 使用 RequestResponseManager 实现请求-响应模式

use crate::connection::MessageHandler;
use crate::connection::{
    heartbeat::HeartbeatManager, io::IoManager, lifecycle::LifecycleManager,
    ws_connection::WsConnectionManager, ConnectionStatus, IoEvent, MatchOutcome, RequestResponseManager,
    WsClientConfig, WsClientEvent,
};
use crate::model::message::Message;
use crate::Result;
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, Mutex, RwLock};
use tokio_tungstenite::tungstenite::protocol::Message as WsMsg;
use tracing::{debug, error, info, warn};

use crate::system::constants::connection::{
    BROADCAST_CHANNEL_CAPACITY, DISCONNECT_TASK_TIMEOUT_SECS, EVENT_FORWARDER_POLL_INTERVAL_MS,
    LOG_PREVIEW_MAX_LEN, PLACEHOLDER_CLIENT_ADDR, RECEIVER_POLL_INTERVAL_MS, SENDER_POLL_INTERVAL_MS,
};

/// WebSocket 客户端
pub struct WsClient {
    config: WsClientConfig,
    connection: Arc<WsConnectionManager>,
    io: Arc<IoManager>,
    heartbeat: Arc<HeartbeatManager>,
    lifecycle: Arc<LifecycleManager>,
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
        let request_manager = RequestResponseManager::new();

        let (event_tx, _) = broadcast::channel(BROADCAST_CHANNEL_CAPACITY);

        Arc::new(Self {
            config: config.clone(),
            connection,
            io,
            heartbeat,
            lifecycle,
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

                                    // 1. 尝试匹配 pending 请求：三态裁决（见 request_response.rs）
                                    //    · Matched = 响应已按 id 投递，无需处理
                                    //    · Push = 可解析的业务推送帧 → 广播 + 交给 handler
                                    //    · Unroutable = 无法解析为业务 Message 的帧（插件端点事件帧/
                                    //      畸形帧）→ **同样交 handler**——handler 内 PluginEventRouter
                                    //      正是在此闭环；匹配层吞掉 = 事件帧到不了路由（票 03 必红）
                                    match request_manager.try_match(WsMsg::Text(text.clone())).await {
                                        MatchOutcome::Matched => {
                                            // 已匹配 pending 请求，无需处理
                                            debug!("[WsClient] Matched pending request");
                                        }
                                        MatchOutcome::Push(_) => {
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
                                        MatchOutcome::Unroutable => {
                                            // 无法解析为业务 Message 的帧（插件端点事件帧 / 畸形帧）：
                                            // 经 handler 侦察路由（PluginEventRouter 事件帧在此闭环；
                                            // 其余按原样留痕丢弃）。不发 PushMessage——插件帧不进旧广播面
                                            debug!(
                                                "[WsClient] Undecodable frame (plugin event?), handler is_some: {}",
                                                handler.is_some()
                                            );
                                            if let Some(h) = &handler {
                                                h.handle(
                                                    WsMsg::Text(text),
                                                    PLACEHOLDER_CLIENT_ADDR.parse().unwrap(),
                                                    None,
                                                    None,
                                                );
                                            } else {
                                                warn!("[WsClient] No handler for undecodable frame!");
                                            }
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
                                    // M1：保留 close **code**（认证类 4001/4003 致命，
                                    // 供 ConnMonitor/自愈监督判定「重连无意义」）；
                                    // 未携带 CloseFrame 时按 1005（无状态码）处理
                                    let code = reason
                                        .as_ref()
                                        .map(|f| u16::from(f.code))
                                        .unwrap_or(1005);
                                    let reason_str = reason
                                        .as_ref()
                                        .map(|r| r.reason.to_string())
                                        .unwrap_or_default();
                                    info!("[WsClient] Server closed: code={} reason={}", code, reason_str);

                                    // 通知所有 pending 请求
                                    request_manager.on_error("Server closed").await;

                                    let _ = event_tx.send(WsClientEvent::ServerClosed {
                                        code,
                                        reason: reason_str,
                                    });
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
                                // IoEvent::ConnectionClosed 无生产方（死路径）；
                                // code 按 1006（异常关闭）兜底，语义 = 非致命网络断连
                                let _ = event_tx.send(WsClientEvent::ServerClosed {
                                    code: 1006,
                                    reason,
                                });
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

        // 握手已成功（connect() 在此之后才调本方法），此刻标记建连时刻作为
        // 首个 Pong 到达前的半开检测基准。缺这一步会让这段窗口（默认 30s）
        // 内的死连接既不报 send 错也不判超时。
        heartbeat.mark_connected().await;

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
            tracing::debug!(
                "[WsClient] >>> SEND to mpsc queue: {}...",
                &json[..json.len().min(LOG_PREVIEW_MAX_LEN)]
            );
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
    pub async fn send_and_wait(&self, message: &Message, timeout: std::time::Duration) -> Result<Message> {
        let message_id = message
            .message_id()
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


}

// ==================== Tests ====================

// 用例按功能拆至 `ws_client/tests/`（本内联模块的子模块路径由 rustc
// 自动解析到该目录；模块树 `connection::ws_client::tests::<文件>` 与内联形态等价，私有项可见性不受影响）。
#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{SinkExt, StreamExt};
    use std::time::{Duration, Instant};
    use tokio::net::TcpListener;
    use tokio_tungstenite::accept_async;
    use tokio_tungstenite::tungstenite::protocol::Message as ServerMsg;
    // 跨分组共享的测试脚手架（子模块经 `use super::*` 可见）

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
    mod send_high_frequency;
}
