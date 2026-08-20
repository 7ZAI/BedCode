//! Connection Manager
//!
//! 业务层连接管理。认证已 HTTP 化（spec §4.5）后，ConnectionManager 持有
//! 自己的连接状态（`status` 字段），不再委托 `WsClient.lifecycle`——
//! WS 层在 04（常驻事件 WS）之前不存在，「已认证」语义（Authed）由设备级
//! status 承载。WS 建连路径（`establish_ws_client`）保留给集成测试与 04 复用。

use anyhow::Context;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::{broadcast, RwLock};
use tracing;

use crate::connection::request::AuthRequest;
use crate::connection::{ClientDefaultMessageHandler, WsClient, WsClientConfig, WsClientEvent};
use crate::model::message::Message;
use crate::state::get_global_token;
use crate::system::error_boundary::spawn_with_error_boundary;
use crate::Result;

use crate::router::{AuthHandler, FileServiceHandler, SyncHandler, SystemHandler, TerminalHandler};
use crate::router::{ClientBusinessRouter, ClientRouteContext, MobileEvent};

use crate::system::constants::connection::{
    BROADCAST_CHANNEL_CAPACITY, CONNECTION_STABILIZE_DELAY_MS, LOG_PREVIEW_MAX_LEN, WS_EVENT_PATH,
};
use crate::system::constants::reconnect::{DEFAULT_MAX_RETRIES, DEFAULT_RETRY_DELAYS_MS};

// Re-export ConnectionStatus for public API
pub use crate::connection::ConnectionStatus;

