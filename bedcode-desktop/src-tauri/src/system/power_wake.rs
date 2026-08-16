//! 电源唤醒监听器
//!
//! Windows：创建隐藏顶层消息窗口监听 `WM_POWERBROADCAST`（`PBT_APMRESUMEAUTOMATIC` /
//! `PBT_APMRESUMESUSPEND`），系统从休眠/睡眠恢复后触发 WebView2 强制重绘。
//!
//! 背景：显示器长时间熄灭/锁屏期间，WebView2 的 GPU 合成器可能崩溃且不自愈
//! （MicrosoftEdge/WebView2Feedback#3817、tauri-apps/tauri#2496），唤醒后窗口永久黑屏，
//! 点击/移动窗口都不会触发重绘。恢复手段：1px 尺寸抖动 + 最小化/还原，强制 DWM 重新
//! 合成并让 WebView2 重建交换链。其他平台暂不监听（macOS NSWorkspace / Linux logind
//! 留待后续按需补充）。

// ==================== 常量与判定 ====================

/// 系统电源广播消息（WM_POWERBROADCAST，winuser.h）
pub(crate) const WM_POWERBROADCAST: u32 = 0x0218;
/// 系统自动恢复（无用户交互唤醒，如定时唤醒）
pub(crate) const PBT_APMRESUMEAUTOMATIC: u32 = 0x0012;
/// 系统从挂起恢复（用户操作唤醒，如按电源键）
pub(crate) const PBT_APMRESUMESUSPEND: u32 = 0x0007;

/// 唤醒来源类型
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ResumeKind {
    /// 用户操作唤醒，可安全抢占焦点
    User,
    /// 系统自动唤醒（定时器/网络等），只做重绘、不抢焦点
    Automatic,
}

/// 判定消息是否为"系统唤醒"广播并返回唤醒来源（纯函数，独立于平台以便测试）
pub(crate) fn resume_kind(msg: u32, wparam: usize) -> Option<ResumeKind> {
    if msg != WM_POWERBROADCAST {
        return None;
    }
    if wparam == PBT_APMRESUMESUSPEND as usize {
        Some(ResumeKind::User)
    } else if wparam == PBT_APMRESUMEAUTOMATIC as usize {
        Some(ResumeKind::Automatic)
    } else {
        None
    }
}

/// 窗口隐藏或最小化时无需恢复（最小化窗口还原时 WebView2 自身会重建渲染）
pub(crate) fn should_skip_recovery(visible: bool, minimized: bool) -> bool {
    !visible || minimized
}

// ==================== 对外入口 ====================

/// 启动电源唤醒监听（后台线程常驻，随进程退出终止）
pub fn spawn_wake_monitor(app: tauri::AppHandle) {
    #[cfg(target_os = "windows")]
    {
        std::thread::Builder::new()
            .name("power-wake-monitor".to_string())
            .spawn(move || unsafe { imp::run_message_loop(app) })
            .expect("failed to spawn power wake monitor thread");
        tracing::info!("[power_wake] wake monitor started");
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = app;
        tracing::debug!("[power_wake] wake monitor not implemented on this platform");
    }
}

// ==================== 唤醒自愈流程（Windows） ====================

#[cfg(target_os = "windows")]
mod recovery {
    use std::time::Duration;

    use tauri::Manager;

    use super::{should_skip_recovery, ResumeKind};

    /// 系统唤醒后的自愈流程：等待 GPU/显示驱动恢复，再强制 WebView2 重新合成帧
    pub(super) async fn recover_after_resume(app: tauri::AppHandle, kind: ResumeKind) {
        // 唤醒瞬间 GPU 驱动可能尚未恢复，立即操作可能无效，稍等片刻再触发重绘
        tokio::time::sleep(Duration::from_millis(1500)).await;

        let Some(window) = app.get_webview_window("main") else {
            tracing::warn!("[power_wake] main window not found, skip recovery");
            return;
        };

        // 用户主动隐藏到托盘时不打扰；最小化窗口还原时 WebView2 自身会重建渲染，无需干预
        let visible = window.is_visible().unwrap_or(false);
        let minimized = window.is_minimized().unwrap_or(false);
        if should_skip_recovery(visible, minimized) {
            tracing::debug!("[power_wake] window hidden or minimized, skip recomposite");
            return;
        }

        tracing::info!("[power_wake] system resumed, forcing webview recomposite");
        force_recomposite(&window, kind).await;
    }

    /// 强制 WebView2 重建合成帧
    ///
    /// 黑屏根因是 WebView2 合成器挂起，以下操作组合是社区验证有效的恢复手段：
    /// 1. 窗口尺寸 1px 抖动 → WM_SIZE → DWM 重新合成 + WebView2 重建交换链
    /// 2. 最小化再还原 → 走 wry 的 WebView2 IsVisible 切换路径（MSDN 建议的 un/minimize 时机）
    async fn force_recomposite(window: &tauri::WebviewWindow, kind: ResumeKind) {
        // 最大化窗口下改尺寸通常被忽略（或改写还原矩形），跳过 1px 抖动；
        // minimize/unminimize 路径对最大化窗口同样有效
        if !window.is_maximized().unwrap_or(false) {
            if let Ok(size) = window.outer_size() {
                let _ = window
                    .set_size(tauri::Size::Physical(tauri::PhysicalSize::new(
                        size.width + 1,
                        size.height + 1,
                    )))
                    .inspect_err(|e| tracing::warn!("[power_wake] resize twiddle (+1px) failed: {e}"));
                let _ = window
                    .set_size(tauri::Size::Physical(size))
                    .inspect_err(|e| tracing::warn!("[power_wake] resize twiddle (restore) failed: {e}"));
            }
        }

        // 两次操作之间留出间隔，避免 wry 将 minimize/unminimize 合并为无状态变化
        tokio::time::sleep(Duration::from_millis(150)).await;
        let _ = window
            .minimize()
            .inspect_err(|e| tracing::warn!("[power_wake] minimize failed: {e}"));
        tokio::time::sleep(Duration::from_millis(150)).await;
        let _ = window
            .unminimize()
            .inspect_err(|e| tracing::warn!("[power_wake] unminimize failed: {e}"));

        // 仅用户操作唤醒时抢焦点：自动唤醒可能是无人在场的定时唤醒，不应打断前台工作
        if kind == ResumeKind::User {
            let _ = window
                .set_focus()
                .inspect_err(|e| tracing::warn!("[power_wake] set_focus failed: {e}"));
        }
    }
}

