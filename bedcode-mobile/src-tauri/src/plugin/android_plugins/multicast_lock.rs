//! 多播锁插件（MulticastLockPlugin）Rust 桥接
//!
//! Android 默认过滤多播 UDP 包，不持 MulticastLock 时 mDNS 响应收不到（广播
//! 不受影响）——对等网络发现守护启动前必须先 acquire（issue 03 决策 D6）。
//! 锁生命周期 = 从守护启动到显式 release，不绑 Activity 生命周期（后台可见性）。
//!
//! gen/android 重建恢复清单：MulticastLockPlugin.kt 须恢复。

use std::sync::OnceLock;
use tauri::plugin::{Builder, PluginHandle};

/// 已注册的 MulticastLockPlugin 句柄（仅 Android 平台使用）
static MULTICAST_LOCK_HANDLE: OnceLock<PluginHandle<tauri::Wry>> = OnceLock::new();

/// 注册 MulticastLockPlugin（幂等 acquire/release 的多播锁持有者）
///
/// gen/android 重建恢复清单：MulticastLockPlugin.kt 须恢复
pub fn multicast_lock_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    Builder::new("multicast-lock")
        .setup(|_app, api| {
            #[cfg(target_os = "android")]
            {
                let handle =
                    api.register_android_plugin("com.bedcode.mobile", "MulticastLockPlugin")?;
                let _ = MULTICAST_LOCK_HANDLE.set(handle);
            }
            #[cfg(not(target_os = "android"))] // 非 Android 平台消除 unused 警告
            let _ = api;
            Ok(())
        })
        .build()
}

/// 申请多播锁，返回是否持锁（幂等：已持锁直接返回 true）
#[cfg(target_os = "android")]
pub async fn multicast_lock_acquire() -> crate::Result<bool> {
    let handle = MULTICAST_LOCK_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("MulticastLockPlugin not registered".to_string())
    })?;
    let response: serde_json::Value = handle
        .run_mobile_plugin_async("acquire", serde_json::json!({}))
        .await
        .map_err(|e| crate::AppError::Plugin(format!("Failed to acquire multicast lock: {}", e)))?;
    Ok(response
        .get("held")
        .and_then(|v| v.as_bool())
        .unwrap_or(false))
}

/// 非 Android 平台无多播过滤问题，锁为空操作（返回 false 表示未持任何系统锁）
#[cfg(not(target_os = "android"))]
pub async fn multicast_lock_acquire() -> crate::Result<bool> {
    Ok(false)
}

/// 释放多播锁，返回释放后状态 false（幂等：未持锁同样成功）
#[cfg(target_os = "android")]
pub async fn multicast_lock_release() -> crate::Result<bool> {
    let handle = MULTICAST_LOCK_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("MulticastLockPlugin not registered".to_string())
    })?;
    let response: serde_json::Value = handle
        .run_mobile_plugin_async("release", serde_json::json!({}))
        .await
        .map_err(|e| crate::AppError::Plugin(format!("Failed to release multicast lock: {}", e)))?;
    Ok(response
        .get("held")
        .and_then(|v| v.as_bool())
        .unwrap_or(false))
}

#[cfg(not(target_os = "android"))]
pub async fn multicast_lock_release() -> crate::Result<bool> {
    Ok(false)
}
