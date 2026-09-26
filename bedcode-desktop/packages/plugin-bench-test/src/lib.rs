//! 桥接基准夹具插件（bench 被测端）
//!
//! 本 crate 是 `bedcode-desktop/bench/` 基准工程在 **wasm 侧**的唯一被测对象：
//! 宿主侧 harness（`src-tauri/tests/wasm_bridge_bench.rs`）把本组件实例化成两个属主
//! （`com.bedcode.bench` 主实例 + `com.bedcode.bench.peer` 对端），经**生产同形**的
//! `PluginHost::invoke_rust_command` 通道下发 `bench.*` 命令，测量
//! 「前端命令面 → 宿主 → wasm → 宿主原语 → wasm → 宿主」整条桥接链的成本。
//!
//! # 场景分组（与 bench/README.md 的场景矩阵一一对应）
//!
//! | 组 | 命令 | 测量对象 |
//! | --- | --- | --- |
//! | A 基线 | `nop` / `echo` / `churn` | 命令通道空载、载荷尺寸扫描、guest 内部 CPU 下界 |
//! | B 原语往返 | `log` / `storage-rt` / `crypto-rt` / `fs-rt` / `db-bulk` | 单一 host import 的固定开销与每字节成本 |
//! | C 事件推送 | `emit` / `emit-chunk` | wasm → 宿主 → 前端 的**大包单发 vs 分块流式**（含 output-ack 论题的实测数据） |
//! | D 插件互调 | `bus-subscribe` / `bus-publish` / `bus-binary` / `bus-recv` / `api-serve` / `api-call` | wasm → 宿主总线 → wasm（JSON vs 二进制）、wasm → 宿主互调门 → wasm（阻塞 JSON-RPC） |
//! | E 流式 | `pty-stream` / `http-fetch` | 终端输出流（ring-fetch 游标拉取 + 可选再转发）、非流式大响应 |
//! | F 异步 | `proc-async` / `proc-sync` / `timer-fire` | 异步事件回调 vs 同步阻塞等待的形态差 |
//!
//! # guest 侧计时的口径
//!
//! guest 用 `std::time::Instant`（wasm32-wasip3 的 wasi:clocks，宿主 p3 linker 提供）
//! 在**命令内部**分段计时并回传（`*Nanos` 字段）；harness 同时测命令的墙钟时间。
//! 二者之差即「宿主侧（含锁 / 序列化 / 事件投递 / 任务调度）开销」，
//! 是把 wasm 边界成本归因到具体环节的依据。
//!
//! # 实例级状态
//!
//! 计数与开关一律放**实例级静态**（`std::sync::Mutex` + 原子）：wasm32-wasip3 的
//! `thread_local!` 是真 TLS，跨调用线程读空（同 `plugin-pty-test` / `plugin-ws-test`
//! 的既有约定），故此处不用 thread_local。

use bedcode_plugin_api::api_call::{self, API_TOPIC_PREFIX, REPLY_TOPIC_PREFIX};
use bedcode_plugin_api::host::pty::PtySpawnConfig;
use bedcode_plugin_api::host::{
    HostBus, HostCrypto, HostEvents, HostFs, HostHttp, HostLog, HostPluginDatabase, HostProcess,
    HostPty, HostStorage, HostTimer,
};
use bedcode_plugin_api::types::PluginManifest;
use bedcode_plugin_api::wasm::WasmPlugin;
use bedcode_plugin_api::wasm_host::WasmHost;
use bedcode_plugin_api::BusMessage;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

/// 基准事件名（前端 `listen` 面；无头 harness 下 emit 降级为 warn + Ok，
/// 真实投递成本由 webview 层测量）
const EVENT_BENCH: &str = "plugin:bench:probe";

/// 互调请求 topic（manifest `api` 全限定名的 `bedcode.api.` 前缀形态）
const API_ECHO: &str = "com.bedcode.bench.echo";
const API_CHURN: &str = "com.bedcode.bench.churn";

// ==================== 跨调用累计状态（每实例一份） ====================

