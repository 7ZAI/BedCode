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
