//! Mobile Connection Manager
//!
//! 连接管理 - 使用 shared WsClient 实现连接/断开/重连

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use anyhow::Context;
use tokio::sync::{broadcast, RwLock};
use tauri::{AppHandle, Emitter};
use tracing;

use crate::mobile::websocket_client::{
    ConnectionStatus as WsConnStatus, WsClient, WsClientConfig, WsClientEvent,
    ClientDefaultMessageHandler, MessageRouter,
};
use crate::shared::model::message::Message;
use crate::shared::system::error_boundary::spawn_with_error_boundary;
use crate::mobile::global::get_global_token;
use crate::Result;

use crate::mobile::router::{ClientBusinessRouter, ClientRouteContext, MobileEvent};
use crate::mobile::router::{TerminalHandler, AuthHandler, SyncHandler, SystemHandler};

// Re-export ConnectionStatus for public API
pub use crate::mobile::websocket_client::ConnectionStatus;

/// 重连配置
const MAX_RETRY: u32 = 5;
const RETRY_DELAYS: &[u64] = &[1000, 2000, 4000, 8000, 16000]; // 指数退避（毫秒）

/// 判断错误是否表示连接已断开或请求失败（需要通知前端）
fn is_disconnect_error(error: &crate::AppError) -> bool {
    match error {
        crate::AppError::WebSocket(msg) => {
            // 检查错误消息是否包含断开或超时相关的关键词
            let msg_lower = msg.to_lowercase();
            msg_lower.contains("not connected")
                || msg_lower.contains("disconnected")
                || msg_lower.contains("connection lost")
                || msg_lower.contains("connection closed")
                || msg_lower.contains("failed to send")
                || msg_lower.contains("channel closed")
                || msg_lower.contains("timeout")  // 超时也可能是连接问题
                || msg_lower.contains("response timeout")
        }
        _ => false,
    }
}

/// 构建业务路由器（connect / reconnect 共用）
fn build_router(event_tx: broadcast::Sender<MobileEvent>) -> Result<ClientBusinessRouter> {
    let ctx = ClientRouteContext::new(event_tx);
    ClientBusinessRouter::builder()
        .context(ctx)
        .route("Terminal", Arc::new(TerminalHandler))
        .route("Auth", Arc::new(AuthHandler))
        .route("SyncData", Arc::new(SyncHandler))
        .route("ServerClosed", Arc::new(SystemHandler))
        .route("Error", Arc::new(SystemHandler))
        .route("Ack", Arc::new(SystemHandler))
        .build()
}

/// 目标设备信息
#[derive(Debug, Clone)]
pub struct TargetDevice {
    pub address: String,
    pub port: u16,
    pub name: Option<String>,
}

/// 连接管理器
///
/// 职责：作为业务层代理，直接使用 WsClient 的状态
/// 不再维护独立的 ConnectionStatus 和 running 状态，避免重复
pub struct ConnectionManager {
    /// 目标设备
    target: Arc<RwLock<Option<TargetDevice>>>,
    /// WebSocket 客户端
    client: Arc<RwLock<Option<Arc<WsClient>>>>,
    /// 事件发送器（用于内部业务逻辑监听）
    event_tx: broadcast::Sender<MobileEvent>,
    /// 手动断开标记，用于区分意外断开（true=用户主动断开，不弹通知）
    manual_disconnect: Arc<AtomicBool>,
    /// 重试计数（用于重连）
    retry_count: Arc<AtomicU32>,
    /// 重连中标记
    is_reconnecting: Arc<AtomicBool>,
}

impl ConnectionManager {
    /// 创建新的连接管理器
    pub fn new() -> Arc<Self> {
        let (event_tx, _) = broadcast::channel(1024);

        Arc::new(Self {
            target: Arc::new(RwLock::new(None)),
            client: Arc::new(RwLock::new(None)),
            event_tx,
            manual_disconnect: Arc::new(AtomicBool::new(false)),
            retry_count: Arc::new(AtomicU32::new(0)),
            is_reconnecting: Arc::new(AtomicBool::new(false)),
        })
    }

    /// 获取当前连接状态（从 WsClient 获取）
    pub async fn get_status(&self) -> WsConnStatus {
        if let Some(client) = self.client.read().await.as_ref() {
            client.get_status().await
        } else {
            WsConnStatus::Disconnected
        }
    }

    /// 获取目标设备
    pub async fn get_target(&self) -> Option<TargetDevice> {
        self.target.read().await.clone()
    }

    /// 订阅事件
    pub fn subscribe(&self) -> broadcast::Receiver<MobileEvent> {
        self.event_tx.subscribe()
    }

