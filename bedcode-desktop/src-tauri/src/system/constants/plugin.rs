//! 插件系统相关常量
//!
//! 共享常量定义在 SDK `bedcode-plugin-api`（单一事实来源），此处 re-export；
//! 本模块仅保留宿主专有常量

/// 共享常量（Claude Code 目录名 / 设置文件 / hook 脚本 / 端口环境变量）
pub use bedcode_plugin_api::constants::{
    CLAUDE_CONFIG_DIR_NAME, CLAUDE_SETTINGS_FILE, ENV_BEDCODE_PORT, HOOK_SCRIPT_NAME,
};

/// 插件回调超时（秒）
///
/// on_startup / on_shutdown 等插件回调的最大执行时间
pub const PLUGIN_CALLBACK_TIMEOUT_SECS: u64 = 5;

/// 插件热重载防抖时间（毫秒）
///
/// 同一插件在防抖窗口内只触发一次重载，避免 cargo build 连续写入多次触发
pub const PLUGIN_RELOAD_DEBOUNCE_MS: u64 = 500;

/// 插件 HTTP 代理连接超时（秒）
pub const PLUGIN_HTTP_CONNECT_TIMEOUT_SECS: u64 = 10;

/// 插件 HTTP 代理非流式请求总超时（秒）
///
/// 流式请求不设总超时（长连接不应被截断），仅受连接超时约束
pub const PLUGIN_HTTP_TIMEOUT_SECS: u64 = 120;

/// 插件 HTTP 代理非流式响应体上限（字节）
///
/// 非流式 `http_fetch` 响应体会经 canonical ABI 拷入插件线性内存并由插件
/// serde 解析（guest 指令，消耗单次导出调用 fuel 预算）。无上限响应体可能耗尽
/// fuel 触发 trap 污染 Store（`CannotEnterComponent`）；大载荷必须走
/// `stream:true` 流式模式（宿主后台任务经事件逐 chunk 推送，不经 guest 内存）。
/// 32MB 对目录列举/元数据绰绰有余（guest 解析约几 G 指令，远低于 FUEL_PER_CALL 64G）。
pub const PLUGIN_HTTP_RESPONSE_BODY_LIMIT_BYTES: usize = 32 * 1024 * 1024;

/// 环境变量：BedCode PTY 会话 ID
pub const ENV_BEDCODE_SESSION_ID: &str = "BEDCODE_SESSION_ID";
