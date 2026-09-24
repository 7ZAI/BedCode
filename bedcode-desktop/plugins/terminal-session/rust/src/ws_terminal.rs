//! WS 终端流端点（websocket 业务下沉票 04）
//!
//! 客户端经 `/ws/plugin/com.bedcode.terminal-session/terminal` 直连（manifest
//! `contributes.wsEndpoints` 声明，`auth: jwt` 由宿主校验）。**终端协议完全归
//! 本插件**：输入直写 PTY、输出经 `ring-fetch` 按**每连接独立游标**拉取并以
//! 二进制帧下发、ACK/截断重同步/停止帧全部插件定义；宿主只转原始帧
//! （text/binary），不解析终端帧、不读 session id、不维护终端订阅表（spec §3.3）。
//!
//! ## 帧协议（插件定义，宿主不透明）
//!
//! 客户端 → 插件（text JSON）：
//! ```json
//! {"type":"subscribe","sessionId":"...","mode":"live"|"poll"}   // mode 缺省 live
//! {"type":"unsubscribe"}
//! {"type":"ack","offset":N}        // 流控信号：客户端确认收到 N 之前全部输出
//! {"type":"resync","offset":N}     // 客户端已清屏，从 N 继续（环淘汰后重锚）
//! {"type":"input","data":"..."}    // UTF-8 文本输入（无控制字符；控制字符走 binary）
//! {"type":"poll"}                  // 主动拉取（客户端驱动 drain 的触发）
//! ```
//! 客户端 → 插件（binary）：原始输入字节（可含控制字符，Ctrl-C 等）
//! 插件 → 客户端（binary）：输出字节（ring-fetch 原始数据，不 JSON 化）
//! 插件 → 客户端（text JSON）：
//! ```json
//! {"type":"subscribed","sessionId":"...","mode":"..."}
//! {"type":"unsubscribed"}
//! {"type":"ring_resync","offset":N}              // 环已淘汰：N 之前数据不可恢复
//! {"type":"session_stopped","sessionId":"...","reason":"stopped|killed|error","exitCode":N?}
//! {"type":"error","message":"..."}
//! ```
//!
//! ## 输出泵（有界 drain，不在宿主回调内形成长链）
//!
//! 任何入站帧（subscribe/ack/resync/input/binary/poll）之后对订阅连接做一次
//! **有界 drain**：逐次 `ring-fetch`（单次上限 16 KiB）直到追平或周期预算用尽
//! （8 次 ≈ 128 KiB）；输出以二进制帧即时下发。慢客户端发送失败只停本人
//! （debug 留痕，fail-visible 计数），不阻塞其他连接与 PTY 产出（pull 模型：
//! 宿主环绝不回传背压，spec §7.4）。兜底 drain 挂既有 1s 调度 tick（无客户端
//! 帧时仍推送新输出，输出延迟 ≤ tick 档位——与桌面前端 `output.pull` 拉取模型
//! 同构，客户端可按需加密 poll 档位）。
//!
//! ## 尾帧与停止帧
//!
//! `pty:exit` 事件在宿主环摘除**之后**发布（host-pty 单一发布者不变量），
//! 故退出时点的尾帧 = 退出前最后一次成功 drain 已下发的字节；exit 处理仍做
//! 一次尽力 `ring-fetch`（窗口竞态下已摘除 → debug 跳过），随后向该会话的
//! 全部订阅连接下发 `session_stopped` 停止帧（尾帧在前、停止帧在后）。
//!
//! ## 连接生命周期
//!
//! 订阅态只存内存（进程级静态表，wasm 同实例串行）；`client-disconnect` /
//! 插件停用 / `unsubscribe` 都摘除对应状态——无宿主订阅表、无后台任务
//! （全部 drain 同步执行于帧回调与 tick，不 spawn）。`pty:exit` 后连接保留
//! （客户端可重订阅重启后的同 id 会话），但订阅置空、游标归零。

