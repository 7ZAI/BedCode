//! 桌面端本地终端输出流的 Channel 传输命令（唯一路径；WS 环回已下线）
//!
//! 背景：WS 环回（`useTerminalOutputStream` → `/ws/terminal/local`）在输出
//! 风暴期会因 WebKitGTK WS 接收缓冲溢出丢整消息（opencode 滚动残渣 + Parsing
//! error 的根因）。Tauri Channel 走 WebView 原生 IPC（大负载经 in-memory
//! fetch 拉取），数据在 Rust 侧缓冲直到前端拉取，天然无丢消息。
//!
//! WS 环回链路（`local_terminal_ws` / `LocalTokenManager` / `get_local_ws_token`）
//! 已整体删除，桌面本地终端输出只经本命令面：`subscribe_terminal_channel` 订阅、
//! `terminal_channel_ack` 背压 ack、`unsubscribe_terminal_channel` 取消订阅。
//!
//! 拉取模型（spec §4.1/§4.3）：本通道与移动端 WS 通道同构——环 → 订阅者执行体
//! → 交接通道 → Channel 推送；窗口门控与 ack 水位是**该订阅者私有**的，
//! 本地通道慢不会冻结源产出，也不会影响移动端订阅者（反之亦然）。
//!
//! 协议复用 WS 同款 TB v3 帧 + 快照字节语义：`subscribe_terminal_channel`
//! 返回值携带快照元数据（min_offset / snapshot_offset / history_bytes / client_id），
//! 输出帧经 Channel 以 Raw 字节持续推送；控制帧（重同步/终止）以 JSON 体推送到
//! 同一 Channel（前端按 `ArrayBuffer` / JSON 体区分）；背压 ack 走
//! `terminal_channel_ack` 命令（推进本订阅者私有 ack 水位）。
//!
//! 订阅生命周期：每次订阅分配**唯一** client_id（`channel-{session}-{n}`），
//! 前端在 stop / 重订阅（gap 恢复）时先 `unsubscribe_terminal_channel` 精确
//! 取消旧订阅，再建立新订阅——避免固定 client_id 覆盖语义下「旧 consumer 退出
//! 时的 unsubscribe 误删同 key 新订阅」（曾导致重订阅后输出流断、终端无回显）。

use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tauri::ipc::{Channel, InvokeResponseBody};

use crate::server::ws::terminal_ws::{forward, subscriber};
use crate::session::GlobalOutputManager;
use crate::system::spawn_with_error_boundary;
use crate::Result;

/// 快照订阅元数据（Channel 传输的 subscribe 返回值；字段与 WS 控制帧
/// subscribe_ok 对齐，camelCase 供前端直接读取，TB v3 字节三件套）
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelSubscribeResponse {
    /// 队列最早存续字节位置（环形淘汰后推进；> 游标表示历史被截断）
    pub min_offset: u64,
    /// 订阅时刻累计字节数（历史边界）
    pub snapshot_offset: u64,
    /// 驻留历史总字节数
    pub history_bytes: u64,
    /// 本次订阅的唯一 client_id（前端 stop / 重订阅时据此精确取消）
    pub client_id: String,
}

/// 本地通道合并窗口（与既有行为一致：4ms 有界延迟合并，非零缓冲直通）
const LOCAL_FLUSH_INTERVAL_MS: u64 = 4;

/// Channel 订阅者 client_id 计数器：每次订阅唯一，前端据此精确 unsubscribe，
/// 不依赖覆盖语义（避免旧订阅清理误伤新订阅）
static CHANNEL_CLIENT_COUNTER: AtomicU64 = AtomicU64::new(0);

/// 生成唯一 client_id：`channel-{session_id}-{n}`，n 单调递增
///
/// 提取为纯函数便于单测：分配逻辑被改坏（如 `fetch_add(0)` 导致所有订阅
/// 同 ID）时测试失败（历史 bug：曾导致重订阅后输出流断、终端无回显，票据 19）
pub fn next_client_id(session_id: &str) -> String {
    format!(
        "channel-{}-{}",
        session_id,
        CHANNEL_CLIENT_COUNTER.fetch_add(1, Ordering::SeqCst)
    )
}

