//! 桥接基准工程 · webview 层的 **Channel 传输面**（**仅 debug 构建**）
//!
//! # 为什么要有这个模块
//!
//! 产品当前只用两条 Tauri 传输面：命令返回值（`plugin_invoke`）与事件
//! （`host-events.emit` → `app_handle.emit`）。**`tauri::ipc::Channel` 全仓零使用**
//! （Rust / TS 两侧都没有）。而基准要回答「若把高频大块的流式面迁到 Channel，
//! 值不值」，就必须有一条真 Channel 通路可测——否则这个决策永远缺数据。
//!
//! 历史注记：桌面端**曾经**有过 Channel 传输的终端输出流
//! （`commands/terminal_stream.rs`：`subscribe_terminal_channel` /
//! `terminal_channel_ack` / `unsubscribe_terminal_channel`，268 行），2026-09-22
//! 随会话下沉（票 05 宿主终端兜底摘除）整体删除。删除原因是**架构**（宿主不留
//! 终端兜底），不是性能——但那条流的形态（Raw 字节经 Channel 持续推送 + ack 背压）
//! 与 output-ack 专项的 P2「宿主侧 push」是同一个问题，值得用数据回答。
//!
//! # 边界（AGENTS §5.1）
//!
//! - **仅 `#[cfg(debug_assertions)]` 编译与注册**：release 产物无此命令面
//!   （先例：`tauri-plugin-wdio` 同样只在 debug 注册，见 `lib.rs`）；
//! - **零业务语义**：只按参数字节数造载荷并回送，不含任何产品名词 / 状态 / 存储 / 路由；
//! - **不碰任何既有能力面**，纯新增，不改既有命令；
//! - 与夹具（`packages/plugin-bench-test`）分工：那条路测「wasm → 宿主原语 → wasm」，
//!   本命令测「宿主 → webview」这一段的**纯传输成本**（把 guest 因素排除，才能与
//!   W1/W2 的读数同口径对比）。
//!
//! # 机制差异（读数解释的前提）
//!
//! 来自 tauri 2.11.1 源码：
//!
//! - **事件**：每个事件一次 `webview.eval`，且 payload 被**格式化进 JS 源码**
//!   （`event/mod.rs::emit_js_script`）→ 高频大块事件最吃亏；
//! - **Channel**：`channel.rs` 按大小分叉——小体走 `eval`（raw 阈值 1024 B），
//!   **大体走 fetch 路径**（body 存 `ChannelDataIpcQueue`，webview 侧 `fetch` 读）；
//! - **命令返回**：单次响应。
//!
//! 所以 W5 的意义是量化「同一批字节走三条面」的差，而不是宣称 Channel 更好。

use tauri::ipc::{Channel, InvokeResponseBody, Response};

/// 单次流式命令的载荷上限（防御：防止基准被参数写错时打爆 webview 内存）
pub const MAX_STREAM_BYTES: usize = 64 * 1024 * 1024;
/// 单块上限（防御：chunkBytes 过小会把 1 MiB 变成上千次 send，失去对比意义）
pub const MAX_STREAM_CHUNK_BYTES: usize = 8 * 1024 * 1024;

/// 解析 `{ bytes, chunkBytes }`，返回 `(总量, 块大小)`
fn parse_stream_args(payload: &serde_json::Value) -> Result<(usize, usize), String> {
    let bytes = payload
        .get("bytes")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| "bench channel: bytes is required (u64)".to_string())? as usize;
    let chunk = payload
        .get("chunkBytes")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| "bench channel: chunkBytes is required (u64)".to_string())?
        .max(1) as usize;
    if bytes > MAX_STREAM_BYTES {
        return Err(format!("bench channel: bytes {bytes} exceeds cap {MAX_STREAM_BYTES}"));
    }
    if chunk > MAX_STREAM_CHUNK_BYTES {
        return Err(format!(
            "bench channel: chunkBytes {chunk} exceeds cap {MAX_STREAM_CHUNK_BYTES}"
        ));
    }
    Ok((bytes, chunk))
}

/// 基准：按块把 `total` 字节经 **Raw Channel** 推给 webview
///
/// 对应 JS 侧 `new Channel<Uint8Array>()`，命中 `channel.rs` 的
/// **fetch 路径**（raw ≥ 1024 B）——这是「终端输出流 / 文件块」的真实形态。
#[tauri::command]
pub async fn bench_channel_stream_bytes(
    payload: serde_json::Value,
    on_chunk: Channel<Vec<u8>>,
) -> Result<serde_json::Value, String> {
    let (total, chunk) = parse_stream_args(&payload)?;
    let mut sent = 0usize;
    let mut chunks = 0usize;
    while sent < total {
        let take = chunk.min(total - sent);
        on_chunk
            .send(vec![b'a'; take])
            .map_err(|e| format!("bench channel send: {e}"))?;
        sent += take;
        chunks += 1;
    }
    Ok(serde_json::json!({ "bytes": sent, "chunks": chunks, "format": "bytes" }))
}

/// 基准：按块把 `total` 字节经 **JSON 字符串 Channel** 推给 webview
///
/// 对应 JS 侧 `new Channel<string>()`，与事件面（payload 是 JSON 字符串）同形态，
/// 用来隔离「raw 通道 vs JSON 通道」的序列化差。
#[tauri::command]
pub async fn bench_channel_stream_text(
    payload: serde_json::Value,
    on_chunk: Channel<String>,
) -> Result<serde_json::Value, String> {
    let (total, chunk) = parse_stream_args(&payload)?;
    let mut sent = 0usize;
    let mut chunks = 0usize;
    while sent < total {
        let take = chunk.min(total - sent);
        on_chunk
            .send("a".repeat(take))
            .map_err(|e| format!("bench channel send: {e}"))?;
        sent += take;
        chunks += 1;
    }
    Ok(serde_json::json!({ "bytes": sent, "chunks": chunks, "format": "text" }))
}

