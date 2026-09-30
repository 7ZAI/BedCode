//! 移动端客户端装配：目标设备 / 全局 token / 事件与终端输出记录替身
//!
//! 替身口径（**只记录、不伪造**）：移动端链路把「状态变化」与「终端输出字节」
//! 交给页面侧出口，真实形态是 Tauri WebView。跨端测试里没有 WebView，用
//! 记录替身承接**已经真实发生**的数据：
//! - [`EventRecorder`] 承接 `TerminalEventSink`（链路状态流转的事实记录）；
//! - [`OutputRecorder`] 承接页面级 `Channel<InvokeResponseBody>`（PTY 真实
//!   输出字节的落点）。
//!
//! 替身不参与任何协议应答：桌面端返回的每一帧都由移动端真实客户端解析，
//! 替身只被「读」用于断言。

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bedcode_mobile_lib::state::{clear_global_token, get_connection_manager, set_global_token};
use bedcode_mobile_lib::terminal_link::TerminalEventSink;
use tauri::ipc::{Channel, InvokeResponseBody};

/// 单次测试的等待预算（CI 慢机放宽：spec §4 预算 5s，此处 15s）
pub const WAIT_TIMEOUT: Duration = Duration::from_secs(15);

/// 把移动端目标设备指向本进程内的桌面端无头服务器
///
/// `set_target` 是 HTTP 认证与终端链路 WS 的**地址真源**（移动端每个请求
/// 经 `resolve_base_url` / 链路 `connect_once` 从它取地址），因此这一行就是
/// 「移动端连上桌面端」的全部接线。
pub async fn set_target(port: u16) {
    get_connection_manager()
        .set_target("127.0.0.1".to_string(), port, Some("cross-end-desktop".to_string()))
        .await;
}

/// 目标设备 base URL（移动端自算，与生产路径同函数）
pub fn base_url(port: u16) -> String {
    bedcode_mobile_lib::auth::http::format_base_url("127.0.0.1", port)
}

/// 场景收尾：清全局 token（防下一场景残留已认证身份）
pub fn clear_identity() {
    clear_global_token();
}

/// 记录 JWT（生产路径由认证中心签发后写入全局，移动端后续请求与 WS 首帧共用）
pub fn remember_token(token: &str) {
    set_global_token(token);
}

/// 终端链路状态事件记录替身（`TerminalEventSink`）
#[derive(Default)]
pub struct EventRecorder {
    events: Mutex<Vec<(String, serde_json::Value)>>,
}

impl EventRecorder {
    pub fn snapshot(&self) -> Vec<(String, serde_json::Value)> {
        self.events.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// `terminal-state` 的 detail 序列（按到达序，用于断言状态流转）
    pub fn state_details(&self) -> Vec<String> {
        self.snapshot()
            .iter()
            .filter(|(e, _)| e == "terminal-state")
            .filter_map(|(_, p)| p.get("detail").and_then(|v| v.as_str()).map(str::to_string))
            .collect()
    }

    /// `terminal-state` 的 phase 序列
    pub fn state_phases(&self) -> Vec<String> {
        self.snapshot()
            .iter()
            .filter(|(e, _)| e == "terminal-state")
            .filter_map(|(_, p)| p.get("phase").and_then(|v| v.as_str()).map(str::to_string))
            .collect()
    }

    /// `terminal-resync` 事件次数（重锚信号）
    pub fn resync_count(&self) -> usize {
        self.snapshot().iter().filter(|(e, _)| e == "terminal-resync").count()
    }

    /// 最后一个 `terminal-state` 且 `detail` 匹配的载荷（无匹配则 None）
    pub fn last_state_payload(&self, detail: &str) -> Option<serde_json::Value> {
        self.snapshot()
            .into_iter()
            .rev()
            .find(|(e, p)| e == "terminal-state" && p.get("detail").and_then(|v| v.as_str()) == Some(detail))
            .map(|(_, p)| p)
    }
}

impl TerminalEventSink for EventRecorder {
    fn emit(&self, event: &str, payload: serde_json::Value) -> Result<(), String> {
        self.events
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push((event.to_string(), payload));
        Ok(())
    }
}

/// 页面级终端输出记录器（`Channel` 回调的落点）
#[derive(Default)]
pub struct OutputRecorder {
    chunks: Mutex<Vec<Vec<u8>>>,
}

impl OutputRecorder {
    /// 至此已推给页面的全部输出字节（按帧序拼接）
    pub fn bytes(&self) -> Vec<u8> {
        self.chunks
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .flatten()
            .copied()
            .collect()
    }

    /// 已推字节按 UTF-8 有损解码（PTY 断言用 ASCII marker）
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.bytes()).to_string()
    }

    pub fn frame_count(&self) -> usize {
        self.chunks.lock().unwrap_or_else(|p| p.into_inner()).len()
    }

    /// 帧边界（`(该帧起始字节偏移, 该帧长度)` 列表）
    ///
    /// 取证用：把「某个字节偏移」映射回「它落在哪一帧 / 帧内第几字节」，即可判定
    /// 字节级错位发生在**帧内部**（环 / 插件拼装）还是**帧边界上**（传输 / 转发）。
    pub fn frame_spans(&self) -> Vec<(usize, usize)> {
        let mut spans = Vec::new();
        let mut offset = 0usize;
        for chunk in self.chunks.lock().unwrap_or_else(|p| p.into_inner()).iter() {
            spans.push((offset, chunk.len()));
            offset += chunk.len();
        }
        spans
    }
}

/// 造一个把移动端终端输出记到 `recorder` 的页面级 Channel
///
/// 生产形态是 Tauri `Channel<InvokeResponseBody>` 推给终端页 WebView；这里用
/// 同类型同构造（`Channel::new` 回调同步触发），把 `Raw` 裸字节原样攒起来。
/// 非 `Raw` 形态（Json 兜底）也一并记录，避免静默丢形态。
pub fn output_channel(recorder: Arc<OutputRecorder>) -> Channel<InvokeResponseBody> {
    let sink = recorder.clone();
    Channel::new(move |body| {
        match body {
            InvokeResponseBody::Raw(bytes) => {
                sink.chunks.lock().unwrap_or_else(|p| p.into_inner()).push(bytes);
            }
            other => {
                // 兜底形态：显性留痕而不是静默丢弃（终端输出恒为 Raw，
                // 出现别的形态说明链路改道了）
                sink.chunks
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .push(format!("[non-raw:{other:?}]").into_bytes());
            }
        }
        Ok(())
    })
}

/// 轮询直到 `pred` 为真（超时 panic 并打印现场，防卡死）
pub async fn wait_until(what: &str, mut pred: impl FnMut() -> bool) {
    let deadline = Instant::now() + WAIT_TIMEOUT;
    loop {
        if pred() {
            return;
        }
        if Instant::now() >= deadline {
            panic!("timed out waiting for {what}");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// 轮询等待 `recorder` 收到满足谓词的状态事件
pub async fn wait_event(
    recorder: &Arc<EventRecorder>,
    what: &str,
    mut pred: impl FnMut(&[(String, serde_json::Value)]) -> bool,
) {
    let deadline = Instant::now() + WAIT_TIMEOUT;
    loop {
        if pred(&recorder.snapshot()) {
            return;
        }
        if Instant::now() >= deadline {
            panic!("timed out waiting for {what}; events={:#?}", recorder.snapshot());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// 轮询等待输出记录器出现 `marker`
pub async fn wait_output(recorder: &Arc<OutputRecorder>, marker: &str) {
    wait_until(&format!("output marker {marker:?}"), || {
        recorder.text().contains(marker)
    })
    .await;
}