/// 订阅终端输出流（Channel 传输）：返回快照元数据 + client_id，输出帧经
/// Channel 持续推送。会话不存在时返回 NotFound。
///
/// 消费任务退出时仅 abort 订阅者执行体，**不 unsubscribe**——取消订阅由前端
/// 显式调用 `unsubscribe_terminal_channel` 控制（见模块文档的误删教训）。
#[tauri::command]
pub async fn subscribe_terminal_channel(
    session_id: String,
    channel: Channel<InvokeResponseBody>,
) -> Result<ChannelSubscribeResponse> {
    let global = GlobalOutputManager::global();
    let client_id = next_client_id(&session_id);

    let manager = global
        .session(&session_id)
        .await
        .ok_or_else(|| crate::AppError::NotFound(format!("subscribe_terminal_channel: 会话 {} 不存在", session_id)))?;

    // 订阅者执行体（环拉取 + 私有 ack 水位）；本地通道恒 realtime（无双速语义）
    let spawned = subscriber::spawn_subscriber(
        &manager,
        &client_id,
        None,
        Arc::new(AtomicU8::new(forward::MODE_REALTIME)),
        subscriber::SubscriberCfg::for_local_route(Duration::from_millis(LOCAL_FLUSH_INTERVAL_MS)),
    )
    .await;

    let response = spawned.response;
    let mut out_rx = spawned.out_rx;
    let subscriber_task = spawned.task;
    let client_id_for_log = client_id.clone();

    // 桥接：交接通道 → Tauri Channel 推送（Raw = 输出帧；Json = 控制帧）
    spawn_with_error_boundary("terminal_channel_bridge", async move {
        while let Some(out) = out_rx.recv().await {
            match out {
                forward::ForwardOutput::Binary(data) => {
                    // Channel 关闭 → 推送失败 → 停止流。Raw 变体直接走字节负载
                    //（非 JSON 数组），前端收到 ArrayBuffer，避免逐字节 JSON 膨胀
                    if channel.send(InvokeResponseBody::Raw(data)).is_err() {
                        tracing::debug!(client_id = %client_id_for_log, "[terminal_stream] channel closed, stopping stream");
                        break;
                    }
                }
                // 重同步（spec §4.7）：本地通道显式下发，前端清屏 + 重锚 + 提示
                forward::ForwardOutput::Resync {
                    min_offset,
                    snapshot_offset,
                } => {
                    tracing::warn!(
                        client_id = %client_id_for_log,
                        min_offset,
                        snapshot_offset,
                        "[terminal_stream] subscriber truncated, sending resync"
                    );
                    let body = InvokeResponseBody::Json(
                        serde_json::json!({
                            "type": "resync",
                            "min_offset": min_offset,
                            "snapshot_offset": snapshot_offset,
                        })
                        .to_string(),
                    );
                    if channel.send(body).is_err() {
                        break;
                    }
                }
                // 僵尸订阅者回收：尽力下发 error 后停止本通道（不影响其他订阅者）
                forward::ForwardOutput::Terminate { code, message } => {
                    tracing::warn!(
                        client_id = %client_id_for_log,
                        code = %code,
                        message = %message,
                        "[terminal_stream] subscriber terminated"
                    );
                    let body = InvokeResponseBody::Json(
                        serde_json::json!({
                            "type": "error",
                            "code": code,
                            "message": message,
                        })
                        .to_string(),
                    );
                    let _ = channel.send(body);
                    break;
                }
                // 历史边界：快照元数据已经命令返回值携带，前端按字节区间自愈
                forward::ForwardOutput::HistoryEnd { .. } => {}
            }
        }
        subscriber_task.abort();
    });

    Ok(ChannelSubscribeResponse {
        min_offset: response.min_offset,
        snapshot_offset: response.snapshot_offset,
        history_bytes: response.history_bytes,
        client_id,
    })
}

