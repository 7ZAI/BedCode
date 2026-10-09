//! host-notify — 系统通知与提醒反馈（逻辑层，ABI v18）
//!
//! 收编原 `host-events.notify`（v18 破坏性收缩，旧形震动/声音硬编码 true）：
//! 5 原语统一经权限位 `notify`（fail-closed）+ `guarded_host_call` 错误边界；
//! Android 经宿主端口调 TaskNotificationPlugin（`notify_show` /
//! `notify_check_permission` / `notify_request_permission` / `notify_vibrate` /
//! `notify_play_sound`），非 Android（桌面 dev / 无头）返回 Err——与退役前
//! host_notify 同语义。
//!
//! 选项解析为**严格解析**：非法 JSON / 字段类型错误拒绝（不静默降级）；
//! `{ vibrate?, sound? }` 缺省均 true（与退役前 `host-events.notify` 行为一致）。

use super::super::WasmPluginState;
// Android 分支专用（非 Android 平台在进端口前即返回 Err——与退役前同语义）
#[cfg(target_os = "android")]
use super::support::guarded_host_call;
use bedcode_plugin_api_mobile::permission::PERMISSION_NOTIFY;

/// 震动时长钳制边界（毫秒）——域内钳制，宿主与 Kotlin 端不再放宽
const VIBRATE_MIN_MS: u32 = 1;
const VIBRATE_MAX_MS: u32 = 5_000;

/// 权限门（fail-closed，5 原语同门）：未声明 `notify` 一律拒绝
fn require_notify(state: &WasmPluginState) -> Result<(), String> {
    if !state.granted_permissions.contains(PERMISSION_NOTIFY) {
        return Err("permission denied: notify".to_string());
    }
    Ok(())
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

    let parsed: NotifyOptionsWire = serde_json::from_str(options_json)
        .map_err(|e| format!("invalid notify options json: {}", e))?;
    Ok((parsed.vibrate.unwrap_or(true), parsed.sound.unwrap_or(true)))
}

/// 逻辑层：发送系统通知（title/body 原样透传；options 控制震动/声音）
pub(crate) fn notify(
    state: &WasmPluginState,
    title: &str,
    body: &str,
    options_json: &str,
) -> Result<(), String> {
    require_notify(state)?;
    let (vibrate, sound) = parse_notify_options(options_json)?;

    #[cfg(target_os = "android")]
    {
        guarded_host_call(
            &state.plugin_id,
            "host_notify",
            Err("host_notify panicked".to_string()),
            || {
                tokio::task::block_in_place(|| {
                    state
                        .runtime_handle
                        .block_on(state.host_ctx.ports.notify_show(
                            &state.plugin_id,
                            title,
                            body,
                            vibrate,
                            sound,
                        ))
                })
            },
        )
        .map_err(|e| format!("notification failed: {}", e))
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (title, body, vibrate, sound);
        Err("only supported on Android".to_string())
    }
}

/// 逻辑层：通知权限是否已授予（Android 13+ POST_NOTIFICATIONS + 系统通知总开关）
pub(crate) fn notify_check_permission(state: &WasmPluginState) -> Result<bool, String> {
    require_notify(state)?;

    #[cfg(target_os = "android")]
    {
        guarded_host_call(
            &state.plugin_id,
            "host_notify_check_permission",
            Err("host_notify_check_permission panicked".to_string()),
            || {
                tokio::task::block_in_place(|| {
                    state
                        .runtime_handle
                        .block_on(state.host_ctx.ports.notify_check_permission())
                })
            },
        )
        .map_err(|e| format!("notification permission check failed: {}", e))
    }
    #[cfg(not(target_os = "android"))]
    {
        Err("only supported on Android".to_string())
    }
}

/// 逻辑层：请求通知权限（弹系统授权框，阻塞至用户响应）
pub(crate) fn notify_request_permission(state: &WasmPluginState) -> Result<bool, String> {
    require_notify(state)?;

    #[cfg(target_os = "android")]
    {
        guarded_host_call(
            &state.plugin_id,
            "host_notify_request_permission",
            Err("host_notify_request_permission panicked".to_string()),
            || {
                tokio::task::block_in_place(|| {
                    state
                        .runtime_handle
                        .block_on(state.host_ctx.ports.notify_request_permission())
                })
            },
        )
        .map_err(|e| format!("notification permission request failed: {}", e))
    }
    #[cfg(not(target_os = "android"))]
    {
        Err("only supported on Android".to_string())
    }
}

/// 逻辑层：立即震动一次（毫秒；时长钳制到 [1, 5000]，不经通知渠道）
pub(crate) fn notify_vibrate(state: &WasmPluginState, duration_ms: u32) -> Result<(), String> {
    require_notify(state)?;
    let duration_ms = duration_ms.clamp(VIBRATE_MIN_MS, VIBRATE_MAX_MS);

    #[cfg(target_os = "android")]
    {
        guarded_host_call(
            &state.plugin_id,
            "host_notify_vibrate",
            Err("host_notify_vibrate panicked".to_string()),
            || {
                tokio::task::block_in_place(|| {
                    state
                        .runtime_handle
                        .block_on(state.host_ctx.ports.notify_vibrate(duration_ms))
                })
            },
        )
        .map_err(|e| format!("vibrate failed: {}", e))
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = duration_ms;
        Err("only supported on Android".to_string())
    }
}

