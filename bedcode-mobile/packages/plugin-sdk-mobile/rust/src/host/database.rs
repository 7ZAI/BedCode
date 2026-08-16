//! 宿主能力：数据库访问（移动端宿主仅暴露执行/查询两个 host function）

use super::HostError;

/// 宿主数据库访问
///
/// 操作宿主 SQLite 主库。需要 `storage` 权限。
/// 移动端宿主当前未做表名前缀强制校验，插件应自觉使用 `plugin_{id}_` 前缀区分自身数据。
pub trait HostDatabase {
    /// 执行 SQL，返回受影响行数
    fn db_execute(&self, sql: &str) -> Result<i32, HostError>;

    /// 查询 SQL，返回行数组 JSON；无结果返回 `Ok(None)`
    fn db_query(&self, sql: &str) -> Result<Option<serde_json::Value>, HostError>;
}
