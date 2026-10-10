//! host-terminal-stream 宿主 adapter（移动独有域；wasm-core-mobile-purity 票 04 自 wasm-core 迁入）
//!
//! 零解析窄转发（ADR 0022 四类薄壳④）：把插件交来的输出**裸字节**按 session-id
//! 转发到已登记的前端页面通道。宿主不读内容、不缓存、不解析终端协议
//! （C3 红线：字节全程零 JSON / 零 base64）。
//!
//! 网关（表 + 转发 + Tauri 命令面）仍在 wasm-core（`terminal_stream_gateway`，
//! Channel 是 Tauri IPC 机制，登记与转发共用同一张表）——本 adapter 只加权限门
//! 与空 id 校验；错误文案由网关持有（无登记 Channel → 网关自身 fail-visible）。

use bedcode_host_kit::{HostModule, HostModuleDesc, ModuleEntry, WasmPluginState};
use bedcode_plugin_api_mobile::permission::PERMISSION_TERMINAL_OUTPUT;

use crate::plugin::bindings::{bedcode, check_permission, guarded};

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
            || {
                bedcode_wasm_core_mobile::terminal_stream_gateway::terminal_stream_gateway()
                    .forward_output(&session_id, &data)
            },
        )
    }
}
