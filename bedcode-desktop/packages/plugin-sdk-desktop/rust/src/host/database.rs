//! 宿主能力：数据库访问（主库前缀隔离 / 插件独立库）

use super::HostError;

/// 宿主主数据库访问
///
/// 操作宿主 SQLite 主库，**表名必须以 `plugin_{sanitized_plugin_id}_` 为前缀**
/// （宿主侧强制校验，防止插件读写宿主或其他插件的表）。
/// 需要 `storage` 权限。
///
/// 优先使用 [`HostPluginDatabase`]（插件独立库，无前缀约束、无全局锁竞争）；
/// 仅当确需与宿主数据同库时使用此 trait。
///
/// **SQL 注入防护**：始终优先使用 `*_params` 参数绑定版本，
/// 不要手工拼接/转义用户数据（`replace('\'', "''")` 模式已废弃）。
///
/// **已知限制（票据 07 文档化契约，无 ABI 变更）**：
/// - BLOB 二进制：查询 BLOB 列输出 **hex 字符串**（如 `"0aff"`），参数绑定无字节数组
///   类型（数组/对象参数 fallback 为 JSON 字符串）。需要二进制存储时只能 hex 存 TEXT
///   或走 host-fs 文件。
/// - i64 精度：查询 i64 → JSON number → 前端 JS 数字是双精度，**> 2^53 会丢精度**。
///   主键/大整数请用 TEXT 或自增整数，不要依赖 > 2^53 的 JSON number 往返。
pub trait HostDatabase {
    /// 执行 SQL，返回受影响行数
    fn db_execute(&self, sql: &str) -> Result<i32, HostError>;

    /// 查询 SQL，返回行数组 JSON；无结果返回 `Ok(None)`
    fn db_query(&self, sql: &str) -> Result<Option<serde_json::Value>, HostError>;

    /// 执行 SQL（参数绑定版）
    ///
    /// SQL 中用 `?1`、`?2` …（或 `?`）占位，`params` 按序绑定（rusqlite 真绑定，防注入）
    fn db_execute_params(&self, sql: &str, params: &[serde_json::Value]) -> Result<i32, HostError>;

    /// 查询 SQL（参数绑定版），返回行数组 JSON；无结果返回 `Ok(None)`
    fn db_query_params(&self, sql: &str, params: &[serde_json::Value]) -> Result<Option<serde_json::Value>, HostError>;

    /// 事务内顺序执行多语句（execute-batch，主库）
    ///
    /// 单次调用内宿主持有连接锁、事务中依次执行 `sqls`，任一句失败整体回滚，
    /// 返回受影响行数合计（票据 06）。禁止跨调用裸 BEGIN/COMMIT——宿主对
    /// `db_execute` / `db_execute_params` 做事务控制语句白名单检测并引导至本接口。
    /// 批次语句数有上限（宿主常量，不可由插件调整）。
    fn db_execute_batch(&self, sqls: &[String]) -> Result<i32, HostError>;
}

/// 插件独立数据库访问
///
/// 每个插件拥有独立的 .db 文件与连接，**无表名前缀约束**，
/// 无全局 Mutex 竞争。需要 `storage` 权限。
///
/// **SQL 注入防护**：始终优先使用 `*_params` 参数绑定版本。
///
/// **已知限制**：与 [`HostDatabase`] 相同（BLOB 读出 hex、参数绑定无字节数组类型、
/// i64 > 2^53 经 JSON number 往返丢精度——主键建议 TEXT / 自增整数）。
pub trait HostPluginDatabase {
    /// 执行 SQL，返回受影响行数
    fn plugin_db_execute(&self, sql: &str) -> Result<i32, HostError>;

    /// 查询 SQL，返回行数组 JSON；无结果返回 `Ok(None)`
    fn plugin_db_query(&self, sql: &str) -> Result<Option<serde_json::Value>, HostError>;

    /// 执行 SQL（参数绑定版）
    ///
    /// SQL 中用 `?1`、`?2` …（或 `?`）占位，`params` 按序绑定
    fn plugin_db_execute_params(&self, sql: &str, params: &[serde_json::Value]) -> Result<i32, HostError>;

    /// 查询 SQL（参数绑定版），返回行数组 JSON；无结果返回 `Ok(None)`
    fn plugin_db_query_params(&self, sql: &str, params: &[serde_json::Value]) -> Result<Option<serde_json::Value>, HostError>;

    /// 事务内顺序执行多语句（execute-batch，插件独立库）
    ///
    /// 语义同 [`HostDatabase::db_execute_batch`]，无表名前缀约束。
    fn plugin_db_execute_batch(&self, sqls: &[String]) -> Result<i32, HostError>;
}
