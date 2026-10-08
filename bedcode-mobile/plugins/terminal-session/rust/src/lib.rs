//! Terminal Session Plugin (WASM, Mobile) — 远程终端控制端（票 12）
//!
//! 终端订阅协议客户端从宿主 `terminal_link.rs`（1,363 行，已退役）整体迁入
//! 本插件：subscribe / ack / ring_resync / special-key 翻译（keys.rs）/
//! 重连编排随迁。协议事实源 = 桌面插件 `ws_terminal.rs`（wire 形状零变化）。
//!
//! ## 职责划分（ADR 0022 判据）
//!
//! - **插件（本 crate）**：协议状态机——fresh subscribe 门控、本地计数与
//!   ack 节流、ring_resync 重锚、session_missing 三振、输入计划（文本 +
//!   特殊键共存、失败上抛）、状态事件发射（`terminal-state` /
//!   `terminal-resync`，载荷形状与退役前逐字段一致）。
//! - **宿主（传输面，本 crate 不感知实现）**：WS 连接生命周期
//!   （`host-websocket` 客户端域）、首消息认证代发（`jwt-auth`——token 不落
//!   插件，C4）、连接级心跳、断线自动重连（R1：退避重建 + `ws:open` 新句柄，
//!   本插件按新句柄重新订阅）、输出字节窄转发（`host-terminal-stream`）。
//!
//! ## 数据通路（C3 性能红线）
//!
//! 输出字节全程零 JSON：桌面 → `ws:message` 二进制信封（`on_message_binary`）
//! → 门控 → `terminal_stream_forward_output`（`list<u8>`）→ 宿主页面 Channel
//! （Raw 字节）→ 前端。控制帧（subscribed / ring_resync / session_stopped /
//! error）是低频 JSON，走 `emit_event`。
//!
//! ## 生命周期
//!
//! - 进入终端页 → `terminal-session.subscribe`：建连（宿主代发 auth）→ 发
//!   subscribe 帧 → fresh subscribe 回放环窗口，收 `subscribed` 即 live。
//! - 离开终端页 → `terminal-session.unsubscribe`：对句柄 `close`（宿主取消
//!   自动重连）——不得后台常拉。
//! - 意外断开 → 宿主自动重连 → `ws:open`（新句柄）→ 本插件重新订阅并按
//!   重订阅语义发 `terminal-resync`（重播与已在屏内容不重叠）。

use bedcode_plugin_api_mobile::host::ws::{
    ws_event_topic, ws_message_topic, WS_CLOSE, WS_ERROR, WS_OPEN, WS_RECONNECT_SCHEDULED,
};
use bedcode_plugin_api_mobile::host::{HostBus, HostConnection, HostLog, HostWs};
use bedcode_plugin_api_mobile::types::PluginManifest;
use bedcode_plugin_api_mobile::wasm_host::WasmHost;
use bedcode_plugin_api_mobile::{BusMessage, WasmPlugin};

mod auth;
mod commands;
mod keys;
mod link;
mod protocol;

pub(crate) use link::LINK_MANAGER;

/// 插件 id（D6 选项 A：与桌面 `com.bedcode.terminal-session` 同名——两端同
/// id 职责不同，本端是远程终端控制端，契约独立不因同名互相约束，C8）
pub(crate) const PLUGIN_ID: &str = "com.bedcode.terminal-session";

/// 终端端点路径（桌面 manifest `contributes.wsEndpoints` 声明；URL = 主连接
/// 目标 + 本路径，见 commands::build_terminal_url）
pub(crate) const TERMINAL_WS_PATH: &str = "/ws/plugin/com.bedcode.terminal-session/terminal";

/// ws 状态事件 topic（属主私有）：`<plugin-id>:ws:open|error|close|reconnect-scheduled`
fn ws_topic(event: &str) -> String {
    ws_event_topic(event, PLUGIN_ID)
}

/// ws 下行帧 topic（属主私有二进制）：`<plugin-id>:ws:message`
fn ws_frame_topic() -> String {
    ws_message_topic(PLUGIN_ID)
}

fn host() -> WasmHost {
    WasmHost
}

pub(crate) struct TerminalSessionPlugin;

impl WasmPlugin for TerminalSessionPlugin {
    const ID: &'static str = PLUGIN_ID;

    fn manifest() -> PluginManifest {
        serde_json::from_str(include_str!("../../plugin.json"))
            .expect("plugin.json must be valid PluginManifest")
    }

    fn activate() -> anyhow::Result<()> {
        let h = host();
        h.log_info("Terminal Session plugin activating (remote-terminal-consumer, mobile)");
        // 认证状态对账（票 14 阶段 B）：事件不重放，激活期以引擎事实为准
        auth::log_auth_state(&h);
        // 订阅必须在 activate 期完成（宿主不缓冲不重放，晚订阅静默丢）：
        // 3 个 JSON 状态事件 + 1 个二进制帧 topic（ws 消费须同时持 bus 权限位）
        for topic in [
            ws_topic(WS_OPEN).as_str(),
            ws_topic(WS_ERROR).as_str(),
            ws_topic(WS_CLOSE).as_str(),
            ws_topic(WS_RECONNECT_SCHEDULED).as_str(),
        ] {
            h.bus_subscribe(topic)?;
        }
        h.bus_subscribe_binary(&ws_frame_topic())?;
        Ok(())
    }

