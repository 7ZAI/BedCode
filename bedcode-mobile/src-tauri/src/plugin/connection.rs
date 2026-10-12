//! host-connection 宿主 adapter（移动独有 `primary-target`；wasm-core-mobile-purity 票 04 自 wasm-core 迁入）
//!
//! 只暴露**引擎事实**（最后配置的目标地址 / 端口 + 连接状态），零业务投影：
//! 不返回设备展示名（name 归插件自持）、不解释连接语义（B1–B6 零命中）。
//! 无权限门（ADR 0022 例外先例 = host-platform）：地址是非凭据的局域网服务事实；
//! 连接行为本身已由 `ws:client` / `network:http` 等权限位门控。
//!
//! 从未配置过任何目标 → Err（fail-visible，禁「返回空对象」——真源读取缺链必须
//! 显性暴露，禁「查不到就返回空」静默降级形态）。

use bedcode_host_kit::{HostModule, HostModuleDesc, ModuleEntry, WasmPluginState};

use crate::plugin::bindings::{bedcode, ctx_of};
use bedcode_wasm_core::host_api::ports::HostEnginePorts;
use bedcode_wasm_core::runtime_util::block_on_async;

bedcode_host_kit::expect_host_module!(MODULE_NAME);

/// 能力模块名
pub const MODULE_NAME: &str = "mobile-connection";

/// 本域提供的 WIT 接口（必须与 `cap-mobile.wit` 逐字一致）
pub const MODULE_INTERFACES: &[&str] = &["bedcode:plugin/host-connection"];

/// 本域无权限门（ADR 0022 已登记的例外，见模块头注）
pub const MODULE_PERMISSIONS: &[&str] = &[];

/// 能力模块描述符
const DESC: HostModuleDesc = HostModuleDesc {
    name: MODULE_NAME,
    interfaces: MODULE_INTERFACES,
    permissions: MODULE_PERMISSIONS,
    abi_min: 15,
};

/// host-connection 能力模块
pub struct ConnectionModule;

impl HostModule for ConnectionModule {
    fn desc(&self) -> HostModuleDesc {
        DESC
    }

    fn register(&self, linker: &mut wasmtime::component::Linker<WasmPluginState>) -> wasmtime::Result<()> {
        bedcode::plugin::host_connection_target::add_to_linker::<WasmPluginState, HasSelf>(linker, |s| s)
    }
}

type HasSelf = wasmtime::component::HasSelf<WasmPluginState>;

static MODULE: ConnectionModule = ConnectionModule;

inventory::submit! {
    ModuleEntry { module: &MODULE }
}

// ==================== WIT 层（import 接口 → 域逻辑） ====================

impl bedcode::plugin::host_connection_target::Host for WasmPluginState {
    fn primary_target(&mut self) -> Result<String, String> {
        let engine = ctx_of(self).ports.connection_engine();
        let Some((target, connected)) = block_on_async(async move { engine.primary_target().await })
            .map_err(|e| format!("connection primary-target: engine read failed: {e}"))?
        else {
            return Err(
                "connection: no primary target configured (device not paired/connected yet)".to_string(),
            );
        };
        serde_json::to_string(&serde_json::json!({
            "address": target.address,
            "port": target.port,
            "connected": connected,
        }))
        .map_err(|e| format!("connection primary-target: serialization failed: {e}"))
    }
}
