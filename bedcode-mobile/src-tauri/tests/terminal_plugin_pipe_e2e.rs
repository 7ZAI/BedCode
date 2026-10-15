//! 终端链路移动侧真产物闭环（**真插件 wasm + 真宿主窄转发 + 假桌面端点**）
//!
//! ## 为什么需要这个文件（守的是什么）
//!
//! 终端订阅协议（prewarm / subscribe / ack / ring_resync / 门控 / 窄转发）已随票 12
//! 从宿主整体下沉到移动 wasm 插件。宿主侧的 `terminal_session_e2e.rs` 用的是**夹具组件**
//! （只 import 域接口、不实现状态机），协议状态机在它身上不可见；而 `cross-end-tests`
//! 的单进程 rig 又装不下「两端宿主域实现」（同一 interface 两端各有一份，`inventory`
//! 是进程级注册，后装的必然 `defined twice`/让位——见 `crates/bedcode-host-kit` 的
//! `fallback` 语义），故移动插件的协议行为在本文件用**随包真产物 + 假桌面端点**钉住。
//!
//! 对端口径：`mock_plugin_ws` 是**真协议形状**夹具（帧名/键名逐字对齐桌面
//! `ws_terminal.rs`）——它只负责「像桌面那样应答」，移动侧全部代码都是真的
//! （真插件组件 + 真宿主 WS 客户端引擎 + 真页面窄转发表 + 真权限门）。
//!
//! 产物缺失**显性失败**（先跑 `pnpm run plugins:build`）。
//!
//! ## 行为契约
//!
//! | 契约 | 判据 | 场景 |
//! |---|---|---|
//! | P-001 | `prewarm` **只建连**：端点收到 auth 帧但**没有** subscribe 帧，页面通道零字节 | 防回归（走 subscribe 预热＝白传一遍整段重放） |
//! | P-002 | `subscribe` → 对端 `subscribed` → 重放字节经窄转发到达页面通道（逐字节一致） | 正例 |
//! | P-003 | 302 KiB 大流全量到达且**逐字节一致、无缺口无重复**（宿主窄转发零丢弃） | 连续性主判据 |
//! | P-004 | `ack-rendered`（≥64KiB 阈值）→ 对端收到 `ack` 帧且 offset 为已渲染字节数 | 背压回路 |
//! | P-005 | `ring_resync` → 计数基准重置后，后续字节仍全量到达（重锚不丢流） | 正例 |
//! | P-006 | `session_stopped` → 链路停止（`get-state.stopped=true`）且其后字节不再转发 | 终态 |

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bedcode_mobile_lib as _;
use bedcode_mobile_lib::plugin::guest_events::MobileGuestEvents;
use bedcode_mobile_lib::plugin::manager::mobile_core_config;
use bedcode_mobile_lib::plugin::terminal_stream::terminal_stream_gateway;
use bedcode_wasm_core::bus::{BusMessage, MessageDispatcher};
use bedcode_wasm_core::host_api::ports::{ConnectionEnginePort, PrimaryTarget};
use bedcode_wasm_core::host_api::WasmHostContext;
use bedcode_wasm_core::manager::runtime::{AbiPolicy, LoadedWasmPlugin};
use bedcode_wasm_core::test_support::{
    build_host_ctx_with, mock_plugin_ws, runtime_engine, setup_headless_runtime_with_config, MockPorts,
};
use bedcode_wasm_core::wasmtime::component::Component;
use tauri::ipc::{Channel, InvokeResponseBody};

/// 随包真产物（`pnpm run plugins:build` 落点；相对 crate 根）
const WASM_REL: &str = "resources/plugins/mobile/com.bedcode.terminal-session/bedcode_plugin_terminal_session.wasm";
/// 插件 id（与 manifest 逐字一致）
const PLUGIN_ID: &str = "com.bedcode.terminal-session";
/// 假桌面端点（真协议形状夹具）
const ENDPOINT: &str = mock_plugin_ws::ENDPOINT_TERMINAL;
/// 测试会话 id
const SESSION_ID: &str = "s-pipe-e2e";
/// 等待预算（真 wasm 组件编译 + 真实 WS 往返）
const WAIT: Duration = Duration::from_secs(20);

