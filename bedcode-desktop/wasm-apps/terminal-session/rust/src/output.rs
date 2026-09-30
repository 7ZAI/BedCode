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
//! 命令面：`session.output.pull` / `session.output.ack` / `session.output.watermarks`
//! （均无互调 api 对应——终端输出是会话域内部数据面，只供本插件前端消费，不进跨插件
//! 互调面）。

// wasm32 分支才真正调用宿主原语；native 目标下仅参数仲裁（编译期剪掉未用 import）
#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::host::{HostEvents, HostLog, HostPty};
#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::wasm_host::WasmHost;

/// 单次拉取上限（对齐宿主 `PLUGIN_PTY_RING_FETCH_MAX_BYTES` = 16 KiB；
/// 宿主侧仍会钳位截断，此值只做前端缺省声明——本模块不重复持有宿主常量）
const MAX_BYTES: u32 = 16 * 1024;

// ==================== 背压：未确认窗口（迁移前 ack 语义，归属收敛到插件） ====================
//
// 迁移前的背压是宿主侧 per-subscriber `SubscriberHandle`（私有 `acked_offset` +
// `ack_notify` + **双水位迟滞** park 等待）；会话下沉后宿主环不再持有订阅者概念
// （`PtyRing::push` 零等待，背压绝不回传读线程）。本次把该语义**原样收敛到插件侧**：
// 前端按交付水位 ack，未确认积压达到上沿即抑制新数据下发（数据留在宿主环内，
// 环满仍按引擎语义淘汰最旧并 `truncated` 上报——与迁移前一致，慢消费者自担）。

/// 未确认窗口**上沿**（字节）：已推送未确认达到它 → 进入驻留（抑制新数据）。
/// 取值对齐迁移前 `subscriber_high_water_bytes = 128 KiB`。
pub const HIGH_WATER_BYTES: u64 = 128 * 1024;

/// 未确认窗口**下沿**（字节）：驻留后未确认**低于**它才解除驻留。
/// 取值对齐迁移前 `subscriber_low_water_bytes = 64 KiB`；下沿与上沿的间隔
/// （64 KiB = 一个客户端 ack 阈值）是**迟滞带**——单阈值会在 ack 逐格推进时
/// 反复进出驻留（震荡），迟滞带保证一次驻留至少跨过一个 ack 周期。
pub const LOW_WATER_BYTES: u64 = 64 * 1024;

// 水位参数不变量（编译期自检，与迁移前 `subscriber_budget_violation` 同一口径）：
//   ack 阈值(64 KiB) ≤ LOW < HIGH 且 HIGH − ack ≤ LOW
// 前端 `TerminalPreview` 的 `ACK_BYTES_THRESHOLD` 必须与这里的 64 KiB 同值；
// 后一条保证驻留期间 ack 推进一个阈值即可跨过下沿，不会被下沿永久卡死。
//
// 环容量约束（当前环更浅）：`HIGH ≤ 环容量/2`——环比窗口还浅时驻留无意义
// （数据先被环淘汰，前端永远 ack 不到该水位）。插件未声明 `ringBytes`，宿主
// 取默认 256 KiB（`PLUGIN_PTY_RING_BYTES`）→ 128 KiB 恰为上界。**若将来声明
// 更小的 `ringBytes`，必须同步下调 HIGH/LOW**（spawn 侧接入时校验）。
const _: () = {
    assert!(LOW_WATER_BYTES < HIGH_WATER_BYTES);
    assert!(HIGH_WATER_BYTES - 64 * 1024 <= LOW_WATER_BYTES);
};

/// 拉取窗口裁决（纯函数；native 单测覆盖，不依赖 wasm 状态）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PullGate {
    /// 放行：未确认积压未超窗口；或前端游标落后（异常回退 / resync 重锚，不可抑制）
    Allow,
    /// 抑制：已推送未确认达上沿（或驻留中未降到下沿）→ 空响应让前端退避等待 ack 回落
    Throttle { unacked: u64 },
}

