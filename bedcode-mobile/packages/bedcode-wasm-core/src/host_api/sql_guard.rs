//! 插件 SQL 护栏与列转换（票 17 批次 2 自宿主 `plugin/wasm_host.rs` 迁入）
//!
//! 机制面：表名前缀纵深（跨插件数据隔离不变量）+ rusqlite 行 → JSON 列转换。
//! 纯函数，无宿主依赖。宿主切换后经垫片 `crate::plugin::wasm_host` 转发
//! （原符号面 `validate_sql_table_prefix` / `column_to_json` 不变）。

use regex::Regex;

use crate::error::Result;

// ==================== SQL Table Name Validation ====================

/// 验证 SQL 语句中的表名是否以插件专属前缀开头
///
/// WASM 插件只能操作 `plugin_{sanitized_id}_` 前缀的表，
/// 防止插件读写宿主或其他插件的数据表
///
/// # Table Name Extraction
/// 从 SQL 中提取表名，覆盖常见 DML/DDL 语句：
/// - CREATE TABLE / INSERT INTO / UPDATE / DELETE FROM
/// - SELECT ... FROM / ALTER TABLE / DROP TABLE
///
/// # Sanitization
/// plugin_id 中的 `.` 和 `-` 替换为 `_`，确保表名前缀合法
pub fn validate_sql_table_prefix(plugin_id: &str, sql: &str) -> Result<()> {
    let sanitized_id = plugin_id.replace('.', "_").replace('-', "_");
    let expected_prefix = format!("plugin_{}_", sanitized_id);

    let table_names = extract_table_names(sql);

    for table in table_names {
        if !table.starts_with(&expected_prefix) {
            return Err(crate::AppError::Plugin(format!(
                "SQL table name '{}' does not match required prefix '{}' for plugin '{}'",
                table, expected_prefix, plugin_id
            )));
        }
    }

    Ok(())
}

/// 从 SQL 语句中提取表名
///
/// 使用正则匹配常见 SQL 关键字后的表名标识符
fn extract_table_names(sql: &str) -> Vec<String> {
    let mut tables = Vec::new();

    let patterns = [
        // 标识符字符类排除引号/空白/分隔符（, ; ( ) [ ]），
        // 支持 SQLite 带引号标识符中的连字符（如 `my-table`）——原 \w+ 会把 `my-table` 截断成 `my`
        r#"(?i)\bCREATE\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?[`"\[]?([^`\s"'(),;\[\]]+)[`"\]]?"#,
        r#"(?i)\bINSERT\s+INTO\s+[`"\[]?([^`\s"'(),;\[\]]+)[`"\]]?"#,
        r#"(?i)\bUPDATE\s+[`"\[]?([^`\s"'(),;\[\]]+)[`"\]]?"#,
        r#"(?i)\bDELETE\s+FROM\s+[`"\[]?([^`\s"'(),;\[\]]+)[`"\]]?"#,
        r#"(?i)\bFROM\s+[`"\[]?([^`\s"'(),;\[\]]+)[`"\]]?"#,
        r#"(?i)\bJOIN\s+[`"\[]?([^`\s"'(),;\[\]]+)[`"\]]?"#,
        r#"(?i)\bALTER\s+TABLE\s+[`"\[]?([^`\s"'(),;\[\]]+)[`"\]]?"#,
        r#"(?i)\bDROP\s+TABLE\s+(?:IF\s+EXISTS\s+)?[`"\[]?([^`\s"'(),;\[\]]+)[`"\]]?"#,
    ];

    for pattern in &patterns {
        if let Ok(re) = Regex::new(pattern) {
            for cap in re.captures_iter(sql) {
                if let Some(m) = cap.get(1) {
                    let name = m.as_str().to_string();
                    if !tables.contains(&name) {
                        tables.push(name);
                    }
                }
            }
        }
    }

    tables
}

// ==================== Database Column Conversion ====================

