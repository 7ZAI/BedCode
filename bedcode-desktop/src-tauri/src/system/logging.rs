//! 日志系统：控制台日志格式化器 + 日志构建 seam
//!
//! 两部分职责：
//! - **控制台格式化器**：在 `tracing_subscriber` 的 Pretty 格式基础上做两处优化：
//!   - 时间戳改用本地时间毫秒精度，开发时对时间更直观（如 `2026-08-02 00:56:48.819`）
//!   - 消息中的 `[plugin:...]` 标签用品红加粗渲染，与宿主日志（蓝/绿/黄/红）形成明显区分
//! - **日志构建 seam**（`build_logging`）：纯函数构建完整订阅器（error / runtime / frontend
//!   文件层 + 控制台层），不依赖 Tauri AppHandle，供应用启动与单元测试共用。文件层全部
//!   走非阻塞异步写盘（worker 线程 + 有界缓冲），高频输出不再阻塞调用线程。
//!
//! 文件层保持默认格式（UTC 时间戳）以便按时间排序排错；控制台格式仅用于控制台输出层。

use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use nu_ansi_term::{Color, Style};
use tracing::{Event, Level, Subscriber};
use tracing_appender::non_blocking::{NonBlocking, NonBlockingBuilder, WorkerGuard};
use tracing_subscriber::field::MakeVisitor;
use tracing_subscriber::filter::{EnvFilter, FilterFn, Targets};
use tracing_subscriber::fmt::format::{PrettyFields, Writer};
use tracing_subscriber::fmt::time::FormatTime;
use tracing_subscriber::fmt::writer::BoxMakeWriter;
use tracing_subscriber::fmt::{FmtContext, FormatEvent};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry::{LookupSpan, Registry};
use tracing_subscriber::reload;
use tracing_subscriber::Layer;

/// 应用配置的日志段（构建 seam 的输入）
use super::config::LogConfig;

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
        Self { display_location: true }
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

// ==================== 日志系统构建（seam） ====================

/// 日志构建产物：运行时句柄集
///
/// worker guard 必须保持存活到进程退出（drop 时触发 worker flush 剩余日志），
/// 因此由应用启动时存入进程级全局（`store_setup`），禁止提前 drop。
pub struct LoggingSetup {
    _error_guard: WorkerGuard,
    _runtime_guard: WorkerGuard,
    _frontend_guard: Option<WorkerGuard>,
    /// runtime 文件层级别热调句柄（02 `set_log_level` 用）
    pub file_level_reload: reload::Handle<EnvFilter, Registry>,
    /// error 文件层丢弃计数读取（03 容量/告警用）
    pub error_writer: NonBlocking,
    /// runtime 文件层丢弃计数读取（03 容量/告警用）
    pub runtime_writer: NonBlocking,
    /// frontend 文件层丢弃计数读取（03 容量/告警用；仅 dev 存在）
    pub frontend_writer: Option<NonBlocking>,
    /// 日志目录（03 容量裁剪 / 04 打开日志目录用）
    pub log_dir: PathBuf,
}

/// 进程级日志句柄（worker guard 存活载体；02/03 从这里取句柄）
static LOGGING_SETUP: OnceLock<LoggingSetup> = OnceLock::new();

// ==================== bootstrap 日志（build_logging 之前） ====================

/// bootstrap 文件层日志（进程级启动早期通道）
///
/// `build_logging` 之前（config 复制/加载、dev reset 等启动早期路径）tracing 宏
/// 是 no-op（全局订阅器未安装），消息只走 `eprintln!`——debug 控制台可见，
/// release 完全不落盘。本通道在这段窗口期内把日志写入 `bootstrap.log`
/// （与 runtime.*.log 同目录），release 构建的启动失败证据不再丢失；
/// `init_logging` 完成后启动早期路径不再调用，bootstrap 自然停止增长，
/// 容量裁剪（`.log` 后缀）覆盖该文件不失控。
///
/// 实现：writer 每次写重新打开 `bootstrap.log`（追加模式）——dev reset 删除
/// 该文件后下一次写自动重建，不依赖 appender 持有 fd；外层套
/// `NonBlockingBuilder`（worker 线程 + 有界缓冲），符合文件层异步写盘规范。
const BOOTSTRAP_LOG_FILE: &str = "bootstrap.log";

/// 打开即追加的 bootstrap 文件 writer（每次写重建文件句柄）
struct BootstrapFileWriter {
    log_dir: PathBuf,
}

impl std::io::Write for BootstrapFileWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.log_dir.join(BOOTSTRAP_LOG_FILE))?;
        f.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// bootstrap 通道进程级产物（worker guard 存活到进程退出，禁止提前 drop）
pub struct BootstrapLogger {
    _guard: WorkerGuard,
    writer: NonBlocking,
}

/// 进程级 bootstrap 通道（worker guard 存活载体）
static BOOTSTRAP: OnceLock<BootstrapLogger> = OnceLock::new();

