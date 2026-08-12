//! 命令参数提取辅助
//!
//! 消除 `args.get("x").and_then(|v| v.as_str()).unwrap_or("")` 重复样板

/// `invoke_command` 参数包装 — JSON 字段提取薄封装
///
/// ```ignore
/// fn invoke_command(name: &str, args: Value) -> anyhow::Result<Value> {
///     let args = CommandArgs::new(args);
///     let session_id = args.str_or("session_id", "");
///     let auto = args.bool_or("auto_approve", false);
///     ...
/// }
/// ```
#[derive(Debug, Clone, Default)]
pub struct CommandArgs(pub serde_json::Value);

impl CommandArgs {
    /// 包装参数；`Null` 归一化为空对象，后续提取一律返回默认值
    pub fn new(args: serde_json::Value) -> Self {
        Self(if args.is_null() { serde_json::json!({}) } else { args })
    }

    /// 取字符串字段；缺失或非字符串返回 default
    pub fn str_or(&self, key: &str, default: &str) -> String {
        self.0
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or(default)
            .to_string()
    }

    /// 取可选字符串字段（空字符串视为无）
    pub fn str(&self, key: &str) -> Option<String> {
        self.0
            .get(key)
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
    }

    /// 取 bool 字段；缺失返回 default
    pub fn bool_or(&self, key: &str, default: bool) -> bool {
        self.0.get(key).and_then(|v| v.as_bool()).unwrap_or(default)
    }

    /// 取原始 JSON 字段引用
    pub fn value(&self, key: &str) -> Option<&serde_json::Value> {
        self.0.get(key)
    }

    /// 取原始 JSON 字段（克隆，用于透传给其他 API）
    pub fn value_owned(&self, key: &str) -> Option<serde_json::Value> {
        self.0.get(key).cloned()
    }
}

impl From<serde_json::Value> for CommandArgs {
    fn from(v: serde_json::Value) -> Self {
        Self::new(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_null_normalized_to_empty_object() {
        // ABI 层解析失败时传入 Null，提取应一律返回默认值而非 panic
        let args = CommandArgs::new(serde_json::Value::Null);
        assert_eq!(args.str_or("session_id", "default"), "default");
        assert_eq!(args.str("session_id"), None);
        assert_eq!(args.bool_or("auto", true), true);
        assert_eq!(args.value("x"), None);
        assert_eq!(args.value_owned("x"), None);
    }

    #[test]
    fn test_str_or_missing_or_wrong_type_returns_default() {
        let args = CommandArgs::new(serde_json::json!({
            "name": "claude",
            "count": 3,
            "flag": true
        }));
        assert_eq!(args.str_or("name", ""), "claude");
        assert_eq!(args.str_or("missing", "fallback"), "fallback");
        // 非字符串字段（数字/bool/对象）一律按缺失处理
        assert_eq!(args.str_or("count", "fallback"), "fallback");
        assert_eq!(args.str_or("flag", "fallback"), "fallback");
        assert_eq!(args.str_or("name", ""), "claude");
    }

    #[test]
    fn test_str_filters_empty_string() {
        // 空字符串视为无（可选参数语义），与 str_or 的显式默认值区分
        let args = CommandArgs::new(serde_json::json!({ "a": "", "b": "x" }));
        assert_eq!(args.str("a"), None);
        assert_eq!(args.str("b"), Some("x".to_string()));
        assert_eq!(args.str("missing"), None);
        assert_eq!(args.str("b"), Some("x".to_string()));
    }

    #[test]
    fn test_bool_or() {
        let args = CommandArgs::new(serde_json::json!({
            "yes": true,
            "no": false,
            "wrong": "true"
        }));
        assert!(args.bool_or("yes", false));
        assert!(!args.bool_or("no", true));
        // 字符串 "true" 不是 bool，按缺失处理
        assert!(!args.bool_or("wrong", false));
        assert!(args.bool_or("missing", true));
    }

    #[test]
    fn test_value_and_value_owned() {
        let inner = serde_json::json!({ "nested": [1, 2] });
        let args = CommandArgs::new(serde_json::json!({ "obj": inner.clone() }));
        assert_eq!(args.value("obj"), Some(&inner));
        assert_eq!(args.value_owned("obj"), Some(inner));
        assert_eq!(args.value_owned("missing"), None);
    }

    #[test]
    fn test_from_value_conversion() {
        // 插件代码常用 `let args = CommandArgs::from(value);`，与 new 语义一致
        let args: CommandArgs = serde_json::json!(null).into();
        assert_eq!(args.str_or("k", "d"), "d");
    }
}