/// 将 rusqlite 行的指定列转换为 serde_json::Value
///
/// 按类型优先级尝试读取：i64 -> f64 -> String -> bool -> blob -> Null
/// rusqlite 的 FromSql 支持 i64/f64/String/bool 等，但不支持 serde_json::Value
pub fn column_to_json(row: &rusqlite::Row<'_>, col_index: usize) -> serde_json::Value {
    // 先尝试整数
    if let Ok(v) = row.get::<_, i64>(col_index) {
        // 区分整数和浮点数：如果该列实际是 REAL 类型，i64 读取可能截断
        if let Ok(fv) = row.get::<_, f64>(col_index) {
            if (fv as i64) as f64 != fv {
                return serde_json::Value::Number(
                    serde_json::Number::from_f64(fv).unwrap_or(serde_json::Number::from(0)),
                );
            }
        }
        return serde_json::Value::Number(serde_json::Number::from(v));
    }
    // 尝试浮点数
    if let Ok(v) = row.get::<_, f64>(col_index) {
        return serde_json::Value::Number(serde_json::Number::from_f64(v).unwrap_or(serde_json::Number::from(0)));
    }
    // 尝试字符串
    if let Ok(v) = row.get::<_, String>(col_index) {
        return serde_json::Value::String(v);
    }
    // 尝试布尔值
    if let Ok(v) = row.get::<_, bool>(col_index) {
        return serde_json::Value::Bool(v);
    }
    // 尝试 blob（Vec<u8>）— 转为 hex 字符串
    if let Ok(v) = row.get::<_, Vec<u8>>(col_index) {
        use std::fmt::Write;
        let mut hex = String::with_capacity(v.len() * 2);
        for byte in &v {
            write!(hex, "{:02x}", byte).unwrap();
        }
        return serde_json::Value::String(hex);
    }
    // NULL 或无法识别的类型
    serde_json::Value::Null
}

// ==================== 测试（自宿主 wasm_host.rs 随迁） ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_sql_table_prefix_sanitizes_plugin_id() {
        // 插件 id 中的 `.` 与 `-` 必须全部替换为 `_`，前缀按 plugin_<sanitized>_ 校验。
        // 本测试引用真实实现（validate_sql_table_prefix）：若消毒逻辑遗漏任一字符，
        // 合法表名会因前缀不符而失败（变异可杀，不复制实现逻辑到预期）。
        let plugin_id = "com.example.my-plugin";
        let ok = validate_sql_table_prefix(
            plugin_id,
            "INSERT INTO plugin_com_example_my_plugin_data (id) VALUES (1)",
        );
        assert!(ok.is_ok(), "消毒后前缀应放行合法表名");

        // 未消毒的表名（保留 . 与 -）必须拒绝，且错误消息携带消毒后前缀
        let err = validate_sql_table_prefix(
            plugin_id,
            "INSERT INTO plugin_com.example.my-plugin_data (id) VALUES (1)",
        );
        assert!(err.is_err(), "保留原字符的表名必须拒绝");
        let msg = err.unwrap_err().to_string();
        assert!(
            msg.contains("plugin_com_example_my_plugin_"),
            "错误消息应含消毒后前缀, got: {msg}"
        );
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
        // 带引号标识符允许连字符：`my-table` 必须整体提取（原 \w+ 截断为 "my" 是 bug）
        let tables = extract_table_names("INSERT INTO `my-table` (id) VALUES (1)");
        assert!(tables.contains(&"my-table".to_string()));
        assert!(!tables.contains(&"my".to_string()));

        // 双引号 / 方括号引号形式同样支持连字符
        let double_quoted = extract_table_names("INSERT INTO \"user-data\" (id) VALUES (1)");
        assert!(double_quoted.contains(&"user-data".to_string()));
        let bracket = extract_table_names("INSERT INTO [my-table] (id) VALUES (1)");
        assert!(bracket.contains(&"my-table".to_string()));
    }

    #[test]
    fn test_extract_table_names_dml_ddl_keywords() {
        // UPDATE / DELETE / ALTER / DROP 关键字独立覆盖（审计 P1）
        assert!(extract_table_names("UPDATE plugin_x SET a = 1 WHERE id = 2").contains(&"plugin_x".to_string()));
        assert!(extract_table_names("DELETE FROM plugin_x WHERE id = 1").contains(&"plugin_x".to_string()));
        assert!(extract_table_names("ALTER TABLE plugin_x ADD COLUMN c TEXT").contains(&"plugin_x".to_string()));
        assert!(extract_table_names("DROP TABLE IF EXISTS plugin_x").contains(&"plugin_x".to_string()));
    }

    #[test]
    fn test_extract_table_names_comma_separated_stops_at_delimiter() {
        // 逗号分隔的多表 FROM 只取关键字后第一个标识符（分隔符不得吞入表名）
        let tables = extract_table_names("SELECT * FROM plugin_a, plugin_b WHERE 1");
        assert!(tables.contains(&"plugin_a".to_string()));
        assert!(!tables.contains(&"plugin_a,".to_string()));
    }
}
