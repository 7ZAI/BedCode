//! BedCode - Entry Point

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // 全局 panic hook：防止静默崩溃
    //
    // 注意：panic hook 中不能调用 tracing::error! 等可能持有锁的操作，
    // 否则若 panic 发生在 tracing subscriber 持锁期间会导致死锁。
    // 这里只使用 eprintln! 确保 panic 信息无条件输出到 stderr。
    std::panic::set_hook(Box::new(|panic_info| {
        let msg = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "Unknown panic".to_string()
        };
        let location = panic_info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_default();
        eprintln!("[FATAL] Panic at {location}: {msg}");
    }));

    bedcode_lib::run()
}
