//! Power Manager
//!
//! 管理系统休眠阻止 - 服务器运行时阻止系统进入休眠状态，但允许屏幕熄灭
//!
//! 平台实现：
//! - Windows：`nosleep` crate（PowerCreateRequest）
//! - macOS：`nosleep` crate（IOPMAssertion）
//! - Linux：优先 systemd-logind（`org.freedesktop.login1.Manager.Inhibit`，
//!   系统总线，与桌面环境无关——KDE/XFCE/i3/GNOME 均可用）；
//!   失败时回退 `nosleep`（GNOME SessionManager / freedesktop.org 旧版会话 API）
//!
//! Linux 为何不用 nosleep 为主：nosleep 的 Linux 实现只走 D-Bus **会话**总线上的
//! org.gnome.SessionManager / org.freedesktop.PowerManagement 接口，现代非 GNOME
//! 桌面（KDE Plasma、XFCE 等）不提供这些服务，抑制会静默失败。logind 的 Inhibit
//! 返回 unix fd，保持 fd 打开即持有锁，是 systemd 发行版的标准休眠抑制手段。

use nosleep::{NoSleep, NoSleepType};
use std::sync::Mutex;

/// 休眠阻止管理器
///
/// 服务器运行时调用 enable() 阻止系统休眠，停止时调用 disable() 释放
/// 内部使用 std::sync::Mutex 保护各平台句柄（句柄自身非线程安全）
pub struct PowerManager {
    inner: Mutex<PowerManagerInner>,
    /// 用户设置开关，false 时不阻止休眠
    enabled: std::sync::atomic::AtomicBool,
}

/// 内部状态
struct PowerManagerInner {
    nosleep: Option<NoSleep>,
    /// Linux systemd-logind 抑制器（非 systemd 环境为 None，回退 nosleep）
    #[cfg(target_os = "linux")]
    logind: Option<linux_logind::LogindInhibitor>,
    active: bool,
}