// ==================== Windows 消息窗口 ====================

#[cfg(target_os = "windows")]
mod imp {
    use std::ptr;
    use std::sync::atomic::{AtomicBool, Ordering};

    use windows_sys::core::{w, PCWSTR};
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, CW_USEDEFAULT, DefWindowProcW, DispatchMessageW, GetMessageW, MSG,
        RegisterClassW, TranslateMessage, UnregisterClassW, WNDCLASSW, WS_OVERLAPPED,
    };

    use super::recovery::recover_after_resume;
    use super::resume_kind;

    /// 消息窗口类名（窗口从不显示，仅作为系统广播接收者）
    const CLASS_NAME: PCWSTR = w!("BedCodePowerWakeMonitor");

    /// 恢复流程进行中标志：一次唤醒可能先后收到多个广播（PBT_APMRESUMESUSPEND 与
    /// PBT_APMRESUMEAUTOMATIC 都会到达），避免重复执行窗口抖动/最小化动画
    static RECOVERING: AtomicBool = AtomicBool::new(false);

    /// 窗口过程：本窗口不处理任何消息，全部交给系统默认处理
    unsafe extern "system" fn wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }

    /// 消息循环（阻塞当前线程直到进程退出）
    ///
    /// WM_POWERBROADCAST 广播给所有顶层窗口，因此创建隐藏的顶层窗口即可收到
    /// 系统休眠/唤醒事件，无需任何可见 UI（不设置 WS_VISIBLE）。
    pub(crate) unsafe fn run_message_loop(app: tauri::AppHandle) {
        let hinstance = GetModuleHandleW(ptr::null());
        let class = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: ptr::null_mut(),
            hCursor: ptr::null_mut(),
            hbrBackground: ptr::null_mut(),
            lpszMenuName: ptr::null(),
            lpszClassName: CLASS_NAME,
        };
        // 类已存在时注册失败（本进程内理论上只会注册一次）
        if RegisterClassW(&class) == 0 {
            tracing::warn!(
                "[power_wake] RegisterClassW failed: {}",
                std::io::Error::last_os_error()
            );
            return;
        }

        let hwnd = CreateWindowExW(
            0,
            CLASS_NAME,
            CLASS_NAME,
            WS_OVERLAPPED,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            0,
            0,
            ptr::null_mut(),
            ptr::null_mut(),
            hinstance,
            ptr::null(),
        );
        if hwnd.is_null() {
            tracing::warn!(
                "[power_wake] CreateWindowExW failed: {}",
                std::io::Error::last_os_error()
            );
            return;
        }

        tracing::info!("[power_wake] hidden message window ready (hwnd={hwnd:?})");

        let mut msg: MSG = std::mem::zeroed();
        loop {
            let ret = GetMessageW(&mut msg, ptr::null_mut(), 0, 0);
            if ret == 0 {
                // WM_QUIT：进程退出
                break;
            }
            if ret == -1 {
                tracing::warn!(
                    "[power_wake] GetMessageW failed: {}",
                    std::io::Error::last_os_error()
                );
                break;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);

            let Some(kind) = resume_kind(msg.message, msg.wParam) else {
                continue;
            };
            tracing::info!(
                "[power_wake] resume event received (wparam=0x{:04x}, kind={kind:?})",
                msg.wParam
            );
            if RECOVERING.swap(true, Ordering::SeqCst) {
                tracing::debug!("[power_wake] recovery already in progress, skip");
                continue;
            }
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                recover_after_resume(app, kind).await;
                RECOVERING.store(false, Ordering::SeqCst);
            });
        }

        let _ = UnregisterClassW(CLASS_NAME, hinstance);
    }
}

// ==================== 测试 ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resume_broadcasts_are_detected_with_kind() {
        // 两种唤醒来源（自动唤醒 / 用户操作唤醒）都应触发恢复，并正确区分来源
        assert_eq!(
            resume_kind(WM_POWERBROADCAST, PBT_APMRESUMESUSPEND as usize),
            Some(ResumeKind::User)
        );
        assert_eq!(
            resume_kind(WM_POWERBROADCAST, PBT_APMRESUMEAUTOMATIC as usize),
            Some(ResumeKind::Automatic)
        );
    }

    #[test]
    fn non_resume_messages_are_ignored() {
        // 0x0011 = PBT_APMSUSPEND（进入休眠），不应触发恢复
        assert_eq!(resume_kind(WM_POWERBROADCAST, 0x0011), None);
        // 其他消息类型（如 0x0219 = WM_QUERYENDSESSION）与无关消息
        assert_eq!(resume_kind(0x0219, PBT_APMRESUMEAUTOMATIC as usize), None);
        assert_eq!(resume_kind(0, 0), None);
    }

    #[test]
    fn skip_recovery_when_hidden_or_minimized() {
        // 隐藏到托盘、最小化时跳过恢复；正常可见窗口需要恢复
        assert!(should_skip_recovery(false, false));
        assert!(should_skip_recovery(true, true));
        assert!(!should_skip_recovery(true, false));
    }
}
