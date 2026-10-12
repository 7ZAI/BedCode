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
pub const MODULE_INTERFACES: &[&str] = &["bedcode:plugin/host-terminal-stream"];

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

/// 会话 → 页面通道的窄转发表（全局单例）
pub struct TerminalStreamGateway {
    channels: Mutex<HashMap<String, Channel<InvokeResponseBody>>>,
}

static GATEWAY: OnceLock<Arc<TerminalStreamGateway>> = OnceLock::new();

/// 全局 gateway 单例（WIT 原语与 Tauri 命令共用同一张表）
pub fn terminal_stream_gateway() -> Arc<TerminalStreamGateway> {
    GATEWAY
        .get_or_init(|| {
            Arc::new(TerminalStreamGateway {
                channels: Mutex::new(HashMap::new()),
            })
        })
        .clone()
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
    /// 通道缺失（页面未订阅/已卸载）或发送失败（WebView 侧已释放）→ Err：
    /// 字节由调用方丢弃、无缓存（页面重进经重订阅回放补齐）。发送失败时
    /// 就地清槽，避免后续每次转发都失败刷日志。`Channel::send` 是同步入队
    /// （非网络 IO），锁内执行与退役前 `terminal_link` 同形态。
    pub fn forward_output(&self, session_id: &str, data: &[u8]) -> Result<(), String> {
        let mut channels = self.channels.lock().unwrap_or_else(|p| p.into_inner());
        let Some(channel) = channels.get_mut(session_id) else {
            return Err(format!(
                "terminal stream: no page channel for session {session_id} (page not subscribed)"
            ));
        };
        if let Err(e) = channel.send(InvokeResponseBody::Raw(data.to_vec())) {
            channels.remove(session_id);
            return Err(format!(
                "terminal stream: channel send failed for session {session_id}, detaching consumer: {e}"
            ));
        }
        Ok(())
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

    /// 无头环境无真实 Channel 可构造：锁「空 session 拒绝」与「未登记转发
    /// 显性报错」（fail-visible 形态①，禁「查不到就静默成功」）
    #[test]
    fn forward_without_channel_fails_visibly() {
        let gw = TerminalStreamGateway {
            channels: Mutex::new(HashMap::new()),
        };
        let err = gw.forward_output("s-404", b"x").expect_err("no channel must fail");
        assert!(err.contains("no page channel"), "{err}");
        assert!(!gw.is_page_subscribed("s-404"));
        // 空 session 的登记/注销是 no-op（与退役前 terminal_link 同语义）
        gw.remove("");
        assert!(!gw.is_page_subscribed(""));
    }
}
