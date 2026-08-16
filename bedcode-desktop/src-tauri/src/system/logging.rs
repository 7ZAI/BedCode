//! Console Log Formatter
//!
//! 自定义控制台日志格式化器，在 `tracing_subscriber` 的 Pretty 格式基础上做两处优化：
//! - 时间戳改用本地时间毫秒精度，开发时对时间更直观（如 `2026-08-02 00:56:48.819`）
//! - 消息中的 `[plugin:...]` 标签用品红加粗渲染，与宿主日志（蓝/绿/黄/红）形成明显区分
//!
//! 仅用于控制台输出层；日志文件层（error.log / runtime.log）保持默认格式不变，
//! 以便日志文件按 UTC 时间戳排序排错。

use std::fmt;

use nu_ansi_term::{Color, Style};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::field::MakeVisitor;
use tracing_subscriber::fmt::format::{PrettyFields, Writer};
use tracing_subscriber::fmt::time::FormatTime;
use tracing_subscriber::fmt::{FmtContext, FormatEvent};
use tracing_subscriber::registry::LookupSpan;

/// 控制台时间格式：本地时间毫秒精度（示例：`2026-08-02 00:56:48.819`）
const CONSOLE_TIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S%.3f";

/// 自定义 `FormatTime`：输出本地时间毫秒精度
struct LocalMsTimer;

impl FormatTime for LocalMsTimer {
    fn format_time(&self, w: &mut Writer<'_>) -> fmt::Result {
        write!(w, "{}", chrono::Local::now().format(CONSOLE_TIME_FORMAT))
    }
}

/// 控制台事件格式化器
///
/// 输出结构与 Pretty 格式一致（两空格缩进、`at file:line` 定位行、事件间空行），
/// 仅消息体中的 `[plugin:...]` 标签改为品红加粗。
pub struct ConsoleFormatter {
    /// 是否显示源文件定位行（`at file:line`）
    display_location: bool,
}

impl ConsoleFormatter {
    /// 创建默认控制台格式化器
    pub fn new() -> Self {
        Self {
            display_location: true,
        }
    }
}

impl Default for ConsoleFormatter {
    fn default() -> Self {
        Self::new()
    }
}

impl<S> FormatEvent<S, PrettyFields> for ConsoleFormatter
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn format_event(
        &self,
        _ctx: &FmtContext<'_, S, PrettyFields>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let meta = event.metadata();
        let ansi = writer.has_ansi_escapes();

        // 与 Pretty 相同的两空格缩进
        write!(&mut writer, "  ")?;

        // 时间戳：本地时间毫秒精度
        LocalMsTimer.format_time(&mut writer)?;

        // 时间戳与级别之间保留空格，避免输出形如 `...27.869DEBUG` 粘连
        writer.write_char(' ')?;

        let level = meta.level();
        let style = level_style(level);

        // 级别（颜色方案与 Pretty 一致）
        write!(writer, "{} ", styled_level(level, ansi))?;

        // 模块 target（加粗级别色）
        if ansi {
            let target_style = style.bold();
            write!(
                writer,
                "{}{}{}:",
                target_style.prefix(),
                meta.target(),
                target_style.infix(style),
            )?;
        } else {
            write!(writer, "{}:", meta.target())?;
        }
        writer.write_char(' ')?;

        // 消息与字段：先用 PrettyFields 渲染为纯文本，再对插件标签着色
        let fields = render_fields(event);
        if ansi {
            if let Some((tag, rest)) = split_plugin_tag(&fields) {
                // 插件标签品红加粗，与宿主日志形成明显区分
                let plugin_style = Style::new().fg(Color::Magenta).bold();
                write!(writer, "{}", plugin_style.paint(tag))?;
                write!(writer, "{}", style.paint(rest))?;
            } else {
                write!(writer, "{}", style.paint(&fields))?;
            }
        } else {
            writer.write_str(&fields)?;
        }
        writer.write_char('\n')?;

        // 源文件定位行（at file:line），与 Pretty 一致
        if self.display_location {
            if let Some(file) = meta.file() {
                let dimmed = if ansi {
                    Style::new().dimmed().italic()
                } else {
                    Style::new()
                };
                write!(writer, "    {} {}", dimmed.paint("at"), file)?;
                if let Some(line) = meta.line() {
                    write!(writer, ":{}", line)?;
                }
                writer.write_char('\n')?;
            }
        }

        // 事件间空行，与 Pretty 一致
        writer.write_char('\n')
    }
}

/// 按级别返回前景色样式（与 Pretty 的颜色方案一致）
fn level_style(level: &Level) -> Style {
    match *level {
        Level::TRACE => Style::new().fg(Color::Purple),
        Level::DEBUG => Style::new().fg(Color::Blue),
        Level::INFO => Style::new().fg(Color::Green),
        Level::WARN => Style::new().fg(Color::Yellow),
        Level::ERROR => Style::new().fg(Color::Red),
    }
}

