//! 文件服务通知桥（v2 批量传输请求：后台/锁屏系统通知 + action 按钮）
//!
//! 前台应答走插件前端 Material 对话框；App 在后台/锁屏时，宿主发带
//! 「接受全部 / 拒绝全部」action 按钮的系统通知（Kotlin TaskNotificationManager
//! 扩展），点击经 PendingIntent → MainActivity → WebView evaluateJavascript
//! 路由回宿主命令 `plugin_filesrv_approve_transfer` / `plugin_filesrv_reject_transfer`。
//!
//! 前台/后台判定用窗口焦点事件（Tauri WindowEvent::Focused，Android 上
//! Activity 失焦即后台/锁屏）。通知文本按宿主全局语言偏好（AppConfig.ui.language）
//! 本地化——宿主 Rust 侧无 vue-i18n，直接读配置决定文案语言。

use std::sync::atomic::{AtomicBool, Ordering};

/// 应用是否持有窗口焦点（false = 后台/锁屏；setup 时挂监听，默认 true 防误判）
static APP_FOCUSED: AtomicBool = AtomicBool::new(true);

/// 注册窗口焦点监听（setup 调用一次；Android 上 Activity 失焦即后台/锁屏）
pub fn attach_focus_listener(app: &tauri::AppHandle) {
    use tauri::Manager;
    let Some(window) = app.get_webview_window("main") else {
        tracing::warn!("notify: main window not found, focus tracking disabled");
        return;
    };
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::Focused(focused) = event {
            set_app_focused(*focused);
        }
    });
}

/// 设置焦点状态
pub fn set_app_focused(focused: bool) {
    APP_FOCUSED.store(focused, Ordering::Relaxed);
    tracing::debug!(focused, "app focus changed");
}

/// 应用是否持有窗口焦点（后台/锁屏时 false）
pub fn is_app_focused() -> bool {
    APP_FOCUSED.load(Ordering::Relaxed)
}

/// 批量传输请求系统通知（后台/锁屏；前台由插件前端对话框应答，不打扰）
///
/// 文案按宿主语言偏好本地化（zh-CN 默认 / en）；非 Android 平台静默降级。
/// 通知带「接受全部 / 拒绝全部」action，点击经 Kotlin 路由回宿主命令。
pub async fn show_transfer_request_notification(
    batch_id: &str,
    plugin_id: &str,
    peer_name: &str,
    file_count: usize,
    total_size: u64,
) {
    #[cfg(target_os = "android")]
    {
        use crate::plugin::android_plugins::notification_plugin_handle;

        let Some(handle) = notification_plugin_handle() else {
            tracing::warn!(
                batch_id = %batch_id,
                "show_transfer_request_notification: TaskNotificationPlugin not registered"
            );
            return;
        };
        let (title, body, accept, reject) = localized_batch_texts(peer_name, file_count, total_size);
        let payload = serde_json::json!({
            "batchId": batch_id,
            "pluginId": plugin_id,
            "title": title,
            "body": body,
            "acceptLabel": accept,
            "rejectLabel": reject,
        });
        if let Err(e) = handle
            .run_mobile_plugin_async::<serde_json::Value>("showTransferRequestNotification", payload)
            .await
        {
            tracing::warn!(batch_id = %batch_id, error = %e, "showTransferRequestNotification failed");
        }
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (batch_id, plugin_id, peer_name, file_count, total_size);
    }
}

/// 取消批量请求系统通知（批已解决（resolved）后如通知仍在则移除）
pub async fn cancel_transfer_request_notification(batch_id: &str) {
    #[cfg(target_os = "android")]
    {
        use crate::plugin::android_plugins::notification_plugin_handle;

        let Some(handle) = notification_plugin_handle() else {
            return;
        };
        let payload = serde_json::json!({ "batchId": batch_id });
        if let Err(e) = handle
            .run_mobile_plugin_async::<serde_json::Value>("cancelTransferRequestNotification", payload)
            .await
        {
            tracing::warn!(batch_id = %batch_id, error = %e, "cancelTransferRequestNotification failed");
        }
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = batch_id;
    }
}

/// 按宿主语言偏好生成通知文案（Rust 侧无 vue-i18n，读 AppConfig.ui.language）
///
/// 总大小用宿主侧粗略格式化（MB/GB），按钮文案与插件前端对话框一致
///（transfer.request.acceptAll / rejectAll）
fn localized_batch_texts(
    peer_name: &str,
    file_count: usize,
    total_size: u64,
) -> (String, String, String, String) {
    let en = crate::system::config::AppConfig::global().ui.language == "en";
    let size_label = if total_size > 0 {
        format!(" ({})", format_bytes_rough(total_size))
    } else {
        String::new()
    };
    if en {
        (
            "File transfer request".to_string(),
            format!(
                "{} wants to send you {} file(s){}",
                peer_name, file_count, size_label
            ),
            "Accept all".to_string(),
            "Reject all".to_string(),
        )
    } else {
        (
            "文件传输请求".to_string(),
            format!(
                "{} 想向你发送 {} 个文件{}",
                peer_name, file_count, size_label
            ),
            "接受全部".to_string(),
            "拒绝全部".to_string(),
        )
    }
}

/// 粗略大小展示（宿主侧无 i18n 大小格式化；MB/GB 两级足够通知场景）
fn format_bytes_rough(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{} B", bytes)
    }
}