/// 裁决本轮拉取是否放行（双水位迟滞，对齐迁移前 `SubscriberHandle::window`）：
///
/// ```text
/// from_offset < acked            -> Allow     # I4：重锚/回退不可抑制（否则死锁）
/// was_parked && unacked < low    -> Allow     # 迟滞下沿：退出驻留
/// !was_parked && unacked >= high -> Throttle  # 迟滞上沿：进入驻留
/// 其余                           -> Allow
/// ```
///
/// `was_parked` 是驻留态（由调用方从水位表读出）——本函数保持纯函数，不持有状态。
/// 上沿/下沿由调用方显式传入（生产取 [`HIGH_WATER_BYTES`] / [`LOW_WATER_BYTES`]，
/// 单测可注入任意窗口）。
pub fn decide_pull_gate(
    pushed: u64,
    acked: u64,
    from_offset: u64,
    was_parked: bool,
    high: u64,
    low: u64,
) -> PullGate {
    if from_offset < acked {
        return PullGate::Allow;
    }
    let unacked = pushed.saturating_sub(acked);
    if was_parked {
        // 驻留中：只有降到下沿以下才解除（区间内保持驻留 = 迟滞）
        return if unacked < low {
            PullGate::Allow
        } else {
            PullGate::Throttle { unacked }
        };
    }
    if unacked >= high {
        return PullGate::Throttle { unacked };
    }
    PullGate::Allow
}

/// 单会话水位快照（诊断面 `session.output.watermarks`；对齐迁移前 `SubscriberStats`
/// 的 `park_count` / `truncated_count` 观测面）。
///
/// **有意不镜像旧实现的 `parked_ms`**：wasm 插件无系统时钟（`SystemTime::now()` 在
/// wasm32 触发 unreachable trap，见 `task::queue` 同款注释），插件侧只持有无时钟的
/// 字节水位与计数；驻留时长由前端计时（它有 `Date.now`）打进日志。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PumpSnapshot {
    pub session_id: String,
    pub pushed: u64,
    pub acked: u64,
    pub unacked: u64,
    pub parked: bool,
    /// 驻留**进入**次数（仅状态翻转计数，驻留期重复抑制不重复计）
    pub park_count: u64,
    /// 驻留**退出**次数（与 `park_count` 不等即当前仍驻留）
    pub unpark_count: u64,
    /// 被抑制的拉取次数（驻留期空响应总数 = 前端退避轮数）
    pub throttled_pulls: u64,
    /// 环淘汰（`truncated`）次数
    pub truncated_count: u64,
}

/// 水位快照 → 诊断 JSON（纯函数，native 可测；行过滤在调用侧完成）
///
/// `ws_rows` 是 **WS 终端连接**的交付水位（每连接独立游标 / 独立 ack 基准，
/// 见 `ws_terminal::Watermark`）。与 `rows`（插件自己前端的会话级拉取水位）
/// 分开上报而不是合并：两者的 ack 基准不同源（会话级用环绝对偏移，WS 级用
/// `base + 客户端本地计数`），混在一张表里会让人按错误的基准对齐两个数字。
pub fn render_watermark_report(
    rows: &[PumpSnapshot],
    ws_rows: &[crate::ws_terminal::WsWatermarkRow],
) -> serde_json::Value {
    let entries: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "sessionId": r.session_id,
                "pushed": r.pushed,
                "acked": r.acked,
                "unacked": r.unacked,
                "parked": r.parked,
                "parkCount": r.park_count,
                "unparkCount": r.unpark_count,
                "throttledPulls": r.throttled_pulls,
                "truncatedCount": r.truncated_count,
            })
        })
        .collect();
    let ws: Vec<serde_json::Value> = ws_rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "endpointId": r.endpoint_id,
                "clientId": r.client_id,
                "sessionId": r.session_id,
                "base": r.base,
                "pushed": r.pushed,
                "acked": r.acked,
                "unacked": r.unacked,
                "parked": r.parked,
                "parkCount": r.park_count,
                "unparkCount": r.unpark_count,
                "throttledCycles": r.throttled_cycles,
                // forcedDrains > 0 = 记账错位信号（背压逃生阀被触发过）
                "forcedDrains": r.forced_drains,
            })
        })
        .collect();
    serde_json::json!({
        "entries": entries,
        "count": rows.len(),
        "ws": ws,
        "wsCount": ws.len(),
    })
}

/// 水位表（wasm 单实例串行；`(pushed, acked)` 两水位单调不回退，`parked` 为驻留态）。
///
/// `#[cfg(test)]` 也让 native 单测直接覆盖水位单调与 `forget` 回收（表本身是纯
/// `std::sync::Mutex`，与 wasm 运行时无关）。
#[cfg(any(target_arch = "wasm32", test))]
mod pump {
    use super::PumpSnapshot;
    use std::sync::Mutex;

