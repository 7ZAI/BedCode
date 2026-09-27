//! ADR 0030 错误信封的插件侧辅助（桌面端；零 ABI —— WASM 导出签名不变，错误仍是字符串）
//!
//! 契约摘要（单一事实源 `docs/adr/0030-error-envelope-and-user-prompt-boundary.md` 决定 6）：
//! - 插件命令面错误签名不变（`invoke_command` 仍返回 `Result<_, anyhow::Error>`，
//!   `wasm_entry!` 宏序列化为 `{"error": "<to_string>"}`）；
//! - 本模块让「业务错误」以**标记 JSON** 作为错误字符串内容（`{"__bedcode_error__":true,
//!   "code":…, "params":…}`）；宿主桥（`invoke_wasm_command`）只做机制检测 + 形状校验 +
//!   透传，**不解释业务语义**；
//! - `code` 约定 = 插件 i18n **注册后的完整 key**（`<plugin_id>.<namespace>.<name>`，
//!   如 `com.bedcode.terminal-session.session.error.sessionNotFound`），前端消费层在
//!   宿主 `errors.*` 无此码时回退到裸 code 查找插件自注册文案（零映射层、零漂移）；
//! - 畸形 / 未标记错误 → 宿主按 `host.internal` + 插件标识参数兜底，原文只进日志。

use serde_json::Value;
use std::fmt;

/// 插件业务错误标记名（宿主桥只做此标记的机制判断；改名为破坏性契约变更）
pub const ENVELOPE_MARKER: &str = "__bedcode_error__";

/// 插件业务错误：携带业务语义码 + 已消毒具名参数（显示名/文件名/秒数等，
/// **禁止**技术文案 / 堆栈 / 凭据）。
///
/// `Display` 输出标记 JSON 字面（`{"__bedcode_error__":true,"code":…,"params":…}`）——
/// 经 `wasm_entry!` 宏作为字面错误串传宿主桥解析恢复。`params` 为 `Value::Null` 时
/// 装饰仍带 `"params":null`（宿主透传时对其归一，前端插值无差异）。
#[derive(Debug, Clone, PartialEq)]
pub struct PluginError {
    /// 业务语义码 = 插件 i18n 注册后的完整 key（见模块 doc）
    pub code: &'static str,
    /// 已消毒具名参数（`serde_json::json!({...})`）
    pub params: Value,
}

impl PluginError {
    /// 构造带参业务错误
    pub fn business(code: &'static str, params: Value) -> Self {
        Self { code, params }
    }

    /// 构造无参业务错误（信封仍带 `"params":null`）
    pub fn business_simple(code: &'static str) -> Self {
        Self::business(code, Value::Null)
    }
}

impl fmt::Display for PluginError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // 手动拼接而非 serde_json::json! 宏：宏会把 const 标识符当作 key 名字符串，
        // 无法展开 ENVELOPE_MARKER 值
        let code_json = serde_json::to_string(self.code).map_err(|_| fmt::Error)?;
        write!(
            f,
            "{{\"{}\":true,\"code\":{},\"params\":{}}}",
            ENVELOPE_MARKER, code_json, self.params
        )
    }
}

impl std::error::Error for PluginError {}

// 注意：不手写 `impl From<PluginError> for anyhow::Error`——anyhow 自带
// blanket impl（`E: StdError + Send + Sync + 'static`），手写会冲突；
// `anyhow::Error::from(PluginError::business(...))` 直接可用。

/// 返回标记 JSON 字符串（适配 `Result<_, String>` 错误面：
/// `Err(bedcode_plugin_api::user_facing_string("…", json!({…})))`）
pub fn user_facing_string(code: &'static str, params: Value) -> String {
    PluginError::business(code, params).to_string()
}

/// 业务错误快捷返回（`invoke_command` 内使用）：
///
/// ```ignore
/// fn invoke_command(name: &str, _args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
///     if name == "session.get" {
///         bail_with_code!("com.bedcode.terminal-session.session.error.sessionNotFound");
///     }
///     bail_with_code!("com.bedcode.demo.throttled", serde_json::json!({ "waitSecs": 5 }));
///     Ok(serde_json::Value::Null)
/// }
/// ```
#[macro_export]
macro_rules! bail_with_code {
    ($code:expr $(,)?) => {
        return Err(::anyhow::Error::from($crate::PluginError::business_simple($code)))
    };
    ($code:expr, $params:expr $(,)?) => {
        return Err(::anyhow::Error::from($crate::PluginError::business($code, $params)))
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Display 形状锁：完整标记 JSON（marker/true、code 字符串、params 对象）
    #[test]
    fn display_is_marked_envelope_json() {
        let e = PluginError::business("com.bedcode.demo.throttled", json!({ "waitSecs": 5 }));
        let s = e.to_string();
        assert_eq!(
            s,
            r#"{"__bedcode_error__":true,"code":"com.bedcode.demo.throttled","params":{"waitSecs":5}}"#
        );
    }

    /// 反例：错误链中不携带技术详情（Display 只有标记 + 码 + 参数）
    #[test]
    fn display_never_carries_technical_detail() {
        let e = PluginError::business_simple("com.bedcode.demo.bad-input");
        assert!(!e.to_string().contains("detail"), "{}", e);
        assert_eq!(
            e.to_string(),
            r#"{"__bedcode_error__":true,"code":"com.bedcode.demo.bad-input","params":null}"#
        );
    }

    /// code 含引号/反斜杠时 JSON 转义（防宿主倒序解析错乱）
    #[test]
    fn display_escapes_code() {
        let e = PluginError::business("a\"b\\c", Value::Null);
        let s = e.to_string();
        assert!(s.contains(r#""code":"a\"b\\c""#), "{s}");
    }

    /// user_facing_string 即 PluginError::business(...).to_string()
    #[test]
    fn user_facing_string_matches_plugin_error_display() {
        let direct = PluginError::business("com.bedcode.x", json!({ "k": 1 })).to_string();
        assert_eq!(user_facing_string("com.bedcode.x", json!({ "k": 1 })), direct);
    }

    /// bail_with_code! 展开：产生携带标记的 anyhow::Error，source 可还原 PluginError
    #[test]
    fn bail_with_code_expands_to_marked_anyhow_error() {
        fn fail_no_params() -> anyhow::Result<()> {
            bail_with_code!("com.bedcode.demo.simple");
        }
        fn fail_with_params() -> anyhow::Result<()> {
            bail_with_code!("com.bedcode.demo.throttled", json!({ "waitSecs": 5 }));
        }

        let e = fail_no_params().unwrap_err();
        assert_eq!(e.to_string(), r#"{"__bedcode_error__":true,"code":"com.bedcode.demo.simple","params":null}"#);

        let e = fail_with_params().unwrap_err();
        assert_eq!(e.to_string(), r#"{"__bedcode_error__":true,"code":"com.bedcode.demo.throttled","params":{"waitSecs":5}}"#);
        // 错误链保留 PluginError（宿主可用 source() 还原，虽宿主桥只按字符串解析）
        assert!(e.downcast_ref::<PluginError>().is_some());
    }
}