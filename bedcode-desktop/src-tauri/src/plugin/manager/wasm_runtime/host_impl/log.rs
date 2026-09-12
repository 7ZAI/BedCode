//! 日志域宿主实现（转发到宿主 tracing，附加 plugin_id 前缀）
//!
//! `emit_plugin_log` 用插件调用点构造 tracing Metadata（组件形态暂不携带
//! file/line，见 wit/bedcode.wit 的 host-log 注释）。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use tracing::callsite::{self, Callsite};
use tracing::field::{self, Value};
use tracing::metadata::Kind;
use tracing::{Event, Level, Metadata};

/// 插件日志统一 callsite：所有动态 Metadata 共享同一字段集（message）
///
/// tracing 的 callsite 机制用于订阅者兴趣缓存；插件日志按调用点动态生成
/// Metadata（file/line 各不相同），字段集一致即可共用此 callsite 标识
struct PluginLogCallsite;

static PLUGIN_LOG_CALLSITE: PluginLogCallsite = PluginLogCallsite;

impl Callsite for PluginLogCallsite {
    fn set_interest(&self, _interest: tracing::subscriber::Interest) {}

    fn metadata(&self) -> &'static Metadata<'static> {
        static META: Metadata<'static> = Metadata::new(
            "bedcode_lib::plugin::plugin_log",
            "bedcode_lib::plugin::plugin_log",
            Level::INFO,
            None,
            None,
            None,
            field::FieldSet::new(&["message"], callsite::Identifier(&PLUGIN_LOG_CALLSITE)),
            Kind::EVENT,
        );
        &META
    }
}

/// 插件日志 Metadata 缓存（按 file + line + level 键控）
///
/// `Event::dispatch` 要求 Metadata 为 `'static`，而插件 file 是 WASM 线性内存
/// 中的运行时字符串，首次出现时泄漏一份（Box::leak）转为 `'static` 复用。
/// 插件日志调用点数量有限（每个插件几十个），缓存有界，不会无限增长。
static PLUGIN_LOG_META_CACHE: OnceLock<Mutex<HashMap<(String, u32, Level), &'static Metadata<'static>>>> =
    OnceLock::new();

/// 获取（或缓存构造）指定插件调用点的日志 Metadata
fn plugin_log_metadata(level: Level, file: &str, line: u32) -> &'static Metadata<'static> {
    let cache = PLUGIN_LOG_META_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
    cache.entry((file.to_string(), line, level)).or_insert_with(|| {
        // 泄漏转 'static：同一调用点后续命中缓存，仅首次泄漏
        let file: &'static str = Box::leak(file.to_string().into_boxed_str());
        let fields = field::FieldSet::new(&["message"], callsite::Identifier(&PLUGIN_LOG_CALLSITE));
        Box::leak(Box::new(Metadata::new(
            "bedcode_lib::plugin::plugin_log",
            "bedcode_lib::plugin::plugin_log",
            level,
            Some(file),
            Some(line),
            Some("bedcode_lib::plugin::plugin_log"),
            fields,
            Kind::EVENT,
        )))
    })
}

/// 发出 info 日志（组件形态不携带插件调用点，file/line 传 ""/0）
pub(crate) fn log_info(plugin_id: &str, message: &str, file: &str, line: u32) {
    emit_plugin_log(plugin_id, Level::INFO, file, line, message);
}

/// 发出 debug 日志
pub(crate) fn log_debug(plugin_id: &str, message: &str, file: &str, line: u32) {
    emit_plugin_log(plugin_id, Level::DEBUG, file, line, message);
}

/// 发出 warn 日志
pub(crate) fn log_warn(plugin_id: &str, message: &str, file: &str, line: u32) {
    emit_plugin_log(plugin_id, Level::WARN, file, line, message);
}

/// 发出 error 日志
pub(crate) fn log_error(plugin_id: &str, message: &str, file: &str, line: u32) {
    emit_plugin_log(plugin_id, Level::ERROR, file, line, message);
}

// ==================== Per-plugin 日志级别 ====================

// 插件日志级别阈值映射缓存（thread_local）：键 = 插件 ID，值 = 该插件日志
// 允许的最低级别。缓存用 thread_local 是因为插件日志事件在调用线程内同步
// dispatch，各线程解析结果一致（进程级 env）；测试环境下不同测试可在线程内
// 独立 set_var + reset 互不污染（进程级 OnceLock 会被并行测试抢先填充，
// 见本文件测试模块的 with_plugin_log_env）
thread_local! {
    static PLUGIN_LOG_THRESHOLDS: std::cell::RefCell<Option<HashMap<String, Level>>> =
        const { std::cell::RefCell::new(None) };
}

