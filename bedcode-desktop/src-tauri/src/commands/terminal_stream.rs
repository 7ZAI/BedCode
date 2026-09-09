//! 桌面端本地终端输出流的 Channel 传输命令（与 WS 环回并行的替代方案）
//!
//! 背景：WS 环回（`useTerminalOutputStream` → `/ws/terminal/local`）在输出
//! 风暴期会因 WebKitGTK WS 接收缓冲溢出丢整消息（opencode 滚动残渣 + Parsing
//! error 的根因）。Tauri Channel 走 WebView 原生 IPC（大负载经 in-memory
//! fetch 拉取），数据在 Rust 侧缓冲直到前端拉取，天然无丢消息。
//!
//! 本模块保留 WS 环回代码不变，提供一条并行的 Channel 传输路径，由前端按
//! `VITE_TERMINAL_TRANSPORT`（"ws" | "channel"）选择，便于 A/B 验证。
//!
//! 协议复用 WS 同款 TB v2 帧 + 快照 seq 语义：`subscribe_terminal_channel`
//! 返回值携带快照元数据（min_seq / snapshot_seq / history_count / client_id），
//! 输出帧经 Channel 以 Raw 字节持续推送；背压 ack 走 `terminal_channel_ack`
//! 命令（同 WS 的 ack 帧语义，source = Desktop）。
//!
//! 订阅生命周期：每次订阅分配**唯一** client_id（`channel-{session}-{n}`），
//! 前端在 stop / 重订阅（gap 恢复）时先 `unsubscribe_terminal_channel` 精确
//! 取消旧订阅，再建立新订阅——避免固定 client_id 覆盖语义下「旧 consumer 退出
//! 时的 unsubscribe 误删同 key 新订阅」（曾导致重订阅后输出流断、终端无回显）。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tauri::ipc::{Channel, InvokeResponseBody};

use crate::server::ws::terminal_ws::forward;
use crate::session::{GlobalOutputManager, OutputFrame, RendererSource};
use crate::system::config::AppConfig;
use crate::Result;

/// 快照订阅元数据（Channel 传输的 subscribe 返回值；字段与 WS 控制帧
/// subscribe_response 对齐，camelCase 供前端直接读取）
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelSubscribeResponse {
    /// 队列最早存续事件序号（> 0 表示历史头部被环形淘汰）
    pub min_seq: u64,
    /// 订阅时刻队列最新序号（历史边界）
    pub snapshot_seq: u64,
    /// 历史事件数量
    pub history_count: usize,
    /// 本次订阅的唯一 client_id（前端 stop / 重订阅时据此精确取消）
    pub client_id: String,
}

/// 本地通道合并窗口（与 terminal_ws.rs 的 LOCAL_FLUSH_INTERVAL_MS 保持一致）
const LOCAL_FLUSH_INTERVAL_MS: u64 = 4;

/// Channel 订阅者 client_id 计数器：每次订阅唯一，前端据此精确 unsubscribe，
/// 不依赖覆盖语义（避免旧订阅清理误伤新订阅）
static CHANNEL_CLIENT_COUNTER: AtomicU64 = AtomicU64::new(0);

