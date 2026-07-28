//! SQL 参数绑定辅助
//!
//! 配合 `HostDatabase` / `HostPluginDatabase` 的 `*_params` 方法使用，
//! 将多个 Rust 值一次性转为 JSON 绑定参数数组，杜绝手写
//! `replace('\'', "''")` 转义（易错且不可组合）。

/// 构造 SQL 绑定参数数组
///
/// 各表达式经 serde 转换（字符串 / 数字 / bool / null / 可序列化结构），
/// 与 SQL 中的 `?1`、`?2` … 占位符按序配合：
/// ```ignore
/// host.plugin_db_execute_params(
///     "UPDATE t SET name = ?1 WHERE id = ?2",
///     &sql_params![name, id],
/// )?;
/// ```
#[macro_export]
macro_rules! sql_params {
    ($($v:expr),+ $(,)?) => {
        vec![$($crate::sql::to_param(&$v)),+]
    };
    () => { vec![] };
}

/// 将任意可序列化值转为绑定参数
///
/// 序列化失败时退化为 `Value::Null`（不 panic）；
/// 基础类型（str / String / 数字 / bool / Option）不会失败
pub fn to_param<T: serde::Serialize + ?Sized>(v: &T) -> serde_json::Value {
    serde_json::to_value(v).unwrap_or(serde_json::Value::Null)
}