/// 按插件日志级别阈值映射（`BEDCODE_PLUGIN_LOG=id=level,id2=level2`）
///
/// 键 = 插件 ID，值 = 该插件日志允许的最低级别。比阈值更 verbose 的级别在
/// `emit_plugin_log` 入口直接丢弃——tracing filter 无法按字段过滤，
/// 宿主侧判定是 per-plugin 级别的唯一可靠位置。未列出的插件无阈值，
/// 沿用宿主全局过滤语义（行为与现状一致）。
///
/// 首次访问时从环境变量懒解析并缓存
fn plugin_log_threshold(plugin_id: &str) -> Option<Level> {
    PLUGIN_LOG_THRESHOLDS.with(|cell| {
        let mut guard = cell.borrow_mut();
        let map = guard.get_or_insert_with(load_plugin_log_thresholds);
        map.get(plugin_id).cloned()
    })
}

/// 从 `BEDCODE_PLUGIN_LOG` 解析阈值映射（逗号分隔 `id=level`）
///
/// 容错：未知插件 ID / 非法级别 / 空条目一律忽略（不 panic、不影响其他
/// 条目）；未设置环境变量时返回空映射（所有插件沿用全局语义）
fn load_plugin_log_thresholds() -> HashMap<String, Level> {
    let mut map = HashMap::new();
    let Ok(raw) = std::env::var("BEDCODE_PLUGIN_LOG") else {
        return map;
    };
    for entry in raw.split(',') {
        let entry = entry.trim();
        let Some((id, level_str)) = entry.split_once('=') else {
            continue;
        };
        let level = match level_str.trim() {
            "trace" => Level::TRACE,
            "debug" => Level::DEBUG,
            "info" => Level::INFO,
            "warn" => Level::WARN,
            "error" => Level::ERROR,
            _ => continue,
        };
        let id = id.trim();
        if id.is_empty() {
            continue;
        }
        map.insert(id.to_string(), level);
    }
    map
}

/// 测试用：清空当前线程的阈值缓存，下次访问时重新读环境变量
#[cfg(test)]
fn reset_plugin_log_thresholds() {
    PLUGIN_LOG_THRESHOLDS.with(|cell| *cell.borrow_mut() = None);
}

/// 以插件调用点位置发出 tracing 事件（消息带 `[plugin:xxx]` 前缀，保持旧格式）
fn emit_plugin_log(plugin_id: &str, level: Level, file: &str, line: u32, message: &str) {
    // per-plugin 阈值过滤：比配置级别更 verbose（数值更大：DEBUG=4 > WARN=2）
    // 的级别直接丢弃（不产生事件、不经过 filter）；等于或低于阈值照常放行
    if let Some(threshold) = plugin_log_threshold(plugin_id) {
        if level > threshold {
            return;
        }
    }
    let meta = plugin_log_metadata(level, file, line);
    let fieldset = meta.fields();
    let formatted = format!("[plugin:{}] {}", plugin_id, message);
    // value_set_all 按字段集顺序填充值；插件日志字段集仅 "message" 一项
    let display_value = field::display(formatted);
    let value_slots = [Some(&display_value as &dyn Value)];
    let values = fieldset.value_set_all(&value_slots);
    Event::dispatch(meta, &values);
}

// ==================== Tests ====================

/// 测试捕获基建（log.rs 与其他宿主模块的 trap/级别过滤测试共用）
///
/// 与项目默认订阅者（文件落盘）互不影响：with_default 只替换当前线程
/// 的默认订阅者，且测试通过共享 Arc 取回捕获结果
#[cfg(test)]
pub(crate) mod capture {
    use std::sync::{Arc, Mutex};
    use tracing::field::Visit;
    use tracing::span;
    use tracing::subscriber::with_default;
    use tracing::{Event, Level, Metadata, Subscriber};

    /// 捕获的单个事件（level / 调用点 / 消息 / 结构化字段）
    #[derive(Clone, Debug)]
    pub(crate) struct CapturedEvent {
        pub(crate) level: Level,
        pub(crate) file: Option<&'static str>,
        pub(crate) line: Option<u32>,
        pub(crate) message: String,
        /// 结构化字段（plugin_id / trap / export 等），供断言宿主日志携带上下文
        pub(crate) fields: Vec<(String, String)>,
    }

