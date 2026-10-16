//! host-terminal-stream 域（移动独有 interface，ADR 0018）：WIT adapter + 零解析窄转发网关
//!
//! ## 本文件持有的两段
//!
//! 1. **窄转发表**（[`TerminalStreamGateway`]）：会话 → 前端页面通道（Tauri Channel）
//!    的登记与裸字节转发。Channel 是 Tauri IPC 机制、WASM 插件无法持有，所以登记
//!    （命令面 `terminal_page_subscribe`）与转发（本域 WIT 出口）共用同一张表。
//!    票 05 绑定归位前这张表住 wasm-core（`terminal_stream_gateway.rs`），随本域
//!    整体迁入移动宿主——它是移动独有域的机制，内核不该带端词汇。
//! 2. **WIT layer**（`Host for WasmPluginState`）：权限门（`terminal:output`）+
//!    空 id 校验 + 转发；错误文案由网关持有（无登记 Channel → 网关自身 fail-visible）。
//!
//! C3 红线（ADR 0022 四类薄壳④）：字节全程零 JSON / 零 base64——本层不读字节内容、
//! 不识别终端协议、不缓存；协议状态机（订阅门控 / ack / resync / 重连）全在插件侧。
//! 判据自查（ADR 0022 B1–B6）：session_id 仅是**寻址键**，本层不定义产品类型、
//! 不做编排、不持业务事实、不解释事件——零命中。
//!
//! `host-terminal-stream` 是移动独有 interface：桌面无此域（`events` 客户端化后
//! 桌面终端输出走 WS 帧下行），故本模块不参与桌面装配。

use bedcode_host_kit::{HostModule, HostModuleDesc, ModuleEntry, WasmPluginState};
use bedcode_plugin_api_mobile::permission::PERMISSION_TERMINAL_OUTPUT;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use tauri::ipc::{Channel, InvokeResponseBody};

use crate::plugin::bindings::{bedcode, check_permission, guarded};

// ==================== 能力模块自报 ====================

bedcode_host_kit::expect_host_module!(MODULE_NAME);

/// 能力模块名
pub const MODULE_NAME: &str = "mobile-terminal-stream";

/// 本域提供的 WIT 接口（必须与 `cap-mobile.wit` 逐字一致）
pub const MODULE_INTERFACES: &[&str] = &["bedcode:plugin@0.2.0/host-terminal-stream"];

/// 本域的权限位（与 WIT / SDK 权限表逐字一致）
pub const MODULE_PERMISSIONS: &[&str] = &["terminal:output"];

/// 能力模块描述符
const DESC: HostModuleDesc = HostModuleDesc {
    name: MODULE_NAME,
    interfaces: MODULE_INTERFACES,
    permissions: MODULE_PERMISSIONS,
    abi_min: 15,
};

/// host-terminal-stream 能力模块
pub struct TerminalStreamModule;

impl HostModule for TerminalStreamModule {
    fn desc(&self) -> HostModuleDesc {
        DESC
    }

    /// 端差异域实现：跨端**同进程**装配（cross-end rig：桌面 lib 与移动 lib 同时链入）
    /// 时让位给已安装的另一端实现（接口同名，重复注册即 `defined twice`）；
    /// 单端装配（生产移动宿主）无竞争，照常安装。
    fn fallback(&self) -> bool {
        true
    }

    fn register(&self, linker: &mut wasmtime::component::Linker<WasmPluginState>) -> wasmtime::Result<()> {
        bedcode::plugin::host_terminal_stream::add_to_linker::<WasmPluginState, HasSelf>(linker, |s| s)
    }
}

type HasSelf = wasmtime::component::HasSelf<WasmPluginState>;

static MODULE: TerminalStreamModule = TerminalStreamModule;

inventory::submit! {
    ModuleEntry { module: &MODULE }
}

// ==================== 窄转发表（Gateway） ====================

/// 单会话通道表容量防御上限（页面订阅配对由前端生命周期保证，此为兜底）
const MAX_CHANNELS: usize = 64;

/// 窄转发统计（诊断：转发与丢弃的字节/帧计数）
///
/// 为什么需要：页面通道未登记 / 通道发送失败时字节**就地丢弃**（无缓存，靠重进
/// 页面的重订阅回放补齐）——没有计数就只能看到「屏幕缺内容」而查不到丢在哪一跳。
/// 计数非零即「有页面输出没到达前端」，与前端 frameStats / 插件侧 cursor 对账。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TerminalStreamStats {
    /// 成功转发的帧数
    pub forwarded_chunks: u64,
    /// 成功转发的字节数
    pub forwarded_bytes: u64,
    /// 因无页面通道而丢弃的帧数（页面未订阅 / 已离开；重进页面重放补齐）
    pub dropped_no_channel: u64,
    /// 因通道发送失败而丢弃的帧数（WebView 侧已释放，槽位已就地摘除）
    pub dropped_send_failed: u64,
}