// ==================== 宿主替身 ====================

/// 主连接目标替身：指向假桌面端点
struct FixedTarget {
    address: String,
    port: u16,
}

#[async_trait::async_trait]
impl ConnectionEnginePort for FixedTarget {
    async fn primary_target(&self) -> bedcode_wasm_core::Result<Option<(PrimaryTarget, bool)>> {
        Ok(Some((
            PrimaryTarget {
                address: self.address.clone(),
                port: self.port,
            },
            true,
        )))
    }
}

/// 锁内投递器（测试线程与投递任务不得并发进同一 Store）
struct LockedDispatcher {
    plugin: Arc<tokio::sync::Mutex<LoadedWasmPlugin>>,
}

impl MessageDispatcher for LockedDispatcher {
    fn dispatch_to_wasm(&self, _plugin_id: &str, msg: &BusMessage) -> anyhow::Result<()> {
        let plugin = self.plugin.clone();
        let msg = msg.clone();
        bedcode_wasm_core::runtime_util::block_on_async(async move {
            let mut plugin = plugin.lock().await;
            if let Some(bytes) = &msg.payload_binary {
                plugin
                    .on_message_binary(&msg.topic, &msg.sender, bytes)
                    .map_err(anyhow::Error::from)
            } else {
                plugin
                    .on_message(&msg.topic, &msg.sender, &msg.payload)
                    .map_err(anyhow::Error::from)
            }
        })
    }

    fn is_activated(&self, _plugin_id: &str) -> bool {
        true
    }
}

/// 页面通道记录器（段2 出口的真落点）
#[derive(Default)]
struct PageSink {
    bytes: Mutex<Vec<u8>>,
}

