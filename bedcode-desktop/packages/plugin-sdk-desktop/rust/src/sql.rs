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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sql_params_empty() {
        // 无参数 SQL（如 DDL）对应空数组
        let params: Vec<serde_json::Value> = sql_params![];
        assert!(params.is_empty());
    }

    #[test]
    fn test_sql_params_order_and_types() {
        // 占位符 ?1、?2 … 按序绑定，数组顺序即绑定顺序 —— 顺序即契约
        let params = sql_params![
            "claude",
            42,
            3.5_f64,
            true,
            Option::<String>::None,
            serde_json::json!({ "k": "v" })
        ];
        assert_eq!(
            params,
            vec![
                serde_json::json!("claude"),
                serde_json::json!(42),
                serde_json::json!(3.5),
                serde_json::json!(true),
                serde_json::Value::Null,
                serde_json::json!({ "k": "v" })
            ]
        );
    }

    #[test]
    fn test_sql_params_trailing_comma() {
        // 宏允许尾逗号，与 Rust 数组风格一致
        assert_eq!(sql_params![1,], vec![serde_json::json!(1)]);
    }

    #[test]
    fn test_to_param_basic_types() {
        assert_eq!(to_param(&"text"), serde_json::json!("text"));
        assert_eq!(to_param(&String::from("s")), serde_json::json!("s"));
        assert_eq!(to_param(&-7_i64), serde_json::json!(-7));
        assert_eq!(to_param(&false), serde_json::json!(false));
        assert_eq!(to_param(&Some(9_u64)), serde_json::json!(9));
        assert_eq!(to_param(&Option::<i32>::None), serde_json::Value::Null);
    }

    #[test]
    fn test_to_param_serializable_struct() {
        // 可序列化结构体直接转为 JSON 对象参数
        #[derive(serde::Serialize)]
        struct Point {
            x: i32,
            y: i32,
        }
        assert_eq!(to_param(&Point { x: 1, y: 2 }), serde_json::json!({ "x": 1, "y": 2 }));
    }

    #[test]
    fn test_to_param_serialization_failure_falls_back_to_null() {
        // serde_json 拒绝非有限浮点数，to_value 失败 → 退化为 Null（不 panic）
        assert_eq!(to_param(&f64::NAN), serde_json::Value::Null);
        assert_eq!(to_param(&f64::INFINITY), serde_json::Value::Null);
    }
}