/// 会话 → 页面通道的窄转发表（全局单例）
pub struct TerminalStreamGateway {
    channels: Mutex<HashMap<String, Channel<InvokeResponseBody>>>,
    /// 成功转发的帧数 / 字节数
    forwarded_chunks: std::sync::atomic::AtomicU64,
    forwarded_bytes: std::sync::atomic::AtomicU64,
    /// 无页面通道丢弃的帧数
    dropped_no_channel: std::sync::atomic::AtomicU64,
    /// 通道发送失败丢弃的帧数
    dropped_send_failed: std::sync::atomic::AtomicU64,
    /// 已就「页面未订阅导致丢弃」告警过的会话（page_subscribe 时复位）：
    /// 只在每段未订阅窗口首次丢弃时留痕，长会话离开页面期间不刷屏
    warned_no_channel: Mutex<std::collections::HashSet<String>>,
}

static GATEWAY: OnceLock<Arc<TerminalStreamGateway>> = OnceLock::new();

/// 全局 gateway 单例（WIT 原语与 Tauri 命令共用同一张表）
pub fn terminal_stream_gateway() -> Arc<TerminalStreamGateway> {
    GATEWAY
        .get_or_init(|| Arc::new(TerminalStreamGateway::default()))
        .clone()
}

impl Default for TerminalStreamGateway {
    fn default() -> Self {
        Self {
            channels: Mutex::new(HashMap::new()),
            forwarded_chunks: std::sync::atomic::AtomicU64::new(0),
            forwarded_bytes: std::sync::atomic::AtomicU64::new(0),
            dropped_no_channel: std::sync::atomic::AtomicU64::new(0),
            dropped_send_failed: std::sync::atomic::AtomicU64::new(0),
            warned_no_channel: Mutex::new(std::collections::HashSet::new()),
        }
    }
}

impl TerminalStreamGateway {
    /// 登记页面通道（进入终端页）。幂等：重复订阅覆盖旧通道（旧通道随即
    /// 失效——前端 markPageEntered 已先作废其 onmessage，在途帧不会写入
    /// 已卸载的 xterm）
    pub fn page_subscribe(&self, session_id: &str, channel: Channel<InvokeResponseBody>) {
        if session_id.is_empty() {
            return;
        }
        let mut channels = self.channels.lock().unwrap_or_else(|p| p.into_inner());
        if channels.len() >= MAX_CHANNELS && !channels.contains_key(session_id) {
            tracing::warn!(
                count = channels.len(),
                "terminal stream gateway: channel table full, refusing new registration"
            );
            return;
        }
        channels.insert(session_id.to_string(), channel);
        // 新页面已订阅：丢弃告警窗口复位（下一次丢弃重新留痕）
        self.warned_no_channel
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(session_id);
        tracing::debug!(session_id = %session_id, "terminal page subscribe (frontend consumer attached)");
    }

    /// 注销页面通道（退出终端页）。输出在页面关闭期间不缓存（由重订阅回放补齐）
    pub fn page_unsubscribe(&self, session_id: &str) {
        if session_id.is_empty() {
            return;
        }
        let removed = self
            .channels
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(session_id);
        if removed.is_some() {
            tracing::debug!(session_id = %session_id, "terminal page unsubscribe (frontend consumer detached)");
        }
    }

    /// 会话删除：清通道（订阅/链路语义在插件侧，此处只管转发面）
    pub fn remove(&self, session_id: &str) {
        if session_id.is_empty() {
            return;
        }
        self.channels
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(session_id);
    }

    /// 清空全部通道（插件停用 purge / 连接全断时由聚合入口调用）
    pub fn clear_all(&self) {
        let count = self.channels.lock().unwrap_or_else(|p| p.into_inner()).len();
        if count > 0 {
            self.channels.lock().unwrap_or_else(|p| p.into_inner()).clear();
            tracing::debug!(count, "terminal stream gateway: all channels cleared");
        }
    }

    /// 诊断：查询页面订阅态（前端 store 用命令面对账，不依赖此处）
    pub fn is_page_subscribed(&self, session_id: &str) -> bool {
        self.channels
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .contains_key(session_id)
    }