    /// 连接到目标设备
    ///
    /// 简化逻辑：
    /// 1. 检查当前状态
    /// 2. 创建 WsClient 并直接 await 连接
    /// 3. WsClient 内部已经 spawn 了 sender_task 和 receiver_task
    pub async fn connect(&self, app_handle: AppHandle, address: String, port: u16, name: Option<String>) -> Result<()> {
        // 检查当前状态（从 WsClient 获取）
        {
            let status = self.get_status().await;
            if status == WsConnStatus::Connecting {
                let _ = app_handle.emit("ws_connecting", serde_json::json!({
                    "address": address,
                    "port": port,
                    "status": "already_connecting"
                }));
                tracing::info!("Already connecting");
                return Ok(());
            }
            if status == WsConnStatus::Connected || status == WsConnStatus::Paired {
                let _ = app_handle.emit("ws_connected", ());
                tracing::info!("Already connected");
                return Ok(());
            }
        }

        // 重置手动断开标记（新连接）
        self.manual_disconnect.store(false, Ordering::SeqCst);

        // 发射连接开始事件
        let _ = app_handle.emit("ws_connecting", serde_json::json!({
            "address": address,
            "port": port,
        }));
        tracing::info!("WebSocket connecting to {}:{}", address, port);

        // 清除上一次连接的客户端（如果有）
        // 先断开旧客户端，确保其 IO 任务停止，避免与新建连接冲突
        if let Some(old_client) = self.client.write().await.take() {
            tracing::debug!("Disconnecting previous client before creating new one");
            let _ = old_client.disconnect().await;
        }

        // 保存目标设备
        tracing::debug!("Saving target device...");
        *self.target.write().await = Some(TargetDevice {
            address: address.clone(),
            port,
            name: name.clone(),
        });
        tracing::debug!("Target device saved");

        // 创建配置和客户端
        tracing::debug!("Creating WsClientConfig with address: {}, port: {}", address, port);
        let config = WsClientConfig::new(&address, port).with_path("/ws/terminal");
        tracing::debug!("WsClientConfig created, url: {}", config.url());

        tracing::debug!("Creating WsClient...");
        let client = WsClient::new(config);
        tracing::debug!("WsClient created");

        // 构建路由器
        let router = build_router(self.event_tx.clone())?;

        // 设置 handler
        client.set_handler(Arc::new(ClientDefaultMessageHandler::new().with_router(Arc::new(router)))).await;

        tracing::debug!("Handler set, now calling client.connect()...");
        tracing::info!("About to call client.connect(), this should show Connection log...");
        match client.connect().await {
            Ok(_) => {
                tracing::info!("client.connect() succeeded");
            }
            Err(e) => {
                tracing::error!("client.connect() failed: {}", e);
                let _ = app_handle.emit("ws_error", serde_json::json!({
                    "message": format!("Connection failed: {}", e)
                }));
                return Err(e);
            }
        }

        // 创建连接断开监控任务
        // 订阅 WsClientEvent，在意外断开时通知前端
        {
            let mut event_rx = client.subscribe();
            let app_clone = app_handle.clone();
            let manual_flag = self.manual_disconnect.clone();
            spawn_with_error_boundary("connection_monitor", async move {
                tracing::debug!("[ConnMonitor] Started monitoring connection");
                while let Ok(event) = event_rx.recv().await {
                    match event {
                        WsClientEvent::Disconnected
                        | WsClientEvent::Error { .. }
                        | WsClientEvent::ServerClosed { .. } => {
                            if !manual_flag.load(Ordering::SeqCst) {
                                tracing::warn!("[ConnMonitor] Unexpected disconnect detected: {:?}", event);
                                let reason = match &event {
                                    WsClientEvent::ServerClosed { reason } => reason.clone(),
                                    WsClientEvent::Error { message } => message.clone(),
                                    _ => "Connection lost".to_string(),
                                };
                                let _ = app_clone.emit("ws_unexpected_disconnect", serde_json::json!({
                                    "reason": reason
                                }));
                            } else {
                                tracing::debug!("[ConnMonitor] Manual disconnect, skipping notification");
                            }
                            break;
                        }
                        _ => {}
                    }
                }
                tracing::debug!("[ConnMonitor] Stopped");
            });
        }

        // 保存客户端引用
        *self.client.write().await = Some(client);

        // 短暂等待连接稳定
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // 发射连接成功事件
        let _ = app_handle.emit("ws_connected", ());
        tracing::info!("Connection established");

        Ok(())
    }