/// 初始化 bootstrap 通道：创建日志目录（幂等）+ 打开 `bootstrap.log`（追加）
///
/// 必须在任何启动早期日志（config 复制/加载、dev reset）之前调用一次；
/// 重复调用幂等（首次生效）。失败（目录无法创建）返回错误，由调用方降级：
/// 启动早期日志仍走 `eprintln!`，不阻断启动。
pub fn bootstrap_init(log_dir: &Path) -> crate::Result<()> {
    if BOOTSTRAP.get().is_some() {
        return Ok(());
    }
    std::fs::create_dir_all(log_dir)?;
    let (writer, guard) = NonBlockingBuilder::default()
        .finish(BootstrapFileWriter { log_dir: log_dir.to_path_buf() });
    match BOOTSTRAP.set(BootstrapLogger { _guard: guard, writer }) {
        Ok(()) => {
            bootstrap_log(
                Level::INFO,
                format!("[bootstrap] logging channel initialized: {}", log_dir.display()),
            );
            Ok(())
        }
        // 并发初始化竞争：另一线程已就绪，视为成功
        Err(_) => Ok(()),
    }
}

/// 启动早期日志：写 `bootstrap.log`（non_blocking 队列）+ 控制台 eprintln 双写
///
/// 统一由本函数处理控制台输出，调用方不再单独 eprintln；release 构建控制台
/// 本就不启用，双写不会产生多余输出。
pub fn bootstrap_log(level: Level, message: impl AsRef<str>) {
    let message = message.as_ref();
    // 控制台双写（dev 可见；release 控制台关闭，输出为空流）
    eprintln!("{message}");
    let Some(logger) = BOOTSTRAP.get() else {
        return; // 通道未初始化（如测试直接调用）：仅控制台，不落盘
    };
    let ts = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ");
    // non_blocking 队列满丢弃（同 runtime 文件层语义，丢失有容量告警兜底）
    let mut writer = logger.writer.clone();
    let _ = writeln!(writer, "{ts} {level:>5} {message}");
}

/// 进程级保存日志句柄；重复初始化仅告警不覆盖（正常只会调用一次）
pub fn store_setup(setup: LoggingSetup) {
    if LOGGING_SETUP.set(setup).is_err() {
        eprintln!("[logging] setup already stored; duplicate init ignored");
    }
}

/// 获取全局日志句柄（02/03 用；未初始化返回 None，调用方应静默跳过）
pub fn global_setup() -> Option<&'static LoggingSetup> {
    LOGGING_SETUP.get()
}

/// 日志目录维护任务运行间隔（秒）：启动时立即执行一轮，之后每 10 分钟
const LOG_MAINTENANCE_INTERVAL_SECS: u64 = 600;

/// 容量裁剪：日志目录总大小超限时按修改时间删除最旧文件（当前在写文件除外），
/// 直至总大小 ≤ 上限；返回被删除的文件路径列表。`max_total_bytes = 0` 表示禁用。
///
/// 纯函数（外部行为可测）：不依赖全局状态，删除失败的文件跳过不阻断（下轮重试）。
pub fn trim_log_dir(log_dir: &Path, max_total_bytes: usize) -> Vec<PathBuf> {
    if max_total_bytes == 0 {
        return Vec::new();
    }
    // 收集 *.log 文件（修改时间、路径、大小）；元数据读取失败的文件跳过
    let mut files: Vec<(std::time::SystemTime, PathBuf, u64)> = Vec::new();
    let mut total: u64 = 0;
    let Ok(entries) = std::fs::read_dir(log_dir) else {
        return Vec::new();
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("log") {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        let mtime = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
        total += meta.len();
        files.push((mtime, path, meta.len()));
    }
    if total <= max_total_bytes as u64 || files.len() <= 1 {
        return Vec::new();
    }
    // 按修改时间升序（最旧在前）；最后一项（最新，当前在写）跳过
    files.sort_by_key(|(mtime, _, _)| *mtime);
    let mut remaining = total;
    let mut deleted = Vec::new();
    for (_, path, size) in files.iter().take(files.len() - 1) {
        if remaining <= max_total_bytes as u64 {
            break;
        }
        match std::fs::remove_file(path) {
            Ok(()) => {
                remaining -= size;
                deleted.push(path.clone());
            }
            Err(e) => {
                // 删除失败（如 Windows 下文件被占用）：不阻断流程，下轮维护再试
                tracing::debug!("[logging] trim skip {}: {e}", path.display());
            }
        }
    }
    deleted
}

/// 启动后台日志维护任务（容量裁剪 + 非阻塞队列丢弃告警）
///
/// - 容量裁剪：按进程级配置的容量上限执行，需重启后生效（04 设置页保存即改配置）
/// - 丢弃告警：non_blocking 有界缓冲溢出丢弃日志时输出带数量的 warn，避免静默丢失
///
/// 由应用启动时调用一次（`init_logging`，位于 tauri setup 回调）；setup 必需为
/// 进程级全局存活实例（`global_setup`）。
///
/// 调用上下文容错：setup 闭包运行在 tauri 事件循环线程，不在 Tokio runtime
/// 上下文中（`Handle::try_current` 失败），直接 `tokio::spawn` 会 panic
/// "no reactor running"。此时经 `tauri::async_runtime::block_on` 进入其内部
/// tokio runtime 取 Handle 再 spawn（与 lib.rs PluginDevWatcher 启动同惯例）
pub fn spawn_log_maintenance(setup: &'static LoggingSetup, capacity_bytes: usize) {
    let maintenance = async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(LOG_MAINTENANCE_INTERVAL_SECS));
        let mut last_dropped: usize = 0;
        loop {
            interval.tick().await;

            // 容量裁剪
            let deleted = trim_log_dir(&setup.log_dir, capacity_bytes);
            if !deleted.is_empty() {
                tracing::warn!(
                    count = deleted.len(),
                    max_bytes = capacity_bytes,
                    "[logging] trimmed old log files to stay under capacity",
                );
            }

            // 非阻塞队列丢弃告警（自上次检查有新增则提示）
            let dropped = setup.error_writer.error_counter().dropped_lines()
                + setup.runtime_writer.error_counter().dropped_lines()
                + setup
                    .frontend_writer
                    .as_ref()
                    .map_or(0, |w| w.error_counter().dropped_lines());
            if dropped > last_dropped {
                tracing::warn!(
                    dropped_since_last_check = dropped - last_dropped,
                    "[logging] non-blocking log queue dropped lines (buffer overflow)",
                );
            }
            last_dropped = dropped;
        }
    };

    let handle = match tokio::runtime::Handle::try_current() {
        Ok(h) => h,
        Err(_) => {
            // setup 阶段（lib.rs::init_logging 在 tauri setup 回调）不在 Tokio 上下文
            tauri::async_runtime::block_on(async { tokio::runtime::Handle::current() })
        }
    };
    // 在 tokio 工作线程上执行 error boundary 包装的任务（inner tokio::spawn 此刻有 reactor）
    handle.spawn(async move {
        crate::system::error_boundary::spawn_with_error_boundary("log-maintenance", maintenance)
            .await
            .ok();
    });
}