/// 判断错误是否表示连接已断开（需要通知前端）
///
/// 仅对真正的连接级故障（通道关闭/未连接/连接丢失/发送失败）返回 true；
/// 超时类错误（Response timeout）**不是**连接断开——订阅/请求可能因服务端
/// 背压或处理慢而超时，此时连接仍存活，误报会触发前端断连提示与重连循环
fn is_disconnect_error(error: &crate::AppError) -> bool {
    match error {
        crate::AppError::WebSocket(msg) => {
            // 检查错误消息是否包含断开相关的关键词（不含 timeout）
            let msg_lower = msg.to_lowercase();
            msg_lower.contains("not connected")
                || msg_lower.contains("disconnected")
                || msg_lower.contains("connection lost")
                || msg_lower.contains("connection closed")
                || msg_lower.contains("failed to send")
                || msg_lower.contains("channel closed")
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
        .route("FileService", Arc::new(FileServiceHandler))
        .route("ServerClosed", Arc::new(SystemHandler))
        .route("Error", Arc::new(SystemHandler))
        .route("Ack", Arc::new(SystemHandler))
        .build()
}

/// 目标设备信息
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetDevice {
    pub address: String,
    pub port: u16,
    pub name: Option<String>,
}

/// 连接管理器
///
/// 职责：作为业务层代理，持有设备级连接状态（HTTP 认证语义），
/// WS 客户端（04 事件 WS）与目标信息按需维护
pub struct ConnectionManager {
    /// 目标设备
    target: Arc<RwLock<Option<TargetDevice>>>,
    /// WebSocket 客户端（03 过渡期无；04 事件 WS 恢复）
    client: Arc<RwLock<Option<Arc<WsClient>>>>,
    /// 设备级连接状态（认证 HTTP 化后不能委托 WsClient.lifecycle——
    /// 「WS 层不再持有连接状态语义」，spec §4.5）
    status: Arc<RwLock<ConnectionStatus>>,
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
        let (event_tx, _) = broadcast::channel(BROADCAST_CHANNEL_CAPACITY);

        Arc::new(Self {
            target: Arc::new(RwLock::new(None)),
            client: Arc::new(RwLock::new(None)),
            status: Arc::new(RwLock::new(ConnectionStatus::Disconnected)),
            event_tx,
            manual_disconnect: Arc::new(AtomicBool::new(false)),
            retry_count: Arc::new(AtomicU32::new(0)),
            is_reconnecting: Arc::new(AtomicBool::new(false)),
        })
    }

    /// 获取当前连接状态（设备级 status）
    pub async fn get_status(&self) -> ConnectionStatus {
        self.status.read().await.clone()
    }

    /// 获取目标设备
    pub async fn get_target(&self) -> Option<TargetDevice> {
        self.target.read().await.clone()
    }

    /// 保存目标设备（HTTP 认证与事件 WS 的地址真源）并复位手动断开标记
    ///
    /// 新目标视为新的连接意图：意外断开应走断连通知/自愈，而不是被旧的
    /// `manual_disconnect` 标记静默吞掉。connect / connect_without_emit /
    /// 监督任务建立事件 WS 前共用（04）。
    pub async fn set_target(&self, address: String, port: u16, name: Option<String>) {
        self.manual_disconnect.store(false, Ordering::SeqCst);
        *self.target.write().await = Some(TargetDevice { address, port, name });
        tracing::debug!("Target device saved");
    }

    /// 订阅事件
    pub fn subscribe(&self) -> broadcast::Receiver<MobileEvent> {
        self.event_tx.subscribe()
    }

    /// 事件发送器
    ///
    /// HTTP 认证成功契口经此广播 `MobileEvent::AuthSuccess`（04 事件 WS
    /// 订阅触发建连）；事件转发层（event.rs）也订阅同一通道。
    pub fn event_tx(&self) -> broadcast::Sender<MobileEvent> {
        self.event_tx.clone()
    }

    /// 连接到目标设备
    ///
    /// 03 过渡语义：认证走 HTTP，WS 尚不存在（04）——connect 只做
    /// 「保存目标 + 状态 Connecting→Connected + 发射事件」，真正的 WS 建连
    /// 由 04 的事件 WS 复用 `establish_ws_client`。
    pub async fn connect(&self, app_handle: AppHandle, address: String, port: u16, name: Option<String>) -> Result<()> {
        // 检查当前状态（自有 status）
        {
            let status = self.get_status().await;
            if status == ConnectionStatus::Connecting {
                let _ = app_handle.emit(
                    "ws_connecting",
                    serde_json::json!({
                        "address": address,
                        "port": port,
                        "status": "already_connecting"
                    }),
                );
                tracing::info!("Already connecting");
                return Ok(());
            }
            // 已认证（Authed）直接复用；Connected 表示目标已保存但未认证，
            // 需要重连（HTTP reauth）而非重复保存目标
            if status == ConnectionStatus::Authed {
                let _ = app_handle.emit("ws_connected", ());
                tracing::info!("Already connected and authenticated");
                return Ok(());
            }
        }

        // 重置手动断开标记（新连接）与保存目标设备——set_target 一并完成
        self.set_target(address.clone(), port, name.clone()).await;

        // 发射连接开始事件
        let _ = app_handle.emit(
            "ws_connecting",
            serde_json::json!({
                "address": address,
                "port": port,
            }),
        );
        tracing::info!("Connecting to {}:{} (HTTP auth)", address, port);

        // 清除上一次连接的客户端（如果有）
        // 03 过渡期 connect 不建 WS，此块恒为空；保留以防 04 复用此函数
        if let Some(old_client) = self.client.write().await.take() {
            tracing::debug!("Disconnecting previous client before creating new one");
            let _ = old_client.disconnect().await;
        }

        // 状态推进：Connecting → Connected（HTTP 认证成功后由
        // set_authed() 转 Authed；04 事件 WS 落地后此状态语义恢复完整）
        *self.status.write().await = ConnectionStatus::Connecting;
        *self.status.write().await = ConnectionStatus::Connected;

        // 短暂等待，保证目标写入落定（与旧建连路径的稳定等待对齐）
        tokio::time::sleep(tokio::time::Duration::from_millis(CONNECTION_STABILIZE_DELAY_MS)).await;

        // 发射连接成功事件
        let _ = app_handle.emit("ws_connected", ());
        tracing::info!("Connection established (HTTP auth target saved)");

        Ok(())
    }

    /// 连接（不带 AppHandle，用于测试）
    ///
    /// 保留 WS 建连路径：集成测试需要真实 WsClient→router→handler 链路
    /// （WS 首消息 JWT 认证、事件路由），与生产路径解耦。
    pub async fn connect_without_emit(&self, address: String, port: u16, name: Option<String>) -> Result<()> {
        // 检查当前状态（自有 status）
        let status = self.get_status().await;
        if status == ConnectionStatus::Connected
            || status == ConnectionStatus::Paired
            || status == ConnectionStatus::Authed
        {
            return Ok(());
        }

        // 保存目标设备（set_target 一并复位手动断开标记）
        self.set_target(address.clone(), port, name).await;

        *self.status.write().await = ConnectionStatus::Connecting;
        // 旧 WS 配对/终端路由已下线，测试辅助路径指向事件 WS（mock 不按 path 分发）
        self.establish_ws_client(&address, port, WS_EVENT_PATH, None).await?;
        *self.status.write().await = ConnectionStatus::Connected;

        // 短暂等待连接稳定
        tokio::time::sleep(tokio::time::Duration::from_millis(CONNECTION_STABILIZE_DELAY_MS)).await;

        Ok(())
    }

    /// 建立 WS 客户端（测试路径与 04 事件 WS 共用）
    ///
    /// 归拢 client 构建 + build_router + 断连监控三件事：04 复用同一 helper
    /// 建常驻事件 WS（`WS_EVENT_PATH`），WS 首消息 JWT 认证语义由桌面端
    /// 02 保证（裸连 10s 被关，故生产路径 04 前不建无认证 WS）。
    async fn establish_ws_client(
        &self,
        address: &str,
        port: u16,
        path: &str,
        app_handle: Option<AppHandle>,
    ) -> Result<Arc<WsClient>> {
        let config = WsClientConfig::new(address, port).with_path(path);
        let client = WsClient::new(config);

        // 构建路由器
        let router = build_router(self.event_tx.clone())?;

        client
            .set_handler(Arc::new(
                ClientDefaultMessageHandler::new().with_router(Arc::new(router)),
            ))
            .await;

        // 直接 await 连接
        client.connect().await?;

        if let Some(ah) = app_handle {
            self.spawn_connection_monitor(ah, &client);
        }

        // 保存客户端引用
        *self.client.write().await = Some(client.clone());

        Ok(client)
    }

    /// 建立常驻事件 WS（04 契口③）：事件路径建连 + WS 首消息 JWT 认证
    ///
    /// 返回的 client 已存入 `self.client`——认证成功后该连接天然成为
    /// SyncData 收信道，且会话/配置/文件请求的 send/send_and_wait 恢复可用
    /// （消除 03 过渡期「Not connected」回归）。`app_handle=Some` 时自动挂
    /// 断连监控（意外断开 → ws_unexpected_disconnect + 桌面 peer 清理）。
    /// 目标未保存（未 connect）时拒绝建连。
    pub async fn establish_event_ws(&self, app_handle: Option<AppHandle>) -> Result<Arc<WsClient>> {
        // 目标不存在说明尚未 connect，无建连地址
        let target = self
            .target
            .read()
            .await
            .clone()
            .ok_or_else(|| crate::AppError::WebSocket("No target device".to_string()))?;

        let client = self
            .establish_ws_client(&target.address, target.port, WS_EVENT_PATH, app_handle)
            .await?;

        // WS 首消息 JWT 认证：凭据对（pairing_id/fingerprint/session_token）
        // 由 HTTP 认证写入；缺失时以空串发送，让桌面端拒绝（可观测而非静默）
        let creds = crate::state::get_auth_manager()
            .get_credentials()
            .await
            .unwrap_or(crate::auth::AuthCredentials {
                pairing_id: String::new(),
                fingerprint: String::new(),
                session_token: String::new(),
            });
        client
            .send(&AuthRequest::reauthenticate(
                &creds.pairing_id,
                &creds.fingerprint,
                &creds.session_token,
            ))
            .await?;

        Ok(client)
    }

    /// 创建连接断开监控任务（WS client 建连后调用）
    ///
    /// 订阅 WsClientEvent，在意外断开时发射 ws_unexpected_disconnect 通知前端。
    /// 重连创建的是全新 WsClient，旧监控随旧客户端销毁——新连接再次断开时
    /// 无监控则前端收不到事件，实时输出静默停止。
    fn spawn_connection_monitor(&self, app_handle: AppHandle, client: &Arc<WsClient>) {
        let mut event_rx = client.subscribe();
        let app_clone = app_handle.clone();
        let manual_flag = self.manual_disconnect.clone();
        spawn_with_error_boundary("connection_monitor", async move {
            tracing::debug!("[ConnMonitor] Started monitoring connection");
            while let Ok(event) = event_rx.recv().await {
                match event {
                    WsClientEvent::Disconnected | WsClientEvent::Error { .. } | WsClientEvent::ServerClosed { .. } => {
                        if !manual_flag.load(Ordering::SeqCst) {
                            tracing::warn!("[ConnMonitor] Unexpected disconnect detected: {:?}", event);
                            let reason = match &event {
                                WsClientEvent::ServerClosed { reason } => reason.clone(),
                                WsClientEvent::Error { message } => message.clone(),
                                _ => "Connection lost".to_string(),
                            };
                            let _ = app_clone.emit(
                                "ws_unexpected_disconnect",
                                serde_json::json!({
                                    "reason": reason
                                }),
                            );

                            // 通知插件连接断开
                            {
                                let pm = crate::state::get_plugin_manager();
                                pm.dispatch_lifecycle_event(crate::plugin::types::PluginLifecycleEvent::Disconnect {
                                    reason: reason.clone(),
                                })
                                .await;
                            }
                        } else {
                            tracing::debug!("[ConnMonitor] Manual disconnect, skipping notification");
                        }

                        // 清理桌面端 peer 记录并推送 online=false（双通道）
                        if let Some(peer_id) = crate::handler::sync::desktop_peer_id().await {
                            let fs = crate::state::get_file_service();
                            fs.registry.remove_peer(&peer_id).await;
                        }

                        break;
                    }
                    _ => {}
                }
            }
            tracing::debug!("[ConnMonitor] Stopped");
        });
    }

    /// 断开连接
    pub async fn disconnect(&self) {
        // 设置手动断开标记，阻止监控任务弹出通知
        self.manual_disconnect.store(true, Ordering::SeqCst);
        // 重置重连标记，确保进行中的重连循环退出
        self.is_reconnecting.store(false, Ordering::SeqCst);
        tracing::info!("Disconnecting...");

        // 在清除 target 之前主动移除桌面 peer 记录并推送 online=false
        // （remove_peer 幂等：monitor 循环后续再调一次无害）
        if let Some(peer_id) = crate::handler::sync::desktop_peer_id().await {
            let fs = crate::state::get_file_service();
            fs.registry.remove_peer(&peer_id).await;
        }

        // 断开 WebSocket（04 前恒为空）
        if let Some(client) = self.client.read().await.as_ref() {
            let _ = client.disconnect().await;
        }

        // 清除客户端
        *self.client.write().await = None;

        // 清除目标
        *self.target.write().await = None;

        // 复位设备级状态
        *self.status.write().await = ConnectionStatus::Disconnected;
    }

    /// 尝试重连（HTTP reauth 退避循环）
    ///
    /// 认证已 HTTP 化：重连 = 用已持有 JWT（凭据优先，前端传入兜底）反复
    /// 调 `/api/auth/reauth`，成功即恢复 Authed。事件契约（reconnecting /
    /// reconnected / reconnect_failed）与退避节奏保持原样，前端零改动。
    /// `app_handle` 可为 `None`：04 监督任务自愈重连在无窗口上下文中可测
    /// （仅跳过前端事件发射，HTTP 语义不变），command 层传 `Some`。
    pub async fn reconnect(&self, app_handle: Option<AppHandle>, token: Option<String>) -> Result<()> {
        let max_retry = DEFAULT_MAX_RETRIES;
        let mut current_retry: u32 = 0;

        // 并发互斥：多个重连入口（前端命令 / supervisor 自愈）只允许一个真正
        // 持有重连循环，已在重连则跳过。flag 由本函数自持，**循环内不再复检**——
        // 否则第一轮失败后 current_retry += 1 续跑会命中自己置的 true 提前 return，
        // 重连永远到不了 max_retry、不发射 ws_reconnect_failed → 前端永久停在
        // 「正在连接」（2026-08-20 真机日志：Already reconnecting, skip）
        if self.is_reconnecting.load(Ordering::SeqCst) {
            tracing::info!("Already reconnecting, skip");
            return Ok(());
        }
        self.is_reconnecting.store(true, Ordering::SeqCst);
        self.retry_count.store(0, Ordering::SeqCst);

        while current_retry < max_retry {
            // 用户主动断开，停止重连循环（统一出口复位 flag）
            if self.manual_disconnect.load(Ordering::SeqCst) {
                tracing::info!("Manual disconnect detected, aborting reconnect");
                break;
            }

            self.retry_count.store(current_retry, Ordering::SeqCst);

            // 发射重连开始事件（无 AppHandle 时跳过：纯后台自愈路径）
            if let Some(ah) = &app_handle {
                let _ = ah.emit(
                    "ws_reconnecting",
                    serde_json::json!({
                        "retry": current_retry + 1,
                        "max_retry": max_retry
                    }),
                );
            }
            tracing::info!("Reconnecting attempt {}/{}", current_retry + 1, max_retry);

            // 等待指数退避间隔（首次不等待）
            if current_retry > 0 {
                let delay = DEFAULT_RETRY_DELAYS_MS[(current_retry - 1) as usize];
                tracing::info!("Waiting {}ms before retry...", delay);
                tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
                // 等待期间用户可能已断开（统一出口复位 flag）
                if self.manual_disconnect.load(Ordering::SeqCst) {
                    tracing::info!("Manual disconnect during reconnect delay, aborting");
                    break;
                }
            }

            // 目标设备：HTTP reauth 的地址真源
            let target = self.target.read().await.clone();
            if target.is_none() {
                tracing::error!("No target device for reconnect");
                break;
            }

            // 取已持有 JWT：Rust 凭据优先（D3），前端传入 token 兜底
            let auth_mgr = crate::state::get_auth_manager();
            let creds_token = auth_mgr
                .get_credentials()
                .await
                .map(|c| c.session_token)
                .unwrap_or_default();
            let session_token = if !creds_token.is_empty() {
                creds_token
            } else {
                token.clone().unwrap_or_default()
            };
            if session_token.is_empty() {
                tracing::warn!("No session token for HTTP reauth, aborting reconnect");
                break;
            }

            // HTTP reauth：成功（含 AuthSuccess 契口广播）即恢复；拒绝/网络
            // 故障都继续退避（业务码 1001 等属于永久性拒绝，退避到上限自然退出）
            match auth_mgr.authenticate_with_token(&session_token).await {
                Ok(true) => {
                    tracing::info!("Reconnect attempt {} succeeded (HTTP reauth)", current_retry + 1);
                    self.is_reconnecting.store(false, Ordering::SeqCst);
                    self.retry_count.store(0, Ordering::SeqCst);

                    if let Some(ah) = &app_handle {
                        let _ = ah.emit("ws_reconnected", ());
                    }
                    tracing::info!("Reconnect successful!");
                    return Ok(());
                }
                Ok(false) => {
                    tracing::warn!("Reconnect attempt {} rejected", current_retry + 1);
                    current_retry += 1;
                }
                Err(e) => {
                    tracing::warn!("Reconnect attempt {} failed: {}", current_retry + 1, e);
                    current_retry += 1;
                }
            }
        }

        // 重连结束统一出口：先复位互斥 flag；手动断开（aborted）不算失败、
        // 不发射失败事件；否则重试耗尽 → ws_reconnect_failed 通知前端终止「正在连接」
        self.is_reconnecting.store(false, Ordering::SeqCst);
        if self.manual_disconnect.load(Ordering::SeqCst) {
            tracing::info!("Reconnect loop aborted after {} attempts", current_retry);
            return Ok(());
        }
        if let Some(ah) = &app_handle {
            let _ = ah.emit(
                "ws_reconnect_failed",
                serde_json::json!({
                    "reason": "Max retries exceeded"
                }),
            );
        }
        tracing::error!("Reconnect failed after {} attempts", max_retry);

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
        tracing::debug!(
            "[ConnectionManager] send() message_type={:?}, preview={}",
            "Message",
            &msg_preview[..msg_preview.len().min(LOG_PREVIEW_MAX_LEN)]
        );

        if let Some(client) = self.client.read().await.as_ref() {
            let result = client.send(&message).await;
            match &result {
                Ok(_) => {
                    tracing::debug!("[ConnectionManager] send() result: OK");
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
            tracing::debug!(
                "[ConnectionManager] send_and_wait: client exists, status={:?}",
                client.get_status().await
            );
            let result = client
                .send_and_wait(&message, timeout)
                .await
                .with_context(|| format!("send_and_wait timeout={}s", timeout.as_secs()))
                .map_err(|e| crate::AppError::WebSocket(e.to_string()));
            tracing::debug!(
                "[ConnectionManager] send_and_wait: result={:?}",
                result.as_ref().map(|m| m.message_type().unwrap_or("unknown"))
            );
            result
        } else {
            tracing::error!("[ConnectionManager] send_and_wait: client is None!");
            Err(crate::AppError::WebSocket("Not connected".to_string()))
        }
    }

    /// 发送消息，失败时检查是否为断开错误并发射事件
    ///
    /// 此方法用于需要自动处理断开场景的调用方
    pub async fn send_with_disconnect_handling(&self, app_handle: &AppHandle, message: &Message) -> Result<()> {
        let result = self.send(message).await;

        if let Err(ref e) = result {
            if is_disconnect_error(e) {
                tracing::warn!(
                    "[ConnectionManager] send_with_disconnect_handling: detected disconnect error: {}",
                    e
                );
                let _ = app_handle.emit(
                    "ws_unexpected_disconnect",
                    serde_json::json!({
                        "reason": format!("连接已断开: {}", e)
                    }),
                );
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
                tracing::warn!(
                    "[ConnectionManager] send_and_wait_with_disconnect_handling: detected disconnect error: {}",
                    e
                );
                let _ = app_handle.emit(
                    "ws_unexpected_disconnect",
                    serde_json::json!({
                        "reason": format!("连接已断开: {}", e)
                    }),
                );
            }
        }

        result
    }

    /// 检查是否已连接（Connected / Authed / Paired）
    pub async fn is_connected(&self) -> bool {
        let status = self.get_status().await;
        matches!(
            status,
            ConnectionStatus::Connected | ConnectionStatus::Authed | ConnectionStatus::Paired
        )
    }

    /// 设置为已认证（设备级 HTTP 认证成功）
    ///
    /// 04 事件 WS 认证成功后同样维持 Authed——在线语义由桌面端
    /// WsSessionRegistry 判定（D8），本状态只表达设备级认证结论。
    pub async fn set_authed(&self) {
        *self.status.write().await = ConnectionStatus::Authed;
    }
}

impl Default for ConnectionManager {
    fn default() -> Self {
        let (event_tx, _) = broadcast::channel(BROADCAST_CHANNEL_CAPACITY);

        Self {
            target: Arc::new(RwLock::new(None)),
            client: Arc::new(RwLock::new(None)),
            status: Arc::new(RwLock::new(ConnectionStatus::Disconnected)),
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
            status: self.status.clone(),
            event_tx: self.event_tx.clone(),
            manual_disconnect: self.manual_disconnect.clone(),
            retry_count: self.retry_count.clone(),
            is_reconnecting: self.is_reconnecting.clone(),
        }
    }
}