    /// 连接（不带 AppHandle，用于测试）
    pub async fn connect_without_emit(&self, address: String, port: u16, name: Option<String>) -> Result<()> {
        // 检查当前状态
        let status = self.get_status().await;
        if status == WsConnStatus::Connected || status == WsConnStatus::Paired {
            return Ok(());
        }

        // 保存目标设备
        *self.target.write().await = Some(TargetDevice {
            address: address.clone(),
            port,
            name,
        });

        // 创建配置和客户端
        let config = WsClientConfig::new(&address, port).with_path("/ws/terminal");
        let client = WsClient::new(config);

        // 构建路由器
        let router = build_router(self.event_tx.clone())?;

        client.set_handler(Arc::new(ClientDefaultMessageHandler::new().with_router(Arc::new(router)))).await;

        // 直接 await 连接
        client.connect().await?;

        // 保存客户端引用
        *self.client.write().await = Some(client);

        // 短暂等待连接稳定
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        Ok(())
    }

    /// 断开连接
    pub async fn disconnect(&self) {
        // 设置手动断开标记，阻止监控任务弹出通知
        self.manual_disconnect.store(true, Ordering::SeqCst);
        // 重置重连标记，确保进行中的重连循环退出
        self.is_reconnecting.store(false, Ordering::SeqCst);
        tracing::info!("Disconnecting...");

        // 断开 WebSocket
        if let Some(client) = self.client.read().await.as_ref() {
            let _ = client.disconnect().await;
        }

        // 清除客户端
        *self.client.write().await = None;

        // 清除目标
        *self.target.write().await = None;
    }

    /// 尝试重连（最多3次，指数退避）
    pub async fn reconnect(&self, app_handle: AppHandle, _token: Option<String>) -> Result<()> {
        let mut current_retry: u32 = 0;

        while current_retry < MAX_RETRY {
            // 用户主动断开，停止重连循环
            if self.manual_disconnect.load(Ordering::SeqCst) {
                tracing::info!("Manual disconnect detected, aborting reconnect");
                self.is_reconnecting.store(false, Ordering::SeqCst);
                return Ok(());
            }

            // 检查是否已经在重连
            if self.is_reconnecting.load(Ordering::SeqCst) {
                tracing::info!("Already reconnecting, skip");
                return Ok(());
            }

            self.is_reconnecting.store(true, Ordering::SeqCst);
            self.retry_count.store(current_retry, Ordering::SeqCst);

            // 发射重连开始事件
            let _ = app_handle.emit("ws_reconnecting", serde_json::json!({
                "retry": current_retry + 1,
                "max_retry": MAX_RETRY
            }));
            tracing::info!("Reconnecting attempt {}/{}", current_retry + 1, MAX_RETRY);

            // 等待指数退避间隔（首次不等待）
            if current_retry > 0 {
                let delay = RETRY_DELAYS[(current_retry - 1) as usize];
                tracing::info!("Waiting {}ms before retry...", delay);
                tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
                // 等待期间用户可能已断开，再次检查
                if self.manual_disconnect.load(Ordering::SeqCst) {
                    tracing::info!("Manual disconnect during reconnect delay, aborting");
                    self.is_reconnecting.store(false, Ordering::SeqCst);
                    return Ok(());
                }
            }

            // 获取目标设备信息
            let target = self.target.read().await.clone();
            let Some(target) = target else {
                tracing::error!("No target device for reconnect");
                break;
            };

            // 断开并清除旧客户端
            if let Some(old_client) = self.client.write().await.take() {
                tracing::debug!("Disconnecting old client before reconnect attempt");
                let _ = old_client.disconnect().await;
            }

            // 创建新客户端
            let config = WsClientConfig::new(&target.address, target.port).with_path("/ws/terminal");
            let client = WsClient::new(config);

            // 构建路由器
            let router = build_router(self.event_tx.clone())?;

            client.set_handler(Arc::new(ClientDefaultMessageHandler::new().with_router(Arc::new(router)))).await;

            match client.connect().await {
                Ok(_) => {
                    tracing::info!("Reconnect attempt {} succeeded", current_retry + 1);

                    // 保存新客户端
                    *self.client.write().await = Some(client);

                    // 重连成功，重置状态
                    self.is_reconnecting.store(false, Ordering::SeqCst);
                    self.retry_count.store(0, Ordering::SeqCst);

                    let _ = app_handle.emit("ws_reconnected", ());
                    tracing::info!("Reconnect successful!");
                    return Ok(());
                }
                Err(e) => {
                    tracing::warn!("Reconnect attempt {} failed: {}", current_retry + 1, e);
                    current_retry += 1;
                }
            }
        }

        // 重连失败
        self.is_reconnecting.store(false, Ordering::SeqCst);
        let _ = app_handle.emit("ws_reconnect_failed", serde_json::json!({
            "reason": "Max retries exceeded"
        }));
        tracing::error!("Reconnect failed after {} attempts", MAX_RETRY);

        Err(crate::AppError::WebSocket("Reconnect failed".to_string()))
    }

