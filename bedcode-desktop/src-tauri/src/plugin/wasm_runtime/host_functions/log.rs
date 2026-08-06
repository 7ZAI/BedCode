//! 日志域 Host Functions（转发到宿主 tracing，附加 plugin_id 前缀）
//!
//! 插件通过 `#[track_caller]` 在调用侧捕获真实源码位置（file:line）并经
//! WASM ABI 传入（ABI v7），此处用插件位置构造 tracing Metadata，
//! 使控制台与日志文件中的 `at file:line` 指向插件代码而非宿主实现。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use super::memory::read_wasm_string_consume;
use crate::plugin::wasm_runtime::WasmPluginState;
use tracing::callsite::{self, Callsite};
use tracing::field::{self, Value, ValueSet};
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

/// 日志：info 级别
pub(super) fn host_log_info(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    msg_ptr: u32,
    msg_len: u32,
    file_ptr: u32,
    file_len: u32,
    line: u32,
) {
    let plugin_id = caller.data().plugin_id.clone();
    let message = read_wasm_string_consume(&mut caller, msg_ptr, msg_len).unwrap_or_default();
    let file = read_wasm_string_consume(&mut caller, file_ptr, file_len).unwrap_or_default();
    emit_plugin_log(&plugin_id, Level::INFO, &file, line, &message);
}

/// 日志：debug 级别
pub(super) fn host_log_debug(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    msg_ptr: u32,
    msg_len: u32,
    file_ptr: u32,
    file_len: u32,
    line: u32,
) {
    let plugin_id = caller.data().plugin_id.clone();
    let message = read_wasm_string_consume(&mut caller, msg_ptr, msg_len).unwrap_or_default();
    let file = read_wasm_string_consume(&mut caller, file_ptr, file_len).unwrap_or_default();
    emit_plugin_log(&plugin_id, Level::DEBUG, &file, line, &message);
}

/// 日志：warn 级别
pub(super) fn host_log_warn(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    msg_ptr: u32,
    msg_len: u32,
    file_ptr: u32,
    file_len: u32,
    line: u32,
) {
    let plugin_id = caller.data().plugin_id.clone();
    let message = read_wasm_string_consume(&mut caller, msg_ptr, msg_len).unwrap_or_default();
    let file = read_wasm_string_consume(&mut caller, file_ptr, file_len).unwrap_or_default();
    emit_plugin_log(&plugin_id, Level::WARN, &file, line, &message);
}

/// 日志：error 级别
pub(super) fn host_log_error(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    msg_ptr: u32,
    msg_len: u32,
    file_ptr: u32,
    file_len: u32,
    line: u32,
) {
    let plugin_id = caller.data().plugin_id.clone();
    let message = read_wasm_string_consume(&mut caller, msg_ptr, msg_len).unwrap_or_default();
    let file = read_wasm_string_consume(&mut caller, file_ptr, file_len).unwrap_or_default();
    emit_plugin_log(&plugin_id, Level::ERROR, &file, line, &message);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::fmt::MakeWriter;
    use tracing_subscriber::layer::SubscriberExt;

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
    fn plugin_log_event_uses_plugin_callsite_location() {
        let buf: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));

        let layer = tracing_subscriber::fmt::layer()
            .with_writer(SharedWriterMaker(buf.clone()))
            .with_ansi(false)
            .with_target(true)
            .with_file(true)
            .with_line_number(true);
        let subscriber = tracing_subscriber::registry().with(layer);

        tracing::subscriber::with_default(subscriber, || {
            // 模拟插件侧调用：宿主应记录插件源码位置（如 rust/src/queue.rs:382），
            // 而非宿主 log.rs 自身的位置
            emit_plugin_log("com.bedcode.auto-task", Level::INFO, "rust/src/queue.rs", 382, "hello world");
        });

        let out = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
        assert!(
            out.contains("rust/src/queue.rs:382"),
            "location line should point to plugin source, got:\n{out}"
        );
        assert!(
            out.contains("[plugin:com.bedcode.auto-task] hello world"),
            "message should keep plugin id prefix, got:\n{out}"
        );
    }

    #[test]
    fn plugin_log_metadata_cache_is_bounded_by_callsite() {
        // 同一调用点（file+line+level）只生成一份 'static metadata，重复调用命中缓存
        let m1 = plugin_log_metadata(Level::INFO, "rust/src/queue.rs", 382);
        let m2 = plugin_log_metadata(Level::INFO, "rust/src/queue.rs", 382);
        assert!(std::ptr::eq(m1, m2), "metadata should be interned per callsite");
        assert_eq!(m1.file(), Some("rust/src/queue.rs"));
        assert_eq!(m1.line(), Some(382));
        assert_eq!(m1.level(), &Level::INFO);

        // 不同级别 / 不同位置各成一份
        let m3 = plugin_log_metadata(Level::DEBUG, "rust/src/queue.rs", 382);
        let m4 = plugin_log_metadata(Level::INFO, "rust/src/state.rs", 20);
        assert!(!std::ptr::eq(m1, m3));
        assert!(!std::ptr::eq(m1, m4));
    }
}