    /// 转发一段输出字节（WIT `forward-output` 的执行体）。
    ///
    /// 通道缺失（页面未订阅/已卸载）或发送失败（WebView 侧已释放）→ Err +
    /// **丢弃计数**：字节由调用方丢弃、无缓存（页面重进经重订阅回放补齐）。
    /// 发送失败时就地清槽，避免后续每次转发都失败刷日志。`Channel::send` 是同步
    /// 入队（非网络 IO），锁内执行与退役前 `terminal_link` 同形态。
    pub fn forward_output(&self, session_id: &str, data: &[u8]) -> Result<(), String> {
        use std::sync::atomic::Ordering;
        let mut channels = self.channels.lock().unwrap_or_else(|p| p.into_inner());
        let Some(channel) = channels.get_mut(session_id) else {
            let dropped = self.dropped_no_channel.fetch_add(1, Ordering::Relaxed) + 1;
            // 每段未订阅窗口只留痕一次（页面离开期间长会话会持续产出，逐帧打点即刷屏）
            let first_in_window = self
                .warned_no_channel
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .insert(session_id.to_string());
            if first_in_window {
                tracing::warn!(
                    session_id = %session_id,
                    dropped_frames = dropped,
                    bytes = data.len(),
                    "terminal stream gateway: no page channel, output dropped (repair by page re-subscribe replay)"
                );
            }
            return Err(format!(
                "terminal stream: no page channel for session {session_id} (page not subscribed)"
            ));
        };
        if let Err(e) = channel.send(InvokeResponseBody::Raw(data.to_vec())) {
            channels.remove(session_id);
            self.dropped_send_failed.fetch_add(1, Ordering::Relaxed);
            return Err(format!(
                "terminal stream: channel send failed for session {session_id}, detaching consumer: {e}"
            ));
        }
        self.forwarded_chunks.fetch_add(1, Ordering::Relaxed);
        self.forwarded_bytes.fetch_add(data.len() as u64, Ordering::Relaxed);
        Ok(())
    }

    /// 转发/丢弃计数快照（诊断：与前端 frameStats、插件侧 cursor 对账）
    pub fn stats(&self) -> TerminalStreamStats {
        use std::sync::atomic::Ordering;
        TerminalStreamStats {
            forwarded_chunks: self.forwarded_chunks.load(Ordering::Relaxed),
            forwarded_bytes: self.forwarded_bytes.load(Ordering::Relaxed),
            dropped_no_channel: self.dropped_no_channel.load(Ordering::Relaxed),
            dropped_send_failed: self.dropped_send_failed.load(Ordering::Relaxed),
        }
    }
}

// ==================== WIT 层（import 接口 → 域逻辑） ====================

impl bedcode::plugin::host_terminal_stream::Host for WasmPluginState {
    fn forward_output(&mut self, session_id: String, data: Vec<u8>) -> Result<(), String> {
        // 权限门在空 id 校验之前（与迁入前同序）
        if !check_permission(self, PERMISSION_TERMINAL_OUTPUT, "host_terminal_stream_forward_output") {
            return Err("permission denied: terminal:output".to_string());
        }
        if session_id.is_empty() {
            return Err("terminal-stream forward-output: session-id is empty".to_string());
        }
        let plugin_id = self.plugin_id.clone();
        guarded(
            &plugin_id,
            "host_terminal_stream_forward_output",
            Err("host_terminal_stream_forward_output panicked".to_string()),
            || terminal_stream_gateway().forward_output(&session_id, &data),
        )
    }
}

// ==================== 测试 ====================

#[cfg(test)]
mod tests {
    use super::*;

    /// 无页面通道：转发显性失败（fail-visible 形态①，禁「查不到就静默成功」）
    /// + 丢弃计数递增（可观测：屏幕缺内容时能查到「丢在哪一跳」）
    #[test]
    fn forward_without_channel_fails_visibly_and_counts_drop() {
        let gw = TerminalStreamGateway::default();
        let err = gw.forward_output("s-404", b"x").expect_err("no channel must fail");
        assert!(err.contains("no page channel"), "{err}");
        assert!(!gw.is_page_subscribed("s-404"));
        let stats = gw.stats();
        assert_eq!(stats.dropped_no_channel, 1, "无通道丢弃必须计数: {stats:?}");
        assert_eq!(stats.forwarded_chunks, 0);
        assert_eq!(stats.forwarded_bytes, 0);
        // 空 session 的登记/注销是 no-op（与退役前 terminal_link 同语义）
        gw.remove("");
        assert!(!gw.is_page_subscribed(""));
    }

    /// 已登记通道：字节原样送达（Raw）+ 转发计数递增；丢弃计数不动
    #[test]
    fn forward_with_channel_sends_raw_and_counts_forwarded() {
        let gw = TerminalStreamGateway::default();
        let received: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = received.clone();
        gw.page_subscribe(
            "s-1",
            Channel::new(move |body| {
                if let InvokeResponseBody::Raw(bytes) = body {
                    sink.lock().unwrap_or_else(|p| p.into_inner()).extend_from_slice(&bytes);
                }
                Ok(())
            }),
        );
        gw.forward_output("s-1", b"hello").expect("forward ok");
        gw.forward_output("s-1", b" world").expect("forward ok");
        assert_eq!(&received.lock().unwrap_or_else(|p| p.into_inner())[..], b"hello world");
        let stats = gw.stats();
        assert_eq!(stats.forwarded_chunks, 2);
        assert_eq!(stats.forwarded_bytes, 11);
        assert_eq!(stats.dropped_no_channel, 0);
        assert_eq!(stats.dropped_send_failed, 0);
    }
}