/// 订阅终端输出流（Channel 传输）：返回快照元数据 + client_id，输出帧经
/// Channel 持续推送。会话不存在时返回 NotFound。
///
/// consumer 退出时仅 abort forward 任务，**不 unsubscribe**——取消订阅由前端
/// 显式调用 `unsubscribe_terminal_channel` 控制（见模块文档的误删教训）。
#[tauri::command]
pub async fn subscribe_terminal_channel(
    session_id: String,
    channel: Channel<InvokeResponseBody>,
) -> Result<ChannelSubscribeResponse> {
    let manager = GlobalOutputManager::global();
    let client_id = format!(
        "channel-{}-{}",
        session_id,
        CHANNEL_CLIENT_COUNTER.fetch_add(1, Ordering::SeqCst)
    );

    // 每会话独立输出通道（容量与 WS forward 一致）
    let (output_tx, output_rx) = tokio::sync::mpsc::channel::<OutputFrame>(32768);

    // 先启动 forward + 消费任务，再订阅：历史入队时可被立即消费，避免
    // 32768 容量被大历史撑满导致 subscribe 内部 `.await send` 阻塞（无消费者）
    let config = AppConfig::global();
    let (out_tx, out_rx) = tokio::sync::mpsc::channel::<forward::ForwardOutput>(64);
    // 每次订阅独立 generation：仅本订阅使用该 forward_loop，无流代数替换，
    // my_gen=0 恒等于 generation，门控不生效（清理靠 abort）
    let generation = Arc::new(AtomicU64::new(0));
    let fwd_handle = tokio::spawn(forward::forward_loop(
        output_rx,
        out_tx,
        Duration::from_millis(LOCAL_FLUSH_INTERVAL_MS),
        config.terminal.max_buffer_size,
        generation,
        0,
    ));

    // 消费：ForwardOutput → Channel 推送。退出（out_rx 关闭）仅 abort forward，
    // 不 unsubscribe（由前端显式取消；见模块文档）
    tokio::spawn(async move {
        let mut out_rx = out_rx;
        while let Some(out) = out_rx.recv().await {
            match out {
                forward::ForwardOutput::Binary(data) => {
                    // Channel 关闭 → 推送失败 → 停止流。Raw 变体直接走字节负载
                    //（非 JSON 数组），前端收到 ArrayBuffer，避免逐字节 JSON 膨胀
                    if channel.send(InvokeResponseBody::Raw(data)).is_err() {
                        tracing::debug!("[terminal_stream] channel closed, stopping stream");
                        break;
                    }
                }
                forward::ForwardOutput::HistoryEnd { .. } => {
                    // 快照历史边界标记：Channel 路径经命令返回值已携带快照元数据，
                    // 前端按 seq 去重跳过历史段，标记本身无需透传
                }
            }
        }
        fwd_handle.abort();
    });

    // 订阅（历史经 output_tx → forward → channel 流式到达；前端在 invoke
    // resolve 前缓冲帧，收到返回值后再按快照元数据排空，语义与 WS 一致）。
    // 会话不存在时 subscribe 返回 None：output_tx 随函数返回 drop → output_rx
    // 关闭 → forward_loop 退出 → out_tx 关闭 → consumer 退出（abort no-op），
    // 整条任务链自然回收，无需显式 abort（fwd_handle 已 move 进 consumer）
    let response = manager
        .subscribe(&session_id, &client_id, output_tx, None)
        .await
        .ok_or_else(|| {
            crate::AppError::NotFound(format!(
                "subscribe_terminal_channel: 会话 {} 不存在",
                session_id
            ))
        })?;

    Ok(ChannelSubscribeResponse {
        min_seq: response.min_seq,
        snapshot_seq: response.snapshot_seq,
        history_count: response.history_count,
        client_id,
    })
}

/// 取消 Channel 订阅（前端 stop / 重订阅前精确清理旧订阅）。
///
/// 移除 subscriber → send_queue 关闭 → forward_loop 退出 → consumer 退出，
/// 整条链路自然回收，不误伤其他订阅。
#[tauri::command]
pub async fn unsubscribe_terminal_channel(session_id: String, client_id: String) -> Result<()> {
    GlobalOutputManager::global()
        .unsubscribe(&session_id, &client_id)
        .await;
    Ok(())
}

/// 终端输出 ack（Channel 路径的背压反馈环 Rust 侧入口）。
///
/// 语义同 WS ack 帧：推进会话未 ack 记账（释放 ≤ acked_seq 的输出字节），
/// 使 PTY 读取得以恢复。source 恒为 Desktop（本地 WebView 渲染端）。
#[tauri::command]
pub async fn terminal_channel_ack(session_id: String, acked_seq: u64) -> Result<()> {
    GlobalOutputManager::global()
        .ack(&session_id, acked_seq, RendererSource::Desktop)
        .await;
    Ok(())
}
