//! 移动端日志初始化与 dev 噪音源头抑制
//!
//! 本模块取代直接调用 `android_logger::init_once`：dev（debug）构建下 logcat 打满
//! Debug，而 Android logcat 主缓冲是系统级环形（每 app 约 256KB~1MB），两类**库内部**
//! 噪音会把业务日志与崩溃证据（FATAL / tombstone）挤出缓冲：
//!
//! 1. **dev 服务代理连接日志**：dev 构建下 WebView 的地址是 `tauri://localhost`，
//!    Tauri 的 `tauri://` protocol handler 用 reqwest 把**每个 WebView 请求**
//!    （模块 import / HMR / 插件前端 import）转发到 `devUrl`（vite dev server，经
//!    adb reverse 落在设备的 `127.0.0.1:<devUrl 端口>`）。reqwest / hyper_util 对每次
//!    新建连接打 Debug ⇒ 实测 3.5 分钟 1400+ 行，占该次 logcat 输出 ~7 成。
//! 2. **wasmtime / cranelift JIT 编译内部 debug**：插件加载瞬间洪水刷屏（历史实测
//!    单日 dev 输出里量级最大的一类）；`mdns_sd::` 库内部 debug 同属此类。
//!
//! 判定口径与宿主侧 `:dev:log` 落盘过滤脚本（`scripts/lib/dev-log-filter.js`）一致：
//! - 库内部 target（cranelift / wasmtime JIT / mdns_sd）的 Debug/Trace 一律丢弃——
//!   纯实现细节，无排障价值；
//! - HTTP 客户端 target 只丢弃**消息含回环 host** 的记录：真机上回环连接只可能是
//!   adb reverse 的 dev 代理（局域网对端是 LAN IP），因此连真实对端的 `reqwest`
//!   连接日志与全部 Info 及以上记录照常保留（认证 / 链路排障依赖它们）。
//!
//! 两道防线分工（改判定口径时同步）：
//! - 本模块：设备侧 logcat（保护环形缓冲，真机崩溃现场不被冲掉）；
//! - 脚本：电脑端控制台与 `.dev-logs` 落盘（按 `devUrl` 的 host:port 精确匹配，
//!   可覆盖 `--host` 指向 LAN 地址的开发模式；本模块按回环匹配、不依赖 devUrl）。
//!
//! release 构建级别收敛到 Info，上述 Debug/Trace 记录本来就进不了 logger，本模块
//! 在其中只是透传（不额外分叉，保持单一初始化路径）。

use log::{Level, LevelFilter, Log, Metadata, Record};

/// logcat tag（与直接 `init_once` 时期保持一致）
const LOG_TAG: &str = "BedCode";

/// 库内部 debug 噪音 target 的 **crate 名**（与落盘过滤脚本同口径）
const NOISY_LIB_CRATES: &[&str] = &[
    "cranelift_codegen",
    "wasmtime_internal_cranelift",
    "wasmtime_cranelift",
    "mdns_sd",
];

/// wasmtime 内部 JIT 内存管理 target（模块路径而非 crate 名，故单独按段判定）
const WASMTIME_CODE_MEMORY_TARGET: &str = "wasmtime::runtime::code_memory";

/// 同上 target 的子模块前缀（避免 `code_memory_extra` 这类同前缀模块误判）
const WASMTIME_CODE_MEMORY_PREFIX: &str = "wasmtime::runtime::code_memory::";

/// HTTP 客户端库的 crate 名：只抑制其「连回环地址」的连接日志（见模块头注释）
const HTTP_CLIENT_CRATES: &[&str] = &["reqwest", "hyper_util", "hyper", "h2", "rustls", "tokio_rustls"];

/// 回环 host 字面量（设备上的回环连接 ⇔ adb reverse 的 dev 代理）
const LOOPBACK_HOSTS: &[&str] = &["127.0.0.1", "localhost", "[::1]"];

/// 初始化全局 logger
///
/// 幂等：Android Activity 重建等场景重复调用时保留已注册的 logger
/// （`log::set_boxed_logger` 只在首次成功，第二次返回 `SetLoggerError`）。
pub fn init(log_level: LevelFilter) {
    let config = android_logger::Config::default()
        .with_max_level(log_level)
        .with_tag(LOG_TAG);
    let logger = DevNoiseFilter::new(android_logger::AndroidLogger::new(config));

    if let Err(err) = log::set_boxed_logger(Box::new(logger)) {
        // 重复初始化：logger 已注册，max level 也已设过，无需重设
        log::debug!("logging: set_boxed_logger failed: {err}");
    } else {
        log::set_max_level(log_level);
        // dev 期确认本层已装配（排障锚点：此后 logcat 不应再出现回环 dev 代理连接行）
        #[cfg(debug_assertions)]
        log::info!("logging: dev 噪音源头抑制已装配（dev 代理回环连接 + wasmtime/cranelift 内部 debug）");
    }
}

