//! 终端输出拉取域（票 04；P1-b 起经 `host-pty.ring-fetch` 拉取）：
//! 会话输出环 = 会话对应 PTY 的 `PtyRing`（会话创建改走 `host-pty.spawn` 后，
//! 业务会话输出天然就是引擎环），经 WIT `list<u8>` 直传（宿主↔插件 WASM 边界
//! 不 JSON 化），转给插件前端写入管线。
//!
//! 形态：**无状态透传**——游标（`fromOffset` / `nextOffset`）由调用方（前端）自持，
//! 本模块只做参数仲裁、经登记域按 sessionId 解出 `pty_id`、再调引擎原语。背压
//! 语义沿用 2026-09-17 pull 模型：慢消费只损失自己的环历史（`truncated` 重锚），
//! 宿主环绝不把背压回传到产出端。
//!
//! 命令面：`session.output.pull`（无互调 api 对应——终端输出是会话域内部数据面，
//! 只供本插件前端消费，不进跨插件互调面）。

// wasm32 分支才真正调用宿主原语；native 目标下仅参数仲裁（编译期剪掉未用 import）
#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::host::HostPty;
#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::wasm_host::WasmHost;

/// 单次拉取上限（对齐宿主 `PLUGIN_PTY_RING_FETCH_MAX_BYTES` = 16 KiB；
/// 宿主侧仍会钳位截断，此值只做前端缺省声明——本模块不重复持有宿主常量）
const MAX_BYTES: u32 = 16 * 1024;

/// 一次历史快照互调的最大拉取次数（≈1 MiB 预算）：宿主直读时代是单次水源
/// 快照（min/max 一次取净），改为插件互调后按 16 KiB 分片续拉，高速产出下
/// 若超过预算（output 比拉得快），以当前已收集区间的上沿为 mock 快照——
/// 历史是尽力快照，`snapshotOffset` 如实上报实际停点（不为拉满无限循环）
const MAX_FETCHES_FOR_HISTORY: u32 = 64;

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

    // 会话登记域 → pty_id（P1-b 起环本体在宿主 PTY 引擎，按句柄拉取）
    let pty_id = crate::session::record_via_host(session_id)?
        .and_then(|r| r.pty_id)
        .ok_or_else(|| format!("会话不存在或缺少 PTY 句柄：{session_id}"))?;

    match WasmHost
        .pty_ring_fetch(&pty_id, from_offset, max_bytes)
        .map_err(|e| format!("pty ring fetch failed: {}", e.message))?
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

/// 一次性历史快照互调 api（`session-history`）：`{sessionId, from}` →
/// `{data: number[], minOffset, snapshotOffset, historyBytes}`
///
/// **websocket 业务下沉票 08**：宿主不再持有「会话 id → pty 句柄」广播映射
/// （`hostBroadcastSessionId` / `broadcast_handle_for_session` 已退役）——HTTP
/// 历史快照改经本互调：插件用自己的 `session record.pty_id` 调 `host-pty.ring-fetch`
/// 拉净驻留历史（spec §4.3「插件直接使用自己的 session record.pty_id 调
/// ring-fetch」）。分片续拉直到追平产出端（`Ok(None)`）或预算用尽；
/// `truncated` 时的实际返回起点即环驻留起点（`minOffset`——from 旧于驻留起点
/// 时如实上报缺口，客户端据此判定截断，不假装连续）。
#[cfg(target_arch = "wasm32")]
pub fn history_via_host(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let session_id = args
        .get("sessionId")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "sessionId required".to_string())?;
    let from = args.get("from").and_then(|v| v.as_u64()).unwrap_or(0);

    // 会话登记域 → pty_id（环本体在宿主 PTY 引擎，按句柄拉取）
    let pty_id = crate::session::record_via_host(session_id)?
        .and_then(|r| r.pty_id)
        .ok_or_else(|| format!("会话不存在或缺少 PTY 句柄：{session_id}"))?;

    let mut cursor = from;
    let mut data: Vec<u8> = Vec::new();
    let mut min_offset: Option<u64> = None;
    let mut fetches = 0u32;
    loop {
        if fetches >= MAX_FETCHES_FOR_HISTORY {
            // 预算用尽（高速产出）：如实上报当前停点，不无限续拉
            break;
        }
        fetches += 1;
        match WasmHost
            .pty_ring_fetch(&pty_id, cursor, MAX_BYTES)
            .map_err(|e| format!("pty ring fetch failed: {}", e.message))?
        {
            None => break, // 追平产出端
            Some(fetched) => {
                if fetched.truncated {
                    // 环已淘汰 from 之前字节：实际返回起点 = 环驻留起点
                    min_offset = Some(fetched.next_offset.saturating_sub(fetched.data.len() as u64));
                }
                if fetched.data.is_empty() {
                    break;
                }
                cursor = fetched.next_offset;
                data.extend_from_slice(&fetched.data);
            }
        }
    }
    let min = min_offset.unwrap_or(from);
    Ok(serde_json::json!({
        "data": data,
        "minOffset": min,
        "snapshotOffset": cursor,
        "historyBytes": cursor.saturating_sub(min),
    }))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn history_via_host(_args: &serde_json::Value) -> Result<serde_json::Value, String> {
    Err("session history unavailable outside wasm runtime".to_string())
}