    struct PumpCursor {
        session_id: String,
        /// 已推送水位（上次响应给出的 `nextOffset`）
        pushed: u64,
        /// 前端已确认消费水位（`session.output.ack`）
        acked: u64,
        /// 驻留态（双水位迟滞的中间状态）：进入上沿置真，降到下沿置假
        parked: bool,
        /// 驻留进入次数（仅翻转计数）
        park_count: u64,
        /// 驻留退出次数（仅翻转计数）
        unpark_count: u64,
        /// 被抑制的拉取次数
        throttled_pulls: u64,
        /// 环淘汰（`truncated`）次数
        truncated_count: u64,
    }

    static PUMPS: Mutex<Vec<PumpCursor>> = Mutex::new(Vec::new());

    /// 以锁内可变引用执行（不存在 → 建零水位条目；锁中毒取回内部值继续）
    fn with_cursor<T>(session_id: &str, f: impl FnOnce(&mut PumpCursor) -> T) -> T {
        let mut pumps = PUMPS.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(index) = pumps.iter().position(|c| c.session_id == session_id) {
            return f(&mut pumps[index]);
        }
        pumps.push(PumpCursor {
            session_id: session_id.to_string(),
            pushed: 0,
            acked: 0,
            parked: false,
            park_count: 0,
            unpark_count: 0,
            throttled_pulls: 0,
            truncated_count: 0,
        });
        let last = pumps.len() - 1;
        f(&mut pumps[last])
    }

    /// 读水位 `(pushed, acked, parked)`
    pub fn watermarks(session_id: &str) -> (u64, u64, bool) {
        with_cursor(session_id, |c| (c.pushed, c.acked, c.parked))
    }

    /// 记录推送水位（单调 max，不回退）
    pub fn note_pushed(session_id: &str, offset: u64) {
        with_cursor(session_id, |c| c.pushed = c.pushed.max(offset));
    }

    /// 记录确认水位（单调 max，不回退；乱序/重放的旧 ack 不生效）
    pub fn note_acked(session_id: &str, offset: u64) {
        with_cursor(session_id, |c| c.acked = c.acked.max(offset));
    }

    /// 置驻留态（进入上沿 / 退出下沿由裁决驱动）；**仅状态翻转**累计进出次数
    /// （驻留期每次抑制都置真，不应把「一次驻留」数成 N 次）
    pub fn set_parked(session_id: &str, parked: bool) {
        with_cursor(session_id, |c| {
            if c.parked == parked {
                return;
            }
            c.parked = parked;
            if parked {
                c.park_count += 1;
            } else {
                c.unpark_count += 1;
            }
        });
    }

    /// 记一次被抑制的拉取（驻留期空响应）
    pub fn note_throttled(session_id: &str) {
        with_cursor(session_id, |c| c.throttled_pulls += 1);
    }

    /// 记一次环淘汰（响应 `truncated`）
    pub fn note_truncated(session_id: &str) {
        with_cursor(session_id, |c| c.truncated_count += 1);
    }

    /// 水位快照（`session_id` 传 `Some` 只取该会话；**不建条目**——诊断读面不产生状态）
    pub fn snapshots(session_id: Option<&str>) -> Vec<PumpSnapshot> {
        let pumps = PUMPS.lock().unwrap_or_else(|e| e.into_inner());
        pumps
            .iter()
            .filter(|c| session_id.is_none_or(|sid| c.session_id == sid))
            .map(|c| PumpSnapshot {
                session_id: c.session_id.clone(),
                pushed: c.pushed,
                acked: c.acked,
                unacked: c.pushed.saturating_sub(c.acked),
                parked: c.parked,
                park_count: c.park_count,
                unpark_count: c.unpark_count,
                throttled_pulls: c.throttled_pulls,
                truncated_count: c.truncated_count,
            })
            .collect()
    }

    /// 会话销毁时摘除水位与驻留态（无泄漏；不存在的会话静默）
    pub fn forget(session_id: &str) {
        let mut pumps = PUMPS.lock().unwrap_or_else(|e| e.into_inner());
        pumps.retain(|c| c.session_id != session_id);
    }

