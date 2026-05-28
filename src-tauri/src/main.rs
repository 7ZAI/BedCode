//! BedCode - Entry Point

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::io::Write;

fn main() {
    // 全局 panic hook：防止静默崩溃
    //
    // 注意：panic hook 中不能调用 tracing::error! 等可能持有锁的操作，
    // 否则若 panic 发生在 tracing subscriber 持锁期间会导致死锁。
    // 这里同时输出到 stderr（调试模式可见）和文件（release 模式保底）。
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

        let backtrace = std::backtrace::Backtrace::force_capture();
        let panic_log = format!(
            "[FATAL] Panic at {location}: {msg}\nBacktrace:\n{backtrace}",
        );

        // stderr（debug 构建可见，release 的 windows_subsystem 不可见）
        eprintln!("{panic_log}");

        // 写入 panic 日志文件（release 构建保底）
        if let Ok(log_path) = panic_log_path() {
            if let Some(parent) = log_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)
            {
                let _ = writeln!(
                    file,
                    "{} {panic_log}",
                    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ"),
                );
            }
        }
    }));

    bedcode_lib::run()
}

/// 获取 panic 日志文件路径
fn panic_log_path() -> Result<std::path::PathBuf, ()> {
    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var("APPDATA").map_err(|_| ())?;
        Ok(std::path::PathBuf::from(appdata)
            .join("com.bedcode.app")
            .join("logs")
            .join("panic.log"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        let home = std::env::var("HOME").map_err(|_| ())?;
        Ok(std::path::PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("com.bedcode.app")
            .join("logs")
            .join("panic.log"))
    }
}