    /// 发送消息（自动注入全局 Token）
    pub async fn send(&self, message: &Message) -> Result<()> {
        let token = get_global_token();
        let message = if !token.is_empty() {
            message.clone().with_token(&token)
        } else {
            message.clone()
        };

        let msg_preview = message.to_json().unwrap_or_default();
        tracing::info!("[ConnectionManager] send() message_type={:?}, preview={}",
            "Message",
            &msg_preview[..msg_preview.len().min(200)]);

        if let Some(client) = self.client.read().await.as_ref() {
            let result = client.send(&message).await;
            match &result {
                Ok(_) => {
                    tracing::info!("[ConnectionManager] send() result: OK");
                }
                Err(e) => {
                    tracing::error!("[ConnectionManager] send() failed: {}", e);
                }
            }
            result
        } else {
            tracing::error!("[ConnectionManager] send() client is None!");
            Err(crate::AppError::WebSocket("Not connected".to_string()))
        }
    }

    /// 发送消息并等待响应（自动注入全局 Token）
    pub async fn send_and_wait(&self, message: &Message, timeout: std::time::Duration) -> Result<Message> {
        let token = get_global_token();
        let message = if !token.is_empty() {
            message.clone().with_token(&token)
        } else {
            message.clone()
        };

        if let Some(client) = self.client.read().await.as_ref() {
            tracing::info!("[ConnectionManager] send_and_wait: client exists, status={:?}", client.get_status().await);
            let result = client.send_and_wait(&message, timeout).await
                .with_context(|| format!("send_and_wait timeout={}s", timeout.as_secs()))
                .map_err(|e| crate::AppError::WebSocket(e.to_string()));
            tracing::info!("[ConnectionManager] send_and_wait: result={:?}", result.as_ref().map(|m| m.message_type().unwrap_or("unknown")));
            result
        } else {
            tracing::error!("[ConnectionManager] send_and_wait: client is None!");
            Err(crate::AppError::WebSocket("Not connected".to_string()))
        }
    }

    /// 发送消息，失败时检查是否为断开错误并发射事件
    ///
    /// 此方法用于需要自动处理断开场景的调用方
    pub async fn send_with_disconnect_handling(
        &self,
        app_handle: &AppHandle,
        message: &Message,
    ) -> Result<()> {
        let result = self.send(message).await;

        if let Err(ref e) = result {
            if is_disconnect_error(e) {
                tracing::warn!("[ConnectionManager] send_with_disconnect_handling: detected disconnect error: {}", e);
                let _ = app_handle.emit("ws_unexpected_disconnect", serde_json::json!({
                    "reason": format!("连接已断开: {}", e)
                }));
            }
        }

        result
    }

    /// 发送消息并等待响应，失败时检查是否为断开错误并发射事件
    ///
    /// 此方法用于需要自动处理断开场景的调用方
    pub async fn send_and_wait_with_disconnect_handling(
        &self,
        app_handle: &AppHandle,
        message: &Message,
        timeout: std::time::Duration,
    ) -> Result<Message> {
        let result = self.send_and_wait(message, timeout).await;

        if let Err(ref e) = result {
            if is_disconnect_error(e) {
                tracing::warn!("[ConnectionManager] send_and_wait_with_disconnect_handling: detected disconnect error: {}", e);
                let _ = app_handle.emit("ws_unexpected_disconnect", serde_json::json!({
                    "reason": format!("连接已断开: {}", e)
                }));
            }
        }

        result
    }

    /// 检查是否已连接
    pub async fn is_connected(&self) -> bool {
        let status = self.get_status().await;
        matches!(status, WsConnStatus::Connected | WsConnStatus::Paired)
    }

    /// 设置为已配对状态
    pub async fn set_paired(&self) {
        if let Some(client) = self.client.read().await.as_ref() {
            client.set_status(WsConnStatus::Paired).await;
        }
    }

}

impl Default for ConnectionManager {
    fn default() -> Self {
        let (event_tx, _) = broadcast::channel(1024);

        Self {
            target: Arc::new(RwLock::new(None)),
            client: Arc::new(RwLock::new(None)),
            event_tx,
            manual_disconnect: Arc::new(AtomicBool::new(false)),
            retry_count: Arc::new(AtomicU32::new(0)),
            is_reconnecting: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl Clone for ConnectionManager {
    fn clone(&self) -> Self {
        Self {
            target: self.target.clone(),
            client: self.client.clone(),
            event_tx: self.event_tx.clone(),
            manual_disconnect: self.manual_disconnect.clone(),
            retry_count: self.retry_count.clone(),
            is_reconnecting: self.is_reconnecting.clone(),
        }
    }
}