#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::host::ws::{ws_event_topic, WS_CLIENT_DISCONNECT};
#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::host::{HostEvents, HostLog, HostPty, HostWebsocket};
#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::wasm_host::WasmHost;
#[cfg(target_arch = "wasm32")]
use std::sync::Mutex;

/// 端点路径（manifest `contributes.wsEndpoints` 声明，宿主注入命名空间段）
pub const ENDPOINT_PATH: &str = "terminal";

/// 单次 ring-fetch 上限（对齐宿主 `PLUGIN_PTY_RING_FETCH_MAX_BYTES` = 16 KiB；
/// 宿主侧仍会钳位）
const MAX_FETCH_BYTES: u32 = 16 * 1024;
/// 单轮 drain 的最大 fetch 次数（≈128 KiB 预算：每帧回调不无限拉取，
/// 慢消费的积压靠下一轮 drain 续拉）
const MAX_FETCHES_PER_CYCLE: u32 = 8;

/// 订阅模式：live = 每帧触发 drain + tick 兜底；poll = 仅在客户端 poll/ack 时拉
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TerminalMode {
    Live,
    Poll,
}

/// 单条连接的终端订阅态（进程级；client_id 为注册表键 = 对端地址串）
#[derive(Debug, Clone)]
struct TerminalConnection {
    client_id: String,
    endpoint_id: String,
    /// 订阅的会话（None = 未订阅）
    session_id: Option<String>,
    /// 会话登记域解析出的 PTY 句柄（订阅时解析，会话重启后重订阅再解析）
    pty_id: Option<String>,
    /// 每连接独立输出游标（ring-fetch 的 from_offset 基准）
    cursor: u64,
    mode: TerminalMode,
}

#[cfg(target_arch = "wasm32")]
static CONNECTIONS: Mutex<Vec<TerminalConnection>> = Mutex::new(Vec::new());

// ==================== 连接状态 ====================

/// 以锁内可变引用执行操作（不存在 → `None`；锁损坏显性上抛）。
///
/// wasm 单线程（同实例串行）：锁只作进程级静态表的互斥纪律，回调内不再嵌套
/// 取锁（发送等宿主调用不重入本表），闭包执行期间不会产生重入死锁。
#[cfg(target_arch = "wasm32")]
fn with_conn<R>(client_id: &str, f: impl FnOnce(&mut TerminalConnection) -> R) -> Option<R> {
    let mut table = CONNECTIONS.lock().unwrap_or_else(|e| e.into_inner());
    let conn = table.iter_mut().find(|c| c.client_id == client_id)?;
    Some(f(conn))
}

/// 连接接入时登记空状态（client-connect 事件驱动；幂等）
#[cfg(target_arch = "wasm32")]
pub fn on_client_connect(client_id: &str, endpoint_id: &str) {
    let mut table = CONNECTIONS.lock().unwrap_or_else(|e| e.into_inner());
    if !table.iter().any(|c| c.client_id == client_id) {
        table.push(TerminalConnection {
            client_id: client_id.to_string(),
            endpoint_id: endpoint_id.to_string(),
            session_id: None,
            pty_id: None,
            cursor: 0,
            mode: TerminalMode::Live,
        });
    }
}

/// 连接断开：摘除该连接的订阅态（client-disconnect 事件驱动；幂等）
#[cfg(target_arch = "wasm32")]
pub fn on_client_disconnect(client_id: &str) {
    let mut table = CONNECTIONS.lock().unwrap_or_else(|e| e.into_inner());
    let before = table.len();
    table.retain(|c| c.client_id != client_id);
    if table.len() != before {
        tracing_dbg_disconnect(client_id);
    }
}

/// 插件停用：清空全部订阅态（不残留连接级任务/状态）
#[cfg(target_arch = "wasm32")]
pub fn purge_all() {
    let mut table = CONNECTIONS.lock().unwrap_or_else(|e| e.into_inner());
    if !table.is_empty() {
        WasmHost.log_info(&format!("ws terminal: purged {} connection(s)", table.len()));
        table.clear();
    }
}

