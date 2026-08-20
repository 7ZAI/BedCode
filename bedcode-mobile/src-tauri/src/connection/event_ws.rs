//! Event WS Supervisor（04：认证成功后自动建连 + 意外断开自动自愈）
//!
//! HTTP 认证成功后（`MobileEvent::AuthSuccess` 广播契口）由单例监督任务
//! 自动建立 `/ws/event` 常驻连接并首消息 JWT 认证；意外断开按既有
//! `DEFAULT_RETRY_DELAYS_MS` 退避经 HTTP reauth 自愈后重建，全程常驻、
//! 不主动断开、不干扰未来 P2 前端终端 WS。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tauri::AppHandle;
use tokio::sync::oneshot;

use crate::connection::manager::ConnectionManager;
use crate::connection::WsClientEvent;
use crate::router::MobileEvent;
use crate::system::error_boundary::spawn_with_error_boundary;

/// 监督任务单例守卫（复制 event.rs OUTPUT_FORWARDING_STARTED 模式：重复启动幂等）
static EVENT_WS_SUPERVISOR_STARTED: AtomicBool = AtomicBool::new(false);

/// 启动常驻事件 WS 监督任务（单例，仅首次调用生效）
///
/// `app_handle` 传 `Some` 时把断连监控挂上（意外断开 → 前端
/// ws_unexpected_disconnect + 桌面 peer 清理）；测试可传 `None`。
pub fn start_event_ws_supervisor(app_handle: Option<AppHandle>) {
    if EVENT_WS_SUPERVISOR_STARTED.swap(true, Ordering::SeqCst) {
        tracing::debug!("[EventWsSupervisor] Already started, skipping");
        return;
    }
    let manager = crate::state::get_connection_manager();
    spawn_with_error_boundary("event_ws_supervisor", async move {
        run_supervisor(manager, app_handle, None).await;
    });
}

/// 常驻事件 WS 监督状态机
///
/// 外层顺序监听 `MobileEvent::AuthSuccess`（HTTP 认证成功契口）：已连接时
/// 直接跳过（防 AuthHandler 对 Authenticated 回复的回吐造成双连）；目标
/// 缺失时跳过。内层监听当前事件 WS 的 `WsClientEvent`：断开类事件 → 清空
/// 当前连接，目标未变则 spawn HTTP reauth 自愈——成功经 apply_auth_success
/// 再广播 AuthSuccess → 外层收到后重建事件 WS。
///
/// `ready` 供测试同步：发送事件前保证监督任务已完成订阅（避免漏首个事件）。
pub async fn run_supervisor(
    manager: Arc<ConnectionManager>,
    app_handle: Option<AppHandle>,
    ready: Option<oneshot::Sender<()>>,
) {
    let mut auth_rx = manager.subscribe();
    if let Some(tx) = ready {
        let _ = tx.send(());
    }

    // 当前事件 WS 客户端（Some = 已连接；重建前守卫防双连）
    let mut current: Option<Arc<crate::connection::WsClient>> = None;
    // 建连时的目标副本：断开时目标变更则跳过自愈（换目标会自带新认证流，
    // 旧流自愈会向错误目标发 HTTP reauth）
    let mut establish_target: Option<crate::connection::manager::TargetDevice> = None;

    loop {
        match auth_rx.recv().await {
            Ok(MobileEvent::AuthSuccess { .. }) => {
                // 已连接：忽略回声（AuthHandler 收到 Authenticated 回复会再 emit
                // 一次 AuthSuccess，此时连接已在，防双连；连接期间事件只排队不
                // 处理，乃至断开后先读到回声也会立即重建——自愈更及时）
                if current.is_some() {
                    tracing::debug!("[EventWsSupervisor] AuthSuccess but already connected, skip");
                    continue;
                }
                // 无目标：未 connect 的异常态，等真实认证流
                let Some(target) = manager.get_target().await else {
                    tracing::debug!("[EventWsSupervisor] AuthSuccess without target, skip");
                    continue;
                };

                match manager.establish_event_ws(app_handle.clone()).await {
                    Ok(client) => {
                        tracing::info!(
                            "[EventWsSupervisor] Event WS established at {}:{} (JWT authenticated)",
                            target.address,
                            target.port
                        );
                        current = Some(client.clone());
                        establish_target = Some(target);
                    }
                    Err(e) => {
                        tracing::warn!("[EventWsSupervisor] Establish failed: {}", e);
                        current = None;
                        continue;
                    }
                }

                // 内层：监听当前连接生命周期直到断开
                let mut client_rx = current.as_ref().expect("just set above").subscribe();
                loop {
                    match client_rx.recv().await {
                        // 断开类事件：清空 + 目标未变则自愈（reconnect 成功经
                        // apply_auth_success 广播 AuthSuccess → 外层重建事件 WS）
                        Ok(WsClientEvent::Disconnected)
                        | Ok(WsClientEvent::Error { .. })
                        | Ok(WsClientEvent::ServerClosed { .. }) => {
                            tracing::warn!("[EventWsSupervisor] Event WS disconnected, self-healing");
                            current = None;
                            if establish_target.as_ref() == manager.get_target().await.as_ref() {
                                let m = manager.clone();
                                let ah = app_handle.clone();
                                // 自愈与 03 的 HTTP reauth 共用：无凭据时空 token
                                // 快速失败（不触 HTTP），等新认证流再重建
                                spawn_with_error_boundary("event_ws_reconnect", async move {
                                    if let Err(e) = m.reconnect(ah, None).await {
                                        tracing::warn!("[EventWsSupervisor] Reconnect failed: {}", e);
                                    }
                                });
                            } else {
                                tracing::info!("[EventWsSupervisor] Target changed since establish, skip self-heal");
                            }
                            break;
                        }
                        // 广播通道关闭（管理器销毁）：监督任务随之退出
                        Err(_) => {
                            tracing::warn!("[EventWsSupervisor] Client event channel closed, exiting");
                            current = None;
                            break;
                        }
                        // Connected / PushMessage / HeartbeatResponse：业务事件已由
                        // build_router 分发到 MobileEvent（event.rs forward 层照常
                        // emit），无需在监督任务重复处理
                        Ok(_) => {}
                    }
                }
            }
            // 非认证事件：不关心，事件已由 forwarder 转发
            Ok(_) => {}
            // 事件洪峰下漏事件：AuthSuccess 低频不应发生，但保持任务存活
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!("[EventWsSupervisor] auth broadcast lagged by {}", n);
            }
            // 事件通道关闭（管理器销毁）：监督任务退出
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                tracing::info!("[EventWsSupervisor] auth broadcast closed, exiting");
                break;
            }
        }
    }
}
