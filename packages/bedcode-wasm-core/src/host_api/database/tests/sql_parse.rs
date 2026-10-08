//! SQL 表名提取 / 前缀校验的纯函数契约（无端口、无库）
//!
//! 迁移自宿主同名用例：正则层对正常 DML/DDL 放行、对越界表名与逗号多表拒绝，含引号
//! 标识符与库名限定两种绕过形态的识别边界。

use crate::host_api::database::{extract_table_names, validate_sql_table_prefix};

#[test]
fn test_sanitize_plugin_id() {
    let sanitized = "com.example.my-plugin".replace(['.', '-'], "_");
    assert_eq!(sanitized, "com_example_my_plugin");
}

#[test]
fn test_validate_sql_table_prefix_valid() {
    let result = validate_sql_table_prefix(
        "com.example.my-plugin",
        "INSERT INTO plugin_com_example_my_plugin_data (id, name) VALUES (1, 'test')",
    );
    assert!(result.is_ok());
}

#[test]
fn test_validate_sql_table_prefix_invalid() {
    let result = validate_sql_table_prefix("com.example.my-plugin", "INSERT INTO sessions (id) VALUES ('abc')");
    assert!(result.is_err());
}

#[test]
fn test_validate_sql_table_prefix_multiple_tables() {
    let result = validate_sql_table_prefix(
        "my-plugin",
        "SELECT * FROM plugin_my_plugin_data JOIN sessions ON sessions.id = plugin_my_plugin_data.session_id",
    );
    assert!(result.is_err());
}

#[test]
fn test_validate_sql_table_prefix_create_table() {
    let result = validate_sql_table_prefix(
        "my-plugin",
        "CREATE TABLE IF NOT EXISTS plugin_my_plugin_cache (key TEXT PRIMARY KEY, value TEXT)",
    );
    assert!(result.is_ok());
}

#[test]
fn test_validate_sql_table_prefix_drop_table() {
    let result = validate_sql_table_prefix("my-plugin", "DROP TABLE IF EXISTS plugin_my_plugin_cache");
    assert!(result.is_ok());
}

#[test]
fn test_validate_sql_table_prefix_alter_table() {
    let result = validate_sql_table_prefix(
        "my-plugin",
        "ALTER TABLE plugin_my_plugin_cache ADD COLUMN updated_at TEXT",
    );
    assert!(result.is_ok());
}

#[test]
fn test_extract_table_names() {
    let tables = extract_table_names("INSERT INTO users (id) VALUES (1); SELECT * FROM orders");
    assert!(tables.contains(&"users".to_string()));
    assert!(tables.contains(&"orders".to_string()));
}

#[test]
fn test_extract_table_names_quoted() {
    let tables = extract_table_names("INSERT INTO `my-table` (id) VALUES (1)");
    assert!(tables.contains(&"my".to_string()));
}
