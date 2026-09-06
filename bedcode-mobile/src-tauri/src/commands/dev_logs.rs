//! 开发者前端控制台日志转发（仅 debug 构建注册，release 不包含）
//!
//! 前端在 debug 构建下将 `console.*` 输出经 IPC 批量转发到此命令，
//! 写进 tracing（target=`frontend`），在 Android 上自动转发 logcat——
//! AI agent 通过 `pnpm run tauri:android:dev:log` 落盘的
//! `.dev-logs/android-dev.*.log` 中 grep `frontend` 即可获取前端控制台输出。
//!
//! 前端实现见 `src/utils/devConsoleRelay.ts`。

use serde::Deserialize;

/// 单条日志的最大字符数，超过部分截断（防止异常超长消息撑爆日志）
pub const MAX_MESSAGE_LEN: usize = 16 * 1024;

/// 前端 console 转发的单条日志记录
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontendLogEntry {
    /// 日志级别：debug / info / warn / error（前端已归一化）
    pub level: String,
    /// 日志内容（前端多参数已拼接为单条字符串）
    pub message: String,
}

/// 将前端日志级别归一化为 tracing 级别；未知级别回退 debug
///
/// 回退 debug 而不是丢弃，保证信息不丢——console.log 语义与 trace 不同，
/// 且 logcat 过滤通常从 debug 起，直接下沉 debug 更易被检索。
pub fn normalized_level(level: &str) -> &'static str {
    match level {
        "info" => "info",
        "warn" => "warn",
        "error" => "error",
        // "log" / "debug" / 未知级别统一走 debug
        _ => "debug",
    }
}

/// 超长消息截断（按字符边界回溯，避免切在多字节 UTF-8 中间 panic）
pub fn truncate_message(message: &str) -> &str {
    if message.len() <= MAX_MESSAGE_LEN {
        return message;
    }
    let mut end = MAX_MESSAGE_LEN;
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    &message[..end]
}

/// 接收前端批量转发的 console 日志并写入 tracing（Android 自动转发 logcat）
#[cfg(debug_assertions)]
#[tauri::command]
pub fn report_frontend_log(logs: Vec<FrontendLogEntry>) {
    for entry in logs {
        if entry.message.is_empty() {
            continue;
        }
        let message = truncate_message(&entry.message);
        match normalized_level(&entry.level) {
            "info" => tracing::info!(target: "frontend", "{message}"),
            "warn" => tracing::warn!(target: "frontend", "{message}"),
            "error" => tracing::error!(target: "frontend", "{message}"),
            _ => tracing::debug!(target: "frontend", "{message}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_level_maps_known_and_unknown() {
        assert_eq!(normalized_level("debug"), "debug");
        assert_eq!(normalized_level("info"), "info");
        assert_eq!(normalized_level("warn"), "warn");
        assert_eq!(normalized_level("error"), "error");
        // console.log 与未知级别均下沉 debug，保证日志不丢
        assert_eq!(normalized_level("log"), "debug");
        assert_eq!(normalized_level("trace"), "debug");
        assert_eq!(normalized_level("whatever"), "debug");
    }

    #[test]
    fn truncate_message_keeps_short_input_unchanged() {
        assert_eq!(truncate_message("short message"), "short message");
        assert_eq!(truncate_message(""), "");
    }

    #[test]
    fn truncate_message_limits_long_input() {
        let long = "x".repeat(MAX_MESSAGE_LEN + 10);
        let truncated = truncate_message(&long);
        assert_eq!(truncated.len(), MAX_MESSAGE_LEN);
    }

    #[test]
    fn truncate_message_does_not_split_multibyte_char() {
        let s = "a".repeat(MAX_MESSAGE_LEN - 1) + "😀😀";
        let truncated = truncate_message(&s);
        assert!(truncated.len() <= MAX_MESSAGE_LEN);
        assert!(truncated.is_char_boundary(truncated.len()));
        assert!(!truncated.ends_with('\u{FFFD}'));
    }
}