/// 非阻塞写盘缓冲行数（有界；队列满时 worker 丢弃并计数，见 03 告警）
const NON_BLOCKING_BUFFER_LINES: usize = 20_000;

/// 构建日志系统（主 seam，纯函数不依赖 Tauri AppHandle）
///
/// 返回 `(LoggingSetup, 订阅器)`：订阅器由调用方安装（`set_global_default`
/// 或测试的 `with_default`）；setup 由调用方保存（guard 存活 + 后续热调/告警）。
///
/// dev 语义与启动初始化的既有约定保持一致：dev 构建强制 runtime 落盘 debug +
/// 附加 frontend 文件层；级别热调（02）只影响本次运行，重启回落到持久化配置。
pub fn build_logging(
    log_dir: &Path,
    log_config: &LogConfig,
    dev: bool,
) -> crate::Result<(LoggingSetup, Box<dyn Subscriber + Send + Sync + 'static>)> {
    let rotation = match log_config.rotation.as_str() {
        "hourly" => tracing_appender::rolling::Rotation::HOURLY,
        "never" => tracing_appender::rolling::Rotation::NEVER,
        _ => tracing_appender::rolling::Rotation::DAILY,
    };

    // ---- 文件 appender：error / runtime / frontend(仅 dev)，沿用按天命名 ----
    let error_appender = rolling_appender("error", &rotation, log_config.max_files, log_dir)?;
    let runtime_appender = rolling_appender("runtime", &rotation, log_config.max_files, log_dir)?;

    // ---- 非阻塞包装：worker 线程写盘 + 有界缓冲 + 满则丢（计数可读） ----
    let (error_writer, error_guard) = NonBlockingBuilder::default()
        .buffered_lines_limit(NON_BLOCKING_BUFFER_LINES)
        .finish(error_appender);
    let (runtime_writer, runtime_guard) = NonBlockingBuilder::default()
        .buffered_lines_limit(NON_BLOCKING_BUFFER_LINES)
        .finish(runtime_appender);

    // error 层：固定 ERROR 及以上（与既有语义一致）
    // 注意：fmt::layer 的 S 泛型必须在嵌套链中自由推断（悬挂），故逐层内联、不封装帮助函数

    // runtime 层 filter：级别可热调（reload handle 存入 setup，02 使用）
    let file_level = if dev { "debug" } else { log_config.file_level.as_str() };
    let (runtime_filter, file_level_reload) = reload::Layer::new(EnvFilter::new(file_level));
    // json 模式事件行带完整 span 链（`with_span_list`，json() 默认已开）：
    // error.json 行可凭 `spans[].fields.request_id/session_id` 按 key 检索（与
    // text 模式对称）。span 字段必须由 json 层自己的 `JsonFields` 格式化（默认
    // DefaultFields 产生 ANSI 文本，span 序列化会 panic）——两处 json 层都要配。
    let json_event_format = tracing_subscriber::fmt::format::json().with_span_list(true);

    // frontend 层（仅 target=`frontend`，前端 console 中继日志）：
    // 层永远存在，dev 时写 frontend 文件，非 dev 时写丢弃 writer（配合 target 过滤，事件零进入）
    let (frontend_writer_opt, frontend_guard_opt) = if dev {
        let frontend_appender = rolling_appender("frontend", &rotation, log_config.max_files, log_dir)?;
        let (frontend_writer, frontend_guard) = NonBlockingBuilder::default()
            .buffered_lines_limit(NON_BLOCKING_BUFFER_LINES)
            .finish(frontend_appender);
        (Some(frontend_writer), Some(frontend_guard))
    } else {
        (None, None)
    };
    let frontend_writer = frontend_writer_opt
        .clone()
        .map(BoxMakeWriter::new)
        .unwrap_or_else(|| BoxMakeWriter::new(DiscardMakeWriter));

    // 控制台层：dev 或配置开启；关闭时 filter=off + 丢弃 writer 双保险
    let console_on = dev || log_config.console_in_release;
    let console_writer = if console_on {
        BoxMakeWriter::new(std::io::stdout)
    } else {
        BoxMakeWriter::new(DiscardMakeWriter)
    };
    // RUST_LOG 环境变量优先级最高，其次使用配置值；关闭时 "off" 全关
    let console_filter = if console_on {
        EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(&log_config.console_filter))
    } else {
        EnvFilter::new("off")
    };

    // runtime/error 文件层格式按配置切换（E 泛型不同无法运行时替换，双分支各自返回 Box 统一）；
    // frontend / 控制台层始终 text（前端 relay 人类可读；控制台是开发通道）
    // ErrorLayer（链尾）在 error 事件注入 span 上下文（05，tracing-error）
    let json_mode = log_config.format.eq_ignore_ascii_case("json");
    // error 层过滤：span 全放行（error 行需要完整 span 链定位请求/会话），事件仅 ERROR 及以上。
    // 不能用 EnvFilter::new("error")——它会把 INFO 级 span 一并过滤，导致 error 行丢失 span 前缀
    let error_filter = FilterFn::new(|meta: &tracing::Metadata<'_>| {
        if meta.is_span() {
            true // span 全放行：error 事件行需要完整 span 链（含 INFO 级父 span）
        } else {
            *meta.level() <= Level::ERROR
        }
    });
    let subscriber: Box<dyn Subscriber + Send + Sync + 'static> = if json_mode {
        Box::new(
            tracing_subscriber::registry()
                // runtime 层（链首：reload filter 的 S 解析为 Registry，句柄类型才能存入 LoggingSetup）
                .with(
                    tracing_subscriber::fmt::layer()
                        .with_writer(runtime_writer.clone())
                        .fmt_fields(tracing_subscriber::fmt::format::JsonFields::new())
                        .event_format(json_event_format.clone())
                        .with_target(true)
                        .with_thread_ids(false)
                        .with_line_number(true)
                        .with_filter(runtime_filter),
                )
                // error 层：固定 ERROR 及以上（json 格式）
                .with(
                    tracing_subscriber::fmt::layer()
                        .with_writer(error_writer.clone())
                        .fmt_fields(tracing_subscriber::fmt::format::JsonFields::new())
                        .event_format(json_event_format.clone())
                        .with_target(true)
                        .with_thread_ids(false)
                        .with_line_number(true)
                        .with_filter(error_filter.clone()),
                )
                // frontend 层（仅 target=`frontend`，text）
                .with(
                    tracing_subscriber::fmt::layer()
                        .with_writer(frontend_writer)
                        .with_ansi(false)
                        .with_target(true)
                        .with_thread_ids(false)
                        .with_line_number(true)
                        .with_filter(Targets::new().with_target("frontend", Level::DEBUG)),
                )
                // 控制台层
                .with(
                    tracing_subscriber::fmt::layer()
                        .with_writer(console_writer)
                        .with_ansi(true)
                        .fmt_fields(PrettyFields::new())
                        .event_format(ConsoleFormatter::new())
                        .with_filter(console_filter),
                )
                // error 事件注入 span 路径（05）
                .with(tracing_error::ErrorLayer::default()),
        )
    } else {
        Box::new(
            tracing_subscriber::registry()
                // runtime 层（链首，见 json 分支注释）
                // 文本 Full 格式默认在事件行打印当前 span 链（05 调用链），无需额外配置
                .with(
                    tracing_subscriber::fmt::layer()
                        .with_writer(runtime_writer.clone())
                        .with_ansi(false)
                        .with_target(true)
                        .with_thread_ids(false)
                        .with_line_number(true)
                        .with_filter(runtime_filter),
                )
                // error 层：固定 ERROR 及以上
                .with(
                    tracing_subscriber::fmt::layer()
                        .with_writer(error_writer.clone())
                        .with_ansi(false)
                        .with_target(true)
                        .with_thread_ids(false)
                        .with_line_number(true)
                        .with_filter(error_filter.clone()),
                )
                // frontend 层（仅 target=`frontend`，前端 console 中继日志）
                .with(
                    tracing_subscriber::fmt::layer()
                        .with_writer(frontend_writer)
                        .with_ansi(false)
                        .with_target(true)
                        .with_thread_ids(false)
                        .with_line_number(true)
                        .with_filter(Targets::new().with_target("frontend", Level::DEBUG)),
                )
                // 控制台层
                .with(
                    tracing_subscriber::fmt::layer()
                        .with_writer(console_writer)
                        .with_ansi(true)
                        .fmt_fields(PrettyFields::new())
                        .event_format(ConsoleFormatter::new())
                        .with_filter(console_filter),
                )
                // error 事件注入 span 路径（05）
                .with(tracing_error::ErrorLayer::default()),
        )
    };

    let setup = LoggingSetup {
        _error_guard: error_guard,
        _runtime_guard: runtime_guard,
        _frontend_guard: frontend_guard_opt,
        file_level_reload,
        error_writer: error_writer.clone(),
        runtime_writer: runtime_writer.clone(),
        frontend_writer: frontend_writer_opt,
        log_dir: log_dir.to_path_buf(),
    };

    Ok((setup, subscriber))
}