/// 渲染级别文本；`ansi=false` 时返回纯文本
fn styled_level(level: &Level, ansi: bool) -> String {
    let (color, text) = match *level {
        Level::TRACE => (Color::Purple, "TRACE"),
        Level::DEBUG => (Color::Blue, "DEBUG"),
        Level::INFO => (Color::Green, " INFO"),
        Level::WARN => (Color::Yellow, " WARN"),
        Level::ERROR => (Color::Red, "ERROR"),
    };
    if ansi {
        format!("{}", color.paint(text))
    } else {
        text.to_string()
    }
}

/// 使用 PrettyFields 将事件的字段渲染为纯文本字符串
///
/// 复用 Pretty 的字段访问器而非手写解析，保证非插件事件的字段输出与 Pretty 一致；
/// 渲染时关闭 ANSI，得到纯文本后再统一着色，避免对转义码做字符串手术。
fn render_fields(event: &Event<'_>) -> String {
    let mut buf = String::new();
    {
        let mut visitor = PrettyFields::new().make_visitor(Writer::new(&mut buf));
        event.record(&mut visitor);
        // visitor 在此作用域结束时释放对 buf 的可变借用
    }
    buf
}

/// 拆分消息中的插件标签前缀
///
/// 消息以 `[plugin:` 开头时返回 `(标签, 剩余部分)`，否则返回 `None`。
/// 标签包含完整 `[plugin:...]`，剩余部分保留起始空格（属消息体）。
fn split_plugin_tag(message: &str) -> Option<(&str, &str)> {
    let rest = message.strip_prefix("[plugin:")?;
    let end = rest.find(']')?;
    let tag_end = "[plugin:".len() + end + 1;
    Some(message.split_at(tag_end))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::fmt::MakeWriter;
    use tracing_subscriber::layer::SubscriberExt;

    #[test]
    fn split_plugin_tag_recognizes_plugin_prefix() {
        let msg = "[plugin:com.bedcode.auto-task] task-status body: {\"status\":\"idle\"}";
        let (tag, rest) = split_plugin_tag(msg).expect("should split plugin tag");
        assert_eq!(tag, "[plugin:com.bedcode.auto-task]");
        assert_eq!(rest, " task-status body: {\"status\":\"idle\"}");
    }

    #[test]
    fn split_plugin_tag_returns_none_for_host_logs() {
        assert!(split_plugin_tag("Session created: abc").is_none());
        assert!(split_plugin_tag("").is_none());
    }

    /// 将 fmt layer 输出收集到内存缓冲的 MakeWriter
    struct SharedWriter(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for SharedWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    struct SharedWriterMaker(Arc<Mutex<Vec<u8>>>);

    impl<'a> MakeWriter<'a> for SharedWriterMaker {
        type Writer = SharedWriter;
        fn make_writer(&'a self) -> Self::Writer {
            SharedWriter(self.0.clone())
        }
    }

    #[test]
    fn console_formatter_uses_local_time_and_colors_plugin_tag() {
        let buf: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));

        let layer = tracing_subscriber::fmt::layer()
            .with_writer(SharedWriterMaker(buf.clone()))
            .with_ansi(true)
            .fmt_fields(PrettyFields::new())
            .event_format(ConsoleFormatter::new());

        let subscriber = tracing_subscriber::registry().with(layer);
        tracing::subscriber::with_default(subscriber, || {
            tracing::debug!(
                target: "test::log",
                "[plugin:com.bedcode.auto-task] task-status body: {}",
                "{\"status\":\"idle\"}",
            );
            tracing::info!("Session created: test");
        });

        let out = String::from_utf8(buf.lock().unwrap().clone()).unwrap();

        // 时间戳为本地时间毫秒精度
        let ts_re = regex::Regex::new(r"\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{3}")
            .expect("valid timestamp regex");
        assert!(
            ts_re.is_match(&out),
            "timestamp should be local ms format, got:\n{out}"
        );

        // 时间戳与级别之间必须有空格（修复日志粘连：`27.869DEBUG`）
        // 级别可能被 ANSI 颜色码包裹（\x1b[34mDEBUG\x1b[0m），正则允许颜色码穿插
        let ts_level_re = regex::Regex::new(
            r"\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{3} (?:\x1b\[[0-9;]*m)?(DEBUG| INFO| WARN|ERROR|TRACE)",
        )
        .expect("valid timestamp-level regex");
        assert!(
            ts_level_re.is_match(&out),
            "timestamp should be followed by a space then level, got:\n{out}"
        );

        // 插件标签出现且被品红着色
        assert!(
            out.contains("[plugin:com.bedcode.auto-task]"),
            "plugin tag should be present, got:\n{out}"
        );
        // 品红（35m）仅出现一次：只用于插件标签，宿主日志不被染成品红
        assert_eq!(
            out.matches(";35m").count(),
            1,
            "magenta should only wrap the plugin tag, got:\n{out}"
        );

        // 保留源文件定位行（"at" 被格式化器渲染为 ANSI 斜体样式，字面量 "at " 不出现，
        // 改为断言定位行内容本身）
        assert!(
            out.contains("logging.rs:"),
            "location line should be kept, got:\n{out}"
        );
    }
}
