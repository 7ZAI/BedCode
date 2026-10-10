//! host-timer 宿主侧接线（路径 B：WIT 绑定 + `Host` impl + 域函数 + 自报，四件同处）
//!
//! wasm-core 纯净性收口票 02 批次 05 自内核迁出（`host_api/timer.rs` 整文件删除，
//! 内核反向锁 `path_b_domains_must_not_return_to_wasm_core` 防回接）。与 auth /
//! crypto 同款：**没有独立能力 crate**——装配方就是宿主本 crate（孤儿规则见
//! [`super::bindings`] 模块文档）。
//!
//! 定时器域宿主实现（v6，ADR 0003）：宿主侧只负责"到点调用插件 command"，具体
//! 到点做什么、幂等与否归插件。

use bedcode_host_kit::ports::downcast_host;
use bedcode_host_kit::{HostModule, HostModuleDesc, ModuleEntry, WasmPluginState};
use bedcode_plugin_api::permission::PERMISSION_TIMER;
use bedcode_wasm_core::host_api::check_permission;
use bedcode_wasm_core::host_api::context::{PermissionScope, ServicesScope, WasmHostContext};
use bedcode_wasm_core::runtime_util::block_on_async;

use crate::plugin::bindings::bedcode;

// 能力模块白名单条目（宿主自报；生效白名单 = 内核 IN_CRATE ∪ 宿主自报，见
// `host_module_whitelist`）。路径 B 域的自报静态住在本 crate（宿主 lib 即最终
// 二进制）⇒ 无需能力 crate 那样的 `use <crate> as _;` 强制引用行。
bedcode_host_kit::expect_host_module!(MODULE_NAME);

/// 能力模块名（白名单键即装载期日志与错误文案里的模块名）
pub const MODULE_NAME: &str = "timer";

/// 本域提供的 WIT 接口（必须与 `bedcode.wit` 逐字一致；改错即 guest import 失配）
pub const MODULE_INTERFACES: &[&str] = &["bedcode:plugin/host-timer"];

/// 本域的权限位（必须与 `bedcode.wit` / SDK 权限表逐字一致）
pub const MODULE_PERMISSIONS: &[&str] = &["timer:schedule"];

/// 能力模块描述符（机制面：接口路径 / 权限位 / ABI 下界；**禁带产品名词**）
///
/// `abi_min = 6`：host-timer 在 ABI v6 引入（ADR 0003）。
const DESC: HostModuleDesc = HostModuleDesc {
    name: MODULE_NAME,
    interfaces: MODULE_INTERFACES,
    permissions: MODULE_PERMISSIONS,
    abi_min: 6,
};

/// host-timer 能力模块
pub struct TimerModule;

impl HostModule for TimerModule {
    fn desc(&self) -> HostModuleDesc {
        DESC
    }

    fn register(&self, linker: &mut wasmtime::component::Linker<WasmPluginState>) -> wasmtime::Result<()> {
        bedcode::plugin::host_timer::add_to_linker::<WasmPluginState, HasSelf>(linker, |s| s)
    }
}

/// getter：让 guest 侧 import 取到可变的状态引用（与内核接线同款）
type HasSelf = wasmtime::component::HasSelf<WasmPluginState>;

/// 静态单例（供 `inventory::submit!` 取址）
static MODULE: TimerModule = TimerModule;

// 能力模块自报（linker-section 静态；收集点在 host-kit）
inventory::submit! {
    ModuleEntry { module: &MODULE }
}

// ==================== WIT 层（import 接口 → 域函数转发） ====================

/// 取本实例的宿主上下文（与内核 `HostCtxOf::host_ctx` 同一转型；类型不符即 panic
/// ——装配期编程错误，fail-visible，不静默降级）
fn ctx_of(state: &WasmPluginState) -> &WasmHostContext {
    downcast_host::<WasmHostContext>(state.host.as_ref())
}

impl bedcode::plugin::host_timer::Host for WasmPluginState {
    fn register(&mut self, interval_secs: u64, command: String) -> Result<(), String> {
        timer_register(ctx_of(self), ctx_of(self), &self.plugin_id, interval_secs, &command)
    }
}

// ==================== 域函数（权限门 + 参数校验 + services 注入；自 `host_api/timer.rs` 迁入） ====================

