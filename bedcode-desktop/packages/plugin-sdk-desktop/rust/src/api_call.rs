//! 插件互调 SDK 层（ADR-0017 / spec §9.3）：JSON-RPC 2.0 over host-bus
//!
//! 消息约定（全部在本模块实现，宿主仅提供门禁 + `host-api-call` 原始
//! 「发布请求、等待回复」原语，不感知 JSON-RPC 形状）：
//! - 请求 topic：`bedcode.api.<plugin-id>.<method>`；payload：
//!   `{ "jsonrpc": "2.0", id, method, params }`
//! - 响应 topic：`bedcode.api.reply.<caller-plugin-id>.<request-id>`；payload：
//!   `{ "jsonrpc": "2.0", id, result | error }`
//!
//! ## 为什么回复等待在宿主侧完成
//!
//! WASM guest 是同步执行模型：调用方在 `api_call` 阻塞等待回复期间无法再
//! 接收 `on_message` —— 宿主投递 on_message 需要锁住调用方插件实例
//! （`with_wasm_plugin_call`），而该锁正被阻塞中的调用持有，必然自锁死锁。
//! 因此 `host-api-call.call` 在宿主侧注册静态回复订阅（不经调用方 wasm 实例）
//! + oneshot 通道 + 超时，SDK 侧负责 correlation id 生成、消息形状与错误解码。
//!
//! ## 请求 id（correlation id）
//!
//! WASM 无系统时钟，id 用进程内单调计数器（thread_local，单线程 wasm 安全）。
//! 调用严格串行（同步引擎一次一个调用），计数器保证顺序调用 id 不重复 ——
//! 超时后迟到的旧回复落在旧 reply topic 上，与新调用的 topic 不冲突。

use crate::host::HostBus;
use crate::wasm::bedcode::plugin::host_api_call;

/// 请求 topic 前缀：`bedcode.api.<plugin-id>.<method>`（总线门禁只校验此前缀）
pub const API_TOPIC_PREFIX: &str = "bedcode.api.";
/// 响应 topic 前缀：`bedcode.api.reply.<caller-plugin-id>.<request-id>`
pub const REPLY_TOPIC_PREFIX: &str = "bedcode.api.reply.";
/// 互调调用默认超时（毫秒，spec §9.3）
pub const DEFAULT_CALL_TIMEOUT_MS: u64 = 10_000;

thread_local! {
    /// 请求 id 计数器（单调递增；wasm 单线程，thread_local + Cell 无并发问题）
    static REQUEST_COUNTER: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// 生成下一个请求 id（correlation id）
pub fn next_request_id() -> String {
    REQUEST_COUNTER.with(|c| {
        let n = c.get().wrapping_add(1);
        c.set(n);
        format!("req-{}", n)
    })
}

/// JSON-RPC 2.0 请求（线协议，字段与 spec §9.3 一致）
#[derive(Debug, Clone, serde::Deserialize)]
pub struct RpcRequest {
    pub jsonrpc: String,
    pub id: String,
    pub method: String,
    #[serde(default)]
    pub params: Option<serde_json::Value>,
}

/// 互调调用错误（调用方侧可见）
#[derive(Debug)]
pub enum ApiCallError {
    /// 宿主侧失败：门禁拒绝（目标 api 未声明）/ 超时 / 宿主错误
    Host(String),
    /// JSON-RPC error 对象（目标方法返回错误，code/message 透传）
    Rpc { code: i32, message: String },
    /// 参数/结果序列化或反序列化失败
    Protocol(String),
    /// 回复形状非法（缺 result/error、非法 JSON）
    Malformed(String),
}

impl ApiCallError {
    /// serde 序列化失败 → Protocol
    pub fn serialize(e: serde_json::Error) -> Self {
        Self::Protocol(format!("serialize: {}", e))
    }

    /// serde 反序列化失败 → Protocol
    pub fn decode_result(e: serde_json::Error) -> Self {
        Self::Protocol(format!("decode result: {}", e))
    }
}

impl std::fmt::Display for ApiCallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Host(m) => write!(f, "api call host error: {}", m),
            Self::Rpc { code, message } => write!(f, "api call error {}: {}", code, message),
            Self::Protocol(m) => write!(f, "api call protocol error: {}", m),
            Self::Malformed(m) => write!(f, "api call malformed reply: {}", m),
        }
    }
}

impl std::error::Error for ApiCallError {}

/// 构造 JSON-RPC 请求载荷（id 自动生成）
pub fn build_request(method: &str, params: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": next_request_id(),
        "method": method,
        "params": params,
    })
}

