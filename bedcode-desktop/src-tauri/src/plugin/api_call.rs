//! host-api-call 宿主侧接线（路径 B **薄转发**：WIT 绑定 + `Host` impl + 自报）
//!
//! wasm-core 纯净性收口票 02 批次 05 迁出：本文件**只有 WIT impl 与自报**，没有
//! 域函数——回复道编排（订阅回复道 → 发布请求 → 等待 → id/属主校验 → 清理）是
//! 内核互调**机制**（`host_api/api.rs` 的 `api_call` + `ReplyHandler`），内核的
//! `intercall`（通用互调客户端）与 `auth_center` 桥接经 `call_plugin_api_host`
//! 消费同一条编排。判据与 peer/ws/http「内核仍消费引擎面」同源：编排留内核、
//! 装配方自带 bindgen（孤儿规则见 [`super::bindings`] 模块文档）。
//!
//! 语义（ADR-0017）：无权限门——互调的准入判据是**目标 api 的声明登记**
//! （`ApiRegistry`，门禁在总线 `bus_publish` 内），不是权限位；调用方身份取
//! Caller state（本实例的 `plugin_id`，guest 无法伪造）。

use bedcode_host_kit::ports::downcast_host;
use bedcode_host_kit::{HostModule, HostModuleDesc, ModuleEntry, WasmPluginState};
use bedcode_wasm_core::host_api::api::api_call;
use bedcode_wasm_core::host_api::context::WasmHostContext;

use crate::plugin::bindings::bedcode;

// 能力模块白名单条目（宿主自报；生效白名单 = 内核 IN_CRATE ∪ 宿主自报，见
// `host_module_whitelist`）。路径 B 域的自报静态住在本 crate（宿主 lib 即最终
// 二进制）⇒ 无需能力 crate 那样的 `use <crate> as _;` 强制引用行。
bedcode_host_kit::expect_host_module!(MODULE_NAME);

/// 能力模块名（白名单键即装载期日志与错误文案里的模块名）
pub const MODULE_NAME: &str = "api_call";

/// 本域提供的 WIT 接口（必须与 `bedcode.wit` 逐字一致；改错即 guest import 失配）
pub const MODULE_INTERFACES: &[&str] = &["bedcode:plugin/host-api-call"];

/// 本域的权限位（必须与 `bedcode.wit` / SDK 权限表逐字一致）
///
/// **空表**：host-api-call 无权限位——互调准入判据是目标 api 的声明登记
/// （ADR-0017 门禁在总线 `bus_publish`），声明越权面不存在可授予的权限。
pub const MODULE_PERMISSIONS: &[&str] = &[];

/// 能力模块描述符（机制面：接口路径 / 权限位 / ABI 下界；**禁带产品名词**）
///
/// `abi_min = 7`：host-api-call（互调，ADR-0017）在 v7 窗口内随计划任务插件
/// 落地引入（从未单独占一个 bump note，紧随其后的是 v8 生命周期契约补全）。
const DESC: HostModuleDesc = HostModuleDesc {
    name: MODULE_NAME,
    interfaces: MODULE_INTERFACES,
    permissions: MODULE_PERMISSIONS,
    abi_min: 7,
};

/// host-api-call 能力模块
pub struct ApiCallModule;

impl HostModule for ApiCallModule {
    fn desc(&self) -> HostModuleDesc {
        DESC
    }

    fn register(&self, linker: &mut wasmtime::component::Linker<WasmPluginState>) -> wasmtime::Result<()> {
        bedcode::plugin::host_api_call::add_to_linker::<WasmPluginState, HasSelf>(linker, |s| s)
    }
}

/// getter：让 guest 侧 import 取到可变的状态引用（与内核接线同款）
type HasSelf = wasmtime::component::HasSelf<WasmPluginState>;

/// 静态单例（供 `inventory::submit!` 取址）
static MODULE: ApiCallModule = ApiCallModule;

// 能力模块自报（linker-section 静态；收集点在 host-kit）
inventory::submit! {
    ModuleEntry { module: &MODULE }
}

// ==================== WIT 层（import 接口 → 内核编排转发） ====================

/// 取本实例的宿主上下文（与内核 `HostCtxOf::host_ctx` 同一转型；类型不符即 panic
/// ——装配期编程错误，fail-visible，不静默降级）
fn ctx_of(state: &WasmPluginState) -> &WasmHostContext {
    downcast_host::<WasmHostContext>(state.host.as_ref())
}

impl bedcode::plugin::host_api_call::Host for WasmPluginState {
    fn call(&mut self, request_topic: String, payload_json: String, timeout_ms: u64) -> Result<String, String> {
        // caller = 本实例 plugin_id（Caller state 派生，guest 无法伪造）；编排
        // （门禁 / 回复订阅 / 属主校验 / 清理）全部在内核 `api_call`——本层零逻辑。
        api_call(
            ctx_of(self),
            ctx_of(self),
            ctx_of(self),
            &self.plugin_id,
            &request_topic,
            &payload_json,
            timeout_ms,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== 白名单 / 自报三件一致（与 crypto/auth 样板同款） ====================

    /// 白名单声明、接口路径、权限位三件与本域常量逐字一致
    #[test]
    fn host_module_declaration_matches_domain_constants() {
        assert!(
            bedcode_host_kit::expected_host_modules().contains(&MODULE_NAME),
            "能力模块白名单缺 {MODULE_NAME}（expect_host_module! 行被删 / 未收集）"
        );
        assert_eq!(MODULE_NAME, "api_call", "白名单键即装载期日志与错误文案里的模块名");
        assert_eq!(
            MODULE_INTERFACES,
            &["bedcode:plugin/host-api-call"],
            "接口路径必须与 WIT 契约逐字一致（改错即 guest import 失配）"
        );
        assert!(
            MODULE_PERMISSIONS.is_empty(),
            "host-api-call 无权限位（互调准入判据是目标 api 的声明登记，ADR-0017）"
        );
    }
}
