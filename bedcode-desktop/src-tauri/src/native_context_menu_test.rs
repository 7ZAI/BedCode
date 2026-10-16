//! `mark_first_context_menu_connect`（lib.rs）的单元测试 —— 「正式版抑制原生右键菜单」
//! 的每 webview 一次性去重契约。
//!
//! 行为契约（见 `mark_first_context_menu_connect` 文档注释）：
//! - 同一 label 首次连接、后续导航不再连接（`connect_context_menu` 追加处理器，重复叠加）
//! - 不同 webview 各自独立连接（终端窗口不能因主窗口已连过而被跳过）
//! - 锁中毒不 panic（页面加载钩子跑在主线程，panic 会连带拖垮窗口加载）

#[cfg(test)]
mod tests {
    use crate::{forget_context_menu_label, mark_first_context_menu_connect};
    use std::collections::HashSet;
    use std::sync::{Arc, Mutex};

    /// 同一 webview 的多次页面加载只应连接一次信号
    #[test]
    fn same_label_connects_only_on_first_navigation() {
        let connected = Mutex::new(HashSet::new());

        assert!(
            mark_first_context_menu_connect(&connected, "main"),
            "首次加载应连接信号"
        );
        assert!(
            !mark_first_context_menu_connect(&connected, "main"),
            "同一窗口再次导航不得重复连接（否则信号处理器叠加）"
        );
        assert!(
            !mark_first_context_menu_connect(&connected, "main"),
            "第三次导航同样不再连接"
        );
    }

    /// 不同 webview 互不影响：主窗口连过之后，运行期新建的终端窗口仍要连接
    #[test]
    fn distinct_webviews_connect_independently() {
        let connected = Mutex::new(HashSet::new());

        assert!(mark_first_context_menu_connect(&connected, "main"));
        assert!(
            mark_first_context_menu_connect(&connected, "terminal-session-1"),
            "终端窗口是独立 webview，不该被主窗口的去重记录挡住"
        );
        assert!(!mark_first_context_menu_connect(&connected, "terminal-session-1"));
        assert!(mark_first_context_menu_connect(&connected, "terminal-session-2"));
    }

    /// 记录在去重表里的 label 必须与传入值一致（避免去重键被写错导致窗口被跳过）
    #[test]
    fn inserted_key_is_the_given_label() {
        let connected = Mutex::new(HashSet::new());

        mark_first_context_menu_connect(&connected, "terminal-session-7");

        let seen = connected.lock().expect("锁未被中毒");
        assert_eq!(seen.len(), 1);
        assert!(seen.contains("terminal-session-7"));
    }

    /// 锁中毒时按「未见过」处理：不 panic，且仍返回 true（本次照常连接信号）
    ///
    /// 注意：子线程 panic 时测试输出会出现一条 panic 消息，那是本用例故意制造的，
    /// 测试本身应为绿。
    #[test]
    fn poisoned_lock_does_not_panic_and_reports_first_connect() {
        let connected = Arc::new(Mutex::new(HashSet::new()));
        let holder = Arc::clone(&connected);
        let _ = std::thread::spawn(move || {
            let _guard = holder.lock().expect("持锁线程不该中毒");
            panic!("故意在持锁时 panic 以制造中毒");
        })
        .join();

        assert!(connected.is_poisoned(), "前置条件：锁已中毒");
        assert!(
            mark_first_context_menu_connect(&connected, "main"),
            "中毒锁按未见过处理，仍应连接信号"
        );
        // 中毒恢复取回内部值后，该 label 同样进入去重表，不应再次连接
        assert!(!mark_first_context_menu_connect(&connected, "main"));
    }

    // ==================== 窗口销毁清理（label 复用导致菜单复活的根因） ====================

    /// 同一会话关窗后重开：label 相同但 webview 是全新实例，必须重新连接信号
    ///
    /// 旧行为（不清理）下这里第二次是 `false` → 新窗口永不被连接 → Linux 正式版
    /// 右上角菜单在该窗口复活。
    #[test]
    fn reopened_window_with_same_label_reconnects_after_destroy() {
        let connected = Mutex::new(HashSet::new());

        assert!(mark_first_context_menu_connect(&connected, "terminal-session-1"));
        // 关窗 → 窗口销毁钩子清理
        forget_context_menu_label(&connected, "terminal-session-1");
        // 重开同会话：同 label、全新实例
        assert!(
            mark_first_context_menu_connect(&connected, "terminal-session-1"),
            "销毁后重开的同 label 窗口必须重新连接信号"
        );
    }

    /// 销毁只清自己的 label：主窗口与其它终端窗口的去重记录不受影响
    #[test]
    fn forget_label_does_not_touch_other_windows() {
        let connected = Mutex::new(HashSet::new());

        mark_first_context_menu_connect(&connected, "main");
        mark_first_context_menu_connect(&connected, "terminal-session-1");
        mark_first_context_menu_connect(&connected, "terminal-session-2");

        forget_context_menu_label(&connected, "terminal-session-1");

        assert!(
            !mark_first_context_menu_connect(&connected, "main"),
            "主窗口仍在去重表里，不该因别的窗口销毁而重连"
        );
        assert!(
            !mark_first_context_menu_connect(&connected, "terminal-session-2"),
            "其它终端窗口同样不受影响"
        );
        assert!(mark_first_context_menu_connect(&connected, "terminal-session-1"));
    }

    /// 边界：清理一个从未记录过的 label 不 panic、不影响已有记录（重复 Destroyed 等形态）
    #[test]
    fn forgetting_unknown_label_is_noop() {
        let connected = Mutex::new(HashSet::new());

        mark_first_context_menu_connect(&connected, "main");

        forget_context_menu_label(&connected, "never-seen");
        forget_context_menu_label(&connected, "terminal-session-9");

        assert_eq!(connected.lock().expect("锁未被中毒").len(), 1);
        assert!(!mark_first_context_menu_connect(&connected, "main"));
    }

    /// 锁中毒时清理不得 panic（钩子跑在事件循环上，panic 会拖垮窗口生命周期）
    #[test]
    fn poisoned_lock_forget_does_not_panic() {
        let connected = Arc::new(Mutex::new(HashSet::new()));
        mark_first_context_menu_connect(&connected, "main");
        let holder = Arc::clone(&connected);
        let _ = std::thread::spawn(move || {
            let _guard = holder.lock().expect("持锁线程不该中毒");
            panic!("故意在持锁时 panic 以制造中毒");
        })
        .join();

        assert!(connected.is_poisoned(), "前置条件：锁已中毒");
        forget_context_menu_label(&connected, "main");

        // 中毒恢复取回内部值后条目已被清掉 → 重建的同 label 窗口可重新连接
        assert!(mark_first_context_menu_connect(&connected, "main"));
    }
}
