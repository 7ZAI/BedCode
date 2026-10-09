//! 宿主能力：插件独立数据库访问（host-plugin-database）

use super::HostError;

/// 插件独立数据库访问
///
/// 每个插件拥有独立的 .db 文件与连接，**无表名前缀约束**，
/// 无全局 Mutex 竞争。需要 `storage` 权限。
///
/// **SQL 注入防护**：始终优先使用 `*_params` 参数绑定版本。
///
/// **已知限制**：BLOB 读出 hex、参数绑定无字节数组类型、
/// i64 > 2^53 经 JSON number 往返丢精度——主键建议 TEXT / 自增整数。
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
    /// 语义同 `db_execute_batch`（主库形态已随 host-database 退役），无表名前缀约束。
    fn plugin_db_execute_batch(&self, sqls: &[String]) -> Result<i32, HostError>;
}
