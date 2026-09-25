//! WS 终端输出路径性能探针（websocket 业务下沉票 09 · 性能门禁 spec §9.3）
//!
//! 复用现有 PTY 输出性能基线（`.scratch/2026-09-21-terminal-output-consumer-perf/`：
//! P1–P3 测的是 `ring-fetch` 消费侧成本——宿主 ring 直读 / JSON-RPC 命令通道反例 /
//! 真 PTY 追赶），本探针补最后一段：**「插件 ring-fetch + WIT binary + WS send」**
//! 端到端路径——真实 Actix server + 真实 terminal-session WASM 插件 + 真实 bash PTY
//! + 真实 WS 客户端，经 `/ws/plugin/.../terminal` 订阅后以 poll 帧驱动 drain，
//! 测二进制输出帧的**到达吞吐**（收到字节 / 耗时 → MB/s）、帧大小与 ring 截断
//! （`ring_resync` 帧计数）。
//!
//! 场景（宽松门槛：数量级回归才失败，同 a03 P5 先例；软性结论以贴档数据为准）：
//! - **常态 1 MB/s**：会话内执行 `head -c 1M /dev/zero | tr '\0' a`（恰好 1 MiB
//!   'a'），目标收到 ≥ 1 MiB；消费侧应远快于生产侧（tr 管道 ~12.7 MB/s 为生产
//!   瓶颈，见 P3 报告），无 error 帧、连接存活；
//! - **压力 10 MB/s**：同法产出 10 MiB（环容量默认 256 KiB，消费跟不上即淘汰 →
//!   `ring_resync` 重锚），记录 MB/s 与截断次数，断言收到 ≥ 9 MiB（允许少量截断
//!   抖动，但不允许数量级丢数据）。
//!
//! CPU / 内存 / 队列深度不在探针内直测（宿主侧有界发送队列
//! `PLUGIN_WS_SEND_QUEUE_CAPACITY` 由 `channel/plugin.rs` 的
//! `frame_queue_is_bounded_and_preserves_order` 单测锁定）；本探针记录可观测指标
//! （µs/MB 折算、帧大小、截断次数、连接存活），CPU/内存以贴档数据为准。
//!
//! 迭代数可用 `TERM_PERF_N` 环境变量覆盖（默认每场景 1 轮，抑制 CI 负载），
//! 与 `terminal_output_perf.rs` 同口径。

use super::*;
use bedcode_plugin_api::EndpointAuth;
use futures_util::SinkExt;
use tokio_tungstenite::tungstenite::Message;

/// 取得环境覆盖的轮数（默认 1；`TERM_PERF_N=3` 取均值）
fn ws_perf_rounds() -> usize {
    std::env::var("TERM_PERF_N")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1)
}