/// 调试留痕（断开清理）
#[cfg(target_arch = "wasm32")]
fn tracing_dbg_disconnect(client_id: &str) {
    WasmHost.log_debug(&format!("ws terminal: connection state dropped (client_id={client_id})"));
}

// ==================== 入站帧处理（events-ws 回调） ====================

/// 帧 JSON 的词表（小写 `type` 字段；未知/畸形 → 显性 error 帧）
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct TerminalFrame {
    #[serde(rename = "type")]
    frame_type: String,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    offset: Option<u64>,
    #[serde(default)]
    data: Option<String>,
}

/// 文本帧分派（订阅/退订/ack/resync/input/poll）——纯状态机，可 native 单测
///
/// 返回给调用方一个「是否需要 drain」标记（任何推进输出的帧之后都要 drain）。
#[cfg(target_arch = "wasm32")]
fn handle_text_frame(conn: &mut TerminalConnection, frame: &serde_json::Value) -> Result<bool, String> {
    let parsed: TerminalFrame = serde_json::from_value(frame.clone())
        .map_err(|e| format!("invalid terminal frame: {e}"))?;
    match parsed.frame_type.as_str() {
        "subscribe" => {
            let session_id = parsed
                .session_id
                .filter(|s| !s.is_empty())
                .ok_or_else(|| "subscribe: missing sessionId".to_string())?;
            let record = crate::session::record_via_host(&session_id)?
                .ok_or_else(|| format!("会话不存在：{session_id}"))?;
            let pty_id = record
                .pty_id
                .ok_or_else(|| format!("会话缺少 PTY 句柄：{session_id}"))?;
            conn.session_id = Some(session_id.clone());
            conn.pty_id = Some(pty_id);
            // 新订阅从头拉（历史 = 环驻留窗口内的字节；环已淘汰 → 首次 fetch 即
            // truncated → ring_resync 重锚）
            conn.cursor = 0;
            conn.mode = match parsed.mode.as_deref() {
                Some("poll") => TerminalMode::Poll,
                _ => TerminalMode::Live,
            };
            let reply = serde_json::json!({
                "type": "subscribed",
                "sessionId": session_id,
                "mode": if conn.mode == TerminalMode::Live { "live" } else { "poll" },
            });
            send_text(&conn.endpoint_id, &conn.client_id, &reply.to_string())?;
            Ok(true)
        }
        "unsubscribe" => {
            conn.session_id = None;
            conn.pty_id = None;
            conn.cursor = 0;
            send_text(
                &conn.endpoint_id,
                &conn.client_id,
                &serde_json::json!({ "type": "unsubscribed" }).to_string(),
            )?;
            Ok(false)
        }
        // 流控信号：客户端确认进度；插件以 poll 语义驱动 drain（不改游标——
        // 游标由插件按 ring-fetch 推进，ack 只作客户端侧流量告知）
        "ack" => {
            let _offset = parsed.offset.ok_or_else(|| "ack: missing offset".to_string())?;
            Ok(true)
        }
        // 客户端已清屏重锚：游标置为客户端给的新基准（ring_resync 后的续拉点）
        "resync" => {
            let offset = parsed.offset.ok_or_else(|| "resync: missing offset".to_string())?;
            conn.cursor = offset;
            Ok(true)
        }
        "input" => {
            let data = parsed
                .data
                .ok_or_else(|| "input: missing data".to_string())?;
            let pty_id = conn.pty_id.clone().ok_or_else(|| "input: 未订阅会话".to_string())?;
            WasmHost
                .pty_write(&pty_id, data.as_bytes())
                .map_err(|e| format!("host pty write failed: {}", e.message))?;
            // 输入后立即 drain（回声与随输入产生的输出即时返回）
            Ok(true)
        }
        "poll" => Ok(true),
        other => Err(format!("unknown terminal frame type: {other}")),
    }
}

