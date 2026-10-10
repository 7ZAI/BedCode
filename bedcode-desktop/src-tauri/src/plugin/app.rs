//! host-app 宿主侧接线（路径 B：WIT 绑定 + `Host` impl + 域函数 + 自报，四件同处）
//!
//! wasm-core 纯净性收口票 02 批次 05 自内核迁出（`host_api/app.rs` 整文件删除，
//! 内核反向锁 `path_b_domains_must_not_return_to_wasm_core` 防回接）。与 auth /
//! crypto 同款：**没有独立能力 crate**——装配方就是宿主本 crate（孤儿规则见
//! [`super::bindings`] 模块文档）。
//!
//! 应用域宿主实现（随包 CLI 生命周期，v8 host-app）：WASM 插件无注册表/PATH 直接
//! 通道：安装/卸载全部由宿主侧完成（`PluginServices::install_cli/uninstall_cli`
//! 经 PluginHost 实现，见 plugin/host/app_cli.rs）。插件只声明权限并传
//! file_name/bin_dir。

use bedcode_host_kit::ports::downcast_host;
use bedcode_host_kit::{HostModule, HostModuleDesc, ModuleEntry, WasmPluginState};
use bedcode_plugin_api::permission::PERMISSION_APP_CLI;
use bedcode_wasm_core::host_api::check_permission;
use bedcode_wasm_core::host_api::context::{PermissionScope, ServicesScope, WasmHostContext};
use bedcode_wasm_core::runtime_util::block_on_async;

use crate::plugin::bindings::bedcode;

// 能力模块白名单条目（宿主自报；生效白名单 = 内核 IN_CRATE ∪ 宿主自报，见
// `host_module_whitelist`）。路径 B 域的自报静态住在本 crate（宿主 lib 即最终
// 二进制）⇒ 无需能力 crate 那样的 `use <crate> as _;` 强制引用行。
bedcode_host_kit::expect_host_module!(MODULE_NAME);

/// 能力模块名（白名单键即装载期日志与错误文案里的模块名）
pub const MODULE_NAME: &str = "app";

/// 本域提供的 WIT 接口（必须与 `bedcode.wit` 逐字一致；改错即 guest import 失配）
pub const MODULE_INTERFACES: &[&str] = &["bedcode:plugin/host-app"];

/// 本域的权限位（必须与 `bedcode.wit` / SDK 权限表逐字一致）
pub const MODULE_PERMISSIONS: &[&str] = &["app:cli"];

/// 能力模块描述符（机制面：接口路径 / 权限位 / ABI 下界；**禁带产品名词**）
///
/// `abi_min = 8`：host-app 在 ABI v8 引入。
const DESC: HostModuleDesc = HostModuleDesc {
    name: MODULE_NAME,
    interfaces: MODULE_INTERFACES,
    permissions: MODULE_PERMISSIONS,
    abi_min: 8,
};

/// host-app 能力模块
pub struct AppModule;

impl HostModule for AppModule {
    fn desc(&self) -> HostModuleDesc {
        DESC
    }

    fn register(&self, linker: &mut wasmtime::component::Linker<WasmPluginState>) -> wasmtime::Result<()> {
        bedcode::plugin::host_app::add_to_linker::<WasmPluginState, HasSelf>(linker, |s| s)
    }
}

/// getter：让 guest 侧 import 取到可变的状态引用（与内核接线同款）
type HasSelf = wasmtime::component::HasSelf<WasmPluginState>;

/// 静态单例（供 `inventory::submit!` 取址）
static MODULE: AppModule = AppModule;

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

impl bedcode::plugin::host_app::Host for WasmPluginState {
    fn install_cli(&mut self, payload_json: String) -> Result<String, String> {
        install_cli(ctx_of(self), ctx_of(self), &self.plugin_id, &payload_json)
    }

    fn uninstall_cli(&mut self, payload_json: String) -> Result<(), String> {
        uninstall_cli(ctx_of(self), ctx_of(self), &self.plugin_id, &payload_json)
    }

    fn plugin_resource_dir(&mut self) -> Result<String, String> {
        plugin_resource_dir(ctx_of(self), &self.plugin_id)
    }
}

// ==================== 域函数（权限门 + 载荷解析 + 宿主服务执行；自 `host_api/app.rs` 迁入） ====================