/// 取消 Channel 订阅（前端 stop / 重订阅前精确清理旧订阅）。
///
/// 移除订阅者句柄 → 执行体在下一次唤醒退出；前端据此精确取消，不误伤其他订阅。
#[tauri::command]
pub async fn unsubscribe_terminal_channel(session_id: String, client_id: String) -> Result<()> {
    GlobalOutputManager::global().unsubscribe(&session_id, &client_id).await;
    Ok(())
}

/// 终端输出 ack（Channel 路径的背压反馈环 Rust 侧入口）。
///
/// 语义同 WS ack 帧：推进**该订阅者私有**的 ack 水位（I6，只解除本订阅者的窗口
/// 驻留；不参与任何共享记账）。`client_id` 由 subscribe 返回，前端原样回传。
#[tauri::command]
pub async fn terminal_channel_ack(session_id: String, client_id: String, acked_offset: u64) -> Result<()> {
    GlobalOutputManager::global()
        .ack_subscriber(&session_id, &client_id, acked_offset)
        .await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// client_id 分配（票据 19 回归锁）：格式 + 单调递增唯一性
    ///
    /// 历史 bug：计数器被改坏（如 `fetch_add(0)`）→ 所有订阅同 ID → 旧订阅
    /// 的 unsubscribe 误删新订阅 → 重订阅后输出流断、终端无回显。
    #[test]
    fn next_client_id_increments_and_formats() {
        let a = next_client_id("sess-1");
        let b = next_client_id("sess-1");
        assert_eq!(a, "channel-sess-1-0", "首个 ID 应为 channel-{{session}}-0，实际: {a}");
        assert_eq!(b, "channel-sess-1-1", "第二个 ID 应递增，实际: {b}");
        assert_ne!(a, b, "连续分配必须唯一");
        // 不同会话不混淆
        let c = next_client_id("sess-2");
        assert!(c.starts_with("channel-sess-2-"), "实际: {c}");
        assert_ne!(c, b);
    }

    /// 会话不存在：subscribe 返回 None（命令层包装为 NotFound）
    #[test]
    fn session_lookup_of_unknown_session_returns_none() {
        let manager = GlobalOutputManager::new();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            assert!(
                manager.session("nonexistent").await.is_none(),
                "不存在会话查不到管理器（命令层据此返回 NotFound）"
            );
        });
    }

    /// ack 路由：订阅者不存在时静默忽略（不 panic、不误伤其他订阅者）
    #[test]
    fn ack_unknown_subscriber_is_silent_ok() {
        let manager = GlobalOutputManager::new();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            assert!(
                !manager.ack_subscriber("nonexistent", "channel-none-0", 42).await,
                "无订阅者时 ack 不生效且不报错"
            );
        });
    }

    /// 取消订阅后不再收到输出：unsubscribe 移除句柄（推送与拉取两条路径都清）
    #[test]
    fn unsubscribe_removes_subscriber_handle() {
        let manager = GlobalOutputManager::new();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            manager.register_session("sess-1").await;
            let session = manager.session("sess-1").await.expect("会话管理器");
            let (_tx, _rx) = tokio::sync::mpsc::channel::<forward::ForwardOutput>(4);
            // 直接登记拉取订阅者，验证 unsubscribe 清理生效
            session
                .register_subscriber(
                    "channel-sess-1-0",
                    None,
                    Arc::new(AtomicU8::new(forward::MODE_REALTIME)),
                )
                .await;
            assert_eq!(session.pull_subscriber_count().await, 1);
            assert!(manager.unsubscribe("sess-1", "channel-sess-1-0").await);
            assert_eq!(
                session.pull_subscriber_count().await,
                0,
                "unsubscribe 必须清掉拉取订阅者"
            );
        });
    }
}