/// 二进制帧 = 原始输入字节（可含控制字符）→ 直写 PTY（特殊键由插件 keys.rs
/// 翻译，但客户端也可直接发送原始控制字节；这里不做二次解释）
#[cfg(target_arch = "wasm32")]
fn handle_binary_frame(conn: &mut TerminalConnection, payload: &[u8]) -> Result<bool, String> {
    let pty_id = conn.pty_id.clone().ok_or_else(|| "binary input: 未订阅会话".to_string())?;
    WasmHost
        .pty_write(&pty_id, payload)
        .map_err(|e| format!("host pty write failed: {}", e.message))?;
    Ok(true)
}

/// events-ws 服务端域回调（声明端点 `terminal` 的入站帧）——宿主只转原始帧
#[cfg(target_arch = "wasm32")]
pub fn on_client_message(endpoint_id: &str, client_id: &str, kind: &str, payload: &[u8]) -> anyhow::Result<()> {
    // 未登记连接（如 auth 事件先于 client-connect 投递的竞态）→ 惰性登记
    if with_conn(client_id, |_| ()).is_none() {
        on_client_connect(client_id, endpoint_id);
    }
    let need_drain = with_conn(client_id, |conn| {
        let result = match kind {
            "text" => {
                let text = String::from_utf8_lossy(payload);
                match serde_json::from_str::<serde_json::Value>(&text) {
                    Ok(frame) => handle_text_frame(conn, &frame),
                    Err(e) => Err(format!("terminal frame 非 JSON: {e}")),
                }
            }
            "binary" => handle_binary_frame(conn, payload),
            other => Err(format!("unknown frame kind: {other}")),
        };
        match result {
            Ok(need) => need,
            Err(e) => {
                let _ = send_text(
                    endpoint_id,
                    client_id,
                    &serde_json::json!({ "type": "error", "message": e }).to_string(),
                );
                false
            }
        }
    })
    .unwrap_or(false);
    if need_drain {
        drain_for(endpoint_id, client_id).map_err(anyhow::Error::msg)?;
    }
    Ok(())
}

// ==================== 输出泵（有界 drain） ====================

/// 对指定连接做一轮有界 drain：逐次 ring-fetch 直到追平 / 预算用尽 /
/// 环淘汰（truncated → ring_resync 控制帧 + 游标重锚到环头）
#[cfg(target_arch = "wasm32")]
fn drain_for(endpoint_id: &str, client_id: &str) -> Result<(), String> {
    let (session_id, pty_id, mut cursor) = match with_conn(client_id, |c| {
        (c.session_id.clone(), c.pty_id.clone(), c.cursor)
    }) {
        Some((Some(session_id), Some(pty_id), cursor)) => (session_id, pty_id, cursor),
        _ => return Ok(()), // 未订阅 / 无句柄：无事可拉
    };
    let mut fetches = 0;
    loop {
        if fetches >= MAX_FETCHES_PER_CYCLE {
            // 本周期预算用尽：下一帧/tick 续拉（不无限占用宿主调用链）
            break;
        }
        fetches += 1;
        match WasmHost.pty_ring_fetch(&pty_id, cursor, MAX_FETCH_BYTES) {
            Ok(Some(fetched)) => {
                if fetched.truncated {
                    // 环已淘汰游标之前的字节：显式重锚协议（客户端清屏续拉）
                    cursor = fetched.next_offset;
                    let _ = send_text(
                        endpoint_id,
                        client_id,
                        &serde_json::json!({ "type": "ring_resync", "offset": cursor }).to_string(),
                    );
                    break;
                }
                if !fetched.data.is_empty() {
                    if let Err(e) = WasmHost.ws_send_binary_to_client(endpoint_id, client_id, &fetched.data) {
                        // 慢客户端/队列满：只停本人（fail-visible debug 留痕），
                        // 不阻塞 PTY 产出与其他连接；游标不动，下轮续拉
                        WasmHost.log_debug(&format!(
                            "ws terminal: send to slow client failed (client_id={client_id}): {}",
                            e.message
                        ));
                        with_conn(client_id, |c| c.cursor = cursor);
                        return Ok(());
                    }
                }
                cursor = fetched.next_offset;
                // 无新数据（fetch 返回空块但未追平）→ 停止本轮，防空转
                if fetched.data.is_empty() {
                    break;
                }
            }
            Ok(None) => break, // 追平（游标 == 产出端）
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("not found") {
                    // 会话/PTY 已摘除（exit 竞态窗口）：drain 到此为止
                    return Ok(());
                }
                return Err(format!("pty ring fetch failed: {msg}"));
            }
        }
    }
    with_conn(client_id, |c| c.cursor = cursor);
    let _ = session_id;
    Ok(())
}