    /// 水位表当前条目数（仅测试用：验证 `forget` 回收无泄漏）
    #[cfg(test)]
    pub fn len() -> usize {
        PUMPS.lock().unwrap_or_else(|e| e.into_inner()).len()
    }
}

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

    // 未确认窗口裁决（背压，双水位迟滞）：进入上沿 → 空响应 + throttled，前端退避
    // 等待；驻留中未降到下沿 → 继续抑制。游标落后于已确认水位（resync 重锚）时
    // 一律放行——见 decide_pull_gate 说明。
    let (pushed, acked, was_parked) = pump::watermarks(session_id);
    match decide_pull_gate(
        pushed,
        acked,
        from_offset,
        was_parked,
        HIGH_WATER_BYTES,
        LOW_WATER_BYTES,
    ) {
        PullGate::Throttle { unacked } => {
            // 抑制计数（观测面：驻留期空响应数 = 前端退避轮数）
            pump::note_throttled(session_id);
            if !was_parked {
                pump::set_parked(session_id, true);
                WasmHost.log_debug(&format!(
                    "输出背压：进入驻留（session_id={session_id}, unacked={unacked}, high={HIGH_WATER_BYTES}）"
                ));
            }
            return Ok(serde_json::json!({
                "data": [],
                "nextOffset": from_offset,
                "truncated": false,
                "throttled": true,
                "unacked": unacked,
            }));
        }
        PullGate::Allow => {
            if was_parked {
                pump::set_parked(session_id, false);
                let unacked = pushed.saturating_sub(acked);
                WasmHost.log_debug(&format!(
                    "输出背压：退出驻留（session_id={session_id}, unacked={unacked}, low={LOW_WATER_BYTES}）"
                ));
            }
        }
    }

    match WasmHost
        .pty_ring_fetch(&pty_id, from_offset, max_bytes)
        .map_err(|e| format!("pty ring fetch failed: {}", e.message))?
    {
        None => Ok(serde_json::Value::Null),
        Some(fetched) => {
            // 环淘汰计数（观测面：对齐迁移前 SubscriberStats.truncated_count）
            if fetched.truncated {
                pump::note_truncated(session_id);
            }
            // 推送水位前移（单调）——下一次裁决的 `unacked = pushed - acked`
            pump::note_pushed(session_id, fetched.next_offset);
            let unacked = pump::watermarks(session_id).0.saturating_sub(acked);
            Ok(serde_json::json!({
                "data": fetched.data,
                "nextOffset": fetched.next_offset,
                "truncated": fetched.truncated,
                "throttled": false,
                "unacked": unacked,
            }))
        }
    }
}

