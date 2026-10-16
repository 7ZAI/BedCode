//! 宿主能力：插件私有库访问（host-plugin-database）
//!
//! `host-database`（主库）随 2026-10-09 双端机制决策退役——主库由 wasm-core
//! 管理、不给插件直接调用（ABI 18→19）；本文件只保留插件私有库面。

use super::HostError;

/// 插件私有库访问（host-plugin-database）
///
/// 每插件一个属主分区（独立 SQLite 库）；仅本插件可访问，停用回收。
/// 需要 `storage` 权限。参数化与批处理语义同 [`HostDatabase`]。
pub trait HostPluginDatabase {
    /// 执行 SQL，返回受影响行数
    fn plugin_db_execute(&self, sql: &str) -> Result<i32, HostError>;

    /// 查询 SQL，返回行数组 JSON；无结果返回 `Ok(None)`
    fn plugin_db_query(&self, sql: &str) -> Result<Option<serde_json::Value>, HostError>;

    /// 参数化执行
    fn plugin_db_execute_params(
        &self,
        sql: &str,
        params: &[serde_json::Value],
    ) -> Result<i32, HostError>;

    /// 参数化查询
    fn plugin_db_query_params(
        &self,
        sql: &str,
        params: &[serde_json::Value],
    ) -> Result<Option<serde_json::Value>, HostError>;

    /// 事务内顺序执行多语句，返回受影响行数合计
    fn plugin_db_execute_batch(&self, sqls: &[String]) -> Result<i32, HostError>;
}
