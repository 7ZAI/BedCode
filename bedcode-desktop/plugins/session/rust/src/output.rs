//! 终端输出拉取域（票 04）：经 `host-session.output-ring-fetch` 原语拉取会话输出
//! 原始字节（WIT `list<u8>` 直传，宿主↔插件 WASM 边界不 JSON 化），转给插件前端
//! 写入管线。
//!
//! 形态：**无状态透传**——游标（`fromOffset` / `nextOffset`）由调用方（前端）自持，
//! 本模块只做参数仲裁与结果形状；宿主 `GlobalOutputManager` 保有环本体、插件注册
//! 游标 + 自有水位。背压语义沿用 2026-09-17 pull 模型：慢消费只损失自己的 ring
//! 历史（`truncated` 重锚），宿主环绝不把背压回传到产出端。
//!
//! 命令面：`session.output.pull`（无互调 api 对应——终端输出是会话域内部数据面，
//! 只供本插件前端消费，不进跨插件互调面）。

// wasm32 分支才真正调用宿主原语；native 目标下仅参数仲裁（编译期剪掉未用 import）
#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::host::HostSession;
#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::wasm_host::WasmHost;

/// 单次拉取上限（对齐宿主 `PLUGIN_SESSION_RING_FETCH_MAX_BYTES` = 16 KiB；
/// 宿主侧仍会钳位截断，此值只做前端缺省声明——本模块不重复持有宿主常量）
const MAX_BYTES: u32 = 16 * 1024;

/// 输出环拉取：`{sessionId, fromOffset, maxBytes?}` →
/// `null`（游标已追平产出端）| `{data: number[], nextOffset, truncated}`
///
/// `data` 为原始字节（未解码，可能非 UTF-8）——WASM 边界直传后在此经 JSON 数组
/// 交给插件前端（插件内部通道，非宿主↔插件契约面）。`truncated = true` 表示游标
/// 落后于环驻留起点（中间字节已被淘汰），前端应清屏重锚（resync）。
#[cfg(target_arch = "wasm32")]
pub fn pull_via_host(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let session_id = args
        .get("sessionId")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "sessionId required".to_string())?;
    let from_offset = args
        .get("fromOffset")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| "fromOffset required".to_string())?;
    let max_bytes = args
        .get("maxBytes")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .unwrap_or(MAX_BYTES);

    match WasmHost
        .session_output_ring_fetch(session_id, from_offset, max_bytes)
        .map_err(|e| format!("output ring fetch failed: {}", e.message))?
    {
        None => Ok(serde_json::Value::Null),
        Some(fetched) => Ok(serde_json::json!({
            "data": fetched.data,
            "nextOffset": fetched.next_offset,
            "truncated": fetched.truncated,
        })),
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn pull_via_host(_args: &serde_json::Value) -> Result<serde_json::Value, String> {
    Err("session output pull unavailable outside wasm runtime".to_string())
}
