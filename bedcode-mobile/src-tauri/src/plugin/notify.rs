//! host-notify / host-notify-mobile 宿主 adapter（移动端；wasm-core-mobile-purity 票 04 自 wasm-core 迁入）
//!
//! ## 两个 interface 的分工（2026-10-13 功能级统一，ADR 0052 D3）
//!
//! - **`host-notify`（共享域契约，真源 `packages/bedcode-notify-engine/wit/notify.wit`）**：
//!   `notify` 本体是双端功能级交集 → 统一签名 `notify(title, body, options-json)`，
//!   双端 caps 各消费同一份契约。移动端就地在 `-mobile` 模块内实现（同一份
//!   Android 桥，行为与统一前逐字一致）。
//! - **`host-notify-mobile`（移动扩展，cap-mobile）**：权限查询 / 请求、即时震动、
//!   提示音 4 条——桌面无对应平台概念，不归交集。
//!
//! 两个 interface 共用一个能力模块与权限位 `notify`（fail-closed，**门在平台分支
//! 之前**——非 Android 的「only supported on Android」不得先于权限拒绝暴露）；
//! Android 经 TaskNotificationPlugin（`crate::plugin::host_ports::HostPorts` 的
//! notify 面）；非 Android 返回 Err（与迁入前同语义）。
//!
//! 选项解析为**严格解析**：非法 JSON / 字段类型错误拒绝（不静默降级）；
//! `{ vibrate?, sound? }` 缺省均 true；震动时长域内钳制 [1, 5000]。

use bedcode_host_kit::{HostModule, HostModuleDesc, ModuleEntry, WasmPluginState};
use bedcode_plugin_api_mobile::permission::PERMISSION_NOTIFY;

use crate::plugin::bindings::{bedcode, check_permission};

bedcode_host_kit::expect_host_module!(MODULE_NAME);

/// 能力模块名（白名单键 = 装载期日志与错误文案里的模块名）
pub const MODULE_NAME: &str = "mobile-notify";

/// 本域提供的 WIT 接口（必须与生成物 `cap-notify.wit` + `cap-mobile.wit` 逐字一致；
/// 改错即 guest import 失配）
pub const MODULE_INTERFACES: &[&str] = &[
    "bedcode:plugin@0.2.0/host-notify",
    "bedcode:plugin@0.2.0/host-notify-mobile",
];

/// 本域的权限位（两个 interface 同门；必须与 WIT / SDK 权限表逐字一致）
pub const MODULE_PERMISSIONS: &[&str] = &["notify"];

/// 能力模块描述符（机制面：接口路径 / 权限位 / ABI 下界；禁带产品名词）
const DESC: HostModuleDesc = HostModuleDesc {
    name: MODULE_NAME,
    interfaces: MODULE_INTERFACES,
    permissions: MODULE_PERMISSIONS,
    abi_min: 18,
};

/// host-notify / host-notify-mobile 能力模块
pub struct NotifyModule;

impl HostModule for NotifyModule {
    fn desc(&self) -> HostModuleDesc {
        DESC
    }

    fn register(&self, linker: &mut wasmtime::component::Linker<WasmPluginState>) -> wasmtime::Result<()> {
        bedcode::plugin::host_notify::add_to_linker::<WasmPluginState, HasSelf>(linker, |s| s)?;
        bedcode::plugin::host_notify_mobile::add_to_linker::<WasmPluginState, HasSelf>(linker, |s| s)
    }
}

/// getter：让 guest 侧 import 取到可变的状态引用（与 wasm-core 接线同款）
type HasSelf = wasmtime::component::HasSelf<WasmPluginState>;

/// 静态单例（供 `inventory::submit!` 取址）
static MODULE: NotifyModule = NotifyModule;

// 能力模块自报（自报静态住在宿主 lib = 最终二进制 ⇒ 无需强制引用行）
inventory::submit! {
    ModuleEntry { module: &MODULE }
}

// ==================== 域逻辑 ====================

/// 震动时长钳制边界（毫秒）——域内钳制，宿主与 Kotlin 端不再放宽
const VIBRATE_MIN_MS: u32 = 1;
const VIBRATE_MAX_MS: u32 = 5_000;