impl PowerManager {
    /// 创建新的 PowerManager 实例
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(PowerManagerInner {
                nosleep: None,
                #[cfg(target_os = "linux")]
                logind: None,
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
    /// 服务器启动时调用，阻止系统休眠但允许屏幕熄灭
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

        // Linux：优先 systemd-logind（与桌面环境无关）；acquire 失败或非 systemd
        // 系统时回退到 nosleep（GNOME 会话 D-Bus API）
        #[cfg(target_os = "linux")]
        {
            if inner.logind.is_none() {
                match linux_logind::LogindInhibitor::acquire() {
                    Ok(inh) => inner.logind = Some(inh),
                    Err(e) => tracing::warn!(
                        "PowerManager: logind inhibit failed ({}), falling back to session D-Bus",
                        e
                    ),
                }
            }
            if inner.logind.is_some() {
                inner.active = true;
                tracing::info!(
                    "PowerManager: system sleep prevention enabled (systemd-logind inhibitor)"
                );
                return;
            }
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
            // PreventUserIdleSystemSleep 只阻止系统休眠，允许屏幕熄灭
            // 服务器需要保持网络连接和后台进程运行，但无需保持屏幕常亮
            match ns.start(NoSleepType::PreventUserIdleSystemSleep) {
                Ok(()) => {
                    inner.active = true;
                    tracing::info!(
                        "PowerManager: system sleep prevention enabled (display sleep allowed)"
                    );
                }
                Err(e) => {
                    tracing::error!("PowerManager: failed to prevent system sleep: {}", e);
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
        inner.active = false;

        // Linux：优先释放 logind 抑制（drop 句柄即关闭 Inhibit fd，锁随之释放）
        #[cfg(target_os = "linux")]
        {
            if inner.logind.take().is_some() {
                tracing::info!(
                    "PowerManager: system sleep prevention disabled (logind inhibitor released)"
                );
            }
        }

        if let Some(ref ns) = inner.nosleep {
            match ns.stop() {
                Ok(()) => {
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

// ==================== Linux：systemd-logind 抑制器 ====================

/// systemd-logind（org.freedesktop.login1）休眠抑制器
///
/// 系统总线调用 `org.freedesktop.login1.Manager.Inhibit(what, who, why, mode)`
/// 获取抑制句柄（unix fd）。`what=sleep` 只阻止系统休眠/挂起、不阻止屏幕熄灭，
/// 与「服务器需持续在线、屏幕无妨熄灭」产品语义一致；`mode=block` 表示强制阻塞。
/// fd 保持打开期间系统不会休眠，Drop 时 fd 关闭、锁自动释放。
///
/// 相比 nosleep 的 Linux 实现（仅支持 GNOME SessionManager / freedesktop 旧版
/// 会话 API），logind 是 systemd 发行版的标准抑制接口，与桌面环境无关；
/// 非 systemd 发行版（如 Alpine openrc）上 acquire 失败，由调用方回退 nosleep。
#[cfg(target_os = "linux")]
mod linux_logind {
    use std::time::Duration;

    use dbus::arg::OwnedFd;
    use dbus::blocking::{BlockingSender, Connection};
    use dbus::Message;

    /// 构造 Inhibit 方法调用（纯函数，便于单元测试）
    fn build_inhibit_message() -> Result<Message, String> {
        let mut msg = Message::new_method_call(
            "org.freedesktop.login1",
            "/org/freedesktop/login1",
            "org.freedesktop.login1.Manager",
            "Inhibit",
        )?;
        msg.append_all((
            "sleep", // what：阻止系统休眠（不阻止屏幕熄灭）
            "BedCode", // who：应用标识
            "BedCode 服务器运行中，阻止系统休眠以保持远程终端在线", // why：人类可读原因
            "block", // mode：强制阻塞
        ));
        Ok(msg)
    }

    pub(super) struct LogindInhibitor {
        /// 抑制句柄（unix fd）：保持打开即持有休眠锁，Drop 时自动释放
        _fd: OwnedFd,
        /// 系统总线连接：与 fd 生命周期无关，仅保持连接避免库内空闲清理
        _bus: Connection,
    }

    impl LogindInhibitor {
        /// 向 systemd-logind 申请休眠抑制
        ///
        /// 返回错误时调用方应回退其他实现（如 nosleep），错误信息需带操作上下文
        pub(super) fn acquire() -> Result<Self, String> {
            let bus = Connection::new_system()
                .map_err(|e| format!("connect system bus for logind inhibit: {e}"))?;
            let reply = bus
                .send_with_reply_and_block(build_inhibit_message()?, Duration::from_secs(5))
                .map_err(|e| format!("call org.freedesktop.login1.Manager.Inhibit: {e}"))?;
            let fd: OwnedFd = reply
                .read1()
                .map_err(|e| format!("read logind inhibitor fd from reply: {e}"))?;
            Ok(Self { _fd: fd, _bus: bus })
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use dbus::arg::messageitem::MessageItem;

        #[test]
        fn inhibit_message_targets_logind_manager() {
            let msg = build_inhibit_message().unwrap();
            assert_eq!("org.freedesktop.login1", &*msg.destination().unwrap());
            assert_eq!("/org/freedesktop/login1", &*msg.path().unwrap());
            assert_eq!("org.freedesktop.login1.Manager", &*msg.interface().unwrap());
            assert_eq!("Inhibit", &*msg.member().unwrap());
        }

        #[test]
        fn inhibit_message_blocks_sleep_for_server() {
            let msg = build_inhibit_message().unwrap();
            // Inhibit(what, who, why, mode)：4 个字符串参数顺序固定
            let strs: Vec<String> = msg
                .get_items()
                .iter()
                .map(|i| match i {
                    MessageItem::Str(s) => s.clone(),
                    other => panic!("expected Str, got {other:?}"),
                })
                .collect();
            assert_eq!(strs.len(), 4);
            assert_eq!(strs[0], "sleep");
            assert_eq!(strs[1], "BedCode");
            assert_eq!(strs[3], "block");
        }
    }
}

/// 全局单实例
static POWER_MANAGER: std::sync::LazyLock<PowerManager> = std::sync::LazyLock::new(PowerManager::new);

/// 获取全局 PowerManager 实例
pub fn power_manager() -> &'static PowerManager {
    &POWER_MANAGER
}