    fn deactivate() -> anyhow::Result<()> {
        let h = host();
        h.log_info("Terminal Session plugin deactivating (closing all terminal links)");
        // 关闭全部终端连接（显式 close = 宿主取消自动重连，purge 兜底不泄漏）
        for handle in LINK_MANAGER.lock().expect("link manager lock").drain_handles() {
            if let Err(e) = h.ws_close(&handle, "{}") {
                h.log_info(&format!("deactivate: ws_close {handle} failed (non-fatal): {e}"));
            }
        }
        for topic in [
            ws_topic(WS_OPEN).as_str(),
            ws_topic(WS_ERROR).as_str(),
            ws_topic(WS_CLOSE).as_str(),
            ws_topic(WS_RECONNECT_SCHEDULED).as_str(),
        ] {
            let _ = h.bus_unsubscribe(topic);
        }
        let _ = h.bus_unsubscribe(&ws_frame_topic());
        Ok(())
    }

    fn invoke_command(name: &str, args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        commands::dispatch(name, &args)
    }

    /// ws 状态事件（JSON）：连接事实 → 协议状态机推进 + 状态事件转发前端。
    /// topic 是属主私有投递，只处理本插件的 ws:*
    fn on_bus_message(msg: &BusMessage) -> anyhow::Result<()> {
        let payload = msg.payload.clone();
        if msg.topic == ws_topic(WS_OPEN) {
            // 连接建立：重连成功事件带 `reconnectedFrom`（旧句柄）→ 恢复该
            // 会话订阅；首连的 open（无该字段）由 subscribe 同步路径处理，
            // 孤儿重连句柄在 link 层显式关闭止损
            if let Some(handle) = payload.get("handle").and_then(|v| v.as_str()) {
                let reconnected_from = payload.get("reconnectedFrom").and_then(|v| v.as_str());
                link::LinkManager::on_ws_open(&host(), handle, reconnected_from);
            }
            return Ok(());
        }
        if msg.topic == ws_topic(WS_CLOSE) {
            // 断开事实：清句柄（宿主 auto-reconnect 接管重建）；已显式停止
            // 的链路保持 stopped 不被重连复活
            if let Some(handle) = payload.get("handle").and_then(|v| v.as_str()) {
                link::LinkManager::on_ws_close(&host(), handle);
            }
            return Ok(());
        }
        if msg.topic == ws_topic(WS_RECONNECT_SCHEDULED) {
            // 宿主退避排期 → 透传前端倒计时（载荷形状与退役前
            // terminal_link 的 reconnect_scheduled detail 一致）
            if let Some(handle) = payload.get("handle").and_then(|v| v.as_str()) {
                let retry_in_ms = payload.get("retryInMs").and_then(|v| v.as_u64()).unwrap_or(0);
                link::LinkManager::on_reconnect_scheduled(&host(), handle, retry_in_ms);
            }
            return Ok(());
        }
        if msg.topic == ws_topic(WS_ERROR) {
            // 连接错误：留痕即可（断开事实由 close 事件承载，错误不改变状态机）
            let message = payload.get("message").and_then(|v| v.as_str()).unwrap_or("");
            host().log_warn(&format!("ws error on terminal link: {message}"));
            return Ok(());
        }
        Ok(())
    }

    /// ws 下行帧（二进制信封）：文本帧 = 控制协议（subscribed / ring_resync /
    /// session_stopped / error），二进制帧 = 终端输出裸字节 → 窄转发
    fn on_message_binary(msg: &BusMessage) -> anyhow::Result<()> {
        if msg.topic != ws_frame_topic() {
            return Ok(());
        }
        let Some(bytes) = msg.payload_binary.as_deref() else {
            return Ok(());
        };
        let frame = match bedcode_plugin_api_mobile::host::ws::parse_ws_frame(bytes) {
            Ok(f) => f,
            Err(e) => {
                host().log_warn(&format!("ws frame parse failed: {e}"));
                return Ok(());
            }
        };
        link::LinkManager::on_ws_frame(&host(), frame.handle, frame.kind, frame.payload);
        Ok(())
    }

    fn on_shutdown() -> anyhow::Result<()> {
        host().log_info("Terminal Session plugin shut down (remote-terminal-consumer, mobile)");
        Ok(())
    }
}

/// 连接目标（`host-connection.primary-target` 解析产物，供命令层拼端点 URL）
pub(crate) struct PrimaryTarget {
    pub(crate) address: String,
    pub(crate) port: u16,
}

/// 读主连接目标（引擎事实，票 13 复用同一原语）；未配置 → 显性错误
pub(crate) fn read_primary_target() -> anyhow::Result<PrimaryTarget> {
    let json = host().connection_primary_target()?;
    let v: serde_json::Value = serde_json::from_str(&json)
        .map_err(|e| anyhow::anyhow!("primary-target: invalid json: {e}"))?;
    Ok(PrimaryTarget {
        address: v
            .get("address")
            .and_then(|a| a.as_str())
            .unwrap_or_default()
            .to_string(),
        port: v.get("port").and_then(|p| p.as_u64()).unwrap_or(0) as u16,
    })
}

bedcode_plugin_api_mobile::wasm_entry!(TerminalSessionPlugin);