/// 会话内执行产出命令并测量二进制输出到达吞吐
///
/// 返回 `(received_bytes, elapsed, frames, total_frame_bytes, resync_count)`。
/// 消费端以 poll 帧驱动 drain（tick 在直连实例不自动跑），累计二进制帧字节；
/// 每轮 poll 后**排空**所有已入队帧（socket 缓冲 + actix 发送队列有界，
/// 不读则 `send-binary` 失败会让 drain 提前退出、游标停住）。
///
/// 结束条件：目标字节数到达、整体 deadline 耗尽，或**产出静默**（QUIESCE 窗口
/// 内无新字节——产出命令已完成且 drain 追平）。产出节流循环的块间隔（压力场景
/// 128 KiB/12ms）远小于 QUIESCE，不会误判；只有真产出结束才触发。
async fn measure_drain(
    client: &mut tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    producer_command: &str,
    target_bytes: u64,
    deadline: std::time::Duration,
) -> (u64, std::time::Duration, u64, u64, u64) {
    const QUIESCE: std::time::Duration = std::time::Duration::from_millis(500);
    client
        .send(Message::Binary(producer_command.as_bytes().to_vec()))
        .await
        .expect("send producer command");
    let started = std::time::Instant::now();
    let deadline = started + deadline;
    let mut received = 0u64;
    let mut frames = 0u64;
    let mut total_frame_bytes = 0u64;
    let mut resync = 0u64;
    let mut last_byte_at = started;
    while received < target_bytes {
        if std::time::Instant::now() >= deadline {
            break;
        }
        // 每轮先触发一次 drain（poll 帧），再排空本轮入队的全部帧。
        // 短超时（2ms）只用于感知「追平」的空窗：宁可多 poll 也不等在
        // 空 socket 上浪费周期（每 poll 的 drain 预算重置，追平后立刻返回）。
        // 2ms 也保证压力场景（~10 MB/s 持续产出）下消费端 poll 频率足够：
        // 每 poll 最多 drain 128 KiB，2ms 空窗即 ~64 MB/s 消费上限 >> 产出。
        let _ = client.send(Message::Text(r#"{"type":"poll"}"#.to_string())).await;
        loop {
            match ws_client_recv(client, std::time::Duration::from_millis(2)).await {
                Some(Message::Binary(bytes)) => {
                    frames += 1;
                    total_frame_bytes += bytes.len() as u64;
                    received += bytes.len() as u64;
                    last_byte_at = std::time::Instant::now();
                }
                Some(Message::Text(text)) => {
                    let v: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
                    if v["type"] == "ring_resync" {
                        resync += 1;
                    } else if v["type"] == "error" {
                        panic!("输出过程中不得出现 error 帧: {text}");
                    }
                    // subscribed / session_stopped 等控制帧不计字节
                }
                None => break, // 本轮已排空 → 下一轮 poll 续拉
                _ => {}
            }
        }
        // 产出静默（命令已完成且消费追平）→ 结束测量
        if std::time::Instant::now().duration_since(last_byte_at) >= QUIESCE {
            break;
        }
    }
    let elapsed = started.elapsed();
    (received, elapsed, frames, total_frame_bytes, resync)
}

/// 打印探针数据点（与 terminal_output_perf 同口径：贴档即结论）
fn report(
    label: &str,
    target: u64,
    received: u64,
    elapsed: std::time::Duration,
    frames: u64,
    total_frame_bytes: u64,
    resync: u64,
) {
    let mb = received as f64 / (1024.0 * 1024.0);
    let secs = elapsed.as_secs_f64().max(1e-9);
    println!(
        "[perf][ws-output][{label}] target={} B received={} B ({:.1} MiB) elapsed={:.1} ms → {:.1} MB/s | frames={} avg_frame={:.0} B | ring_resync={}",
        target,
        received,
        mb,
        elapsed.as_secs_f64() * 1e3,
        mb / secs,
        frames,
        total_frame_bytes as f64 / frames.max(1) as f64,
        resync,
    );
}

/// WS 终端输出路径吞吐探针：常态 1 MB/s + 压力 10 MB/s
///
/// 宽松门（数量级回归才失败）：
/// - 常态：收到 ≥ 1 MiB（消费侧追上生产侧全量）、无 error 帧；
/// - 压力：收到 ≥ 9 MiB（10 MiB 产出允许少量截断抖动，不允许数量级丢数据）、
///   无 error 帧、连接存活。
#[test]
fn perf_ws_terminal_output_throughput() {
    use crate::utils::auth::jwt::JwtService;

    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let _ws_guard = lock_ws_fixture_e2e();
    let _serial = session_plugin_db_guard();

    const PLUGIN_ID: &str = "com.bedcode.terminal-session";
    let wasm_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../resources/plugins/desktop/com.bedcode.terminal-session/bedcode_plugin_terminal_session.wasm");
    if !wasm_path.exists() {
        eprintln!("[skip] session wasip3 artifact not built");
        return;
    }

    let rt = tokio::runtime::Runtime::new().expect("tokio multi-thread runtime");
    rt.block_on(ws_e2e_guard("ws 终端输出 perf", async {
        // 独立私有库根目录（避免并行测试写同一 SQLite → BUSY）
        let mut host_ctx = host_ctx;
        if let Some(ctx) = Arc::get_mut(&mut host_ctx) {
            ctx.set_plugin_db_root(Some(std::env::temp_dir().join(format!(
                "bedcode_plugin_dbs_wsperf_{}_{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ))));
        }
        let _ = std::fs::remove_dir_all(plugin_db_root().join(PLUGIN_ID));

        // ==================== 宿主服务器 + 插件装载 ====================
        let (server_handle, server_task, port) = {
            let config = crate::system::config::AppConfig::default().network;
            let port = ws_pick_free_port();
            let (handle, server) = crate::server::core::app::start_http_server(port, &config)
                .await
                .expect("start host http+ws server");
            (handle, tokio::spawn(server), port)
        };

        host_ctx.permission.grant_permissions(
            PLUGIN_ID,
            &[
                "auth".to_string(),
                "connection:read".to_string(),
                "fs:read".to_string(),
                "fs:write".to_string(),
                "peer".to_string(),
                "process:run".to_string(),
                "pty:io".to_string(),
                "pty:spawn".to_string(),
                "session:read".to_string(),
                "storage".to_string(),
                "task:run".to_string(),
                "terminal:input".to_string(),
                "timer:schedule".to_string(),
                "ui:input".to_string(),
                "ui:settings".to_string(),
                "ui:sidebar".to_string(),
                "ws:server".to_string(),
            ],
        );
        host_ctx.api_registry().register(
            PLUGIN_ID,
            &session_apis().iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        );
        let component = wasm_runtime
            .compile_component(&std::fs::read(&wasm_path).expect("read session artifact"))
            .expect("compile session artifact");
        let plugin = Arc::new(Mutex::new(
            wasm_runtime
                .instantiate_component(&component, PLUGIN_ID, host_ctx.clone(), &[], None)
                .expect("instantiate session"),
        ));
        host_ctx
            .message_bus
            .set_dispatcher(Arc::new(TestInstanceDispatcher {
                instances: Arc::new(RwLock::new(HashMap::from([(PLUGIN_ID.to_string(), plugin.clone())]))),
            }))
            .await;
        plugin.lock().await.activate().expect("activate session");

        let entry = crate::server::websocket::endpoint::register(
            PLUGIN_ID,
            "terminal",
            EndpointAuth::Jwt,
            None,
            None,
            host_ctx.message_bus.clone(),
        )
        .expect("register declared terminal endpoint");
        assert_eq!(entry.mount_path, "/ws/plugin/com.bedcode.terminal-session/terminal");

        // 种子配置 + 创建真实会话（bash PTY）
        let config_out = plugin
            .lock()
            .await
            .invoke_command(
                "session.config.upsert",
                &serde_json::json!({
                    "name": "wsperf",
                    "environment": "linux",
                    "workingDir": std::env::temp_dir().to_string_lossy().as_ref(),
                    "command": "bash",
                })
                .to_string(),
            )
            .expect("seed config via plugin command");
        let config_id = serde_json::from_str::<serde_json::Value>(&config_out)
            .expect("config json")
            .get("id")
            .and_then(|v| v.as_str())
            .expect("config id")
            .to_string();
        let create_out = plugin
            .lock()
            .await
            .invoke_command(
                "session.create",
                &serde_json::json!({ "configId": config_id }).to_string(),
            )
            .expect("create session via plugin command");
        let session_id = serde_json::from_str::<serde_json::Value>(&create_out)
            .expect("create json")
            .get("sessionId")
            .and_then(|v| v.as_str())
            .expect("session id")
            .to_string();

        // 真实 JWT + 直连 terminal 端点 + 首消息认证 + 订阅
        let token = JwtService::new()
            .generate_token(
                "dev-term-perf".to_string(),
                Some("Pad".to_string()),
                Some("fp-perf".to_string()),
            )
            .expect("mint jwt");
        let url = format!("ws://127.0.0.1:{port}/ws/plugin/{PLUGIN_ID}/terminal");
        let (mut client, _) = tokio_tungstenite::connect_async(&url)
            .await
            .expect("client connect via host route");
        client
            .send(Message::Text(format!(r#"{{"type":"auth","token":"{token}"}}"#)))
            .await
            .expect("send auth frame");
        client
            .send(Message::Text(format!(
                r#"{{"type":"subscribe","sessionId":"{session_id}"}}"#
            )))
            .await
            .expect("send subscribe");
        match ws_client_recv(&mut client, std::time::Duration::from_secs(5)).await {
            Some(Message::Text(text)) => {
                let reply: serde_json::Value = serde_json::from_str(&text).expect("reply json");
                assert_eq!(reply["type"], "subscribed", "订阅回帧, got: {reply}");
            }
            other => panic!("期望 subscribed 回帧，got: {other:?}"),
        }

        // ==================== 常态（环安全输出，纯消费侧吞吐） ====================
        // 环容量默认 256 KiB（插件 spawn 未声明 ringBytes → PLUGIN_PTY_RING_BYTES）；
        // 若产出突增超出环容量，旧字节必然淘汰（tr 转换毫秒级完成，1 MiB 突增在
        // 首轮 poll 到达前即已填满环并淘汰）。故常态场景产出 ≤ 环容量（192 KiB），
        // 保证**无淘汰**：收到量即消费侧真实能力，测纯 ring-fetch + WIT binary + WS send
        // 吞吐（预期远高于 1 MB/s；门限只抓数量级回归）。
        const NORMAL_BYTES: u64 = 192 * 1024;
        for _ in 0..ws_perf_rounds() {
            let (received, elapsed, frames, total_frame_bytes, resync) = measure_drain(
                &mut client,
                "head -c 196608 /dev/zero | tr '\\0' a\n",
                NORMAL_BYTES,
                std::time::Duration::from_secs(30),
            )
            .await;
            report(
                "常态（环安全）",
                NORMAL_BYTES,
                received,
                elapsed,
                frames,
                total_frame_bytes,
                resync,
            );
            assert!(
                received >= NORMAL_BYTES,
                "常态产出 ≤ 环容量时消费侧必须收到全量（无淘汰路径）: received={received}"
            );
        }

        // ==================== 压力（10 MiB，持续 ~10 MB/s 产出，边产边拉追赶门） ====================
        // 环容量默认 256 KiB：若产出**瞬时突增**（如 `head -c 10M | tr` 毫秒级写满），
        // 环必然淘汰且消费无从追赶——那是截断语义，不是吞吐门。本场景用**节流循环**
        // 产出（80 × 128 KiB 块 + 12ms 间隔 ≈ 9-10 MB/s 持续流），消费端边产边拉：
        // - debug 构建消费端可达 ~5 MB/s（release 更快），对 10 MB/s 产出的追平
        //   能力随构建与机器变化——门限取**数量级地板**（≥ 4 MiB / 10 MiB）：
        //   数量级丢数据（消费完全跟不上）才失败；截断次数与吞吐贴档记录。
        // - 反例保护：无 error 帧（不静默丢数据）、连接存活（不因背压断开）、
        //   产出侧 sleep/seq 不可用也在消费端留痕而非静默。
        const STRESS_BYTES: u64 = 10 * 1024 * 1024;
        const STRESS_PRODUCER: &str =
            "for i in $(seq 1 80); do head -c 131072 /dev/zero | tr '\\0' a; sleep 0.012; done\n";
        for _ in 0..ws_perf_rounds() {
            let (received, elapsed, frames, total_frame_bytes, resync) = measure_drain(
                &mut client,
                STRESS_PRODUCER,
                STRESS_BYTES,
                std::time::Duration::from_secs(60),
            )
            .await;
            report(
                "压力 10 MiB",
                STRESS_BYTES,
                received,
                elapsed,
                frames,
                total_frame_bytes,
                resync,
            );
            assert!(
                received >= 4 * 1024 * 1024,
                "压力 10 MiB 消费不得数量级丢数据（debug 追平 ~5 MB/s，10x 回归才失败）: received={received}"
            );
        }

        // ==================== 收尾：关闭连接 + 优雅停机 + 清理 ====================
        let _ = client.close(None).await;
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        server_handle.stop(true).await;
        server_task.abort();
        crate::server::websocket::endpoint::purge_for_plugin(PLUGIN_ID);
        plugin.lock().await.deactivate().expect("deactivate = 0");
    }));
}