/// 逻辑层：播放系统默认通知提示音（重复触发先停上一次，避免叠音）
pub(crate) fn notify_play_sound(state: &WasmPluginState) -> Result<(), String> {
    require_notify(state)?;

    #[cfg(target_os = "android")]
    {
        guarded_host_call(
            &state.plugin_id,
            "host_notify_play_sound",
            Err("host_notify_play_sound panicked".to_string()),
            || {
                tokio::task::block_in_place(|| {
                    state
                        .runtime_handle
                        .block_on(state.host_ctx.ports.notify_play_sound())
                })
            },
        )
        .map_err(|e| format!("play sound failed: {}", e))
    }
    #[cfg(not(target_os = "android"))]
    {
        Err("only supported on Android".to_string())
    }
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造最小 `WasmPluginState`（host_impl 是 manager::runtime 的子模块，
    /// 私有字段构造合法；宿主上下文用 test_support 的无头夹具）
    fn state_with(plugin_id: &str, granted: &[&str]) -> WasmPluginState {
        WasmPluginState {
            plugin_id: plugin_id.to_string(),
            host_ctx: crate::test_support::build_host_ctx(),
            runtime_handle: tokio::runtime::Handle::current(),
            granted_permissions: granted.iter().map(|s| s.to_string()).collect(),
            on_message_binary: None,
        }
    }

    /// 权限门 fail-closed：5 原语未授权一律拒绝（拒绝文案逐字锁定）
    #[tokio::test(flavor = "multi_thread")]
    async fn notify_domain_permission_denied_text_unchanged() {
        let state = state_with("test-plugin", &[]);
        assert_eq!(
            notify(&state, "t", "b", "{}").unwrap_err(),
            "permission denied: notify"
        );
        assert_eq!(
            notify_check_permission(&state).unwrap_err(),
            "permission denied: notify"
        );
        assert_eq!(
            notify_request_permission(&state).unwrap_err(),
            "permission denied: notify"
        );
        assert_eq!(
            notify_vibrate(&state, 200).unwrap_err(),
            "permission denied: notify"
        );
        assert_eq!(
            notify_play_sound(&state).unwrap_err(),
            "permission denied: notify"
        );
    }

    /// 授权插件：非 Android（本测试环境）恒 Err「only supported on Android」——
    /// 权限门放行后才可达；证明门在平台分支之前生效
    #[tokio::test(flavor = "multi_thread")]
    async fn notify_domain_authorized_fails_on_non_android_platform() {
        let state = state_with("test-plugin", &[PERMISSION_NOTIFY]);
        assert_eq!(
            notify(&state, "t", "b", "{}").unwrap_err(),
            "only supported on Android"
        );
        assert_eq!(
            notify_vibrate(&state, 200).unwrap_err(),
            "only supported on Android"
        );
    }

    /// 选项严格解析：缺省 true/true、显式覆盖、非法 JSON 拒绝
    #[test]
    fn parse_notify_options_strict() {
        assert_eq!(parse_notify_options("{}").unwrap(), (true, true));
        assert_eq!(
            parse_notify_options(r#"{"vibrate":false}"#).unwrap(),
            (false, true)
        );
        assert_eq!(
            parse_notify_options(r#"{"vibrate":false,"sound":false}"#).unwrap(),
            (false, false)
        );
        // 未知字段按增量演进原则忽略（老端兼容）
        assert_eq!(
            parse_notify_options(r#"{"futureField":1}"#).unwrap(),
            (true, true)
        );
        // 非法 JSON / 字段类型错误：显性拒绝，不静默降级
        assert!(parse_notify_options("").is_err());
        assert!(parse_notify_options("{").is_err());
        assert!(parse_notify_options(r#"{"vibrate":"yes"}"#).is_err());
    }

    /// 非法 options 在权限门之后拒绝（授权插件 → Err 文案带解析上下文）
    #[tokio::test(flavor = "multi_thread")]
    async fn notify_invalid_options_rejected_after_permission_gate() {
        let state = state_with("test-plugin", &[PERMISSION_NOTIFY]);
        let err = notify(&state, "t", "b", "{").unwrap_err();
        assert!(err.contains("invalid notify options json"), "实际: {err}");
    }

    /// 震动时长钳制到 [1, 5000]（域内钳制在本环境不可直测 Android 调用面，
    /// 经常量断言锁定边界）
    #[test]
    fn vibrate_bounds_are_locked() {
        assert_eq!(VIBRATE_MIN_MS, 1);
        assert_eq!(VIBRATE_MAX_MS, 5_000);
        assert_eq!(0u32.clamp(VIBRATE_MIN_MS, VIBRATE_MAX_MS), 1);
        assert_eq!(u32::MAX.clamp(VIBRATE_MIN_MS, VIBRATE_MAX_MS), 5_000);
    }
}