    /// 极简订阅者：把 event 原样记录到共享 vec，供断言
    pub(crate) struct CaptureSubscriber {
        pub(crate) events: Arc<Mutex<Vec<CapturedEvent>>>,
    }

    /// 从 event 字段集中提取 message 与其余结构化字段
    struct MessageVisitor(String, Vec<(String, String)>);

    impl Visit for MessageVisitor {
        fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
            if field.name() == "message" {
                self.0 = value.to_string();
            } else {
                self.1.push((field.name().to_string(), value.to_string()));
            }
        }

        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            if field.name() == "message" {
                self.0 = format!("{:?}", value);
            } else {
                self.1.push((field.name().to_string(), format!("{:?}", value)));
            }
        }

        fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
            self.1.push((field.name().to_string(), value.to_string()));
        }

        fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
            self.1.push((field.name().to_string(), value.to_string()));
        }

        fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
            self.1.push((field.name().to_string(), value.to_string()));
        }
    }

    impl Subscriber for CaptureSubscriber {
        fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
            true
        }

        fn new_span(&self, _attrs: &span::Attributes<'_>) -> span::Id {
            span::Id::from_u64(1)
        }

        fn record(&self, _span: &span::Id, _values: &span::Record<'_>) {}

        fn record_follows_from(&self, _span: &span::Id, _follows: &span::Id) {}

        fn event(&self, event: &Event<'_>) {
            let mut visitor = MessageVisitor(String::new(), Vec::new());
            event.record(&mut visitor);
            self.events.lock().unwrap().push(CapturedEvent {
                level: *event.metadata().level(),
                file: event.metadata().file(),
                line: event.metadata().line(),
                message: visitor.0,
                fields: visitor.1,
            });
        }

        fn enter(&self, _span: &span::Id) {}

        fn exit(&self, _span: &span::Id) {}
    }

    /// 以捕获订阅者为默认订阅者执行闭包，返回捕获到的事件列表
    pub(crate) fn capture<F: FnOnce()>(f: F) -> Vec<CapturedEvent> {
        let events = Arc::new(Mutex::new(Vec::new()));
        let subscriber = CaptureSubscriber { events: events.clone() };
        with_default(subscriber, f);
        let captured = events.lock().unwrap();
        captured.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::capture::*;

    // ==================== plugin_log_metadata ====================

    /// 同一调用点（file+line+level）命中缓存：返回同一 'static 指针
    #[test]
    fn plugin_log_metadata_cached_same_pointer() {
        let m1 = plugin_log_metadata(Level::INFO, "guest.rs", 10);
        let m2 = plugin_log_metadata(Level::INFO, "guest.rs", 10);
        assert!(std::ptr::eq(m1, m2), "cache must return the same metadata");
        assert_eq!(m1.level(), &Level::INFO);
        assert_eq!(m1.file(), Some("guest.rs"));
        assert_eq!(m1.line(), Some(10));
        assert_eq!(m1.target(), "bedcode_lib::plugin::plugin_log");
    }

    /// 不同 level 视为不同调用点：各自独立缓存（字段集一致但级别不同）
    #[test]
    fn plugin_log_metadata_level_is_cache_key_part() {
        let info = plugin_log_metadata(Level::INFO, "guest.rs", 10);
        let warn = plugin_log_metadata(Level::WARN, "guest.rs", 10);
        assert!(!std::ptr::eq(info, warn));
        assert_eq!(info.level(), &Level::INFO);
        assert_eq!(warn.level(), &Level::WARN);
        // file/line 相同：证明 key 区分的是 level
        assert_eq!(info.file(), warn.file());
        assert_eq!(info.line(), warn.line());
    }

    /// 不同 file 视为不同调用点：各自独立缓存
    #[test]
    fn plugin_log_metadata_file_is_cache_key_part() {
        let m1 = plugin_log_metadata(Level::DEBUG, "a.rs", 1);
        let m2 = plugin_log_metadata(Level::DEBUG, "b.rs", 1);
        assert!(!std::ptr::eq(m1, m2));
        assert_eq!(m1.file(), Some("a.rs"));
        assert_eq!(m2.file(), Some("b.rs"));
    }

    // ==================== emit_plugin_log ====================

    /// 消息带 [plugin:{id}] 前缀（保持旧格式，日志可区分来源插件），
    /// 调用点位置透传到 Metadata
    #[test]
    fn emit_plugin_log_formats_message_with_prefix() {
        let captured = capture(|| {
            emit_plugin_log("my-plugin", Level::WARN, "virtual.rs", 42, "boom");
        });
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].level, Level::WARN);
        assert_eq!(captured[0].file, Some("virtual.rs"));
        assert_eq!(captured[0].line, Some(42));
        assert_eq!(captured[0].message, "[plugin:my-plugin] boom");
    }

    /// 四个级别宏映射到对应 Level（组件形态调用点为空字符串/0）
    #[test]
    fn log_level_macros_map_to_levels() {
        let captured = capture(|| {
            log_info("p", "i", "", 0);
            log_debug("p", "d", "", 0);
            log_warn("p", "w", "", 0);
            log_error("p", "e", "", 0);
        });
        assert_eq!(captured.len(), 4);
        let levels: Vec<Level> = captured.iter().map(|e| e.level.clone()).collect();
        assert_eq!(levels, vec![Level::INFO, Level::DEBUG, Level::WARN, Level::ERROR]);
        // 前缀对各级别一致生效
        assert_eq!(captured[3].message, "[plugin:p] e");
    }

    // ==================== per-plugin 日志级别 ====================

    /// env 修改测试串行化：BEDCODE_PLUGIN_LOG 是进程级环境变量，并行测试
    /// 同时 set_var 会互相污染；阈值缓存首次访问后不再重读 env，重置后
    /// 的重新读取同样需要串行保护
    static ENV_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// 在受保护区域设置 env + 重置缓存，执行闭包后还原 env
    fn with_plugin_log_env(raw: &str, f: impl FnOnce()) {
        let _guard = ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let prev = std::env::var("BEDCODE_PLUGIN_LOG").ok();
        std::env::set_var("BEDCODE_PLUGIN_LOG", raw);
        reset_plugin_log_thresholds();
        f();
        reset_plugin_log_thresholds();
        match prev {
            Some(v) => std::env::set_var("BEDCODE_PLUGIN_LOG", v),
            None => std::env::remove_var("BEDCODE_PLUGIN_LOG"),
        }
    }

    /// 解析容错：多条目 / 非法级别忽略 / 空条目忽略 / 未设环境变量空映射
    #[test]
    fn load_plugin_log_thresholds_tolerant_parsing() {
        let _guard = ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("BEDCODE_PLUGIN_LOG", "a=trace,b=warn,,c=verbose,x=");
        let map = load_plugin_log_thresholds();
        std::env::remove_var("BEDCODE_PLUGIN_LOG");
        assert_eq!(map.get("a"), Some(&Level::TRACE));
        assert_eq!(map.get("b"), Some(&Level::WARN));
        // 非法级别（verbose）、空 id、空级别一律忽略
        assert!(!map.contains_key("c"));
        assert!(!map.contains_key("x"));
        assert_eq!(map.len(), 2);

        std::env::remove_var("BEDCODE_PLUGIN_LOG");
        assert!(load_plugin_log_thresholds().is_empty());
    }

    /// 低于阈值的级别被丢弃、高于阈值的正常出现、未列出的插件不受影响
    #[test]
    fn emit_plugin_log_filters_below_threshold() {
        with_plugin_log_env("noisy=warn", || {
            let captured = capture(|| {
                // noisy 阈值 warn：debug/info 丢弃，warn/error 保留
                log_debug("noisy", "d", "", 0);
                log_info("noisy", "i", "", 0);
                log_warn("noisy", "w", "", 0);
                log_error("noisy", "e", "", 0);
                // 未列出的插件沿用全局语义，全部正常出现
                log_debug("quiet", "d", "", 0);
            });
            let levels: Vec<Level> = captured.iter().map(|e| e.level.clone()).collect();
            assert_eq!(levels, vec![Level::WARN, Level::ERROR, Level::DEBUG]);
            assert_eq!(captured[0].message, "[plugin:noisy] w");
            assert_eq!(captured[1].message, "[plugin:noisy] e");
            assert_eq!(captured[2].message, "[plugin:quiet] d");
        });
    }

    /// 未设置环境变量时全部插件不受过滤（回归保护：既有行为不变）
    #[test]
    fn emit_plugin_log_no_env_means_no_filtering() {
        with_plugin_log_env("", || {
            let captured = capture(|| {
                log_debug("any", "d", "", 0);
                log_info("any", "i", "", 0);
            });
            assert_eq!(captured.len(), 2);
        });
    }
}
