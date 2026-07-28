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
