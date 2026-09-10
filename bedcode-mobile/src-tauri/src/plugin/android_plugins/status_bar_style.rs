//! 系统状态栏/导航栏图标外观同步插件 Rust 桥接
//!
//! 背景：App 启用 edge-to-edge 后内容绘制在透明系统栏之后，而
//! MainActivity 的 enableEdgeToEdge() 默认只按「设备系统」深浅色决定
//! 系统栏图标外观（SystemBarStyle.auto()）。BedCode 有独立的主题设置
//! （light/dark/system），与设备设置不一致时（典型：设备浅色 + App 夜间
//! 模式）深色背景上仍是深色图标，状态栏时间/日期/电量不可读。
//!
//! 本桥将前端主题变化（useTheme 的 applyTheme / 系统主题跟随）同步到
//! Kotlin StatusBarStylePlugin，由插件设置 WindowInsetsControllerCompat
//! 的图标外观：App 深色 → 浅色图标；App 浅色 → 深色图标。
//!
//! gen/android 重建恢复清单：StatusBarStylePlugin.kt 须恢复。

use std::sync::OnceLock;
use tauri::plugin::{Builder, PluginHandle};

/// 已注册的 StatusBarStylePlugin 句柄（仅 Android 平台使用）
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
static STATUS_BAR_STYLE_HANDLE: OnceLock<PluginHandle<tauri::Wry>> = OnceLock::new();

/// 注册 StatusBarStylePlugin
///
/// gen/android 重建恢复清单：StatusBarStylePlugin.kt 须恢复
pub fn status_bar_style_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    Builder::new("status-bar-style")
        .setup(|_app, api| {
            #[cfg(target_os = "android")]
            {
                let handle =
                    api.register_android_plugin("com.bedcode.mobile", "StatusBarStylePlugin")?;
                let _ = STATUS_BAR_STYLE_HANDLE.set(handle);
            }
            #[cfg(not(target_os = "android"))] // 非 Android 平台消除 unused 警告
            let _ = api;
            Ok(())
        })
        .build()
}

/// 同步系统栏图标外观（幂等）：dark=true（App 深色）→ 浅色图标
#[cfg(target_os = "android")]
pub async fn set_status_bar_style(dark: bool) -> crate::Result<()> {
    let handle = STATUS_BAR_STYLE_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("StatusBarStylePlugin not registered".to_string())
    })?;
    // T 必须显式指定:该调用是语句(返回值被丢弃),若不写 turbofish,
    // 无约束泛型结果只能靠 never type fallback(→ ())兜底,触发
    // dependency_on_unit_never_type_fallback lint(Android target 编译报错)。
    // Kotlin 端 setStyle 为 void 命令,响应为 null,用 Value 兼容任意响应。
    handle
        .run_mobile_plugin_async::<serde_json::Value>(
            "setStyle",
            serde_json::json!({ "dark": dark }),
        )
        .await
        .map_err(|e| crate::AppError::Plugin(format!("Failed to sync status bar style: {}", e)))?;
    Ok(())
}

/// 非 Android 平台无系统栏外观概念，空操作
#[cfg(not(target_os = "android"))]
pub async fn set_status_bar_style(_dark: bool) -> crate::Result<()> {
    Ok(())
}