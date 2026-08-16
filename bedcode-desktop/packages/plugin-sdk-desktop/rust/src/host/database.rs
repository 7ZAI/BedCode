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
}

/// 插件独立数据库访问
///
/// 每个插件拥有独立的 .db 文件与连接，**无表名前缀约束**，
/// 无全局 Mutex 竞争。需要 `storage` 权限。
///
/// **SQL 注入防护**：始终优先使用 `*_params` 参数绑定版本。
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
}
