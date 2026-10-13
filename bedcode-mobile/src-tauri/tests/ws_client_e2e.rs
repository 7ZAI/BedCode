//! 移动 host-websocket 客户端域全链路（票 11 §7.3；票 05 迁移动宿主集成测试）


use std::collections::HashSet;
use std::sync::Arc;

use bedcode_host_kit::WasmPluginState;
use bedcode_wasm_core::manager::runtime::{LoadedWasmPlugin, WasmRuntime};
use bedcode_wasm_core::test_support::{build_host_ctx, build_test_component, test_engine, TEST_PLUGIN_ID};
use wasmtime::component::Component;

use bedcode_plugin_api_mobile::permission::{PERMISSION_BUS, PERMISSION_WS_CLIENT};

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

/// host-websocket 客户端域全链路（真实 WASM 组件闭环）：guest 经 bindgen
/// import 调宿主原语——activate 期订阅 → connect（真实握手）→ send-text
/// （对端收到）→ 对端回帧（帧信封出现在属主二进制 topic）→ close（对端回
/// Close 1000 → `ws:close` 事件 wasClean=true）→ is-connected 翻 false。
/// 权限门（ws:client 已授权）与帧信封形状在闭环内被真实驱动

/// 移动运行时装配（与生产 `PluginManager::init_wasm_runtime_with` 同款）：
/// 创建 runtime 后**注入 guest 事件派发器**——`events` 导出是宿主契约面，内核缺省
/// `UnwiredGuestEvents` 派发即 fail-visible（不注入时 `on_bus_message` 必 Err）。
fn mobile_runtime(aot_cache_dir: Option<std::path::PathBuf>) -> WasmRuntime {
    // 统一壳形态规格注入（单形态）：移动 WIT 无 `abi-form` 导出 ⇒ 形态要求为
    // `None`。整数 ABI 版本协商已随 wit 包版本化退役（ADR 0051 D2）——产物版本
    // 由 linker 的 import 名逐字匹配裁决，不再有"上限"可注入。
    let runtime = bedcode_wasm_core::test_support::setup_headless_runtime_with_abi(
        aot_cache_dir,
        bedcode_wasm_core::manager::runtime::AbiPolicy { form: None },
    )
    .expect("create wasm runtime");
    runtime.set_guest_events(std::sync::Arc::new(
        bedcode_mobile_lib::plugin::guest_events::MobileGuestEvents,
    ));
    runtime
}