/// 权限门（fail-closed，两个 interface 同门）：未声明 `notify` 一律拒绝
fn require_notify(state: &WasmPluginState) -> Result<(), String> {
    if check_permission(state, PERMISSION_NOTIFY, "host_notify") {
        Ok(())
    } else {
        Err("permission denied: notify".to_string())
    }
}

/// 解析 options-json（`{ vibrate?: bool, sound?: bool }` → 缺省 true/true）
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

// ==================== WIT 层（import 接口 → 域逻辑） ====================

/// 共享 `host-notify`：notify 本体（与桌面同一份契约）
impl bedcode::plugin::host_notify::Host for WasmPluginState {
    fn notify(&mut self, title: String, body: String, options_json: String) -> Result<(), String> {
        require_notify(self)?;
        let (vibrate, sound) = parse_notify_options(&options_json)?;
        let plugin_id = self.plugin_id.clone();

        #[cfg(target_os = "android")]
        {
            guarded(
                &plugin_id,
                "host_notify",
                Err("host_notify panicked".to_string()),
                || {
                    let ports = crate::plugin::host_ports::HostPorts;
                    block_on_async(ports.notify_show(&plugin_id, &title, &body, vibrate, sound))
                        .map_err(|e| format!("notification failed: {}", e))
                },
            )
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = (plugin_id, title, body, vibrate, sound);
            Err("only supported on Android".to_string())
        }
    }
}

/// 移动扩展 `host-notify-mobile`：权限查询 / 请求、震动、提示音
impl bedcode::plugin::host_notify_mobile::Host for WasmPluginState {
    fn check_permission(&mut self) -> Result<bool, String> {
        require_notify(self)?;
        let plugin_id = self.plugin_id.clone();

        #[cfg(target_os = "android")]
        {
            guarded(
                &plugin_id,
                "host_notify_check_permission",
                Err("host_notify_check_permission panicked".to_string()),
                || {
                    let ports = crate::plugin::host_ports::HostPorts;
                    block_on_async(ports.notify_check_permission())
                        .map_err(|e| format!("notification permission check failed: {}", e))
                },
            )
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = plugin_id;
            Err("only supported on Android".to_string())
        }
    }

    fn request_permission(&mut self) -> Result<bool, String> {
        require_notify(self)?;
        let plugin_id = self.plugin_id.clone();

        #[cfg(target_os = "android")]
        {
            guarded(
                &plugin_id,
                "host_notify_request_permission",
                Err("host_notify_request_permission panicked".to_string()),
                || {
                    let ports = crate::plugin::host_ports::HostPorts;
                    block_on_async(ports.notify_request_permission())
                        .map_err(|e| format!("notification permission request failed: {}", e))
                },
            )
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = plugin_id;
            Err("only supported on Android".to_string())
        }
    }

    fn vibrate(&mut self, duration_ms: u32) -> Result<(), String> {
        require_notify(self)?;
        let duration_ms = duration_ms.clamp(VIBRATE_MIN_MS, VIBRATE_MAX_MS);
        let plugin_id = self.plugin_id.clone();

        #[cfg(target_os = "android")]
        {
            guarded(
                &plugin_id,
                "host_notify_vibrate",
                Err("host_notify_vibrate panicked".to_string()),
                || {
                    let ports = crate::plugin::host_ports::HostPorts;
                    block_on_async(ports.notify_vibrate(duration_ms)).map_err(|e| format!("vibrate failed: {}", e))
                },
            )
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = (plugin_id, duration_ms);
            Err("only supported on Android".to_string())
        }
    }

    fn play_sound(&mut self) -> Result<(), String> {
        require_notify(self)?;
        let plugin_id = self.plugin_id.clone();

        #[cfg(target_os = "android")]
        {
            guarded(
                &plugin_id,
                "host_notify_play_sound",
                Err("host_notify_play_sound panicked".to_string()),
                || {
                    let ports = crate::plugin::host_ports::HostPorts;
                    block_on_async(ports.notify_play_sound()).map_err(|e| format!("play sound failed: {}", e))
                },
            )
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = plugin_id;
            Err("only supported on Android".to_string())
        }
    }
}