/// 构建按天轮转的 rolling appender（文件名前缀 + `.log` 后缀，max_files=0 时不限数量）
fn rolling_appender(
    prefix: &str,
    rotation: &tracing_appender::rolling::Rotation,
    max_files: usize,
    log_dir: &Path,
) -> crate::Result<tracing_appender::rolling::RollingFileAppender> {
    let mut builder = tracing_appender::rolling::RollingFileAppender::builder()
        .rotation(rotation.clone())
        .filename_prefix(prefix)
        .filename_suffix("log");
    if max_files > 0 {
        builder = builder.max_log_files(max_files);
    }
    builder.build(log_dir).map_err(|e| {
        // InitError 非 io::Error，包装为带上下文的 Io 错误，保留“创建日志文件失败”语义
        crate::AppError::Io(std::io::Error::other(format!(
            "create log appender '{prefix}' failed: {e}"
        )))
    })
}

// 日志文件层（固定 4 层之一）的通用属性：无 ANSI、带 target 与行号。
// 不封装成帮助函数：fmt::Layer 的 S 泛型必须在 `with` 嵌套链中自由推断，
// 提前固定（如 `Layer<Registry, ...>`）会使后续嵌套层类型无法满足约束。
// 各层直接使用 `tracing_subscriber::fmt::layer()` 并配置相同属性即可。