/// 消费确认（ack）：`{sessionId, offset}` → `{ok, offset}`
///
/// 前端按**交付水位**（`onData` 入队后推进，对齐迁移前 `useTerminalOutputStreamChannel`
/// 的 ack 时点）节流上报——未确认窗口裁决据此推进；水位单调不回退（乱序/重放的
/// 旧 ack 不生效）。
#[cfg(target_arch = "wasm32")]
pub fn ack_via_host(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let session_id = args
        .get("sessionId")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "sessionId required".to_string())?;
    let offset = args
        .get("offset")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| "offset required".to_string())?;
    pump::note_acked(session_id, offset);
    Ok(serde_json::json!({ "ok": true, "offset": offset }))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn pull_via_host(_args: &serde_json::Value) -> Result<serde_json::Value, String> {
    Err("session output pull unavailable outside wasm runtime".to_string())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn ack_via_host(_args: &serde_json::Value) -> Result<serde_json::Value, String> {
    Err("session output ack unavailable outside wasm runtime".to_string())
}

/// 水位诊断读面（G2 水位可观测 / 对齐迁移前 `SubscriberStats`）：
/// `{sessionId?}` → `{entries: [...], count, ws: [...], wsCount}`
///
/// **纯读**——不建条目、不改状态（未知 `sessionId` 返回 `count: 0`，不伪造零水位）。
/// 无 `sessionId` 时返回全部在册会话。前端仅在驻留退出时取一次快照打日志（诊断用），
/// 另有闭环测试 / devtools 直调面。
#[cfg(target_arch = "wasm32")]
pub fn watermarks_via_host(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let session_id = args
        .get("sessionId")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty());
    // `sessionId` 过滤只作用在会话级段；WS 段是连接级（一个连接一个基准），
    // 仍按同一 sessionId 过滤以便「查某会话的所有消费者」
    let ws_rows: Vec<crate::ws_terminal::WsWatermarkRow> =
        crate::ws_terminal::watermark_rows()
            .into_iter()
            .filter(|r| session_id.is_none_or(|sid| r.session_id == sid))
            .collect();
    Ok(render_watermark_report(
        &pump::snapshots(session_id),
        &ws_rows,
    ))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn watermarks_via_host(_args: &serde_json::Value) -> Result<serde_json::Value, String> {
    Err("session output watermarks unavailable outside wasm runtime".to_string())
}

/// 会话销毁时回收未确认窗口水位与驻留态（`close` / `remove` 接线，见
/// [`crate::session::note_removed`] / [`crate::session::close_via_pty`]）：
/// 会话重启会换新 PTY 句柄、环偏移从 0 起，残留的旧水位会把新环误判为
/// 「已推送远超已确认」而永久驻留。
#[cfg(target_arch = "wasm32")]
pub fn forget_via_host(session_id: &str) {
    pump::forget(session_id);
}

// ==================== 输出可用通知（P2：宿主限频唤醒） ====================
//
// 宿主在环有新字节时向属主私有 topic `<owner>::pty:output` **限频**发布
// （同一句柄 ≥50 ms 一条，payload `{ ptyId }`）；本域把它转成前端事件，前端据此
// 立刻拉一轮——感知延迟从慢档 250 ms 降到毫秒级（宿主限频唤醒，ADR 0029 §7 的路线甲）。
//
// **提示而非承诺**：事件可被合并 / 丢弃（无订阅、订阅队列满、插件未激活、被限频合并），
// 正确性兜底仍是前端轮询 + `truncated` resync——数据面没有被改成 push，
// 环与背压语义（ADR 0022 D3）一字未变。

/// 前端事件名：某会话的输出环有新字节可拉（宿主限频唤醒的转发）
///
/// 载荷 `{ sessionId }`；订阅方（终端组件）只在**本组件的会话 id 命中**时拉取。
pub const EVENT_OUTPUT_AVAILABLE: &str = "session:output-available";

/// 通知载荷（纯函数，native 可测）：`{ sessionId }`
pub fn output_available_payload(session_id: &str) -> serde_json::Value {
    serde_json::json!({ "sessionId": session_id })
}

/// 从宿主通知载荷取句柄（纯函数，native 可测）：缺失 / 空串 / 非字符串 → `None`
///
/// 宿主契约是 `{ ptyId }`，但总线载荷是 JSON——形状不对就静默丢弃（不 panic、
/// 不伪造句柄），一次脏载荷不得打断整条输出链。
pub fn notify_pty_id(payload: &serde_json::Value) -> Option<&str> {
    payload
        .get("ptyId")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
}

/// 处理 `<owner>::pty:output` 通知：`ptyId` → `sessionId` → 前端事件（P2）
///
/// 返回解析出的 `sessionId`（供调用方驱动 **WS 终端连接的 drain**——同一份
/// 限频唤醒对两条消费路径都有用，见 `ws_terminal::drain_session`）。
///
/// 反查不到会话时只留 debug 日志——正常竞态（spawn 之后登记之前、会话刚移除），
/// 此刻前端也没有接入输出源，丢了不影响正确性。
#[cfg(target_arch = "wasm32")]
pub fn on_pty_output(payload: &serde_json::Value) -> Option<String> {
    let pty_id = notify_pty_id(payload)?;
    match crate::session::session_id_by_pty(pty_id) {
        Some(session_id) => {
            WasmHost.emit_event(
                EVENT_OUTPUT_AVAILABLE,
                &output_available_payload(&session_id),
            );
            Some(session_id)
        }
        None => {
            WasmHost.log_debug(&format!(
                "输出可用通知：pty 未登记会话（忽略；pty_id={pty_id}）"
            ));
            None
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn on_pty_output(_payload: &serde_json::Value) -> Option<String> {
    None
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
                    min_offset = Some(
                        fetched
                            .next_offset
                            .saturating_sub(fetched.data.len() as u64),
                    );
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

#[cfg(test)]
mod tests {
    use super::*;

    const HIGH: u64 = HIGH_WATER_BYTES;
    const LOW: u64 = LOW_WATER_BYTES;

    /// C1 正例：无未确认积压 → 放行
    #[test]
    fn gate_allows_when_nothing_unacked() {
        assert_eq!(decide_pull_gate(0, 0, 0, false, HIGH, LOW), PullGate::Allow);
    }

    /// C2 边界（上沿下 1 字节）：未驻留时积压 = HIGH − 1 → 放行
    #[test]
    fn gate_allows_just_below_high() {
        assert_eq!(
            decide_pull_gate(HIGH - 1, 0, 0, false, HIGH, LOW),
            PullGate::Allow
        );
    }

    /// C3 边界（上沿）：未驻留时积压恰达 HIGH → 进入驻留并如实上报未确认量
    #[test]
    fn gate_parks_at_high() {
        assert_eq!(
            decide_pull_gate(HIGH, 0, 0, false, HIGH, LOW),
            PullGate::Throttle { unacked: HIGH }
        );
    }

    /// C4 迟滞保持（核心：区间内不解除）：驻留中积压落在 [LOW, HIGH) → 继续抑制
    #[test]
    fn gate_stays_parked_inside_hysteresis_band() {
        assert_eq!(
            decide_pull_gate(LOW, 0, 0, true, HIGH, LOW),
            PullGate::Throttle { unacked: LOW }
        );
        assert_eq!(
            decide_pull_gate(HIGH - 1, 0, 0, true, HIGH, LOW),
            PullGate::Throttle { unacked: HIGH - 1 }
        );
    }

    /// C5 迟滞下沿：驻留中积压降到 LOW − 1 → 退出驻留（恢复放行）
    #[test]
    fn gate_unparks_below_low() {
        assert_eq!(
            decide_pull_gate(LOW - 1, 0, 0, true, HIGH, LOW),
            PullGate::Allow
        );
    }

    /// C6 正例（对照 C3）：ack 前移后同积压下未驻留 → 放行（消费驱动推进）
    #[test]
    fn gate_resumes_after_ack_moves() {
        assert_eq!(
            decide_pull_gate(HIGH, 1, 1, false, HIGH, LOW),
            PullGate::Allow
        );
    }

    /// C7 边界（重锚不可抑制）：游标落后于已确认水位 → 即使积压远超上沿也放行
    /// （且驻留态一并解除），否则 truncated/resync 后永远拿不到数据也永远不 ack（死锁）
    #[test]
    fn gate_allows_when_cursor_lags_acked_even_over_high() {
        assert_eq!(
            decide_pull_gate(HIGH * 2, HIGH, 0, false, HIGH, LOW),
            PullGate::Allow
        );
        // 驻留中同样放行（I4 优先于迟滞）
        assert_eq!(
            decide_pull_gate(HIGH * 2, HIGH, 0, true, HIGH, LOW),
            PullGate::Allow
        );
    }

    /// C8 异常边界：窗口为 0（显式完全背压）→ 未确认即进入驻留；重锚路径仍放行
    #[test]
    fn gate_with_zero_window_throttles_any_unacked() {
        assert_eq!(
            decide_pull_gate(0, 0, 0, false, 0, 0),
            PullGate::Throttle { unacked: 0 }
        );
        assert_eq!(decide_pull_gate(10, 5, 4, false, 0, 0), PullGate::Allow);
    }

    /// C9 常量不变量：LOW < HIGH 且 HIGH − ack(64 KiB) ≤ LOW（迟滞带 ≥ 一个 ack 周期）
    ///
    /// 环容量约束挂在 spawn 侧声明的 [`crate::launch::SESSION_PTY_RING_BYTES`]
    /// 上（不写死 256 KiB：那是宿主**默认**值，本插件已显式声明更大的环）。
    /// 编译期版本在 `launch.rs`（常量自检在编译期跑，不依赖测试进程）。
    #[test]
    fn water_level_constants_hold_hysteresis_invariant() {
        assert!(LOW < HIGH);
        assert!(HIGH - 64 * 1024 <= LOW);
        // 环容量约束：背压窗口 ≤ 声明环容量 / 2（窗口比环还深时抑制无意义）
        assert!(HIGH * 2 <= crate::launch::SESSION_PTY_RING_BYTES);
    }

    // ==================== 水位表（单调 + 回收） ====================

    /// W1 水位单调：pushed/acked 只进不退（乱序/重放的旧值被忽略）
    #[test]
    fn watermarks_are_monotonic() {
        let sid = "test-watermarks-monotonic";
        pump::note_pushed(sid, 100);
        pump::note_pushed(sid, 50); // 回退值被忽略
        pump::note_acked(sid, 80);
        pump::note_acked(sid, 10); // 回退值被忽略
        assert_eq!(pump::watermarks(sid), (100, 80, false));
        pump::forget(sid);
    }

    /// W2 驻留态读写：set_parked 翻转且随水位一并读出
    #[test]
    fn park_state_round_trips() {
        let sid = "test-park-round-trip";
        assert_eq!(pump::watermarks(sid).2, false);
        pump::set_parked(sid, true);
        assert_eq!(pump::watermarks(sid).2, true);
        pump::set_parked(sid, false);
        assert_eq!(pump::watermarks(sid).2, false);
        pump::forget(sid);
    }

    /// W3 forget 回收：会话销毁后条目摘除（无泄漏），再次读取回到零水位
    #[test]
    fn forget_reclaims_entry() {
        let sid = "test-forget-reclaims";
        pump::note_pushed(sid, 4096);
        pump::set_parked(sid, true);
        assert!(pump::len() > 0);
        pump::forget(sid);
        // 已摘除：再次读取建新零水位条目（不是残留旧水位）
        assert_eq!(pump::watermarks(sid), (0, 0, false));
        pump::forget(sid);
    }

    // ==================== 诊断读面（水位快照 + 观测计数） ====================

    /// D1 驻留计数只在**状态翻转**时累计：驻留期 N 次抑制 → parkCount 仍为 1
    /// （把重复置真计成 N 次会让「一次驻留」在诊断里被放大成 N 次震荡）
    #[test]
    fn park_counts_only_on_state_flips() {
        let sid = "test-park-counts";
        pump::set_parked(sid, true);
        pump::set_parked(sid, true); // 重复抑制：不应再计
        pump::note_throttled(sid);
        pump::note_throttled(sid);
        pump::note_throttled(sid);
        pump::set_parked(sid, false);
        pump::set_parked(sid, false); // 重复放行：不应再计
        let row = &pump::snapshots(Some(sid))[0];
        assert_eq!(row.park_count, 1);
        assert_eq!(row.unpark_count, 1);
        assert_eq!(row.throttled_pulls, 3);
        assert!(!row.parked);
        pump::forget(sid);
    }

    /// D2 环淘汰计数 + 未确认量派生：unacked = pushed − acked（快照自算，不依赖调用方）
    #[test]
    fn snapshot_reports_unacked_and_truncation_count() {
        let sid = "test-snapshot-unacked";
        pump::note_pushed(sid, 10_000);
        pump::note_acked(sid, 4_000);
        pump::note_truncated(sid);
        pump::note_truncated(sid);
        let row = &pump::snapshots(Some(sid))[0];
        assert_eq!((row.pushed, row.acked, row.unacked), (10_000, 4_000, 6_000));
        assert_eq!(row.truncated_count, 2);
        pump::forget(sid);
    }

    /// D3 过滤是**纯读且不建条目**：指定会话只回自己那行；未知会话 → 空列表
    /// （不伪造零水位行），且查询本身不留下条目
    #[test]
    fn snapshot_filter_is_pure_read() {
        let a = "test-snapshot-filter-a";
        let b = "test-snapshot-filter-b";
        pump::note_pushed(a, 111);
        pump::note_pushed(b, 222);
        // 指定会话：只回该行（其他行不得泄漏）
        let only_a = pump::snapshots(Some(a));
        assert_eq!(only_a.len(), 1);
        assert_eq!(only_a[0].session_id, a);
        // 不过滤：两行都在
        let all_rows = pump::snapshots(None);
        let all: Vec<&str> = all_rows.iter().map(|r| r.session_id.as_str()).collect();
        assert!(all.contains(&a) && all.contains(&b));
        // 未知会话：空列表（不是零水位行）
        let unknown = "test-snapshot-filter-unknown";
        assert!(pump::snapshots(Some(unknown)).is_empty());
        // 纯读：查询本身不留下条目
        assert!(!pump::snapshots(None)
            .iter()
            .any(|r| r.session_id == unknown));
        pump::forget(a);
        pump::forget(b);
    }

    // ==================== P2：输出可用通知（宿主限频唤醒） ====================

    /// P2：通知载荷解析——只认形状正确的 `{ ptyId: <非空字符串> }`
    /// （脏载荷静默丢弃，不 panic、不伪造句柄）
    #[test]
    fn notify_pty_id_reads_only_well_formed_payloads() {
        assert_eq!(
            notify_pty_id(&serde_json::json!({ "ptyId": "pty-1" })),
            Some("pty-1")
        );
        assert_eq!(notify_pty_id(&serde_json::json!({ "ptyId": "" })), None);
        assert_eq!(notify_pty_id(&serde_json::json!({ "ptyId": 7 })), None);
        assert_eq!(notify_pty_id(&serde_json::json!({ "other": "x" })), None);
        assert_eq!(notify_pty_id(&serde_json::Value::Null), None);
    }

    /// P2：事件名与载荷形状锁——前端 `context.events.on` 的 key 与本常量逐字一致
    /// （改名而前端未跟 = 永久收不到，且只在真机可见）
    #[test]
    fn output_available_event_name_and_payload_are_stable() {
        assert_eq!(EVENT_OUTPUT_AVAILABLE, "session:output-available");
        assert_eq!(
            output_available_payload("sess-1"),
            serde_json::json!({ "sessionId": "sess-1" })
        );
    }

    /// P2 结构锁：`lib.rs` 必须两处出现 `PTY_OUTPUT` 常量——activate 订阅 + `on_message` 路由
    ///
    /// 漏订阅 = **静默退化回纯轮询**（功能不坏，延迟特性丢失，行为测试看不见）；
    /// 漏路由 = 订阅了也白订。两处都钉死在常量字面量上：少一处即红，多一处要来说明。
    #[test]
    fn lib_wires_output_notify_subscription_and_routing() {
        let path = format!("{}/src/lib.rs", env!("CARGO_MANIFEST_DIR"));
        let src = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
        let hits = src
            .lines()
            .filter(|line| !line.trim_start().starts_with("//") && line.contains("PTY_OUTPUT"))
            .count();
        assert_eq!(
            hits, 2,
            "lib.rs 需恰好两处 PTY_OUTPUT：activate 期订阅 + on_message 路由（实测 {hits}）"
        );
    }

    /// D4 报告 JSON 形状：字段名与计数逐项落到线上（诊断面靠它被人读/被脚本抓）
    ///
    /// 会话级段（`entries`）与连接级段（`ws`）分开上报：两者 ack 基准不同源
    /// （会话级 = 环绝对偏移；WS 级 = `base + 客户端本地计数`）。
    #[test]
    fn report_json_exposes_every_stat() {
        let rows = vec![PumpSnapshot {
            session_id: "sess-x".to_string(),
            pushed: 300,
            acked: 100,
            unacked: 200,
            parked: true,
            park_count: 2,
            unpark_count: 1,
            throttled_pulls: 7,
            truncated_count: 5,
        }];
        let ws_rows = vec![crate::ws_terminal::WsWatermarkRow {
            endpoint_id: "terminal".to_string(),
            client_id: "10.0.0.2:5001".to_string(),
            session_id: "sess-x".to_string(),
            base: 4096,
            pushed: 8192,
            acked: 6144,
            unacked: 2048,
            parked: false,
            park_count: 1,
            unpark_count: 1,
            throttled_cycles: 9,
            forced_drains: 0,
        }];
        let json = render_watermark_report(&rows, &ws_rows);
        assert_eq!(json["count"], 1);
        let entry = &json["entries"][0];
        assert_eq!(entry["sessionId"], "sess-x");
        assert_eq!(entry["pushed"], 300);
        assert_eq!(entry["acked"], 100);
        assert_eq!(entry["unacked"], 200);
        assert_eq!(entry["parked"], true);
        assert_eq!(entry["parkCount"], 2);
        assert_eq!(entry["unparkCount"], 1);
        assert_eq!(entry["throttledPulls"], 7);
        assert_eq!(entry["truncatedCount"], 5);

        // WS 段：连接级行 + 背压计数（含 forcedDrains 错位信号）
        assert_eq!(json["wsCount"], 1);
        let ws_entry = &json["ws"][0];
        assert_eq!(ws_entry["endpointId"], "terminal");
        assert_eq!(ws_entry["clientId"], "10.0.0.2:5001");
        assert_eq!(ws_entry["sessionId"], "sess-x");
        assert_eq!(ws_entry["base"], 4096);
        assert_eq!(ws_entry["pushed"], 8192);
        assert_eq!(ws_entry["acked"], 6144);
        assert_eq!(ws_entry["unacked"], 2048);
        assert_eq!(ws_entry["parked"], false);
        assert_eq!(ws_entry["parkCount"], 1);
        assert_eq!(ws_entry["unparkCount"], 1);
        assert_eq!(ws_entry["throttledCycles"], 9);
        assert_eq!(ws_entry["forcedDrains"], 0);

        // 空表：仍是合法形状（count 0 / entries 空 / ws 空），不是缺字段的 null
        let empty = render_watermark_report(&[], &[]);
        assert_eq!(empty["count"], 0);
        assert!(empty["entries"].as_array().is_some_and(|e| e.is_empty()));
        assert_eq!(empty["wsCount"], 0);
        assert!(empty["ws"].as_array().is_some_and(|e| e.is_empty()));
    }
}