/// 基准：按块把 `total` 字节经 **真正的 raw 字节 Channel**（`Channel<Response>`）推给 webview
///
/// **这条才是「字节流」该用的形态**，与上一条形成对照：
///
/// tauri 2.11 的 `IpcResponse` 有**泛型 blanket impl**（`ipc/mod.rs`）：
/// `impl<T: Serialize> IpcResponse for T` → `serde_json::to_string` → `InvokeResponseBody::Json`。
/// 于是 `Channel<Vec<u8>>` 走的是 **JSON 数组**（JS 侧收到 `[object Array]`，字节被序列化成
/// `97,97,97,…`），**不是** Uint8Array / ArrayBuffer。只有 `Channel<Response>` +
/// `InvokeResponseBody::Raw` 才进 raw 通路（webview 侧 `new Uint8Array(...).buffer`，
/// 且 ≥1024 B 时可走 fetch 快路径）。
///
/// 这个坑对「若把终端输出流迁到 Channel」是决定性的：写成 `Channel<Vec<u8>>` 会静默
/// 拿到最贵的形态（实测 W5-vec8 行）。
#[tauri::command]
pub async fn bench_channel_stream_raw(
    payload: serde_json::Value,
    on_chunk: Channel<Response>,
) -> Result<serde_json::Value, String> {
    let (total, chunk) = parse_stream_args(&payload)?;
    let mut sent = 0usize;
    let mut chunks = 0usize;
    while sent < total {
        let take = chunk.min(total - sent);
        on_chunk
            .send(Response::new(InvokeResponseBody::Raw(vec![b'a'; take])))
            .map_err(|e| format!("bench channel send: {e}"))?;
        sent += take;
        chunks += 1;
    }
    Ok(serde_json::json!({ "bytes": sent, "chunks": chunks, "format": "raw" }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **闸门锁**：这条命令面不得漏进 release 产物。
    ///
    /// 门是 `#[cfg(debug_assertions)]` 的**声明处与注册处**两行；本测试
    /// 从源码文本上钉死它们——模块声明与 `mod` 项必须在 lib.rs 里带 cfg 门，
    /// 两条 `generate_handler!` 项也必须带 cfg 门。删掉任一门即红。
    #[test]
    fn bench_channel_surface_stays_debug_only() {
        let lib = std::fs::read_to_string("src/lib.rs").expect("read src/lib.rs");
        let lines: Vec<&str> = lib.lines().collect();

        // 门在**属性行**上（`#[cfg(debug_assertions)]` 是 `mod` 的上一行），
        // 故每个待查项都要连同前一行一起看——只看命中行会把“门在前一行”误判为无门
        let with_prev = |needle: &str| -> (String, String) {
            let idx = lines
                .iter()
                .position(|l| l.contains(needle))
                .unwrap_or_else(|| panic!("src/lib.rs 应含 {needle}"));
            let hit = lines[idx].trim().to_string();
            let prev = idx
                .checked_sub(1)
                .map(|i| lines[i].trim().to_string())
                .unwrap_or_default();
            (hit, prev)
        };

        let (hit, prev) = with_prev("mod bench_channel");
        assert!(
            hit.contains("cfg(debug_assertions)") || prev.contains("cfg(debug_assertions)"),
            "bench_channel 模块声明必须带 #[cfg(debug_assertions)] 门，否则 release 产物会带基准命令面：{prev} / {hit}"
        );

        for cmd in [
            "bench_channel_stream_bytes",
            "bench_channel_stream_text",
            "bench_channel_stream_raw",
        ] {
            let (hit, prev) = with_prev(cmd);
            assert!(
                hit.contains("cfg(debug_assertions)") || prev.contains("cfg(debug_assertions)"),
                "{cmd} 的注册必须紧邻 #[cfg(debug_assertions)] 门：{prev} / {hit}"
            );
        }
    }

    /// 参数契约：bytes / chunkBytes 必填，超限显性拒绝（不打爆 webview）
    #[test]
    fn parse_args_rejects_missing_and_oversized() {
        let ok = parse_stream_args(&serde_json::json!({ "bytes": 1024, "chunkBytes": 256 })).unwrap();
        assert_eq!(ok, (1024, 256));

        assert!(parse_stream_args(&serde_json::json!({ "chunkBytes": 16 })).is_err());
        assert!(parse_stream_args(&serde_json::json!({ "bytes": 16 })).is_err());
        assert!(
            parse_stream_args(&serde_json::json!({ "bytes": MAX_STREAM_BYTES + 1, "chunkBytes": 16 })).is_err(),
            "超总量上限必须拒绝"
        );
        assert!(
            parse_stream_args(&serde_json::json!({ "bytes": 16, "chunkBytes": MAX_STREAM_CHUNK_BYTES + 1 })).is_err(),
            "超单块上限必须拒绝"
        );
        // chunkBytes = 0 → 夹到 1（不发 0 长度块，避免死循环）
        assert_eq!(
            parse_stream_args(&serde_json::json!({ "bytes": 4, "chunkBytes": 0 })).unwrap(),
            (4, 1)
        );
    }
}