/// 丢弃全部输出的占位 writer（非 dev / 控制台关闭时的 frontend/console 层兜底；
/// 对应 filter 已 off，此处仅保证层可组装，不产生任何 I/O）
struct DiscardMakeWriter;

struct DiscardWriter;

impl std::io::Write for DiscardWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for DiscardMakeWriter {
    type Writer = DiscardWriter;
    fn make_writer(&'a self) -> Self::Writer {
        DiscardWriter
    }
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

    // ==================== build_logging（01 seam）验收 ====================

    /// 每个测试独立临时目录（并行测试互不干扰），测试结束自动清理
    fn temp_log_dir(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "bedcode-log-test-{}-{}-{}",
            tag,
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("create temp log dir");
        dir
    }

    /// 读取目录下指定前缀的当天（UTC）日志文件内容；不存在返回空串
    fn read_log_file(dir: &Path, prefix: &str) -> String {
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let path = dir.join(format!("{prefix}.{today}.log"));
        std::fs::read_to_string(&path).unwrap_or_default()
    }

    fn log_file_exists(dir: &Path, prefix: &str) -> bool {
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        dir.join(format!("{prefix}.{today}.log")).exists()
    }

    fn test_config(file_level: &str) -> LogConfig {
        LogConfig {
            file_level: file_level.to_string(),
            console_filter: String::new(),
            rotation: "daily".to_string(),
            max_files: 0,
            capacity_bytes: 0, // 测试默认禁用容量裁剪，避免干扰其他断言
            format: "text".to_string(),
            console_in_release: false,
        }
    }

