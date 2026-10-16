//! host-notify 宿主侧接线（路径 B：WIT 绑定 + `Host` impl + 域函数同处）
//!
//! 2026-10-13 功能级统一（ADR 0052 D3 / domain-owned-contracts 票 04）：桌面
//! `host-events-desktop.notify(title, body)` 与移动
//! `host-notify.notify(title, body, options-json)` 是同一功能 → 统一为共享域契约
//! `host-notify`（真源 `packages/bedcode-notify-engine/wit/notify.wit`，双端 caps
//! 直接消费）。本文件是桌面端的实现：经 Tauri 事件发前端 toast（`plugin:notify`
//! 事件，payload 与迁移前逐字一致）。
//!
//! 权限：`notify` **无权限门**（与迁移前 `host-events-desktop.notify` /
//! 宿主命令面 `plugin_notify` 口径逐字一致——弹提示是插件常规能力，不构成资源面）；
//! 移动端同契约的实现在其 `plugin/notify.rs`，那边有 `notify` 权限门（ADR 0018
//! 移动独立契约），门在各端实现内，契约面不规定。
//!
//! options-json 与移动端同口径**严格解析**（非法 JSON / 字段类型错误 → Err，禁静默
//! 降级成默认值），但取值在桌面**静默忽略**（无震动 / 提示音通道）——契约文档已声明
//! 「平台不支持的选项静默忽略」；报错会让只想发通知的桌面调用点无谓失败。

use bedcode_host_kit::ports::downcast_host;
use bedcode_host_kit::{HostModule, HostModuleDesc, ModuleEntry, WasmPluginState};
use bedcode_wasm_core::host_api::context::WasmHostContext;
use tauri::Emitter;

use crate::plugin::bindings::bedcode;

// 能力模块白名单条目（宿主自报；生效白名单 = 内核 IN_CRATE ∪ 宿主自报）。路径 B 域
// 的自报静态住在本 crate（宿主 lib 即最终二进制）⇒ 无需强制引用行。
bedcode_host_kit::expect_host_module!(MODULE_NAME);

/// 能力模块名（白名单键即装载期日志与错误文案里的模块名）
pub const MODULE_NAME: &str = "notify";

/// 本域提供的 WIT 接口（必须与 `cap-notify.wit` 逐字一致；改错即 guest import 失配）
pub const MODULE_INTERFACES: &[&str] = &["bedcode:plugin@0.2.0/host-notify"];

/// 本域的权限位（`notify` 无权限门 ⇒ 空清单）
pub const MODULE_PERMISSIONS: &[&str] = &[];

/// 能力模块描述符（机制面：接口路径 / 权限位 / ABI 下界；**禁带产品名词**）
///
/// `abi_min = 38`：统一 `host-notify` 契约随 WIT 包 0.2.0 引入（ADR 0051 后整数
/// ABI 已退役，此处取桌面退役前最后一版号，仅作历史下界元数据）。
const DESC: HostModuleDesc = HostModuleDesc {
    name: MODULE_NAME,
    interfaces: MODULE_INTERFACES,
    permissions: MODULE_PERMISSIONS,
    abi_min: 38,
};

/// host-notify 能力模块
pub struct NotifyModule;

impl HostModule for NotifyModule {
    fn desc(&self) -> HostModuleDesc {
        DESC
    }

    fn register(&self, linker: &mut wasmtime::component::Linker<WasmPluginState>) -> wasmtime::Result<()> {
        bedcode::plugin::host_notify::add_to_linker::<WasmPluginState, HasSelf>(linker, |s| s)
    }
}

/// getter：让 guest 侧 import 取到可变的状态引用（与内核接线同款）
type HasSelf = wasmtime::component::HasSelf<WasmPluginState>;

/// 静态单例（供 `inventory::submit!` 取址）
static MODULE: NotifyModule = NotifyModule;

// 能力模块自报（linker-section 静态；收集点在 host-kit）
inventory::submit! {
    ModuleEntry { module: &MODULE }
}

// ==================== WIT 层（import 接口 → 域函数转发） ====================

impl bedcode::plugin::host_notify::Host for WasmPluginState {
    fn notify(&mut self, title: String, body: String, options_json: String) -> Result<(), String> {
        // 严格解析先于投递：非法 options 与「忽略取值」是两件事——前者是调用方
        // 编程错误（必须可见），后者是平台差异（静默忽略，契约已声明）。
        parse_notify_options(&options_json)?;
        notify_desktop(ctx_of(self), &self.plugin_id, &title, &body)
    }
}

