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
    cache
        .entry((file.to_string(), line, level))
        .or_insert_with(|| {
            // 泄漏转 'static：同一调用点后续命中缓存，仅首次泄漏
            let file: &'static str = Box::leak(file.to_string().into_boxed_str());
            let fields = field::FieldSet::new(
                &["message"],
                callsite::Identifier(&PLUGIN_LOG_CALLSITE),
            );
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

/// 以插件调用点位置发出 tracing 事件（消息带 `[plugin:xxx]` 前缀，保持旧格式）
fn emit_plugin_log(plugin_id: &str, level: Level, file: &str, line: u32, message: &str) {
    let meta = plugin_log_metadata(level, file, line);
    let fieldset = meta.fields();
    let formatted = format!("[plugin:{}] {}", plugin_id, message);
    // value_set_all 按字段集顺序填充值；插件日志字段集仅 "message" 一项
    let display_value = field::display(formatted);
    let value_slots = [Some(&display_value as &dyn Value)];
    let values = fieldset.value_set_all(&value_slots);
    Event::dispatch(&meta, &values);
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

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tracing::field::Visit;
    use tracing::span;
    use tracing::subscriber::with_default;
    use tracing::Subscriber;

    /// 捕获的单个事件（level / 调用点 / 消息）
    #[derive(Clone)]
    struct CapturedEvent {
        level: Level,
        file: Option<&'static str>,
        line: Option<u32>,
        message: String,
    }

    /// 极简订阅者：把 event 原样记录到共享 vec，供断言
    ///
    /// 与项目默认订阅者（文件落盘）互不影响：with_default 只替换当前线程
    /// 的默认订阅者，且测试通过共享 Arc 取回捕获结果
    struct CaptureSubscriber {
        events: Arc<Mutex<Vec<CapturedEvent>>>,
    }

    /// 从 event 字段集中提取 message（插件日志字段集仅 "message" 一项）
    struct MessageVisitor(String);

    impl Visit for MessageVisitor {
        fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
            if field.name() == "message" {
                self.0 = value.to_string();
            }
        }

        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            if field.name() == "message" {
                self.0 = format!("{:?}", value);
            }
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
            let mut visitor = MessageVisitor(String::new());
            event.record(&mut visitor);
            self.events.lock().unwrap().push(CapturedEvent {
                level: *event.metadata().level(),
                file: event.metadata().file(),
                line: event.metadata().line(),
                message: visitor.0,
            });
        }

        fn enter(&self, _span: &span::Id) {}

        fn exit(&self, _span: &span::Id) {}
    }

    /// 以捕获订阅者为默认订阅者执行闭包，返回捕获到的事件列表
    fn capture<F: FnOnce()>(f: F) -> Vec<CapturedEvent> {
        let events = Arc::new(Mutex::new(Vec::new()));
        let subscriber = CaptureSubscriber { events: events.clone() };
        with_default(subscriber, f);
        let captured = events.lock().unwrap();
        captured.clone()
    }

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
}