/// 构造 JSON-RPC 响应（result | error 二选一）
pub fn rpc_reply(id: &str, result: std::result::Result<serde_json::Value, (i32, String)>) -> serde_json::Value {
    let mut m = serde_json::Map::new();
    m.insert("jsonrpc".to_string(), serde_json::Value::String("2.0".to_string()));
    m.insert("id".to_string(), serde_json::Value::String(id.to_string()));
    match result {
        Ok(v) => {
            m.insert("result".to_string(), v);
        }
        Err((code, message)) => {
            m.insert(
                "error".to_string(),
                serde_json::json!({ "code": code, "message": message }),
            );
        }
    }
    serde_json::Value::Object(m)
}

/// 发布 JSON-RPC 响应到 reply topic（实现方分派用；经宿主总线门禁，
/// `bedcode.api.reply.` 前缀免校验）
pub fn publish_reply(reply_topic: &str, reply: &serde_json::Value) -> Result<(), ApiCallError> {
    let host = crate::wasm_host::WasmHost;
    host.bus_publish(reply_topic, reply)
        .map_err(|e| ApiCallError::Host(format!("publish_reply: {}", e)))
}

/// 调用宿主 `host-api-call` 原语：发布请求 + 阻塞等待回复（宿主侧超时）
pub fn api_call(
    request_topic: &str,
    payload: &serde_json::Value,
    timeout_ms: u64,
) -> Result<serde_json::Value, ApiCallError> {
    let payload_str = serde_json::to_string(payload)
        .map_err(|e| ApiCallError::Protocol(format!("api_call: serialize request: {}", e)))?;
    let reply_str = host_api_call::call(request_topic, &payload_str, timeout_ms)
        .map_err(|e| ApiCallError::Host(format!("api_call: {}", e)))?;
    serde_json::from_str(&reply_str)
        .map_err(|e| ApiCallError::Malformed(format!("api_call: invalid reply JSON: {}", e)))
}

/// 解码 JSON-RPC 响应：error 对象 → `Rpc` 错误；result 字段 → 成功值
pub fn decode_reply(reply: &serde_json::Value) -> Result<serde_json::Value, ApiCallError> {
    if let Some(err) = reply.get("error") {
        return Err(ApiCallError::Rpc {
            code: err
                .get("code")
                .and_then(|v| v.as_i64())
                .unwrap_or(-32000) as i32,
            message: err
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("rpc error")
                .to_string(),
        });
    }
    reply
        .get("result")
        .cloned()
        .ok_or_else(|| ApiCallError::Malformed("reply missing result/error".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_ids_monotonic() {
        // correlation id 单调递增：顺序调用不重复（超时后迟到回复不串台的关键）
        let a = next_request_id();
        let b = next_request_id();
        assert_ne!(a, b);
        assert!(a.starts_with("req-"));
    }

    #[test]
    fn build_request_shape() {
        // 线协议锁定：jsonrpc/id/method/params 四字段（spec §9.3）
        let req = build_request("schedule.list", serde_json::json!([1, 2]));
        assert_eq!(req["jsonrpc"], "2.0");
        assert_eq!(req["method"], "schedule.list");
        assert_eq!(req["params"], serde_json::json!([1, 2]));
        assert!(req["id"].as_str().unwrap().starts_with("req-"));
    }

    #[test]
    fn rpc_reply_ok_and_error() {
        // result 与 error 互斥：Ok → result 字段；Err → error 对象（code/message）
        let ok = rpc_reply("req-1", Ok(serde_json::json!({"n": 1})));
        assert_eq!(ok["result"], serde_json::json!({"n": 1}));
        assert!(ok.get("error").is_none());

        let err = rpc_reply("req-2", Err((-32000, "boom".to_string())));
        assert!(err.get("result").is_none());
        assert_eq!(err["error"]["code"], -32000);
        assert_eq!(err["error"]["message"], "boom");
    }

    #[test]
    fn decode_reply_result_and_error() {
        // 成功：result 原样返回
        let ok = serde_json::json!({ "jsonrpc": "2.0", "id": "req-1", "result": [1] });
        assert_eq!(decode_reply(&ok).unwrap(), serde_json::json!([1]));

        // 错误：error.code/message 透传（错误传播验收路径）
        let err = serde_json::json!({ "jsonrpc": "2.0", "id": "req-1", "error": { "code": -32000, "message": "boom" } });
        let e = decode_reply(&err).unwrap_err();
        match e {
            ApiCallError::Rpc { code, message } => {
                assert_eq!(code, -32000);
                assert_eq!(message, "boom");
            }
            other => panic!("expected Rpc error, got: {:?}", other),
        }

        // 缺 result/error：协议错误
        let bad = serde_json::json!({ "jsonrpc": "2.0", "id": "req-1" });
        assert!(matches!(decode_reply(&bad), Err(ApiCallError::Malformed(_))));
    }
}