/// 调度 tick 兜底 drain：所有 live 模式订阅连接拉一轮新输出
/// （挂既有 1s scheduler tick，输出延迟 ≤ tick 档位）
#[cfg(target_arch = "wasm32")]
pub fn drain_all_on_tick() {
    let snapshot: Vec<(String, String)> = {
        let table = CONNECTIONS.lock().unwrap_or_else(|e| e.into_inner());
        table
            .iter()
            .filter(|c| c.session_id.is_some() && c.mode == TerminalMode::Live)
            .map(|c| (c.endpoint_id.clone(), c.client_id.clone()))
            .collect()
    };
    for (endpoint_id, client_id) in snapshot {
        if let Err(e) = drain_for(&endpoint_id, &client_id) {
            WasmHost.log_warn(&format!(
                "ws terminal tick drain failed (client_id={client_id}): {e}"
            ));
        }
    }
}

// ==================== PTY 退出 → 尾帧 + 停止帧 ====================

/// 会话终止（`<owner>::pty:exit` 驱动，lib.rs 在 session::on_pty_exit 后调用）：
/// 尽力尾帧 fetch（环可能已摘除）→ 向该会话全部订阅连接下发 `session_stopped`
/// 停止帧（tail 在前、停止帧在后）；订阅置空、游标归零（客户端可重订阅）
#[cfg(target_arch = "wasm32")]
pub fn on_session_terminated(session_id: &str, reason: &str, exit_code: Option<i32>) {
    let targets: Vec<(String, String, String)> = {
        let mut table = CONNECTIONS.lock().unwrap_or_else(|e| e.into_inner());
        table
            .iter_mut()
            .filter(|c| c.session_id.as_deref() == Some(session_id))
            .map(|c| {
                // 尽力尾帧：环仍在（罕见窗口）则拉取剩余字节；已摘除 → debug 跳过
                if let Some(pty_id) = c.pty_id.clone() {
                    match WasmHost.pty_ring_fetch(&pty_id, c.cursor, MAX_FETCH_BYTES) {
                        Ok(Some(fetched)) if !fetched.data.is_empty() => {
                            let _ = WasmHost.ws_send_binary_to_client(&c.endpoint_id, &c.client_id, &fetched.data);
                            c.cursor = fetched.next_offset;
                        }
                        _ => {}
                    }
                }
                let target = (c.endpoint_id.clone(), c.client_id.clone(), session_id.to_string());
                c.session_id = None;
                c.pty_id = None;
                c.cursor = 0;
                target
            })
            .collect()
    };
    for (endpoint_id, client_id, sid) in targets {
        let mut payload = serde_json::json!({
            "type": "session_stopped",
            "sessionId": sid,
            "reason": reason,
        });
        if let Some(code) = exit_code {
            payload["exitCode"] = serde_json::json!(code);
        }
        let _ = send_text(&endpoint_id, &client_id, &payload.to_string());
        WasmHost.log_info(&format!(
            "ws terminal: session stopped frame sent (session_id={sid}, client_id={client_id}, reason={reason})"
        ));
    }
}

// ==================== 发送工具 ====================

#[cfg(target_arch = "wasm32")]
fn send_text(endpoint_id: &str, client_id: &str, text: &str) -> Result<(), String> {
    WasmHost
        .ws_send_text_to_client(endpoint_id, client_id, text)
        .map_err(|e| format!("ws terminal: send-text-to-client failed: {}", e.message))
}