/// 互调服务开关：同一产物以两个属主实例化（主实例只调、对端只服），
/// 未开启者收到请求**不回**——否则两实例抢答同一 reply topic
static SERVING: AtomicBool = AtomicBool::new(false);
/// 互调服务累计应答数 / 应答字节
static API_SERVED: AtomicU64 = AtomicU64::new(0);
static API_SERVED_BYTES: AtomicU64 = AtomicU64::new(0);
/// 进程完成事件回调计数
static PROCESS_DONE: AtomicU64 = AtomicU64::new(0);
/// 定时器命令触发计数（`bench.timer-fire` 注册的命令即本插件命令面）
static TIMER_FIRED: AtomicU64 = AtomicU64::new(0);
/// 总线 JSON / 二进制收讫统计
static RECV_JSON: Mutex<(u64, u64)> = Mutex::new((0, 0));
static RECV_BIN: Mutex<(u64, u64)> = Mutex::new((0, 0));
/// 最近一次 http fetch 的响应体（供 harness 做不透明搬运校验）
static LAST_HTTP_BODY: Mutex<(u64, u64)> = Mutex::new((0, 0));

/// 桥接基准夹具插件
pub struct BenchPlugin;

impl WasmPlugin for BenchPlugin {
    const ID: &'static str = "com.bedcode.bench";

    fn manifest() -> PluginManifest {
        serde_json::from_str(include_str!("../plugin.json"))
            .expect("plugin.json must be valid PluginManifest")
    }

    /// 订阅互调请求道（服务方角色由 `bench.api-serve` 开关控制，见 SERVING）
    fn activate() -> anyhow::Result<()> {
        let host = WasmHost;
        for api in [API_ECHO, API_CHURN] {
            let topic = format!("{API_TOPIC_PREFIX}{api}");
            if let Err(e) = host.bus_subscribe(&topic) {
                host.log_warn(&format!("bench: subscribe {topic} failed: {e}"));
            }
        }
        Ok(())
    }

    fn deactivate() -> anyhow::Result<()> {
        Ok(())
    }

