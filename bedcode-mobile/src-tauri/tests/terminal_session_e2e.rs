//! 移动 host-terminal-stream / host-connection / jwt-auth 全链路
//! （票 12 · ABI v15；票 05 迁移动宿主集成测试）


use std::collections::HashSet;
use std::sync::Arc;

use bedcode_wasm_core::manager::runtime::{LoadedComponentPlugin, WasmRuntime};
use bedcode_wasm_core::test_support::{build_test_component, test_engine, TEST_PLUGIN_ID};
use wasmtime::component::Component;

use bedcode_plugin_api_mobile::permission::{PERMISSION_BUS, PERMISSION_TERMINAL_OUTPUT, PERMISSION_WS_CLIENT};

use bedcode_wasm_core::test_support::{build_host_ctx_with, MockPorts};

/// 捕获型投递器：记录总线消息，不进 guest
///
/// 测试线程直调 `exports.call_invoke` **绕过** manager 的单插件锁（ADR 0029
/// 的实例门在 manager 层），若用真实 dispatcher（也调 guest 回调）会与测试
/// 线程并发进同一 Store——wasmtime Store 非 Sync，并发即 UB。捕获型投递器
/// 让「宿主 → 总线」段真实投递、断言在测试线程侧完成，不触碰 Store。
struct CapturingDispatcher {
    captured: Arc<std::sync::Mutex<Vec<bedcode_server_base::wire::BusMessage>>>,
}

impl bedcode_wasm_core::bus::MessageDispatcher for CapturingDispatcher {
    fn dispatch_to_wasm(
        &self,
        _plugin_id: &str,
        msg: &bedcode_server_base::wire::BusMessage,
    ) -> anyhow::Result<()> {
        self.captured.lock().unwrap().push(msg.clone());
        Ok(())
    }

    fn is_activated(&self, _plugin_id: &str) -> bool {
        true
    }
}

/// host-terminal-stream / host-connection / jwt-auth 全链路（票 12 · ABI v15，
/// 真实 WASM 组件闭环）：guest 经 bindgen import 走终端协议客户端的宿主面——
/// connect 带 `jwt-auth`（宿主代发首消息认证帧，token 不落插件）→ subscribe
/// 帧到 mock terminal 端点 → 回 subscribed（下行 text 帧经 ws:message 信封
/// 回流 guest 观测）→ `ts-forward` 驱动 forward-output（无头无登记 Channel →
/// 宿主显性 Err = fail-visible）→ ack 帧上行 → close 命中。权限门
/// （terminal:output）与 primary-target fail-visible 在闭环内被真实驱动。
/// 终端协议状态机（订阅门控/ack 节流/resync）的 native 单测在插件 crate
/// （bedcode_plugin_terminal_session），本用例不重复

/// 移动运行时装配（与生产 `PluginManager::init_wasm_runtime_with` 同款）：
/// 创建 runtime 后**注入 guest 事件派发器**——`events` 导出是宿主契约面，内核缺省
/// `UnwiredGuestEvents` 派发即 fail-visible（不注入时 `on_bus_message` 必 Err）。
fn mobile_runtime(aot_cache_dir: Option<std::path::PathBuf>) -> WasmRuntime {
    let runtime = WasmRuntime::new(aot_cache_dir).expect("create wasm runtime");
    runtime.set_guest_events(std::sync::Arc::new(
        bedcode_mobile_lib::plugin::guest_events::MobileGuestEvents,
    ));
    runtime
}

