//! host-events-desktop（通知）宿主侧接线（路径 B：WIT 绑定 + `Host` impl + 域函数同处）
//!
//! v36 交集接口切片（票 02 批次 06）：`host-events.notify` 拆入
//! `host-events-desktop`（桌面独有；移动端收编在 `host-notify`，不跟演）。WIT
//! impl 落本文件——域函数随 impl 同行（内核 `host_api/events.rs` 只剩交集
//! `emit_event`，其实现也不带通知面）。
//!
//! 权限：`notify` **无权限门**（与迁移前 `host_api/events.rs::notify` 逐字一致，
//! 也随宿主命令面 `plugin_notify` 口径——弹提示是插件常规能力，不构成资源面）。

use bedcode_host_kit::ports::downcast_host;
use bedcode_host_kit::{HostModule, HostModuleDesc, ModuleEntry, WasmPluginState};
use bedcode_wasm_core::host_api::context::WasmHostContext;
use tauri::Emitter;

use crate::plugin::bindings::bedcode;

// 能力模块白名单条目（宿主自报；生效白名单 = 内核 IN_CRATE ∪ 宿主自报）。路径 B 域
// 的自报静态住在本 crate（宿主 lib 即最终二进制）⇒ 无需强制引用行。
bedcode_host_kit::expect_host_module!(MODULE_NAME);

/// 能力模块名（白名单键即装载期日志与错误文案里的模块名）
pub const MODULE_NAME: &str = "events-desktop";

/// 本域提供的 WIT 接口（必须与 `bedcode.wit` 逐字一致；改错即 guest import 失配）
pub const MODULE_INTERFACES: &[&str] = &["bedcode:plugin/host-events-desktop"];

/// 本域的权限位（`notify` 无权限门 ⇒ 空清单）
pub const MODULE_PERMISSIONS: &[&str] = &[];

/// 能力模块描述符（机制面：接口路径 / 权限位 / ABI 下界；**禁带产品名词**）
///
/// `abi_min = 36`：`host-events-desktop` 在 ABI v36（交集接口切片）引入。
const DESC: HostModuleDesc = HostModuleDesc {
    name: MODULE_NAME,
    interfaces: MODULE_INTERFACES,
    permissions: MODULE_PERMISSIONS,
    abi_min: 36,
};

/// host-events-desktop 能力模块
pub struct EventsDesktopModule;

impl HostModule for EventsDesktopModule {
    fn desc(&self) -> HostModuleDesc {
        DESC
    }

    fn register(&self, linker: &mut wasmtime::component::Linker<WasmPluginState>) -> wasmtime::Result<()> {
        bedcode::plugin::host_events_desktop::add_to_linker::<WasmPluginState, HasSelf>(linker, |s| s)
    }
}

/// getter：让 guest 侧 import 取到可变的状态引用（与内核接线同款）
type HasSelf = wasmtime::component::HasSelf<WasmPluginState>;

/// 静态单例（供 `inventory::submit!` 取址）
static MODULE: EventsDesktopModule = EventsDesktopModule;

// 能力模块自报（linker-section 静态；收集点在 host-kit）
inventory::submit! {
    ModuleEntry { module: &MODULE }
}

// ==================== WIT 层（import 接口 → 域函数转发） ====================

impl bedcode::plugin::host_events_desktop::Host for WasmPluginState {
    fn notify(&mut self, title: String, body: String) -> Result<(), String> {
        events_desktop_notify(ctx_of(self), &self.plugin_id, &title, &body)
    }
}

/// guest 侧 import 取到 store state 里的宿主上下文（孤儿规则要求装配方自带
/// bindgen，`context` 由 host-kit 的 HostPorts 擦除面持有，此处向下转型还原）
fn ctx_of(state: &WasmPluginState) -> &WasmHostContext {
    downcast_host::<WasmHostContext>(state.host.as_ref())
}

// ==================== 域函数 ====================

/// 通过 Tauri 事件发送前端 toast（`plugin:notify` 事件）
///
/// 文案与弹窗强需求语义随迁**逐字保留**（v36 自 wasm-core `host_api/events.rs`
/// 迁移）：无头上下文（测试 / 无 AppHandle）**显性报错**——弹窗是强需求能力，
/// 降级成「假装已弹」会让插件误以为用户看到提示；与 `emit` 的无头幂等约定
/// 刻意不同（两处契约别改成一致）。
fn events_desktop_notify(app: &WasmHostContext, plugin_id: &str, title: &str, body: &str) -> Result<(), String> {
    let Some(app_handle) = app.app_handle() else {
        return Err("notify error: app_handle not available in headless context".to_string());
    };
    app_handle
        .emit(
            "plugin:notify",
            serde_json::json!({
                "plugin_id": plugin_id,
                "title": title,
                "body": body,
            }),
        )
        .map_err(|e| format!("notify error: emit failed: {}", e))
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use bedcode_wasm_core::test_support::build_host_ctx_at;

    /// 无头上下文（AppHandle=None）：notify 显性报错（与 emit 的幂等约定不同，
    /// 弹窗是强需求能力——迁移前内核同款语义逐字保留）
    #[test]
    fn notify_headless_rejected() {
        let ctx = build_host_ctx_at(None);
        let err = events_desktop_notify(ctx.as_ref(), "test-plugin", "title", "body").unwrap_err();
        assert!(err.contains("app_handle not available"), "got: {err}");
    }

    /// 白名单三件一致：期望模块名 / 描述符与 WIT 接口、权限清单逐字一致
    /// （与 crypto / auth 样板同款）
    #[test]
    fn host_module_declaration_matches_domain_constants() {
        assert!(
            bedcode_host_kit::expected_host_modules().contains(&MODULE_NAME),
            "能力模块白名单缺 {MODULE_NAME}（expect_host_module! 行被删 / 未收集）"
        );
        assert_eq!(
            MODULE_NAME, "events-desktop",
            "白名单键即装载期日志与错误文案里的模块名"
        );
        assert_eq!(
            MODULE_INTERFACES,
            &["bedcode:plugin/host-events-desktop"],
            "接口路径必须与 WIT 契约逐字一致（改错即 guest import 失配）"
        );
        assert_eq!(MODULE_PERMISSIONS, &[] as &[&str], "notify 无权限门 ⇒ 空清单");
        assert_eq!(DESC.name, MODULE_NAME);
        assert_eq!(DESC.interfaces, MODULE_INTERFACES);
        assert_eq!(DESC.permissions, MODULE_PERMISSIONS);
    }
}