    #[test]
    fn build_logging_release_semantics_filters_by_level() {
        let dir = temp_log_dir("release");
        let config = test_config("info");
        let (setup, subscriber) = build_logging(&dir, &config, false).expect("build logging");
        tracing::subscriber::with_default(subscriber, || {
            tracing::debug!(target: "bedcode_test", "debug line must not land");
            tracing::info!(target: "bedcode_test", "info line lands");
            tracing::error!(target: "bedcode_test", "error line lands");
        });
        drop(setup); // worker guard drop → flush 剩余日志后才断言

        let runtime = read_log_file(&dir, "runtime");
        assert!(runtime.contains("info line lands"), "runtime should have info: {runtime}");
        assert!(runtime.contains("error line lands"), "runtime should have error: {runtime}");
        assert!(
            !runtime.contains("debug line must not land"),
            "runtime at info level must not contain debug: {runtime}"
        );
        // release 语义（dev=false）不创建 frontend 文件
        assert!(!log_file_exists(&dir, "frontend"), "no frontend file when dev=false");

        let error_log = read_log_file(&dir, "error");
        assert!(
            error_log.contains("error line lands"),
            "error file should contain error: {error_log}"
        );
        assert!(
            !error_log.contains("info line lands"),
            "error file must not contain info: {error_log}"
        );
        drop_dir(&dir);
    }

    #[test]
    fn build_logging_dev_semantics_forces_debug_and_frontend_file() {
        let dir = temp_log_dir("dev");
        let config = test_config("info"); // dev 强制 debug，应覆盖配置值
        let (setup, subscriber) = build_logging(&dir, &config, true).expect("build logging");
        tracing::subscriber::with_default(subscriber, || {
            tracing::debug!(target: "bedcode_test", "debug line lands in dev");
            tracing::info!(target: "frontend", "frontend console line lands");
        });
        drop(setup);

        let runtime = read_log_file(&dir, "runtime");
        assert!(
            runtime.contains("debug line lands in dev"),
            "dev forces debug level: {runtime}"
        );
        // frontend target 事件既进 frontend 文件也混入 runtime（既有行为，见 AGENTS.md 日志节）
        assert!(
            runtime.contains("frontend console line lands"),
            "frontend events also land in runtime (existing behavior): {runtime}"
        );
        let frontend = read_log_file(&dir, "frontend");
        assert!(
            frontend.contains("frontend console line lands"),
            "frontend file should contain console relay line: {frontend}"
        );
        drop_dir(&dir);
    }

    #[test]
    fn build_logging_exposes_dropped_counters() {
        let dir = temp_log_dir("dropped");
        let config = test_config("debug");
        let (setup, _subscriber) = build_logging(&dir, &config, true).expect("build logging");
        // 丢弃计数接口可用（03 容量任务读取入口基准为 0）
        assert_eq!(setup.error_writer.error_counter().dropped_lines(), 0);
        assert_eq!(setup.runtime_writer.error_counter().dropped_lines(), 0);
        let frontend = setup.frontend_writer.as_ref().expect("dev has frontend writer");
        assert_eq!(frontend.error_counter().dropped_lines(), 0);
        drop_dir(&dir);
    }

    /// 构造 JSON 格式的测试配置
    fn json_config(file_level: &str) -> LogConfig {
        LogConfig {
            format: "json".to_string(),
            ..test_config(file_level)
        }
    }