// ==================== native：wasm 专属路径为空实现 ====================

#[cfg(not(target_arch = "wasm32"))]
pub fn on_client_message(_endpoint_id: &str, _client_id: &str, _kind: &str, _payload: &[u8]) -> anyhow::Result<()> {
    anyhow::bail!("ws terminal unavailable outside wasm runtime")
}

#[cfg(not(target_arch = "wasm32"))]
pub fn on_client_connect(_client_id: &str, _endpoint_id: &str) {}

#[cfg(not(target_arch = "wasm32"))]
pub fn on_client_disconnect(_client_id: &str) {}

#[cfg(not(target_arch = "wasm32"))]
pub fn purge_all() {}

#[cfg(not(target_arch = "wasm32"))]
pub fn drain_all_on_tick() {}

#[cfg(not(target_arch = "wasm32"))]
pub fn on_session_terminated(_session_id: &str, _reason: &str, _exit_code: Option<i32>) {}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(r#type: &str) -> serde_json::Value {
        serde_json::json!({ "type": r#type })
    }

    /// 文本帧解析：subscribe 缺 sessionId / 未知类型 / 非法 JSON → 显性错误
    #[test]
    fn text_frame_parsing_rejects_bad_shapes() {
        // handle_text_frame 是 wasm 专用（走 WasmHost）；native 侧解析器
        // TerminalFrame 与词表分支由 serde 形状锁覆盖：
        let parsed: Result<TerminalFrame, _> = serde_json::from_value(frame("subscribe"));
        assert!(parsed.is_ok(), "subscribe 帧形状可解析");
        let parsed: Result<TerminalFrame, _> = serde_json::from_value(serde_json::json!({"type": "ack"}));
        assert!(parsed.is_ok());
        let parsed: Result<TerminalFrame, _> = serde_json::from_value(serde_json::json!({}));
        assert!(parsed.is_err(), "缺 type 字段拒绝");
    }

    /// 模式缺省 live / 显式 poll（帧解析形状锁）
    #[test]
    fn mode_defaults_to_live() {
        let parsed: TerminalFrame = serde_json::from_value(serde_json::json!({
            "type": "subscribe", "sessionId": "s1"
        }))
        .unwrap();
        assert!(parsed.mode.is_none(), "缺省无 mode 字段");
        let parsed: TerminalFrame = serde_json::from_value(serde_json::json!({
            "type": "subscribe", "sessionId": "s1", "mode": "poll"
        }))
        .unwrap();
        assert_eq!(parsed.mode.as_deref(), Some("poll"));
    }

    /// 结构锁一：终端协议生产路径**不经宿主会话/终端业务类型**——本文件实现段
    /// 不得出现 `Message::` / `SyncEvent::` / `SessionControlAction` / `hostBroadcastSessionId`
    #[test]
    fn ws_terminal_has_no_host_business_types() {
        let root = env!("CARGO_MANIFEST_DIR");
        let src = std::fs::read_to_string(format!("{root}/src/ws_terminal.rs")).expect("read ws_terminal.rs");
        let implementation = src.split("#[cfg(test)]").next().unwrap_or(&src);
        let mut violations: Vec<String> = Vec::new();
        for (idx, raw) in implementation.lines().enumerate() {
            let line = raw.trim_start();
            if line.starts_with("//") || line.starts_with("///") || line.starts_with("//!") {
                continue;
            }
            for marker in ["Message::", "SyncEvent::", "SessionControlAction", "hostBroadcastSessionId", "WatchMode", "SessionStopped"] {
                if line.contains(marker) {
                    violations.push(format!("{}:{}: {}", "ws_terminal.rs", idx + 1, line.trim()));
                }
            }
        }
        assert!(
            violations.is_empty(),
            "ws_terminal 实现段不得出现宿主业务类型（票 04）：\n{}",
            violations.join("\n")
        );
    }
}
