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

// ==================== host-pty（ABI v16，spec `.scratch/2026-09-19-pty-base-service/`） ====================

/// 每插件在册私有 PTY 数量上限：超限 `spawn` 直接 `Err`（fail-visible，不排队、
/// 不静默淘汰自己已有的句柄，spec D9）
///
/// 取值依据：一条私有 PTY 的资源画像 = 一个子进程 + 一对 fd + 一条读线程 + 一块
/// 输出环，与 `PLUGIN_WS_MAX_CONNS_PER_PLUGIN`（出站连接 8 条）同档；8 条够
/// 「多 shell 并发」型插件的常态用量，且越界不牵连同宿主其他插件的配额。
pub const PLUGIN_PTY_MAX_SESSIONS_PER_PLUGIN: usize = 8;

/// 插件私有 PTY 输出环的**默认**容量（字节）：spawn config 未声明 `ringBytes` 时取本值
///
/// 满则淘汰最旧字节（消费者以 `truncated` 感知缺口）——**淘汰只发生在插件自己的
/// 历史上，绝不把背压踢回 PTY 读取端**（spec D3）。刻意不与业务会话环
/// （`channels.global_queue_max_bytes`，每会话 50 MB 档）同档：插件环随 pty 句柄
/// 存活、每插件可有多条，256 KB 已够一个 TUI 全屏重绘数十帧。需要更深历史的插件
/// 在 spawn 时自行声明 `ringBytes`（上限见 [`PLUGIN_PTY_RING_MAX_BYTES`]）。
pub const PLUGIN_PTY_RING_BYTES: u64 = 256 * 1024;

/// 插件可声明的 `ringBytes` 上限：超过即 `spawn` 返回 `Err`
///
/// **不夹取到上限**——静默降级会让插件按自己声明的深度规划上下文、实际却少得多，
/// 与「配额失败要可见」的分级一致（spec D9）。取值依据：单插件 8 条 PTY 全开即
/// 32 MiB 常驻上界，仍显著小于业务线单条会话队列的 50 MB 档。
pub const PLUGIN_PTY_RING_MAX_BYTES: u64 = 4 * 1024 * 1024;

/// 单次 `ring-fetch` 返回字节上限：限制一次 wasm 边界拷贝的量
/// （插件传入的 `max-bytes` 超过本值即**截断**——读侧是数据面，截断不是错误，
/// 余下字节按 `next-offset` 续拉即可，spec D9）
pub const PLUGIN_PTY_RING_FETCH_MAX_BYTES: u32 = 16 * 1024;

/// 单次 `write` 字节上限：超限直接 `Err`（fail-visible，不静默截断，spec D6/D9）
///
/// 与引擎侧 4000 字节分块 + 逐块让出（`PtySession::write`）协调：本值是「一次调用」
/// 的准入上限，分块是上限之内的投递节奏。取值 64 KiB ≈ 16 个分块，够一次粘贴级输入。
pub const PLUGIN_PTY_MAX_WRITE_BYTES: usize = 64 * 1024;

// ==================== host-session 输出环（票 04，output-ring-fetch） ====================

/// 单次 `output-ring-fetch` 返回字节上限：限制一次 wasm 边界拷贝的量
/// （插件传入的 `max-bytes` 超过本值即**截断**——读侧是数据面，截断不是错误，
/// 余下字节按 `next-offset` 续拉即可；与 [`PLUGIN_PTY_RING_FETCH_MAX_BYTES`]
/// 同档——两条环共享同一 wasm 边界成本模型，spec 票 04）
pub const PLUGIN_SESSION_RING_FETCH_MAX_BYTES: u32 = 16 * 1024;

/// 环境变量：BedCode PTY 会话 ID
pub const ENV_BEDCODE_SESSION_ID: &str = "BEDCODE_SESSION_ID";

// ==================== host-task（ABI v20，spec `.scratch/2026-09-21-host-task-concurrency/`） ====================

/// 全局池线程数：宿主专用 OS 线程池执行插件单元操作计划的并行度。
///
/// **不与 tokio blocking 池共用**：`spawn_blocking` 池与 PTY 读线程、WS 任务共享，
/// 插件批量单元可饿死它们；专用池独立队列 + 利用率可观测（spec §5.2）。
/// 取值 8：经 block_on_async ambient 桥的 IO/CPU 混合单元，8 线程已覆盖典型
/// 并发扇出（50 stat / 3 git），且不与宿主主 runtime 抢调度。
pub const PLUGIN_TASK_POOL_THREADS: usize = 8;

/// 每插件并发在册任务上限（含 running + queued）：超限 `submit` / `execute-batch`
/// 直接 `Err`（fail-visible，宿主仲裁、不排队、不静默降级，spec §7）
pub const PLUGIN_TASK_MAX_JOBS_PER_PLUGIN: usize = 4;

/// 单计划单元数上限：超限 plan 整体拒绝（防单批超大 plan 打爆池队列）
pub const PLUGIN_TASK_MAX_UNITS_PER_PLAN: usize = 256;

/// 单元结果上限（字节，JSON 文本长度）：超出**截断** + `truncated` 标记
/// （保护回调载荷与插件线性内存，spec §7；大结果别靠 status 兜底）
pub const PLUGIN_TASK_UNIT_RESULT_MAX_BYTES: usize = 1024 * 1024;

/// 单元缺省超时（毫秒）：plan 未给 `timeoutMs` 时按此执行（与 process
/// DEFAULT_TIMEOUT_MS 同档）；超时按该单元失败收集（fail-collect，不拖垮任务）
pub const PLUGIN_TASK_UNIT_TIMEOUT_MS: u64 = 600_000;

/// 任务墙钟缺省超时（毫秒）：plan 未给 `jobTimeoutMs` 时按此执行；超时 →
/// cancelled + 已完成单元结果保留（运行中单元跑完或超时）
pub const PLUGIN_TASK_JOB_TIMEOUT_MS: u64 = 3_600_000;

/// 每插件回调队列深度：`events-task` 事件的有界队列。溢出策略：progress 事件可丢
/// （丢弃 + warn + `droppedEvents` 计数，status 可见）；terminal 事件优先入队，
/// 极端情况下 terminal 也丢则 error! 留痕——回调是尽力投递，`status` 是权威快照
/// （spec §5.3）
pub const PLUGIN_TASK_CALLBACK_QUEUE_DEPTH: usize = 64;

/// status 终态结果保留条数上限：超出只留计数（大结果别靠 status 兜底，spec §7）
pub const PLUGIN_TASK_STATUS_RESULTS_MAX: usize = 64;