    #[test]
    fn build_logging_json_format_is_parseable() {
        let dir = temp_log_dir("json");
        let config = json_config("info");
        let (setup, subscriber) = build_logging(&dir, &config, false).expect("build logging");
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(target: "bedcode_test", session_id = 42, "json info line lands");
            tracing::error!(target: "bedcode_test", "json error line lands");
        });
        drop(setup);

        let runtime = read_log_file(&dir, "runtime");
        assert!(!runtime.is_empty(), "runtime json file should be written");
        let first: serde_json::Value = serde_json::from_str(runtime.lines().next().unwrap())
            .expect("runtime line should be valid JSON");
        assert_eq!(first["level"], "INFO", "json should carry level field: {first}");
        assert_eq!(
            first["fields"]["session_id"], 42,
            "json should carry message fields: {first}"
        );
        assert!(
            first["fields"]["message"]
                .as_str()
                .is_some_and(|m| m.contains("json info line lands")),
            "json should carry message text: {first}"
        );

        let error_log = read_log_file(&dir, "error");
        assert!(!error_log.is_empty(), "error json file should be written");
        let err_line: serde_json::Value = serde_json::from_str(error_log.lines().next().unwrap())
            .expect("error line should be valid JSON");
        assert_eq!(err_line["level"], "ERROR", "error file only ERROR: {err_line}");
        drop_dir(&dir);
    }

    /// 02 验收：json 模式事件行携带 span 链（`with_span_list`），error.json 行
    /// 可凭 `span[].fields.request_id/session_id` 按 key 检索（与 text 模式对称）
    #[test]
    fn json_format_events_carry_span_list() {
        let dir = temp_log_dir("json-span");
        let config = json_config("info");
        let (setup, subscriber) = build_logging(&dir, &config, false).expect("build logging");
        tracing::subscriber::with_default(subscriber, || {
            let parent = tracing::span!(
                tracing::Level::INFO,
                "http_request",
                request_id = "req-span-1",
                method = "POST"
            );
            parent.in_scope(|| {
                tracing::error!(target: "bedcode_test", session_id = 7, "boom inside span");
            });
        });
        drop(setup);

        // error 文件只收 ERROR 事件（FilterFn 语义不变），且该行带 span 链
        let error_log = read_log_file(&dir, "error");
        let err_line: serde_json::Value = serde_json::from_str(error_log.lines().next().unwrap())
            .expect("error line should be valid JSON");
        assert_eq!(err_line["level"], "ERROR", "error file only ERROR: {err_line}");
        // span 链：json() 全链默认输出 `spans` 数组（root→leaf），并在 `span` 键
        // 带当前 span 对象；断言全链数组内 request_id 可检索
        let spans = err_line["spans"]
            .as_array()
            .unwrap_or_else(|| panic!("error line should carry spans list: {err_line}"));
        let outer = spans
            .iter()
            .find(|s| s["name"] == "http_request")
            .unwrap_or_else(|| panic!("spans should contain http_request: {err_line}"));
        // span 对象里字段平铺（SerializableSpan 把 span 字段写进对象顶层）
        assert_eq!(
            outer["request_id"], "req-span-1",
            "span fields should carry request_id: {err_line}"
        );
        assert_eq!(outer["method"], "POST", "span field method: {err_line}");
        // 既有 json 字段不变（level/target/line_number）
        assert_eq!(err_line["target"], "bedcode_test", "target field kept: {err_line}");
        assert!(err_line["line_number"].is_number(), "line_number kept: {err_line}");
        drop_dir(&dir);
    }

    /// 03 验收：bootstrap 通道在 build_logging 之前落盘 `bootstrap.log`，
    /// 文件删除（dev reset）后下一次写自动重建；格式带级别与 UTC 时间戳
    #[test]
    fn bootstrap_channel_writes_file_and_recreates_after_delete() {
        let dir = temp_log_dir("bootstrap");
        bootstrap_init(&dir).expect("bootstrap init");
        // worker 线程异步写盘：短暂等待后读取，保证确定性
        let settle = || std::thread::sleep(std::time::Duration::from_millis(100));

        bootstrap_log(Level::INFO, "bootstrap line one");
        bootstrap_log(Level::ERROR, "bootstrap line two");
        settle();

        let path = dir.join("bootstrap.log");
        let content = std::fs::read_to_string(&path).expect("bootstrap.log written");
        assert!(content.contains("bootstrap line one"), "first line: {content}");
        assert!(content.contains("bootstrap line two"), "second line: {content}");
        // 行结构：UTC 时间戳 + 级别 + 消息
        assert!(content.lines().any(|l| l.contains(" INFO bootstrap line one")), "level+msg: {content}");
        assert!(content.lines().any(|l| l.contains("ERROR bootstrap line two")), "level+msg: {content}");

        // dev reset 语义：删除后下一次写重建文件
        std::fs::remove_file(&path).expect("remove bootstrap.log");
        bootstrap_log(Level::INFO, "bootstrap line three");
        settle();
        let content = std::fs::read_to_string(&path).expect("bootstrap.log recreated");
        assert!(
            content.contains("bootstrap line three") && !content.contains("bootstrap line one"),
            "recreated file should only hold post-reset lines: {content}"
        );
        drop_dir(&dir);
    }

    /// 创建指定名称与字节数的日志文件；`old` 为 true 时把修改时间推到 1 天前
    fn write_log_file(dir: &Path, name: &str, bytes: usize, old: bool) {
        let path = dir.join(name);
        std::fs::write(&path, "x".repeat(bytes)).unwrap();
        if old {
            let older = std::time::SystemTime::now() - std::time::Duration::from_secs(86_400);
            let f = std::fs::File::options().write(true).open(&path).unwrap();
            let _ = f.set_modified(older);
        }
    }

    #[test]
    fn trim_log_dir_removes_oldest_files_and_keeps_current() {
        let dir = temp_log_dir("trim-basic");
        // 当前文件最小化关注：c.log 最新（视为在写），a/b 一天前
        write_log_file(&dir, "a.log", 100, true);
        write_log_file(&dir, "b.log", 100, true);
        write_log_file(&dir, "c.log", 100, false);

        // 上限 150：总 300 → 删最旧 a（100 → 剩 200）仍在超限 → 删 b（→ 剩 100）停
        let deleted = trim_log_dir(&dir, 150);
        assert_eq!(deleted.len(), 2, "should delete 2 oldest files");
        assert_eq!(deleted[0].file_name().unwrap().to_str().unwrap(), "a.log");
        assert_eq!(deleted[1].file_name().unwrap().to_str().unwrap(), "b.log");
        assert!(!dir.join("a.log").exists());
        assert!(!dir.join("b.log").exists());
        assert!(dir.join("c.log").exists(), "current (newest) file must be kept");
        drop_dir(&dir);
    }

    #[test]
    fn trim_log_dir_under_capacity_keeps_everything() {
        let dir = temp_log_dir("trim-under");
        write_log_file(&dir, "a.log", 100, true);
        write_log_file(&dir, "b.log", 100, false);
        let deleted = trim_log_dir(&dir, 300); // 总 200 ≤ 300
        assert!(deleted.is_empty(), "no deletion when under capacity: {deleted:?}");
        assert!(dir.join("a.log").exists());
        assert!(dir.join("b.log").exists());
        drop_dir(&dir);
    }

    #[test]
    fn trim_log_dir_zero_capacity_disables() {
        let dir = temp_log_dir("trim-off");
        write_log_file(&dir, "a.log", 100, true);
        write_log_file(&dir, "b.log", 100, false);
        let deleted = trim_log_dir(&dir, 0);
        assert!(deleted.is_empty(), "capacity 0 must disable trimming");
        assert!(dir.join("a.log").exists());
        assert!(dir.join("b.log").exists());
        drop_dir(&dir);
    }

    #[test]
    fn trim_log_dir_single_current_file_is_untouched() {
        let dir = temp_log_dir("trim-single");
        write_log_file(&dir, "runtime.log", 500, false);
        // 唯一文件且超限：视为当前在写文件，不删
        let deleted = trim_log_dir(&dir, 100);
        assert!(deleted.is_empty(), "single current file must survive: {deleted:?}");
        assert!(dir.join("runtime.log").exists());
        drop_dir(&dir);
    }

    /// 05 验收：文本文件层事件行携带 span 链（Full 格式默认），error 文件带 span 路径
    #[test]
    fn runtime_logs_carry_span_chain_in_text_mode() {
        let dir = temp_log_dir("span-chain");
        let config = test_config("info");
        let (setup, subscriber) = build_logging(&dir, &config, false).expect("build logging");
        tracing::subscriber::with_default(subscriber, || {
            // 注意：子 span 必须在父 span 的 in_scope 内创建，否则父链不绑定
            let parent = tracing::span!(
                tracing::Level::INFO,
                "http_request",
                request_id = "req-1",
                method = "POST"
            );
            parent.in_scope(|| {
                let child = tracing::span!(tracing::Level::INFO, "session_create", session_id = "s-100");
                child.in_scope(|| {
                    tracing::info!(target: "bedcode_test", "msg inside nested spans");
                    tracing::error!(target: "bedcode_test", "boom inside nested spans");
                })
            });
        });
        drop(setup);

        let runtime = read_log_file(&dir, "runtime");
        // Full 文本格式默认打印当前 span 链（含名称与字段）
        assert!(runtime.contains("http_request"), "runtime should show outer span: {runtime}");
        assert!(runtime.contains("session_create"), "runtime should show inner span: {runtime}");
        assert!(runtime.contains("req-1"), "span fields should be printed: {runtime}");

        // ErrorLayer + span 上下文：error 行携带所属 span 链，可定位请求/会话
        let error_log = read_log_file(&dir, "error");
        assert!(
            error_log.contains("session_create") && error_log.contains("http_request"),
            "error file should carry span path: {error_log}"
        );
        drop_dir(&dir);
    }

    #[test]
    fn runtime_level_reload_takes_effect_without_restart() {
        let dir = temp_log_dir("reload");
        let config = test_config("info");
        let (setup, subscriber) = build_logging(&dir, &config, false).expect("build logging");
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(target: "bedcode_test", "before reload: info lands");
            tracing::debug!(target: "bedcode_test", "before reload: debug must not land");

            // 热调到 debug：新写 debug 落盘
            setup
                .file_level_reload
                .reload(EnvFilter::new("debug"))
                .expect("reload to debug");
            tracing::debug!(target: "bedcode_test", "after reload: debug lands");

            // 热调到 error：warn 不再落盘
            setup
                .file_level_reload
                .reload(EnvFilter::new("error"))
                .expect("reload to error");
            tracing::warn!(target: "bedcode_test", "after reload: warn must not land");
            tracing::error!(target: "bedcode_test", "after reload: error lands");
        });
        drop(setup);

        let runtime = read_log_file(&dir, "runtime");
        assert!(runtime.contains("before reload: info lands"), "info at initial level: {runtime}");
        assert!(
            !runtime.contains("before reload: debug must not land"),
            "debug filtered at info level: {runtime}"
        );
        assert!(runtime.contains("after reload: debug lands"), "debug after reload to debug: {runtime}");
        assert!(
            !runtime.contains("after reload: warn must not land"),
            "warn filtered after reload to error: {runtime}"
        );
        assert!(runtime.contains("after reload: error lands"), "error after reload to error: {runtime}");
        drop_dir(&dir);
    }

    /// 清理临时目录（断言失败时保留目录便于排查）
    fn drop_dir(dir: &Path) {
        let _ = std::fs::remove_dir_all(dir);
    }

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
        let ts_re = regex::Regex::new(r"\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{3}").expect("valid timestamp regex");
        assert!(ts_re.is_match(&out), "timestamp should be local ms format, got:\n{out}");

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
        assert!(out.contains("logging.rs:"), "location line should be kept, got:\n{out}");
    }
}
