//! Power Manager
//!
//! 管理系统休眠阻止 - 服务器运行时阻止系统进入休眠状态
//! 使用 nosleep crate 跨平台实现（Windows: PowerCreateRequest, macOS: IOPMAssertion, Linux: systemd-inhibit）

use nosleep::{NoSleep, NoSleepType};
use std::sync::Mutex;

/// 休眠阻止管理器
///
/// 服务器运行时调用 enable() 阻止系统休眠，停止时调用 disable() 释放
/// 内部使用 std::sync::Mutex 保护 NoSleep 实例（NoSleep 包含平台原生句柄）
pub struct PowerManager {
    inner: Mutex<PowerManagerInner>,
    /// 用户设置开关，false 时不阻止休眠
    enabled: std::sync::atomic::AtomicBool,
}

/// 内部状态
struct PowerManagerInner {
    nosleep: Option<NoSleep>,
    active: bool,
}

impl PowerManager {
    /// 创建新的 PowerManager 实例
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(PowerManagerInner {
                nosleep: None,
                active: false,
            }),
            enabled: std::sync::atomic::AtomicBool::new(true),
        }
    }

    /// 设置用户偏好开关
    pub fn set_enabled(&self, enabled: bool) {
        let was_enabled = self.enabled.swap(enabled, std::sync::atomic::Ordering::SeqCst);

        if was_enabled && !enabled {
            // 用户关闭了阻止休眠功能，立即释放当前锁
            self.disable();
        }
    }

    /// 获取用户偏好开关状态
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// 阻止系统休眠
    ///
    /// 服务器启动时调用，阻止显示器休眠和系统休眠
    /// 如果用户设置关闭了此功能，则不执行任何操作
    pub fn enable(&self) {
        if !self.is_enabled() {
            tracing::debug!("PowerManager: prevent_sleep disabled by user setting, skipping");
            return;
        }

        let mut inner = self.inner.lock().unwrap();
        if inner.active {
            tracing::debug!("PowerManager: already active, skipping");
            return;
        }

        // 延迟初始化 NoSleep 实例
        if inner.nosleep.is_none() {
            match NoSleep::new() {
                Ok(ns) => inner.nosleep = Some(ns),
                Err(e) => {
                    tracing::error!("PowerManager: failed to initialize NoSleep: {}", e);
                    return;
                }
            }
        }

        if let Some(ref mut ns) = inner.nosleep {
            // PreventUserIdleDisplaySleep 同时阻止显示器休眠和系统休眠
            // 服务器需要保持网络连接，显示器也需要保持唤醒以显示状态
            match ns.start(NoSleepType::PreventUserIdleDisplaySleep) {
                Ok(()) => {
                    inner.active = true;
                    tracing::info!("PowerManager: system sleep prevention enabled");
                }
                Err(e) => {
                    tracing::error!("PowerManager: failed to prevent display sleep: {}", e);
                }
            }
        }
    }

    /// 释放休眠阻止
    ///
    /// 服务器停止时调用，恢复系统正常休眠行为
    pub fn disable(&self) {
        let mut inner = self.inner.lock().unwrap();
        if !inner.active {
            return;
        }

        if let Some(ref ns) = inner.nosleep {
            match ns.stop() {
                Ok(()) => {
                    inner.active = false;
                    tracing::info!("PowerManager: system sleep prevention disabled");
                }
                Err(e) => {
                    tracing::error!("PowerManager: failed to release sleep prevention: {}", e);
                }
            }
        }
    }

    /// 当前是否正在阻止休眠
    pub fn is_active(&self) -> bool {
        self.inner.lock().unwrap().active
    }
}

/// 全局单实例
static POWER_MANAGER: std::sync::LazyLock<PowerManager> =
    std::sync::LazyLock::new(PowerManager::new);

/// 获取全局 PowerManager 实例
pub fn power_manager() -> &'static PowerManager {
    &POWER_MANAGER
}