#[test]
fn ws_client_domain_full_loop_with_real_component() {
    use futures_util::{SinkExt, StreamExt};
    use tokio::net::TcpListener;
    use tokio_tungstenite::tungstenite::protocol::Message as WsMsg;

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async move {
        // ---- echo 服务器：记录收到的文本帧；首条文本后回二进制 echo 帧；Close 结束 ----
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind echo server");
        let addr = listener.local_addr().expect("addr");
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            let ws = tokio_tungstenite::accept_async(stream).await.expect("server handshake");
            let (mut sink, mut source) = ws.split();
            let mut texts = Vec::new();
            let mut echoed = false;
            while let Some(Ok(msg)) = source.next().await {
                match msg {
                    WsMsg::Text(t) => {
                        texts.push(t.to_string());
                        if !echoed {
                            echoed = true;
                            sink.send(WsMsg::Binary(b"echo-payload".to_vec())).await.expect("echo send");
                        }
                    }
                    WsMsg::Close(_) => break,
                    _ => {}
                }
            }
            let _ = sink.close().await;
            texts
        });

        // ---- 捕获型投递器 + 真实组件实例化（权限 = ws:client）----
        let host_ctx = build_host_ctx();
        let captured: Arc<std::sync::Mutex<Vec<bedcode_server_base::wire::BusMessage>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        host_ctx
            .message_bus
            .set_dispatcher(Arc::new(CapturingDispatcher { captured: Arc::clone(&captured) }))
            .await;

        let tmp = tempfile::tempdir().expect("tempdir");
        let runtime = mobile_runtime(Some(tmp.path().join("aot")));
        let component = Component::from_binary(bedcode_wasm_core::test_support::runtime_engine(&runtime), &build_test_component(&["ws-client"]))
            .expect("compile ws component");
        let mut plugin = runtime
            .instantiate_component(
                &component,
                TEST_PLUGIN_ID,
                // 权限经 for_plugin 派生 per-plugin 上下文注入（统一 instantiate 签名）
                Arc::new(host_ctx.for_plugin(HashSet::from([
                    PERMISSION_WS_CLIENT.to_string(),
                    bedcode_plugin_api_mobile::permission::PERMISSION_BUS.to_string(),
                ]))),
                &[],
                None,
            )
            .expect("instantiate ws component");

        // activate（guest 订阅 <PID>:ws:open / :ws:close / :ws:message）+ connect（真实握手）
        let handle = {
            plugin.activate().expect("activate");
            let args = serde_json::json!({ "op": "ws-connect", "url": format!("ws://{addr}") }).to_string();
            let out = plugin.invoke_command("ws-op", &args).expect("call invoke");
            let v: serde_json::Value = serde_json::from_str(&out).expect("invoke json");
            assert!(v["ok"].as_bool().unwrap(), "connect failed: {out}");
            let h = v["handle"].as_str().expect("handle").to_string();
            assert!(h.starts_with("wsc-"), "handle shape: {h}");
            h
        };

        // send-text：guest → 宿主 → echo 服务器（上行闭环）
        {
            let args = serde_json::json!({ "op": "ws-send-text", "handle": handle, "text": "hello-ws-loop" })
                .to_string();
            let out = plugin.invoke_command("ws-op", &args).expect("call invoke");
            let v: serde_json::Value = serde_json::from_str(&out).expect("invoke json");
            assert!(v["ok"].as_bool().unwrap(), "send-text failed: {out}");
        }

        // 等 ws:open 事件与 echo 回帧出现在总线（属主 topic）
        async fn wait_for(mut cond: impl FnMut() -> bool, what: &str) {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            while !cond() {
                assert!(std::time::Instant::now() < deadline, "timed out waiting for {what}");
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        }
        wait_for(
            || {
                let cap = captured.lock().unwrap();
                cap.iter().any(|m| m.topic.ends_with(":ws:open"))
                    && cap.iter().any(|m| m.topic.ends_with(":ws:message"))
            },
            "bus ws:open + ws:message",
        )
        .await;

        // open 事件：payload 含连接事实（handle + url），sender 恒为 host
        {
            let cap = captured.lock().unwrap();
            let open = cap.iter().find(|m| m.topic.ends_with(":ws:open")).expect("open event");
            assert_eq!(open.sender, "host", "状态事件 sender 恒为 host");
            assert_eq!(open.payload["handle"], handle.as_str(), "open payload 携带句柄");
            assert!(
                open.payload["url"].as_str().unwrap().contains(&addr.port().to_string()),
                "open payload 携带连接目标"
            );
        }
        // 帧信封：kind=binary(2) + handle + 原始 echo 字节（宿主 → 插件下行形状）
        {
            let cap = captured.lock().unwrap();
            let frame = cap.iter().find(|m| m.topic.ends_with(":ws:message")).expect("frame");
            let bin = frame.payload_binary.as_deref().expect("binary payload");
            assert_eq!(bin[0], 2, "echo 回帧 kind=binary(2)");
            let hl = u16::from_be_bytes([bin[1], bin[2]]) as usize;
            assert_eq!(&bin[3..3 + hl], handle.as_bytes(), "信封 handle 段");
            assert_eq!(&bin[3 + hl..], b"echo-payload", "信封载荷段 = 对端原始字节");
        }

        // close：对端回 Close 1000 → ws:close 事件（wasClean=true）
        {
            let args = serde_json::json!({ "op": "ws-close", "handle": handle }).to_string();
            let out = plugin.invoke_command("ws-op", &args).expect("call invoke");
            let v: serde_json::Value = serde_json::from_str(&out).expect("invoke json");
            assert_eq!(v["hit"], true, "close 命中: {out}");
        }
        wait_for(
            || captured.lock().unwrap().iter().any(|m| m.topic.ends_with(":ws:close")),
            "ws:close event",
        )
        .await;
        {
            let cap = captured.lock().unwrap();
            let close = cap.iter().find(|m| m.topic.ends_with(":ws:close")).expect("close event");
            assert_eq!(close.payload["wasClean"], true, "对端回 1000 → wasClean=true");
            assert_eq!(close.payload["code"], 1000, "close 事件携带对端关闭码");
        }

        // 服务器侧：上行文本帧内容正确（exactly 一次）
        let texts = tokio::time::timeout(std::time::Duration::from_secs(3), server)
            .await
            .expect("server join timeout")
            .expect("server task");
        assert_eq!(texts, vec!["hello-ws-loop".to_string()], "对端收到的文本帧");

        // is-connected：条目已由 reader 摘除 → false（真实查询链路）
        {
            let args = serde_json::json!({ "op": "ws-is-connected", "handle": handle }).to_string();
            let out = plugin.invoke_command("ws-op", &args).expect("call invoke");
            let v: serde_json::Value = serde_json::from_str(&out).expect("invoke json");
            assert_eq!(v["open"], false, "关闭后 is-connected 为 false");
        }
    });
}