#[test]
fn terminal_session_domain_full_loop_with_real_component() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async move {
        // mock 桌面插件端点（terminal 端点 = 真协议形状夹具；session-control
        // 端点在本用例闲置）
        let mock = bedcode_wasm_core::test_support::mock_plugin_ws::MockPluginWsServer::start().await;
        // jwt-auth 帧的 token 经端口投影（批次 2：进程级静态退役，token
        // 注入到 MockPorts——测试结束随 ctx 释放，无跨用例竞争）
        let host_ctx = build_host_ctx_with(MockPorts::new().with_token("t12-loop-token"));
        let captured: Arc<std::sync::Mutex<Vec<bedcode_server_base::wire::BusMessage>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        host_ctx
            .message_bus
            .set_dispatcher(Arc::new(CapturingDispatcher { captured: Arc::clone(&captured) }))
            .await;

        let tmp = tempfile::tempdir().expect("tempdir");
        let runtime = mobile_runtime(Some(tmp.path().join("aot")));
        let component =
            Component::from_binary(runtime.engine(), &build_test_component(&["terminal-session"]))
                .expect("compile terminal-session component");
        let mut plugin = runtime
            .instantiate_component(
                &component,
                TEST_PLUGIN_ID,
                host_ctx,
                // 终端协议客户端三权限位：ws:client（连接面）+ bus（事件与帧
                // 走消息总线属主 topic）+ terminal:output（输出窄转发出口）
                HashSet::from([
                    PERMISSION_WS_CLIENT.to_string(),
                    bedcode_plugin_api_mobile::permission::PERMISSION_BUS.to_string(),
                    bedcode_plugin_api_mobile::permission::PERMISSION_TERMINAL_OUTPUT.to_string(),
                ]),
            )
            .expect("instantiate terminal-session component");

        // activate（guest 订阅 ws:* 四 topic + ws:message 二进制）→
        // connect(jwt-auth) + subscribe 帧一次到位
        let handle = {
            plugin.activate().expect("activate");
            let args = serde_json::json!({
                "op": "ts-connect",
                "url": mock.url(bedcode_wasm_core::test_support::mock_plugin_ws::ENDPOINT_TERMINAL),
                "sessionId": "s-t12",
            })
            .to_string();
            let out = plugin.invoke_command("terminal-op", &args).expect("call invoke");
            let v: serde_json::Value = serde_json::from_str(&out).expect("invoke json");
            assert!(v["ok"].as_bool().unwrap(), "ts-connect failed: {out}");
            v["handle"].as_str().expect("handle").to_string()
        };

        // 对端侧：首帧 = 宿主代发的 auth 帧（token 从宿主认证状态取——
        // guest 全程无 token 可见），随后是 subscribe 帧
        mock.wait_for_text(
            bedcode_wasm_core::test_support::mock_plugin_ws::ENDPOINT_TERMINAL,
            |f| f["type"] == "subscribe",
            std::time::Duration::from_secs(3),
        )
        .await;
        {
            let texts = mock.received_text(bedcode_wasm_core::test_support::mock_plugin_ws::ENDPOINT_TERMINAL).await;
            assert_eq!(texts[0]["type"], "auth", "首帧必为宿主代发的 auth 帧");
            let token = texts[0]["token"].as_str().expect("auth token");
            assert_eq!(token, "t12-loop-token", "代发 token 与宿主认证状态同源");
            assert_eq!(texts[1]["type"], "subscribe", "次帧为 subscribe（协议编排在 guest）");
            assert_eq!(texts[1]["sessionId"], "s-t12");
            assert_eq!(texts[1]["mode"], "live", "移动端固定 live 模式");
        }

        // 下行：mock 回 subscribed（text 控制帧）→ 帧信封经 ws:message 回流
        mock.send_raw_text(
            bedcode_wasm_core::test_support::mock_plugin_ws::ENDPOINT_TERMINAL,
            &bedcode_wasm_core::test_support::mock_plugin_ws::subscribed_frame("s-t12", "live").to_string(),
        )
        .await;
        {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            loop {
                let got = captured
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|m| {
                        m.topic.ends_with(":ws:message")
                            && m.payload_binary
                                .as_deref()
                                .map(|b| b.windows(10).any(|w| w == b"subscribed"))
                                .unwrap_or(false)
                    });
                if got {
                    break;
                }
                if std::time::Instant::now() >= deadline {
                    let topics: Vec<String> = captured
                        .lock()
                        .unwrap()
                        .iter()
                        .map(|m| format!("{} (bin={})", m.topic, m.payload_binary.is_some()))
                        .collect();
                    panic!(
                        "timed out waiting for subscribed frame on ws:message; captured topics: {topics:?}"
                    );
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        }

        // forward-output：无头测试无登记 Channel → 宿主显性 Err（fail-visible
        // 形态①，禁「成功但没人收到」）；错误文本回读 guest 观测记录断言
        {
            let args = serde_json::json!({
                "op": "ts-forward",
                "sessionId": "s-t12",
                "data": [0x1b, 0x5b, 0x33, 0x31, 0x6d], // ANSI 红色转义序列（任意裸字节）
            })
            .to_string();
            plugin.invoke_command("terminal-op", &args).expect("call invoke");
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            loop {
                let ok = {
                    let out = plugin.invoke_command("terminal-op", r#"{"op":"ws-collect"}"#).expect("call invoke");
                    out.contains("no page channel")
                };
                if ok {
                    break;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "timed out waiting for forward-output fail-visible error"
                );
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        }

        // ack 帧上行（对端断言帧形状——节流判据本身在插件 crate native 单测）
        {
            let args = serde_json::json!({
                "op": "ts-ack", "handle": handle, "offset": 65536u64,
            })
            .to_string();
            let out = plugin.invoke_command("terminal-op", &args).expect("call invoke");
            let v: serde_json::Value = serde_json::from_str(&out).expect("invoke json");
            assert!(v["ok"].as_bool().unwrap(), "ts-ack failed: {out}");
        }
        mock.wait_for_text(
            bedcode_wasm_core::test_support::mock_plugin_ws::ENDPOINT_TERMINAL,
            |f| f["type"] == "ack",
            std::time::Duration::from_secs(3),
        )
        .await;
        {
            let texts = mock.received_text(bedcode_wasm_core::test_support::mock_plugin_ws::ENDPOINT_TERMINAL).await;
            let ack = texts.iter().find(|f| f["type"] == "ack").expect("ack frame");
            assert_eq!(ack["offset"], 65536, "ack offset 水位形状");
        }

        // primary-target：宿主无主连接目标 → 显性 Err（无头单例态；
        // fail-visible，禁空对象）——fail path 同样证明原语被真实驱动
        {
            let out = plugin.invoke_command("terminal-op", r#"{"op":"ts-primary-target"}"#).expect("call invoke");
            let v: serde_json::Value = serde_json::from_str(&out).expect("invoke json");
            // 测试进程可能已配 target（单例全局）——两种结果都合法，但形态
            // 必须二选一：合法 JSON 事实 或 显性 Err（禁第三种）
            assert!(
                (v["ok"].as_bool().unwrap() && v["target"].as_str().is_some())
                    || (!v["ok"].as_bool().unwrap() && v["error"].as_str().is_some()),
                "primary-target 必须返回事实或显性错误: {out}"
            );
        }

        // close：显式关闭命中（宿主 auto-reconnect 会话一并取消）
        {
            let args = serde_json::json!({ "op": "ts-close", "handle": handle }).to_string();
            let out = plugin.invoke_command("terminal-op", &args).expect("call invoke");
            let v: serde_json::Value = serde_json::from_str(&out).expect("invoke json");
            assert_eq!(v["hit"], true, "close 命中: {out}");
        }
        mock.wait_for_text(
            bedcode_wasm_core::test_support::mock_plugin_ws::ENDPOINT_TERMINAL,
            |f| f["type"] == "auth",
            std::time::Duration::from_secs(1),
        )
        .await; // 连接已在（wait_for_text 复用为握手收敛窗口）
    });
}