/// 安装 CLI（权限 + 载荷解析 + 宿主服务执行），返回 bin 目录绝对路径
///
/// payload: `{ "file_name": "bedtask", "bin_dir": "" }` —— file_name 缺省
/// "bedtask"（Windows 自动补 .exe）；bin_dir 为空用平台默认。
pub(crate) fn install_cli(
    svc: &dyn ServicesScope,
    perm: &dyn PermissionScope,
    plugin_id: &str,
    payload_json: &str,
) -> Result<String, String> {
    if !check_permission(perm, plugin_id, PERMISSION_APP_CLI, "host_app_install_cli") {
        return Err("permission denied".to_string());
    }
    let payload: serde_json::Value =
        serde_json::from_str(payload_json).map_err(|e| format!("app error: invalid payload JSON: {}", e))?;
    let file_name = payload
        .get("file_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let bin_dir = payload
        .get("bin_dir")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let services = block_on_async(svc.services()).ok_or_else(|| "app error: host services unavailable".to_string())?;
    block_on_async(services.install_cli(plugin_id.to_string(), file_name, bin_dir))
}

/// 卸载 CLI（权限 + 载荷解析 + 宿主服务执行）
pub(crate) fn uninstall_cli(
    svc: &dyn ServicesScope,
    perm: &dyn PermissionScope,
    plugin_id: &str,
    payload_json: &str,
) -> Result<(), String> {
    if !check_permission(perm, plugin_id, PERMISSION_APP_CLI, "host_app_uninstall_cli") {
        return Err("permission denied".to_string());
    }
    let payload: serde_json::Value =
        serde_json::from_str(payload_json).map_err(|e| format!("app error: invalid payload JSON: {}", e))?;
    let file_name = payload
        .get("file_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let bin_dir = payload
        .get("bin_dir")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let services = block_on_async(svc.services()).ok_or_else(|| "app error: host services unavailable".to_string())?;
    block_on_async(services.uninstall_cli(plugin_id.to_string(), file_name, bin_dir))
}

/// 插件自身资源目录（v25 函数级追加，**无权限门**——只返回调用方自己的安装路径）
///
/// 会话创建编排移交插件后（session-engine-downsink P1-b）宿主不再产生 `Creating`
/// 生命周期事件，本原语成为插件取自身资源目录的唯一途径（Agent 集成 hook 脚本源
/// 位于该目录）。未加载的插件 / 服务不可用 → `Err`（不静默返回空串）。
pub(crate) fn plugin_resource_dir(svc: &dyn ServicesScope, plugin_id: &str) -> Result<String, String> {
    let services = block_on_async(svc.services()).ok_or_else(|| "app error: host services unavailable".to_string())?;
    block_on_async(services.plugin_resource_dir(plugin_id.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bedcode_wasm_core::host_api::grant_permissions;
    use bedcode_wasm_core::test_support::build_host_ctx_at;

    const PLUGIN: &str = "test-plugin";

    /// 无 app:cli 权限：install/uninstall 被拒绝
    #[test]
    fn cli_permission_denied() {
        let ctx = build_host_ctx_at(None);
        let err = install_cli(ctx.as_ref(), ctx.as_ref(), PLUGIN, "{}").unwrap_err();
        assert_eq!(err, "permission denied");
        let err = uninstall_cli(ctx.as_ref(), ctx.as_ref(), PLUGIN, "{}").unwrap_err();
        assert_eq!(err, "permission denied");
    }

    /// 有权限但载荷畸形：拒绝
    #[test]
    fn cli_invalid_payload_rejected() {
        let ctx = build_host_ctx_at(None);
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_APP_CLI]);
        let err = install_cli(ctx.as_ref(), ctx.as_ref(), PLUGIN, "not-json").unwrap_err();
        assert!(err.contains("invalid payload"), "got: {}", err);
    }

    /// 有权限、载荷合法但 services 为 None（测试上下文）：报服务不可用
    #[tokio::test]
    async fn cli_services_unavailable_in_test_ctx() {
        let ctx = build_host_ctx_at(None);
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_APP_CLI]);
        let err = install_cli(ctx.as_ref(), ctx.as_ref(), PLUGIN, r#"{"file_name":"bedtask"}"#).unwrap_err();
        assert!(err.contains("services unavailable"), "got: {}", err);
    }

    /// 资源目录**无权限门**：未授予任何权限的插件也拿不到「permission denied」
    ///
    /// 设计口径（同 `host-platform`）：本原语只返回调用方自己的安装路径、不含跨
    /// 插件信息，没有可授予的权力——加门只会造出一个恒过的死门。本用例锁住该口径：
    /// 若日后有人补上权限检查，这里会转红并迫使其回到裁剪线论证。
    #[tokio::test]
    async fn plugin_resource_dir_has_no_permission_gate() {
        let ctx = build_host_ctx_at(None);
        let err = plugin_resource_dir(ctx.as_ref(), PLUGIN).unwrap_err();
        assert!(!err.contains("permission denied"), "资源目录不得设权限门，got: {err}");
        assert!(err.contains("services unavailable"), "got: {}", err);
    }

    // ==================== 白名单 / 自报三件一致（与 crypto/auth 样板同款） ====================

    /// 白名单声明、接口路径、权限位三件与本域常量逐字一致
    #[test]
    fn host_module_declaration_matches_domain_constants() {
        assert!(
            bedcode_host_kit::expected_host_modules().contains(&MODULE_NAME),
            "能力模块白名单缺 {MODULE_NAME}（expect_host_module! 行被删 / 未收集）"
        );
        assert_eq!(MODULE_NAME, "app", "白名单键即装载期日志与错误文案里的模块名");
        assert_eq!(
            MODULE_INTERFACES,
            &["bedcode:plugin/host-app"],
            "接口路径必须与 WIT 契约逐字一致（改错即 guest import 失配）"
        );
        assert_eq!(
            MODULE_PERMISSIONS,
            &["app:cli"],
            "权限位必须与 `bedcode.wit` / SDK 权限表逐字一致（装载期一致性核对用）"
        );
    }
}
