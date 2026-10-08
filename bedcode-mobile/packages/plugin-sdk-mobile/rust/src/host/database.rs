//! 宿主能力：数据库访问（host-database 主库 5 函数 + host-plugin-database 插件
//! 私有库 5 函数，票 05 对齐桌面 13 原语）

use super::HostError;

/// 宿主数据库访问（主库）
///
/// 操作宿主 SQLite 主库。需要 `storage` 权限。宿主强制表名前缀校验
/// （`plugin_{id}_` 前缀纵深——跨插件数据隔离不变量，票 05 起与桌面同构）。
///
/// **已知限制（与桌面端 SDK 同构标注，票据 07；不改契约）**：
/// - BLOB 二进制：查询 BLOB 列输出 **hex 字符串**，参数绑定无字节数组类型
///   （数组/对象参数 fallback 为 JSON 字符串）。需二进制存储时 hex 存 TEXT 或走 host-fs。
/// - i64 精度：查询 i64 → JSON number → 前端 JS 双精度 **> 2^53 丢精度**。
///   主键/大整数请用 TEXT 或自增整数。
pub trait HostDatabase {
    /// 执行 SQL，返回受影响行数
    fn db_execute(&self, sql: &str) -> Result<i32, HostError>;

    /// 查询 SQL，返回行数组 JSON；无结果返回 `Ok(None)`
    fn db_query(&self, sql: &str) -> Result<Option<serde_json::Value>, HostError>;

    /// 参数化执行（params 为 JSON 值数组，按序绑定；null→NULL，bool→INT，
    /// number→REAL/INTEGER，string→TEXT，数组/对象→JSON 字符串）
    fn db_execute_params(
        &self,
        sql: &str,
        params: &[serde_json::Value],
    ) -> Result<i32, HostError>;

    /// 参数化查询（同上绑定语义）
    fn db_query_params(
        &self,
        sql: &str,
        params: &[serde_json::Value],
    ) -> Result<Option<serde_json::Value>, HostError>;

    /// 事务内顺序执行多语句，返回受影响行数合计
    fn db_execute_batch(&self, sqls: &[String]) -> Result<i32, HostError>;
}

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