    fn invoke_command(name: &str, args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let host = WasmHost;
        match name {
            // ==================== A · 基线 ====================

            // A1 命令通道空载：一次完整的「宿主 → guest → 宿主」往返，无任何原语调用。
            // 这是所有场景的地板值，也是 `plugin_invoke` 命令面的固定开销下界。
            "bench.nop" => Ok(serde_json::json!({ "ok": true })),

            // A2 载荷尺寸扫描：guest 生成 `bytes` 长度的串并原样回传。
            // 命令总耗时 = 通道固定开销 + 2×载荷编解码 + guest 分配。
            "bench.echo" => {
                let bytes = arg_usize(&args, "bytes");
                let payload = "a".repeat(bytes);
                Ok(serde_json::json!({ "bytes": bytes, "payload": payload }))
            }

            // A3 guest 内部 CPU 下界：同样的产出量，但不经边界回传。
            // 与 A2 同尺寸之差 ≈ 载荷跨越 WASM 边界（序列化 + 拷贝 + 反序列化）的成本。
            "bench.churn" => {
                let bytes = arg_usize(&args, "bytes");
                let t0 = std::time::Instant::now();
                let mut acc: u64 = 0;
                let mut sink = String::with_capacity(bytes);
                while sink.len() < bytes {
                    sink.push('a');
                    acc = acc.wrapping_add(1);
                }
                std::hint::black_box(&sink);
                Ok(
                    serde_json::json!({ "bytes": sink.len(), "acc": acc, "guestNanos": t0.elapsed().as_nanos() as u64 }),
                )
            }

            // ==================== B · 单一原语往返 ====================

            // B1 host-log 往返 ×count：最廉价的 host import，用作「一次 import 调用」
            // 的固定开销标尺（无序列化、无分配）。
            "bench.log" => {
                let count = arg_usize(&args, "count");
                let bytes = arg_usize(&args, "bytes");
                let message = "x".repeat(bytes);
                let t0 = std::time::Instant::now();
                for _ in 0..count {
                    host.log_debug(&message);
                }
                let elapsed = t0.elapsed().as_nanos() as u64;
                Ok(serde_json::json!({ "count": count, "guestNanos": elapsed }))
            }

            // B2 插件 KV：大 value 的 set + get（宿主 SQLite plugin_storage 表）
            "bench.storage-rt" => {
                let key = arg_str(&args, "key").unwrap_or_else(|| "bench.payload".to_string());
                let bytes = arg_usize(&args, "bytes");
                let value = serde_json::Value::String("a".repeat(bytes));
                let t0 = std::time::Instant::now();
                host.storage_set(&key, &value)
                    .map_err(|e| anyhow::anyhow!("storage_set: {e}"))?;
                let set_done = t0.elapsed().as_nanos() as u64;
                let read = host
                    .storage_get(&key)
                    .map_err(|e| anyhow::anyhow!("storage_get: {e}"))?
                    .unwrap_or(serde_json::Value::Null);
                let read_bytes = read.as_str().map(|s| s.len()).unwrap_or(0);
                Ok(serde_json::json!({
                    "bytes": bytes,
                    "readBytes": read_bytes,
                    "setNanos": set_done,
                    "guestNanos": t0.elapsed().as_nanos() as u64,
                }))
            }

            // B3 AEAD 加解密（传输加密面）：host-crypto 引擎的每字节成本
            "bench.crypto-rt" => {
                let bytes = arg_usize(&args, "bytes");
                let algorithm =
                    arg_str(&args, "algorithm").unwrap_or_else(|| "aes-256-gcm".to_string());
                let key = host
                    .aead_generate_key(&algorithm)
                    .map_err(|e| anyhow::anyhow!("aead_generate_key: {e}"))?;
                let nonce = host
                    .aead_generate_nonce(&algorithm)
                    .map_err(|e| anyhow::anyhow!("aead_generate_nonce: {e}"))?;
                let plaintext = vec![b'a'; bytes];
                let t0 = std::time::Instant::now();
                let cipher = host
                    .aead_encrypt(&algorithm, &key, &nonce, &plaintext, None)
                    .map_err(|e| anyhow::anyhow!("aead_encrypt: {e}"))?;
                let enc_nanos = t0.elapsed().as_nanos() as u64;
                let plain = host
                    .aead_decrypt(&algorithm, &key, &nonce, &cipher, None)
                    .map_err(|e| anyhow::anyhow!("aead_decrypt: {e}"))?;
                Ok(serde_json::json!({
                    "bytes": bytes,
                    "cipherBytes": cipher.len(),
                    "plainBytes": plain.len(),
                    "encNanos": enc_nanos,
                    "guestNanos": t0.elapsed().as_nanos() as u64,
                }))
            }

            // B4 文件读写（host-fs；宿主 fs_auth 需已授权前缀，harness 预授权临时目录）
            "bench.fs-rt" => {
                let path =
                    arg_str(&args, "path").ok_or_else(|| anyhow::anyhow!("path is required"))?;
                let bytes = arg_usize(&args, "bytes");
                let data = "a".repeat(bytes);
                let t0 = std::time::Instant::now();
                host.fs_write(&path, &data)
                    .map_err(|e| anyhow::anyhow!("fs_write: {e}"))?;
                let write_nanos = t0.elapsed().as_nanos() as u64;
                let read = host
                    .fs_read(&path)
                    .map_err(|e| anyhow::anyhow!("fs_read: {e}"))?;
                Ok(serde_json::json!({
                    "bytes": bytes,
                    "readBytes": read.map(|s| s.len()).unwrap_or(0),
                    "writeNanos": write_nanos,
                    "guestNanos": t0.elapsed().as_nanos() as u64,
                }))
            }

            // B5 插件私有库批量写 + 全量读回（业务真源面：会话表 / 传输历史）
            "bench.db-bulk" => {
                let rows = arg_usize(&args, "rows");
                let bytes = arg_usize(&args, "bytes");
                host.plugin_db_execute(
                    "CREATE TABLE IF NOT EXISTS bench_rows (id INTEGER PRIMARY KEY, blob TEXT)",
                )
                .map_err(|e| anyhow::anyhow!("plugin_db_execute(create): {e}"))?;
                host.plugin_db_execute("DELETE FROM bench_rows")
                    .map_err(|e| anyhow::anyhow!("plugin_db_execute(clean): {e}"))?;
                let blob = "a".repeat(bytes);
                let t0 = std::time::Instant::now();
                for i in 0..rows {
                    host.plugin_db_execute_params(
                        "INSERT INTO bench_rows (id, blob) VALUES (?1, ?2)",
                        &[serde_json::json!(i), serde_json::json!(blob)],
                    )
                    .map_err(|e| anyhow::anyhow!("plugin_db_execute_params(insert): {e}"))?;
                }
                let write_nanos = t0.elapsed().as_nanos() as u64;
                let read = host
                    .plugin_db_query("SELECT blob FROM bench_rows ORDER BY id")
                    .map_err(|e| anyhow::anyhow!("plugin_db_query: {e}"))?;
                let read_rows = read
                    .as_ref()
                    .and_then(|v| v.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);
                Ok(serde_json::json!({
                    "rows": rows,
                    "bytes": bytes,
                    "readRows": read_rows,
                    "writeNanos": write_nanos,
                    "guestNanos": t0.elapsed().as_nanos() as u64,
                }))
            }

            // B6 事务批执行（execute-batch）：一条语句 vs 一批语句的固定开销差
            "bench.db-batch" => {
                let statements = arg_usize(&args, "statements");
                let mut sqls = Vec::with_capacity(statements);
                for _ in 0..statements {
                    sqls.push(
                        "CREATE TABLE IF NOT EXISTS bench_batch (id INTEGER PRIMARY KEY, v TEXT)"
                            .to_string(),
                    );
                }
                let t0 = std::time::Instant::now();
                let affected = host
                    .plugin_db_execute_batch(&sqls)
                    .map_err(|e| anyhow::anyhow!("plugin_db_execute_batch: {e}"))?;
                Ok(serde_json::json!({
                    "statements": statements,
                    "affected": affected,
                    "guestNanos": t0.elapsed().as_nanos() as u64,
                }))
            }

            // ==================== C · wasm → 宿主 → 前端 事件推送 ====================

            // C1 大包单发 ×count：单次 `host-events.emit` 的固定开销 × 载荷规模。
            // 无头上下文中 emit 降级（无 AppHandle → warn + Ok），测到的是
            // 「guest 序列化 + import 桥」这一段；真实前端投递见 webview 层。
            "bench.emit" => {
                let bytes = arg_usize(&args, "bytes");
                let count = arg_usize(&args, "count").max(1);
                let payload = serde_json::json!({ "seq": 0, "blob": "a".repeat(bytes) });
                let t0 = std::time::Instant::now();
                for i in 0..count {
                    host.emit_event(
                        EVENT_BENCH,
                        &serde_json::json!({ "seq": i, "blob": "a".repeat(bytes) }),
                    );
                }
                let _ = payload;
                Ok(
                    serde_json::json!({ "bytes": bytes, "count": count, "guestNanos": t0.elapsed().as_nanos() as u64 }),
                )
            }

            // C2 分块流式转发：同体量按 `chunkBytes` 切片逐块 emit。
            // 与 C1 对比即「单大包 vs N 小包」在前端侧的摊薄曲线——
            // output-ack / 分片推送议题的实测依据。
            "bench.emit-chunk" => {
                let bytes = arg_usize(&args, "bytes");
                let chunk = arg_usize(&args, "chunkBytes").max(1);
                let mut emitted = 0usize;
                let mut chunks = 0usize;
                let t0 = std::time::Instant::now();
                while emitted < bytes {
                    let take = chunk.min(bytes - emitted);
                    host.emit_event(
                        EVENT_BENCH,
                        &serde_json::json!({ "seq": chunks, "blob": "a".repeat(take) }),
                    );
                    emitted += take;
                    chunks += 1;
                }
                Ok(serde_json::json!({
                    "bytes": bytes,
                    "chunks": chunks,
                    "guestNanos": t0.elapsed().as_nanos() as u64,
                }))
            }

            // ==================== D · wasm ↔ wasm ====================

            // D1 订阅（JSON 或二进制偏好）；`binary=true` 走 subscribe-binary
            "bench.bus-subscribe" => {
                let topic = arg_str(&args, "topic").unwrap_or_else(|| "bench.probe".to_string());
                let binary = args
                    .get("binary")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if binary {
                    host.bus_subscribe_binary(&topic)
                        .map_err(|e| anyhow::anyhow!("bus_subscribe_binary: {e}"))?;
                } else {
                    host.bus_subscribe(&topic)
                        .map_err(|e| anyhow::anyhow!("bus_subscribe: {e}"))?;
                }
                Ok(serde_json::json!({ "topic": topic, "binary": binary }))
            }

            // D2 JSON 载荷发布 ×count（总线条：wasm → 宿主总线 → 另一实例 wasm）
            "bench.bus-publish" => {
                let topic = arg_str(&args, "topic").unwrap_or_else(|| "bench.probe".to_string());
                let bytes = arg_usize(&args, "bytes");
                let count = arg_usize(&args, "count").max(1);
                let t0 = std::time::Instant::now();
                for i in 0..count {
                    host.bus_publish(
                        &topic,
                        &serde_json::json!({ "seq": i, "blob": "a".repeat(bytes) }),
                    )
                    .map_err(|e| anyhow::anyhow!("bus_publish: {e}"))?;
                }
                Ok(
                    serde_json::json!({ "topic": topic, "bytes": bytes, "count": count, "guestNanos": t0.elapsed().as_nanos() as u64 }),
                )
            }

            // D3 二进制载荷发布 ×count（零 JSON 编解码，MB 级载荷通道）
            "bench.bus-binary" => {
                let topic =
                    arg_str(&args, "topic").unwrap_or_else(|| "bench.probe.bin".to_string());
                let bytes = arg_usize(&args, "bytes");
                let count = arg_usize(&args, "count").max(1);
                let payload = vec![b'a'; bytes];
                let t0 = std::time::Instant::now();
                for _ in 0..count {
                    host.bus_publish_binary(&topic, &payload)
                        .map_err(|e| anyhow::anyhow!("bus_publish_binary: {e}"))?;
                }
                Ok(
                    serde_json::json!({ "topic": topic, "bytes": bytes, "count": count, "guestNanos": t0.elapsed().as_nanos() as u64 }),
                )
            }

            // D4 收讫快照（订阅侧实例自报；总线不投递给发送者自身，
            // 故必须由**另一个实例**查询才有数）
            "bench.bus-recv" => {
                let (json_count, json_bytes) = *RECV_JSON.lock().unwrap();
                let (bin_count, bin_bytes) = *RECV_BIN.lock().unwrap();
                Ok(serde_json::json!({
                    "jsonCount": json_count,
                    "jsonBytes": json_bytes,
                    "binCount": bin_count,
                    "binBytes": bin_bytes,
                }))
            }

            "bench.bus-reset" => {
                *RECV_JSON.lock().unwrap() = (0, 0);
                *RECV_BIN.lock().unwrap() = (0, 0);
                Ok(serde_json::json!({ "ok": true }))
            }

            // D5 开启互调服务角色（对端实例调用）
            "bench.api-serve" => {
                let enable = args.get("enable").and_then(|v| v.as_bool()).unwrap_or(true);
                SERVING.store(enable, Ordering::SeqCst);
                Ok(serde_json::json!({ "serving": enable }))
            }

            // D6 互调调用（JSON-RPC over host-api-call，guest 侧阻塞等待回复）×repeats。
            // 这条路径是「wasm → 宿主互调门 + 回复静态订阅 + oneshot → 对方 wasm」，
            // 阻塞语义与并发模型议题（见 .scratch/2026-09-26-plugin-concurrency-model）相关。
            "bench.api-call" => {
                let bytes = arg_usize(&args, "bytes");
                let repeats = arg_usize(&args, "repeats").max(1);
                let t0 = std::time::Instant::now();
                let mut last = serde_json::Value::Null;
                for _ in 0..repeats {
                    let request = api_call::build_request(
                        if bytes == 0 { "churn" } else { "echo" },
                        serde_json::json!({ "bytes": bytes }),
                    );
                    let topic = format!(
                        "{API_TOPIC_PREFIX}{}",
                        if bytes == 0 { API_CHURN } else { API_ECHO }
                    );
                    let reply =
                        api_call::api_call(&topic, &request, api_call::DEFAULT_CALL_TIMEOUT_MS)
                            .map_err(|e| anyhow::anyhow!("api_call: {e:?}"))?;
                    last = api_call::decode_reply(&reply)
                        .map_err(|e| anyhow::anyhow!("decode_reply: {e:?}"))?;
                }
                Ok(serde_json::json!({
                    "bytes": bytes,
                    "repeats": repeats,
                    "resultBytes": serde_json::to_string(&last).map(|s| s.len()).unwrap_or(0),
                    "guestNanos": t0.elapsed().as_nanos() as u64,
                }))
            }

            // ==================== E · 流式 ====================

            // E1 终端输出流：spawn 产出确定字节数的命令 → 游标拉取环输出，
            // `forward=true` 时每块经二进制总线再转发给对端实例（模拟「消费 + 再分发」）。
            // 关键量：`calls`（跨边界次数）与 `bytes`（体量）之比即批量摊薄曲线。
            "bench.pty-stream" => {
                let bytes = arg_usize(&args, "bytes");
                let chunk = arg_usize(&args, "maxBytes").clamp(1, u32::MAX as usize) as u32;
                let forward = args
                    .get("forward")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let forward_topic =
                    arg_str(&args, "forwardTopic").unwrap_or_else(|| "bench.probe.bin".to_string());
                let ring_bytes = (bytes as u64 * 2).max(4 * 1024 * 1024);
                let config = PtySpawnConfig::new("/bin/sh")
                    .args(vec![
                        "-c".to_string(),
                        format!("head -c {bytes} /dev/zero | tr '\\0' a; sleep 30"),
                    ])
                    .ring_bytes(ring_bytes);
                let pty_id = host
                    .pty_spawn(&config.to_json())
                    .map_err(|e| anyhow::anyhow!("pty_spawn: {e}"))?;

                // 轮询确认产出端已把目标字节写进环（否则测到的是生产等待）
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
                let produced = loop {
                    match host.pty_ring_fetch(&pty_id, bytes as u64 - 1, 1) {
                        Ok(Some(_)) => break true,
                        Ok(None) => {}
                        Err(e) => {
                            let _ = host.pty_kill(&pty_id);
                            return Err(anyhow::anyhow!("pty_ring_fetch(probe): {e}"));
                        }
                    }
                    if std::time::Instant::now() > deadline {
                        break false;
                    }
                };
                if !produced {
                    let _ = host.pty_kill(&pty_id);
                    return Err(anyhow::anyhow!("producer did not fill {bytes} B in 20s"));
                }

                let t0 = std::time::Instant::now();
                let mut cursor = 0u64;
                let mut calls = 0usize;
                let mut read_bytes = 0usize;
                let mut truncated = false;
                while cursor < bytes as u64 {
                    match host.pty_ring_fetch(&pty_id, cursor, chunk) {
                        Ok(Some(fetched)) => {
                            calls += 1;
                            read_bytes += fetched.data.len();
                            truncated |= fetched.truncated;
                            if forward {
                                host.bus_publish_binary(&forward_topic, &fetched.data)
                                    .map_err(|e| anyhow::anyhow!("bus_publish_binary: {e}"))?;
                            }
                            if fetched.next_offset <= cursor {
                                break; // 防御：游标未推进即停（避免死循环）
                            }
                            cursor = fetched.next_offset;
                        }
                        Ok(None) => break,
                        Err(e) => {
                            let _ = host.pty_kill(&pty_id);
                            return Err(anyhow::anyhow!("pty_ring_fetch: {e}"));
                        }
                    }
                }
                let guest_nanos = t0.elapsed().as_nanos() as u64;
                host.pty_kill(&pty_id)
                    .map_err(|e| anyhow::anyhow!("pty_kill: {e}"))?;
                Ok(serde_json::json!({
                    "bytes": bytes,
                    "readBytes": read_bytes,
                    "maxBytes": chunk,
                    "calls": calls,
                    "forwarded": forward,
                    "truncated": truncated,
                    "guestNanos": guest_nanos,
                }))
            }

            // E2 非流式大响应（host-http 客户端域）。AI 供应商响应 / 资源下载的形态。
            // guest 无法消费流式响应（流事件只到前端），故此处只测非流式整包。
            "bench.http-fetch" => {
                let url =
                    arg_str(&args, "url").ok_or_else(|| anyhow::anyhow!("url is required"))?;
                let t0 = std::time::Instant::now();
                let response = host
                    .http_fetch(&serde_json::json!({ "method": "GET", "url": url }))
                    .map_err(|e| anyhow::anyhow!("http_fetch: {e}"))?;
                let body_bytes = response
                    .as_ref()
                    .and_then(|r| r.get("body"))
                    .and_then(|b| b.as_str())
                    .map(|s| s.len())
                    .unwrap_or(0);
                let status = response
                    .as_ref()
                    .and_then(|r| r.get("status"))
                    .and_then(|s| s.as_i64())
                    .unwrap_or(0);
                *LAST_HTTP_BODY.lock().unwrap() = (status as u64, body_bytes as u64);
                Ok(serde_json::json!({
                    "status": status,
                    "bodyBytes": body_bytes,
                    "guestNanos": t0.elapsed().as_nanos() as u64,
                }))
            }

            // ==================== F · 同步 / 异步形态 ====================

            // F1 异步进程执行：立即返 run-id，终态经 `on_process_done` 回调。
            // `outputPath` 必填：wasip3 的 `std::env::temp_dir()` 在无 TMPDIR 时 panic，
            // 路径由调用方（harness）给定。
            "bench.proc-async" => {
                let output_path = arg_str(&args, "outputPath")
                    .ok_or_else(|| anyhow::anyhow!("outputPath is required"))?;
                let request = serde_json::json!({
                    "command": arg_str(&args, "command").unwrap_or_else(|| "/bin/sh".to_string()),
                    "args": arg_str(&args, "script").map(|s| vec!["-c".to_string(), s]).unwrap_or_default(),
                    "output_path": output_path,
                });
                let t0 = std::time::Instant::now();
                let run_id = host
                    .process_run(&request.to_string())
                    .map_err(|e| anyhow::anyhow!("process_run: {e}"))?;
                Ok(
                    serde_json::json!({ "runId": run_id, "guestNanos": t0.elapsed().as_nanos() as u64 }),
                )
            }

            // F2 同步进程执行：阻塞至进程结束并捕获输出（同一命令的同步对照）
            "bench.proc-sync" => {
                let request = serde_json::json!({
                    "command": arg_str(&args, "command").unwrap_or_else(|| "/bin/sh".to_string()),
                    "args": arg_str(&args, "script").map(|s| vec!["-c".to_string(), s]).unwrap_or_default(),
                    "timeout_ms": 60_000u64,
                });
                let t0 = std::time::Instant::now();
                let result = host
                    .process_run_sync(&request.to_string())
                    .map_err(|e| anyhow::anyhow!("process_run_sync: {e}"))?;
                Ok(serde_json::json!({
                    "exitCode": result.exit_code,
                    "stdoutBytes": result.stdout.len(),
                    "stderrBytes": result.stderr.len(),
                    "timedOut": result.timed_out,
                    "guestNanos": t0.elapsed().as_nanos() as u64,
                }))
            }

            // F3 周期定时器：`intervalMs` 毫秒注册一条本插件命令面，
            // 回调次数由 TIMER_FIRED 计数（harness 轮询 `bench.counters` 取样）
            "bench.timer-fire" => {
                let interval_ms = arg_u64(&args, "intervalMs").max(1);
                host.timer_register(0, "bench.timer-tick")
                    .map_err(|e| anyhow::anyhow!("timer_register: {e}"))?;
                // 宿主 timer 以秒为粒度（WIT `interval-secs: u64`）：
                // 亚秒需求按秒向上取整，最小 1s
                let secs = ((interval_ms + 999) / 1000).max(1);
                host.timer_register(secs, "bench.timer-tick")
                    .map_err(|e| anyhow::anyhow!("timer_register: {e}"))?;
                Ok(serde_json::json!({ "intervalSecs": secs }))
            }

            // 定时器回调落点（由宿主按注册的 command 名回调本插件命令面）
            "bench.timer-tick" => {
                TIMER_FIRED.fetch_add(1, Ordering::SeqCst);
                Ok(serde_json::json!({ "ok": true }))
            }

            // ==================== 观测面 ====================
            "bench.counters" => {
                let (json_count, json_bytes) = *RECV_JSON.lock().unwrap();
                let (bin_count, bin_bytes) = *RECV_BIN.lock().unwrap();
                let (http_status, http_bytes) = *LAST_HTTP_BODY.lock().unwrap();
                Ok(serde_json::json!({
                    "apiServed": API_SERVED.load(Ordering::SeqCst),
                    "apiServedBytes": API_SERVED_BYTES.load(Ordering::SeqCst),
                    "processDone": PROCESS_DONE.load(Ordering::SeqCst),
                    "timerFired": TIMER_FIRED.load(Ordering::SeqCst),
                    "jsonCount": json_count,
                    "jsonBytes": json_bytes,
                    "binCount": bin_count,
                    "binBytes": bin_bytes,
                    "httpStatus": http_status,
                    "httpBytes": http_bytes,
                    "serving": SERVING.load(Ordering::SeqCst),
                }))
            }

            // 归零全部计数器（每场景独立取样用）
            "bench.reset" => {
                API_SERVED.store(0, Ordering::SeqCst);
                API_SERVED_BYTES.store(0, Ordering::SeqCst);
                PROCESS_DONE.store(0, Ordering::SeqCst);
                TIMER_FIRED.store(0, Ordering::SeqCst);
                *RECV_JSON.lock().unwrap() = (0, 0);
                *RECV_BIN.lock().unwrap() = (0, 0);
                Ok(serde_json::json!({ "ok": true }))
            }

            // 存活探针（harness 启动即调用，确认实例可用）
            "bench.ping" => Ok(serde_json::json!({ "pong": true, "id": Self::ID })),

            other => Err(anyhow::anyhow!("Unknown command: {other}")),
        }
    }