/// 定时器最小间隔（秒）——防止插件误传 0 导致空转循环
const MIN_TIMER_INTERVAL_SECS: u64 = 1;

/// 注册周期回调（权限 + 参数校验 + services 注入）
///
/// 插件调用后，宿主以 tokio interval 按间隔调用插件指定 command，
/// 参数附带 `now_ms`（Unix 毫秒）与 `now_utc`（UTC "YYYY-MM-DD HH:MM:SS"，
/// 与 SQLite datetime('now') 同格式）。重复注册替换已有定时器。
pub(crate) fn timer_register(
    svc: &dyn ServicesScope,
    perm: &dyn PermissionScope,
    plugin_id: &str,
    interval_secs: u64,
    command: &str,
) -> Result<(), String> {
    if !check_permission(perm, plugin_id, PERMISSION_TIMER, "host_timer_register") {
        return Err("permission denied".to_string());
    }
    if command.is_empty() {
        return Err("timer error: empty command name".to_string());
    }
    let interval = interval_secs.max(MIN_TIMER_INTERVAL_SECS);
    // 两阶段初始化：PluginHost 构造完成后才注入 services
    let services = block_on_async(svc.services())
        .ok_or_else(|| format!("timer error: plugin services not initialized yet for '{}'", plugin_id))?;
    services.register_plugin_timer(plugin_id.to_string(), interval, command.to_string());
    tracing::info!(
        "Plugin timer registered for '{}': interval={}s command={}",
        plugin_id,
        interval,
        command
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bedcode_wasm_core::host_api::grant_permissions;
    use bedcode_wasm_core::test_support::build_host_ctx_at;

    /// 无 timer:schedule 权限：注册被权限门禁拒绝
    #[test]
    fn timer_register_permission_denied() {
        let ctx = build_host_ctx_at(None);
        let err = timer_register(ctx.as_ref(), ctx.as_ref(), "test-plugin", 5, "my.command").unwrap_err();
        assert_eq!(err, "permission denied");
    }

    /// 空 command 名：权限通过后参数校验拒绝（防注册无效定时器空转）
    #[test]
    fn timer_register_empty_command_rejected() {
        let ctx = build_host_ctx_at(None);
        grant_permissions(&ctx, "test-plugin", &[PERMISSION_TIMER]);
        let err = timer_register(ctx.as_ref(), ctx.as_ref(), "test-plugin", 5, "").unwrap_err();
        assert_eq!(err, "timer error: empty command name");
    }

    /// 参数合法但 services 未注入（两阶段初始化完成前）：明确报错而非静默忽略
    #[tokio::test]
    async fn timer_register_services_not_ready() {
        let ctx = build_host_ctx_at(None);
        grant_permissions(&ctx, "test-plugin", &[PERMISSION_TIMER]);
        let err = timer_register(ctx.as_ref(), ctx.as_ref(), "test-plugin", 5, "my.command").unwrap_err();
        assert!(err.contains("not initialized yet"), "got: {}", err);
    }

    // 间隔钳制（interval_secs.max(MIN_TIMER_INTERVAL_SECS)）生效于 services 注入后：
    // 注册的定时器句柄存于 PluginHost，测试环境无 PluginHost 无法观测最终间隔，
    // 交由集成/手动测试覆盖；0 秒钳制为 1 秒的语义由常量注释保证

    // ==================== 白名单 / 自报三件一致（与 crypto/auth 样板同款） ====================

    /// 白名单声明、接口路径、权限位三件与本域常量逐字一致
    #[test]
    fn host_module_declaration_matches_domain_constants() {
        assert!(
            bedcode_host_kit::expected_host_modules().contains(&MODULE_NAME),
            "能力模块白名单缺 {MODULE_NAME}（expect_host_module! 行被删 / 未收集）"
        );
        assert_eq!(MODULE_NAME, "timer", "白名单键即装载期日志与错误文案里的模块名");
        assert_eq!(
            MODULE_INTERFACES,
            &["bedcode:plugin/host-timer"],
            "接口路径必须与 WIT 契约逐字一致（改错即 guest import 失配）"
        );
        assert_eq!(
            MODULE_PERMISSIONS,
            &["timer:schedule"],
            "权限位必须与 `bedcode.wit` / SDK 权限表逐字一致（装载期一致性核对用）"
        );
    }
}