impl PageSink {
    fn bytes(&self) -> Vec<u8> {
        self.bytes.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    fn channel(self: &Arc<Self>) -> Channel<InvokeResponseBody> {
        let sink = self.clone();
        Channel::new(move |body| {
            if let InvokeResponseBody::Raw(bytes) = body {
                let mut guard = sink.bytes.lock().unwrap_or_else(|p| p.into_inner());
                // 逐帧拼接 = 页面真实收到的字节流（帧边界不改变字节序）
                guard.extend_from_slice(&bytes);
            }
            Ok(())
        })
    }
}

struct World {
    _aot: std::path::PathBuf,
    plugin: Arc<tokio::sync::Mutex<LoadedWasmPlugin>>,
    #[allow(dead_code)]
    host_ctx: Arc<WasmHostContext>,
}

impl World {
    async fn invoke(&self, command: &str, args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let out = self
            .plugin
            .lock()
            .await
            .invoke_command(command, &args.to_string())
            .map_err(|e| anyhow::anyhow!("{command}: {e}"))?;
        serde_json::from_str(&out).map_err(|e| anyhow::anyhow!("{command} non-JSON: {e} ({out})"))
    }
}

async fn wait_until(what: &str, mut pred: impl FnMut() -> bool) {
    let deadline = Instant::now() + WAIT;
    loop {
        if pred() {
            return;
        }
        assert!(Instant::now() < deadline, "等待超时：{what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[test]
fn mobile_terminal_plugin_pipe_contracts() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    rt.block_on(async move {
        let mock = mock_plugin_ws::MockPluginWsServer::start().await;
        let ports = MockPorts::new()
            .with_token("pipe-e2e-token")
            .with_connection(Arc::new(FixedTarget {
                address: "127.0.0.1".to_string(),
                port: mock.addr().port(),
            }));
        let host_ctx = build_host_ctx_with(ports);
        host_ctx.grant_permissions(
            PLUGIN_ID,
            &[
                "auth".to_string(),
                "bus".to_string(),
                "network:http".to_string(),
                "session:read".to_string(),
                "storage".to_string(),
                "terminal:output".to_string(),
                "ws:client".to_string(),
            ],
        );

        let aot = std::env::temp_dir().join(format!("bedcode-pipe-e2e-aot-{}", std::process::id()));
        let runtime =
            setup_headless_runtime_with_config(Some(aot.clone()), AbiPolicy { form: None }, mobile_core_config(None))
                .expect("create headless runtime");
        runtime.set_guest_events(Arc::new(MobileGuestEvents));
        let component = Component::from_file(runtime_engine(&runtime), WASM_REL)
            .expect("移动端插件产物缺失或不可编译：先跑 `pnpm run plugins:build`（workdir bedcode-mobile）");
        let plugin = runtime
            .instantiate_component(&component, PLUGIN_ID, host_ctx.clone(), &[], None)
            .expect("instantiate terminal plugin");
        let plugin = Arc::new(tokio::sync::Mutex::new(plugin));
        host_ctx
            .message_bus
            .set_dispatcher(Arc::new(LockedDispatcher { plugin: plugin.clone() }))
            .await;
        plugin.lock().await.activate().expect("activate plugin");
        let world = World {
            _aot: aot,
            plugin,
            host_ctx,
        };

        let sink = Arc::new(PageSink::default());
        terminal_stream_gateway().page_subscribe(SESSION_ID, sink.channel());

        // ==================== P-001 prewarm 只建连（不发 subscribe） ====================
        world
            .invoke(
                "terminal-session.prewarm",
                serde_json::json!({ "sessionId": SESSION_ID }),
            )
            .await
            .expect("prewarm 可调用");
        mock.wait_for_text(ENDPOINT, |f| f["type"] == "auth", WAIT).await;
        tokio::time::sleep(Duration::from_millis(400)).await;
        {
            let frames = mock.received_text(ENDPOINT).await;
            assert!(
                !frames.iter().any(|f| f["type"] == "subscribe"),
                "P-001 prewarm 不得发 subscribe 帧（走了 subscribe 就是白传一遍整段重放）：{frames:?}"
            );
            assert!(sink.bytes().is_empty(), "P-001 prewarm 期间页面通道必须零字节");
        }

        // ==================== P-002 subscribe → 重放到达页面通道 ====================
        world
            .invoke(
                "terminal-session.subscribe",
                serde_json::json!({ "sessionId": SESSION_ID }),
            )
            .await
            .expect("subscribe 可调用");
        mock.wait_for_text(ENDPOINT, |f| f["type"] == "subscribe", WAIT).await;
        let replay_a = b"HEADER-A\r\n".repeat(4);
        mock.send_text(ENDPOINT, &mock_plugin_ws::subscribed_frame(SESSION_ID, "live"))
            .await;
        mock.send_binary(ENDPOINT, &replay_a).await;
        wait_until("P-002 重放字节到达页面通道", || {
            sink.bytes().len() >= replay_a.len()
        })
        .await;
        assert_eq!(
            sink.bytes(),
            replay_a,
            "P-002 重放必须逐字节一致（顺序/内容不得被宿主窄转发改写）"
        );

        // ==================== P-003 大流连续性：逐字节一致、无缺口无重复 ====================
        // 302 KiB 载荷：以**严格递增的 u32 大端计数器**编码——任何丢失/重复/乱序都能
        // 精确指出位置与方向（周期序列会掩盖小位移，不用）
        let words: u32 = 302 * 1024 / 4;
        let big: Vec<u8> = (0..words).flat_map(|i| i.to_be_bytes()).collect();
        for chunk in big.chunks(16 * 1024) {
            mock.send_binary(ENDPOINT, chunk).await;
        }
        let expect_len = replay_a.len() + big.len();
        wait_until("P-003 大流全量到达", || sink.bytes().len() >= expect_len).await;
        let got = sink.bytes();
        assert_eq!(got.len(), expect_len, "P-003 字节数必须精确相等（多/少都是链路缺陷）");
        let stream = &got[replay_a.len()..];
        // 逐块（16 KiB）诊断：每个收到块的**起始字值** = 原块索引 ⇒ 直接暴露
        // 丢块 / 重复块 / 块序错乱
        let chunk_words = 16 * 1024 / 4;
        let received_chunks: Vec<u32> = stream
            .chunks(16 * 1024)
            .map(|c| u32::from_be_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        // 每块起始字的索引 = 块序号 × 块内字数（块内是严格递增的 u32 计数器）
        let expected_chunks: Vec<u32> = (0..words.div_ceil(chunk_words)).map(|i| i * chunk_words).collect();
        assert_eq!(
            received_chunks, expected_chunks,
            "P-003 大流块序/块完整性（元素 = 块的起始字索引）；共 {} 字",
            words
        );
        let stats = terminal_stream_gateway().stats();
        assert_eq!(
            stats.dropped_no_channel, 0,
            "P-003 页面已订阅期间不得有无通道丢弃：{stats:?}"
        );
        assert_eq!(stats.dropped_send_failed, 0, "P-003 通道发送不得失败：{stats:?}");

        // ==================== P-004 ack 回发（阈值节流） ====================
        let rendered = got.len() as u64;
        world
            .invoke(
                "terminal-session.ack-rendered",
                serde_json::json!({ "sessionId": SESSION_ID, "offset": rendered }),
            )
            .await
            .expect("ack-rendered 可调用");
        mock.wait_for_text(ENDPOINT, |f| f["type"] == "ack", WAIT).await;
        {
            let frames = mock.received_text(ENDPOINT).await;
            let ack = frames.iter().find(|f| f["type"] == "ack").expect("ack 帧必须到达对端");
            assert_eq!(
                ack["offset"].as_u64(),
                Some(rendered),
                "P-004 ack.offset 必须是前端已渲染字节数（本地计数口径）"
            );
        }

        // ==================== P-005 ring_resync 重锚后仍全量收流 ====================
        mock.send_text(ENDPOINT, &mock_plugin_ws::ring_resync_frame(4096)).await;
        let after_resync = b"AFTER-RESYNC\r\n".to_vec();
        mock.send_binary(ENDPOINT, &after_resync).await;
        let expect_after = expect_len + after_resync.len();
        wait_until("P-005 重锚后续流到达", || sink.bytes().len() >= expect_after).await;
        assert_eq!(
            &sink.bytes()[expect_len..expect_after],
            &after_resync[..],
            "P-005 重锚只是基准归零，不得丢后续字节"
        );

        // ==================== P-006 session_stopped → 停止且不再转发 ====================
        mock.send_text(
            ENDPOINT,
            &mock_plugin_ws::session_stopped_frame(SESSION_ID, "stopped", Some(0)),
        )
        .await;
        let deadline = Instant::now() + WAIT;
        loop {
            let state = world
                .invoke(
                    "terminal-session.get-state",
                    serde_json::json!({ "sessionId": SESSION_ID }),
                )
                .await
                .expect("get-state 可调用");
            if state["stopped"] == true {
                break;
            }
            assert!(Instant::now() < deadline, "P-006 等待链路 stopped 超时：{state}");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let len_at_stop = sink.bytes().len();
        mock.send_binary(ENDPOINT, b"AFTER-STOP-MUST-DROP").await;
        tokio::time::sleep(Duration::from_millis(400)).await;
        assert_eq!(
            sink.bytes().len(),
            len_at_stop,
            "P-006 停止后到达的字节必须被丢弃（状态机漏关 = 终端冻结后仍看似有输出）"
        );

        terminal_stream_gateway().page_unsubscribe(SESSION_ID);
        mock.shutdown().await;
    });
}