    /// 总线入口：互调请求分派（服务方）+ 收讫计数（订阅方）
    fn on_message(msg: &BusMessage) -> anyhow::Result<()> {
        // D · 互调服务：仅开启 SERVING 的实例应答（同一产物以两个属主实例化）
        if msg.topic.starts_with(API_TOPIC_PREFIX) {
            if !SERVING.load(Ordering::SeqCst) {
                return Ok(());
            }
            let request: api_call::RpcRequest = match serde_json::from_value(msg.payload.clone()) {
                Ok(r) => r,
                Err(e) => {
                    WasmHost.log_warn(&format!("bench: malformed api request: {e}"));
                    return Ok(());
                }
            };
            let result = match request.method.as_str() {
                "echo" => {
                    let bytes = request
                        .params
                        .as_ref()
                        .and_then(|p| p.get("bytes"))
                        .and_then(|b| b.as_u64())
                        .unwrap_or(0) as usize;
                    Ok(serde_json::json!({ "bytes": bytes, "blob": "a".repeat(bytes) }))
                }
                "churn" => {
                    let bytes = request
                        .params
                        .as_ref()
                        .and_then(|p| p.get("bytes"))
                        .and_then(|b| b.as_u64())
                        .unwrap_or(0) as usize;
                    let t0 = std::time::Instant::now();
                    let mut sink = String::with_capacity(bytes);
                    while sink.len() < bytes {
                        sink.push('a');
                    }
                    std::hint::black_box(&sink);
                    Ok(
                        serde_json::json!({ "bytes": sink.len(), "guestNanos": t0.elapsed().as_nanos() as u64 }),
                    )
                }
                other => Err((-32601, format!("method not found: {other}"))),
            };
            let reply = api_call::rpc_reply(&request.id, result);
            let reply_topic = format!("{REPLY_TOPIC_PREFIX}{}.{}", msg.sender, request.id);
            api_call::publish_reply(&reply_topic, &reply)
                .map_err(|e| anyhow::anyhow!("publish_reply: {e:?}"))?;
            API_SERVED.fetch_add(1, Ordering::SeqCst);
            API_SERVED_BYTES.fetch_add(
                serde_json::to_string(&reply).map(|s| s.len()).unwrap_or(0) as u64,
                Ordering::SeqCst,
            );
            return Ok(());
        }

        // 普通总线消息：累计收讫（JSON 偏好订阅面）
        let payload_bytes = serde_json::to_string(&msg.payload)
            .map(|s| s.len())
            .unwrap_or(0);
        let mut guard = RECV_JSON.lock().unwrap();
        guard.0 += 1;
        guard.1 += payload_bytes as u64;
        Ok(())
    }

    /// 二进制总线消息入口（`subscribe-binary` 偏好订阅面）
    fn on_message_binary(msg: &BusMessage) -> anyhow::Result<()> {
        let len = msg.payload_binary.as_ref().map(|b| b.len()).unwrap_or(0);
        let mut guard = RECV_BIN.lock().unwrap();
        guard.0 += 1;
        guard.1 += len as u64;
        Ok(())
    }

    /// 异步进程完成事件（host-process 事件模型）
    fn on_process_done(event: &bedcode_plugin_api::events::ProcessDoneEvent) -> anyhow::Result<()> {
        let _ = event;
        PROCESS_DONE.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

// ==================== 参数读取助手 ====================

fn arg_str(args: &serde_json::Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn arg_u64(args: &serde_json::Value, key: &str) -> u64 {
    args.get(key).and_then(|v| v.as_u64()).unwrap_or(0)
}

fn arg_usize(args: &serde_json::Value, key: &str) -> usize {
    arg_u64(args, key) as usize
}

bedcode_plugin_api::wasm_entry!(BenchPlugin);