/// 带 dev 噪音抑制的 android_logger 包装（判定逻辑见模块头注释）
///
/// 泛型只为单测可注入计数替身（生产恒为 `AndroidLogger`）；struct 私有，不外泄类型。
struct DevNoiseFilter<L: Log> {
    inner: L,
}

impl<L: Log> DevNoiseFilter<L> {
    fn new(inner: L) -> Self {
        Self { inner }
    }
}

impl<L: Log> Log for DevNoiseFilter<L> {
    fn enabled(&self, metadata: &Metadata) -> bool {
        self.inner.enabled(metadata)
    }

    fn log(&self, record: &Record) {
        if is_dev_noise(record) {
            return;
        }
        self.inner.log(record);
    }

    fn flush(&self) {
        self.inner.flush();
    }
}

/// 判定一条日志记录是否属于 dev 噪音（纯函数，单测覆盖）
///
/// 顺序：级别闸门（Info 及以上一律放行）→ 库内部 target → HTTP 客户端 target +
/// 消息含回环 host。提前返回的路径不格式化消息（非 HTTP 客户端记录零额外开销）。
fn is_dev_noise(record: &Record) -> bool {
    if record.level() < Level::Debug {
        return false;
    }

    let target = record.target();
    // crate 名 = target 的第一段（target 即模块路径，如
    // `hyper_util::client::legacy::connect::http`）；前缀匹配会把
    // `reqwest_extra` / `hyper_utilx` 这类不同 crate 误判，必须按段相等
    let crate_name = target.split("::").next().unwrap_or_default();
    if NOISY_LIB_CRATES.contains(&crate_name) {
        return true;
    }
    if target == WASMTIME_CODE_MEMORY_TARGET || target.starts_with(WASMTIME_CODE_MEMORY_PREFIX) {
        return true;
    }
    if !HTTP_CLIENT_CRATES.contains(&crate_name) {
        return false;
    }

    // HTTP 客户端记录只有消息里带目标地址；回环 ⇒ dev 服务代理
    let message = record.args().to_string();
    LOOPBACK_HOSTS.iter().any(|host| message.contains(host))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造 log 记录（判定只看 level / target / 消息，与真实记录同形）
    fn is_noise(level: Level, target: &str, message: &str) -> bool {
        // 先绑定再传：`format_args!` 的临时值必须在 Record 存活期间有效
        let args = format_args!("{message}");
        let record = Record::builder().level(level).target(target).args(args).build();
        is_dev_noise(&record)
    }

    #[test]
    fn drops_dev_proxy_connections_when_message_targets_loopback() {
        // 实测刷屏样例（reqwest / hyper_util 对每次 WebView 请求打 debug）
        assert!(is_noise(
            Level::Debug,
            "reqwest::connect",
            "starting new connection 'Some(\"127.0.0.1\")'"
        ));
        assert!(is_noise(
            Level::Debug,
            "hyper_util::client::legacy::connect::http",
            "connecting to 127.0.0.1:1423"
        ));
        assert!(is_noise(
            Level::Debug,
            "hyper_util::client::legacy::connect::http",
            "connected to 127.0.0.1:1423"
        ));
        assert!(is_noise(
            Level::Debug,
            "hyper_util::client::legacy::pool",
            "pooling idle connection for (\"http\", 127.0.0.1:1423)"
        ));
    }

    #[test]
    fn drops_loopback_alias_forms() {
        assert!(is_noise(
            Level::Debug,
            "hyper::client::connect::http",
            "connecting to localhost:1423"
        ));
        assert!(is_noise(
            Level::Trace,
            "h2::codec::framed_write",
            "flushing frame to [::1]:1423"
        ));
    }

    #[test]
    fn keeps_lan_peer_connections() {
        // 反例防线：连真实对端的链路排障日志必须保留（局域网对端不是回环）
        assert!(!is_noise(
            Level::Debug,
            "reqwest::connect",
            "starting new connection 'Some(\"192.168.1.5\")'"
        ));
        assert!(!is_noise(
            Level::Debug,
            "hyper_util::client::legacy::connect::http",
            "connecting to 192.168.1.5:8765"
        ));
        assert!(!is_noise(
            Level::Debug,
            "hyper_util::client::legacy::pool",
            "pooling idle connection for (\"http\", 10.0.0.2:8765)"
        ));
    }

    #[test]
    fn keeps_info_and_above_even_for_loopback_http_clients() {
        // 反例防线：错误 / 告警级记录不得被噪音抑制吞掉
        assert!(!is_noise(
            Level::Info,
            "reqwest::connect",
            "starting new connection 'Some(\"127.0.0.1\")'"
        ));
        assert!(!is_noise(
            Level::Warn,
            "hyper_util::client::legacy::connect::http",
            "connecting to 127.0.0.1:1423"
        ));
        assert!(!is_noise(
            Level::Error,
            "reqwest::async_impl::client",
            "error sending request to 127.0.0.1:1423"
        ));
    }

    #[test]
    fn keeps_business_targets_mentioning_loopback() {
        // 反例防线：只有 HTTP 客户端 crate 才按消息判定；
        // 业务日志里出现 127.0.0.1（如连接目标回显）必须保留
        assert!(!is_noise(
            Level::Debug,
            "bedcode_mobile_lib::system::info",
            "peer target 127.0.0.1 listed"
        ));
        assert!(!is_noise(
            Level::Debug,
            "bedcode_peer_net::identity",
            "connecting to 127.0.0.1"
        ));
    }

    #[test]
    fn crate_name_match_is_exact_by_segment() {
        // 反例防线：前缀匹配会误杀 reqwest_extra / hyper_utilx 这类别的 crate
        assert!(!is_noise(
            Level::Debug,
            "reqwest_extra::connect",
            "connecting to 127.0.0.1:1423"
        ));
        assert!(!is_noise(
            Level::Debug,
            "hyper_utilx::client",
            "connecting to 127.0.0.1:1423"
        ));
    }

    #[test]
    fn drops_wasmtime_and_cranelift_internals() {
        // 插件加载瞬间的 JIT 编译洪水（含 wasmtime 的 code_memory 模块）
        assert!(is_noise(
            Level::Debug,
            "cranelift_codegen::machinst::compile",
            "Number of CLIF instructions to lower: 18"
        ));
        assert!(is_noise(
            Level::Trace,
            "cranelift_codegen::timing::enabled",
            "timing: Ending VCode lowering: 0ms"
        ));
        assert!(is_noise(
            Level::Debug,
            "wasmtime_internal_cranelift::compiler",
            "compiled in 1.4ms"
        ));
        assert!(is_noise(Level::Debug, "wasmtime_cranelift::compiler", "x"));
        assert!(is_noise(
            Level::Debug,
            "wasmtime::runtime::code_memory",
            "memory image mapped"
        ));
        assert!(is_noise(Level::Debug, "mdns_sd::service_daemon", "daemon tick"));
    }

    #[test]
    fn keeps_other_wasmtime_targets_and_higher_levels() {
        // 反例防线：wasmtime 其余模块（如配置 / 错误路径）与 Info 及以上必须保留
        assert!(!is_noise(
            Level::Debug,
            "wasmtime::runtime::code_memory_extra",
            "not the code_memory module"
        ));
        assert!(!is_noise(Level::Info, "mdns_sd::service_daemon", "daemon started"));
        assert!(!is_noise(
            Level::Warn,
            "cranelift_codegen::machinst",
            "fallback to interpreter"
        ));
    }

    /// 计数替身：断言记录是否真的落到下游 logger（验证包装器的转发行为本身）
    #[derive(Default)]
    struct CountingSink {
        records: std::sync::Mutex<Vec<String>>,
    }

    impl Log for CountingSink {
        fn enabled(&self, _metadata: &Metadata) -> bool {
            true
        }

        fn log(&self, record: &Record) {
            self.records
                .lock()
                .expect("sink 锁不可中毒")
                .push(record.args().to_string());
        }

        fn flush(&self) {}
    }

    #[test]
    fn wrapper_forwards_business_records_and_blocks_noise() {
        // 行为契约：噪音记录被拦在包装器内（不触达下游 logger），业务记录原样转发
        let filter = DevNoiseFilter::new(CountingSink::default());

        let noise_args = format_args!("connecting to 127.0.0.1:1423");
        let noise = Record::builder()
            .level(Level::Debug)
            .target("hyper_util::client::legacy::connect::http")
            .args(noise_args)
            .build();
        filter.log(&noise);

        let business_args = format_args!("pty spawn ok");
        let business = Record::builder()
            .level(Level::Info)
            .target("bedcode_mobile_lib::pty")
            .args(business_args)
            .build();
        filter.log(&business);

        let records = filter.inner.records.lock().expect("sink 锁不可中毒");
        assert_eq!(*records, vec!["pty spawn ok"], "只有业务记录应触达下游 logger");
    }
}
