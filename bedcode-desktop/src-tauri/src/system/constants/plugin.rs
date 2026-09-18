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

/// 插件包文件名（zip 安装约定）
pub const PLUGIN_MANIFEST_FILE: &str = "plugin.json";

/// WASM 模块文件扩展名
pub const WASM_FILE_EXT: &str = ".wasm";

/// 用户安装来源标记（写于插件目录，与移动端约定一致）
pub const PLUGIN_SOURCE_MARKER: &str = ".bedcode-source";

/// 来源标记值：本地 zip 安装
pub const SOURCE_FILE_INSTALL: &str = "file-install";

/// zip 安装临时目录（用户插件目录下，安装失败/完成后清理）
pub const PLUGIN_DOWNLOAD_TEMP_DIR: &str = "plugins/_download_tmp";

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

// ==================== host-database 执行护栏（票据 05，spec `.scratch/2026-09-18-db-http-base-service/`） ====================

/// 插件 SQL 语句执行超时（秒）：
///
/// 主库是内核与全部插件共用的单一连接（全局 Mutex），慢查询会阻塞配对/会话配置/
/// 设置等全部内核 DB 读写。SQLite progress handler 在宿主侧硬中断超时语句，
/// 超时调用返回「查询超时」类错误。保守起步，语义对齐 HTTP 非流式总超时 `PLUGIN_HTTP_TIMEOUT_SECS`。
/// 护栏不可由插件参数调整（安全边界，AGENTS.md §8）。
pub const PLUGIN_DB_STATEMENT_TIMEOUT_SECS: u64 = 120;

/// 插件 SQL 查询结果集行数上限：
///
/// 取行循环内计数，超限立即截断并返回带明确说明的错误（引导插件加 LIMIT 或分批）。
/// 查询面（query）结果行会经 canonical ABI 拷入插件线性内存，无上限结果集可能耗尽
/// 单次调用 fuel 预算触发 trap 污染 Store。
pub const PLUGIN_DB_QUERY_MAX_ROWS: usize = 10_000;

/// 插件 SQL 查询结果集序列化字节上限（对齐 HTTP 响应体上限 32MB 的既有模式）：
///
/// 取行循环内对每行序列化长度累计计数，超限立即截断报错。
pub const PLUGIN_DB_QUERY_MAX_BYTES: usize = 32 * 1024 * 1024;

/// 插件 execute-batch 单次调用语句数上限：
///
/// 事务持有全局连接锁期间其他插件/内核调用会等待，语句数上限 + 05 超时护栏
/// 双兜底避免长事务阻塞内核 DB（票据 06）。
pub const PLUGIN_DB_EXECUTE_BATCH_MAX_STATEMENTS: usize = 64;

// ==================== host-websocket（ABI v14，spec `.scratch/2026-09-18-ws-base-service/`） ====================

/// 插件 WS 客户端域握手超时上限（秒）：`connect` 同步阻塞至握手完成，
/// 插件传入的 `connect-timeout-secs` 超过本值即截断（spec D4 / §4.4）
pub const PLUGIN_WS_CONNECT_TIMEOUT_SECS: u64 = 10;

/// 插件 WS 单帧/单消息字节上限兜底（配置不可读时使用）。
///
/// 正常路径取 `server::app::ws_frame_limit()`（网络配置：ws_max_frame_size_kb ×
/// ws_max_message_size_mb），与移动端终端链路同一事实源（spec §4.4）
pub const PLUGIN_WS_MAX_MESSAGE_BYTES: usize = 1024 * 1024;

/// 每条 WS 连接的发送队列长度：入队成功即 `Ok`（不代表已送达对端），
/// 队列满立即 `Err`（fail-visible，宿主不做无界缓冲与背压等待，spec D10）
pub const PLUGIN_WS_SEND_QUEUE_CAPACITY: usize = 64;

/// 单插件 WS 客户端域（出站）连接数上限：超限 `connect` 直接 `Err`（spec §4.4）
pub const PLUGIN_WS_MAX_CONNS_PER_PLUGIN: usize = 32;

/// 单端点入站客户端数上限：超限在协议升级前拒绝（503，不产生连接事件，spec §4.4）
pub const PLUGIN_WS_MAX_CLIENTS_PER_ENDPOINT: usize = 64;

/// 单插件可注册端点数量上限：超限 `register-endpoint` 返回 `Err`，无副作用
pub const PLUGIN_WS_MAX_ENDPOINTS_PER_PLUGIN: usize = 16;

/// 插件端点首消息认证窗口（秒）：`auth:"jwt"` 未在窗口内完成认证 → close(4001)
/// （默认对齐终端链路 `WS_AUTH_TIMEOUT_SECS`，spec D8）
pub const PLUGIN_WS_AUTH_TIMEOUT_SECS: u64 = 10;

/// 插件端点 path 段长度上限（字符）：防超长路径占用路由匹配（spec §4.2）
pub const PLUGIN_WS_ENDPOINT_PATH_MAX_LEN: usize = 64;

/// 环境变量：BedCode PTY 会话 ID
pub const ENV_BEDCODE_SESSION_ID: &str = "BEDCODE_SESSION_ID";