/// guest 侧 import 取到 store state 里的宿主上下文（孤儿规则要求装配方自带
/// bindgen，`context` 由 host-kit 的 HostPorts 擦除面持有，此处向下转型还原）
fn ctx_of(state: &WasmPluginState) -> &WasmHostContext {
    downcast_host::<WasmHostContext>(state.host.as_ref())
}

// ==================== 域函数 ====================

/// 解析 options-json（与移动端同口径）：`{ vibrate?: bool, sound?: bool }` →
/// 缺省 true/true。桌面只做校验，取值在投递端静默忽略。
fn parse_notify_options(options_json: &str) -> Result<(bool, bool), String> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct NotifyOptionsWire {
        #[serde(default)]
        vibrate: Option<bool>,
        #[serde(default)]
        sound: Option<bool>,
    }

    let parsed: NotifyOptionsWire =
        serde_json::from_str(options_json).map_err(|e| format!("invalid notify options json: {}", e))?;
    Ok((parsed.vibrate.unwrap_or(true), parsed.sound.unwrap_or(true)))
}

/// 通过 Tauri 事件发送前端 toast（`plugin:notify` 事件）
///
/// 文案与弹窗强需求语义随迁**逐字保留**（v36 自 wasm-core `host_api/events.rs`
/// 迁移、2026-10-13 统一契约后仍同）：无头上下文（测试 / 无 AppHandle）**显性
/// 报错**——弹窗是强需求能力，降级成「假装已弹」会让插件误以为用户看到提示；
/// 与 `emit` 的无头幂等约定刻意不同（两处契约别改成一致）。
fn notify_desktop(app: &WasmHostContext, plugin_id: &str, title: &str, body: &str) -> Result<(), String> {
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
        let err = notify_desktop(ctx.as_ref(), "test-plugin", "title", "body").unwrap_err();
        assert!(err.contains("app_handle not available"), "got: {err}");
    }

    /// options 严格解析：非法 JSON 拒绝（不静默降级成默认值）
    #[test]
    fn invalid_options_json_rejected() {
        let err = parse_notify_options("{not json").unwrap_err();
        assert!(err.contains("invalid notify options json"), "got: {err}");
        // 字段类型错误同样拒绝（vibrate 是字符串）
        let err = parse_notify_options(r#"{"vibrate":"yes"}"#).unwrap_err();
        assert!(err.contains("invalid notify options json"), "got: {err}");
    }

    /// options 缺省与显式取值：缺省 true/true（与移动端一致）；桌面只校验不消费
    #[test]
    fn options_defaults_are_true_true() {
        assert_eq!(parse_notify_options("{}").unwrap(), (true, true));
        assert_eq!(parse_notify_options(r#"{"vibrate":false}"#).unwrap(), (false, true));
        assert_eq!(
            parse_notify_options(r#"{"vibrate":false,"sound":false}"#).unwrap(),
            (false, false)
        );
        // 空串不是合法 JSON ⇒ Err（协议面由 SDK 恒发 `{}` 或对象字面量）
        assert!(parse_notify_options("").is_err());
    }

    /// 白名单三件一致：期望模块名 / 描述符与 WIT 接口、权限清单逐字一致
    /// （与 crypto / auth 样板同款）
    #[test]
    fn host_module_declaration_matches_domain_constants() {
        assert!(
            bedcode_host_kit::expected_host_modules().contains(&MODULE_NAME),
            "能力模块白名单缺 {MODULE_NAME}（expect_host_module! 行被删 / 未收集）"
        );
        assert_eq!(MODULE_NAME, "notify", "白名单键即装载期日志与错误文案里的模块名");
        assert_eq!(
            MODULE_INTERFACES,
            &["bedcode:plugin@0.2.0/host-notify"],
            "接口路径必须与 WIT 契约逐字一致（改错即 guest import 失配）"
        );
        assert_eq!(MODULE_PERMISSIONS, &[] as &[&str], "notify 无权限门 ⇒ 空清单");
        assert_eq!(DESC.name, MODULE_NAME);
        assert_eq!(DESC.interfaces, MODULE_INTERFACES);
        assert_eq!(DESC.permissions, MODULE_PERMISSIONS);
    }
}
