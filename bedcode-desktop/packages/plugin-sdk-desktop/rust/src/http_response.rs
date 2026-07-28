//! 插件 HTTP 端点响应辅助
//!
//! 统一的 `{ status, body: { code, message, data? } }` 响应格式：
//! 宿主 `plugin_controller` 解析 `status` 设置 HTTP 状态码，
//! `body` 原样返回给调用方（hook 脚本 / 移动端 / 前端）。

use serde_json::Value;

/// 成功响应
pub fn ok() -> Value {
    serde_json::json!({
        "status": 200,
        "body": { "code": 0, "message": "ok" }
    })
}

/// 成功响应（带 data）
pub fn ok_with_data(data: Value) -> Value {
    serde_json::json!({
        "status": 200,
        "body": { "code": 0, "message": "ok", "data": data }
    })
}

/// 错误响应
pub fn error(status: u16, message: &str) -> Value {
    serde_json::json!({
        "status": status,
        "body": { "code": status as i32, "message": message }
    })
}
