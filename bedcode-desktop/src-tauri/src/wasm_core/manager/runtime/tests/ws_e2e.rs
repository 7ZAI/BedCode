//! host-websocket 客户端/服务端/隔离端到端（ABI v14）
//!
//! 自 `wasm_runtime.rs` 的 `mod tests` 拆出（共享脚手架在 `mod tests`，
//! 经 `use super::*` 可见）；fixture 互斥与产物构建语义不变。

use super::*;
use bedcode_plugin_api::host::{ws_event_topic, WS_CLIENT_CONNECT, WS_CLIENT_DISCONNECT, WS_CLOSE, WS_OPEN};
/// host-websocket 客户端域端到端（ABI v14）
///
/// fixture 插件（`packages/plugin-ws-test`）→ 宿主 `connect`（**真握手**）→
/// 文本 / 二进制回文经 `events-ws` 回灌 → 属主私有状态事件
/// （`ws:open` / `ws:close`）经 host-bus 投递 → `close` 后 `is-connected`
/// 立即为 false（spec D3 时序）。
///
/// mock echo 服务**进程内**（随机端口 + `accept_async`），不引入外部进程，
/// 测试结束随 runtime 关闭（无残留进程与端口）。
/// 总线与帧投递共用 `TestInstanceDispatcher`（生产 = PluginHost）。
///
/// 运行时形态与同文件其它用例一致（`Runtime::new()` + `block_on`）：
/// guest 调用需在 `block_on` 体内执行，`host_impl` 的 `block_on_async`
/// 桥在这一形态下已验证可用（如 `test_session_list`）。
#[test]

fn test_ws_client_outbound_roundtrip() {
    // `setup_wasm_runtime` 内部自建 runtime 并 block_on（建库/建上下文），
    // 必须在 `rt.block_on` **之外**调用：嵌套 block_on 会 panic
    // `Cannot start a runtime from within a runtime`
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let _e2e_guard = lock_ws_fixture_e2e();
    let rt = tokio::runtime::Runtime::new().expect("tokio multi-thread runtime");
    rt.block_on(ws_e2e_guard("ws 客户端域 e2e", async {
        use futures_util::{SinkExt, StreamExt};
        use tokio_tungstenite::tungstenite::Message;

        const PLUGIN_ID: &str = "com.bedcode.ws-test";
        let open_topic = ws_event_topic(WS_OPEN, PLUGIN_ID);
        let close_topic = ws_event_topic(WS_CLOSE, PLUGIN_ID);

        // ==================== mock echo server（进程内） ====================
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind in-process echo server");
        let port = listener.local_addr().expect("local addr").port();
        let echo = tokio::spawn(async move {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let Ok(mut ws) = tokio_tungstenite::accept_async(stream).await else {
                return;
            };
            while let Some(Ok(msg)) = ws.next().await {
                match msg {
                    Message::Text(text) => {
                        if ws.send(Message::Text(text)).await.is_err() {
                            break;
                        }
                    }
                    Message::Binary(payload) => {
                        if ws.send(Message::Binary(payload)).await.is_err() {
                            break;
                        }
                    }
                    Message::Close(_) => {
                        // 对端 Close 的应答：tungstenite 收到 Close 时已把回帧（echo 收到的 code）
                        // 排入 `additional_send`，用 `SinkExt::close` 驱动 flush 即完成握手。
                        // 注意：不能用 `WebSocketStream::close(Some(..))`——其内部走
                        // `write(Message::Close)`，而在 `ClosedByPeer` 状态下
                        // `WebSocketContext::write` 直接返回 `SendAfterClosing`，回帧不会发出，
                        // 对端只能读到 EOF（wasClean 判定因此失真）。
                        let _ = futures_util::SinkExt::close(&mut ws).await;
                        break;
                    }
                    _ => {}
                }
            }
        });

        // ==================== 加载 fixture 并接线 dispatcher ====================
        // 单测不走 manifest 授权路径：显式授予（storage 由 SDK 默认授予）
        host_ctx
            .permission
            .grant_permissions(PLUGIN_ID, &["storage".to_string(), "ws:client".to_string()]);
        let component = wasm_runtime
            .compile_component(&build_ws_test_component())
            .expect("compile ws fixture component");
        let plugin = Arc::new(Mutex::new(
            wasm_runtime
                .instantiate_component(&component, PLUGIN_ID, host_ctx.clone(), &[], None)
                .expect("instantiate ws fixture"),
        ));
        host_ctx
            .message_bus
            .set_dispatcher(Arc::new(TestInstanceDispatcher {
                instances: Arc::new(RwLock::new(HashMap::from([(PLUGIN_ID.to_string(), plugin.clone())]))),
            }))
            .await;

        plugin.lock().await.activate().expect("activate = 0");
        // 订阅为异步投递（bus_subscribe 内部 spawn）：等其落地再发 connect
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // ==================== connect（同步阻塞至握手完成） ====================
        let connected = plugin
            .lock()
            .await
            .invoke_command(
                "ws-connect",
                &serde_json::json!({ "url": format!("ws://127.0.0.1:{port}/") }).to_string(),
            )
            .expect("ws-connect");
        let handle = serde_json::from_str::<serde_json::Value>(&connected).expect("connect json")["handle"]
            .as_str()
            .expect("handle")
            .to_string();
        assert!(handle.starts_with("wsc-"), "连接句柄形状应为 wsc-<uuid>，got: {handle}");

        // <owner>::ws:open（属主私有 topic，activate 期已订阅）必须投递且带 handle
        let state = ws_poll_state(
            &plugin,
            |s| ws_event_payload(s, &open_topic).is_some(),
            std::time::Duration::from_secs(3),
        )
        .await;
        let open_payload =
            ws_event_payload(&state, &open_topic).unwrap_or_else(|| panic!("ws:open 必须投递，got: {state}"));
        assert_eq!(open_payload["handle"], handle, "ws:open payload 应带连接句柄");
        assert!(
            open_payload["url"]
                .as_str()
                .unwrap_or_default()
                .starts_with("ws://127.0.0.1:"),
            "ws:open payload 应带 url，got: {open_payload}"
        );

        let connected_state = plugin
            .lock()
            .await
            .invoke_command("ws-is-connected", &serde_json::json!({ "handle": handle }).to_string())
            .expect("ws-is-connected");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&connected_state).unwrap()["connected"],
            true,
            "握手完成后 is-connected 必须为 true"
        );

        // ==================== 文本回文（events-ws 回灌） ====================
        plugin
            .lock()
            .await
            .invoke_command(
                "ws-send-text",
                &serde_json::json!({ "handle": handle, "text": "ping-text" }).to_string(),
            )
            .expect("ws-send-text");
        let state = ws_poll_state(
            &plugin,
            |s| ws_has_frame(s, "text", Some("ping-text")),
            std::time::Duration::from_secs(3),
        )
        .await;
        assert!(
            ws_has_frame(&state, "text", Some("ping-text")),
            "文本回文必须经 events-ws 回灌，got: {state}"
        );

        // ==================== 二进制回文（含非 UTF-8 字节） ====================
        let bytes = serde_json::json!([0, 1, 255, 254]);
        plugin
            .lock()
            .await
            .invoke_command(
                "ws-send-binary",
                &serde_json::json!({ "handle": handle, "bytes": bytes }).to_string(),
            )
            .expect("ws-send-binary");
        let state = ws_poll_state(
            &plugin,
            |s| ws_frame_len(s, "binary") == Some(4),
            std::time::Duration::from_secs(3),
        )
        .await;
        assert_eq!(
            ws_frame_len(&state, "binary"),
            Some(4),
            "二进制回文长度一致（非 UTF-8 直通，零 JSON 转义），got: {state}"
        );
        assert!(
            state["frames"]
                .as_array()
                .map(|frames| frames.iter().all(|f| f["target"] == handle))
                .unwrap_or(false),
            "客户端域帧标识即连接句柄，got: {state}"
        );

        // ==================== close → ws:close（对端回 1000 → wasClean=true） ====================
        let closed = plugin
            .lock()
            .await
            .invoke_command(
                "ws-close",
                &serde_json::json!({ "handle": handle, "code": 1000 }).to_string(),
            )
            .expect("ws-close");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&closed).unwrap()["hit"],
            true,
            "close 应命中句柄"
        );
        let state = ws_poll_state(
            &plugin,
            |s| ws_event_payload(s, &close_topic).is_some(),
            std::time::Duration::from_secs(3),
        )
        .await;
        let close_payload =
            ws_event_payload(&state, &close_topic).unwrap_or_else(|| panic!("ws:close 必须上报，got: {state}"));
        assert_eq!(
            close_payload["wasClean"], true,
            "对端回复 Close(1000) → wasClean=true（spec §4.5 / D11），got: {close_payload}"
        );
        assert_eq!(close_payload["handle"], handle, "ws:close payload 应带连接句柄");

        // 关闭后 is-connected 立即 false（快照自愈路径）
        let after = plugin
            .lock()
            .await
            .invoke_command("ws-is-connected", &serde_json::json!({ "handle": handle }).to_string())
            .expect("ws-is-connected");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&after).unwrap()["connected"],
            false,
            "关闭后 is-connected 必须为 false"
        );

        plugin.lock().await.deactivate().expect("deactivate = 0");
        echo.abort();
    }));
}

/// host-websocket 服务端域端到端（ABI v14，票 05）
///
/// 真实宿主 WS 服务器（进程内随机端口）+ 真实 tokio-tungstenite 客户端 +
/// fixture 插件（`packages/plugin-ws-test`）一次贯通：
///
/// 1. `register-endpoint`（`auth: none`，`maxClients: 1`）→ 通配路由挂载
///    `/ws/plugin/<owner>/echo`；
/// 2. 客户端连入 → `ws:client-connect` 事件（先于首帧）+ `list-clients` 认证态；
/// 3. 入站文本 / 二进制帧经 `events-ws` 投给插件 → 插件回显原样回客户端
///    （宿主零业务语义，回显是插件行为）；
/// 4. 单播 / 广播 / 踢出（缺省 4004）/ 注销端点（4005）逐条验证，并在
///    `ws:client-disconnect` 上核对 code 与 `wasClean`；
/// 5. 上限与门禁：`maxClients` 超限在升级前 503、注销后握手 404；
/// 6. 关闭码与「恰好一次」：每次断开都有且仅有一条 disconnect 事件。
///
/// 运行时形态与同文件其它用例一致（`Runtime::new()` + `block_on`）。
#[test]

fn test_ws_endpoint_server_domain_roundtrip() {
    // `setup_wasm_runtime` 内部自建 runtime 并 block_on：必须在 `rt.block_on` 之外
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let _e2e_guard = lock_ws_fixture_e2e();
    let rt = tokio::runtime::Runtime::new().expect("tokio multi-thread runtime");
    rt.block_on(ws_e2e_guard("ws 服务端域 e2e", async {
        use futures_util::SinkExt;
        use tokio_tungstenite::tungstenite::Message;
        use crate::utils::auth::jwt::JwtService;

        const PLUGIN_ID: &str = "com.bedcode.ws-test";
        let connect_topic = ws_event_topic(WS_CLIENT_CONNECT, PLUGIN_ID);
        let disconnect_topic = ws_event_topic(WS_CLIENT_DISCONNECT, PLUGIN_ID);

        // ==================== 宿主服务器 + fixture 装载 ====================
        let (server_handle, server_task, port) = {
            let config = crate::system::config::AppConfig::default().network;
            let port = ws_pick_free_port();
            let (handle, server) = crate::server::core::app::start_http_server(port, &config)
                .await
                .expect("start host http+ws server");
            (handle, tokio::spawn(server), port)
        };

        host_ctx
            .permission
            .grant_permissions(PLUGIN_ID, &["storage".to_string(), "ws:server".to_string()]);
        let component = wasm_runtime
            .compile_component(&build_ws_test_component())
            .expect("compile ws fixture component");
        let plugin = Arc::new(Mutex::new(
            wasm_runtime
                .instantiate_component(&component, PLUGIN_ID, host_ctx.clone(), &[], None)
                .expect("instantiate ws fixture"),
        ));
        host_ctx
            .message_bus
            .set_dispatcher(Arc::new(TestInstanceDispatcher {
                instances: Arc::new(RwLock::new(HashMap::from([(PLUGIN_ID.to_string(), plugin.clone())]))),
            }))
            .await;
        plugin.lock().await.activate().expect("activate = 0");
        // 订阅为异步投递（bus_subscribe 内部 spawn）：等其落地再注册端点
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // ==================== 1. 注册端点 + 打开回显 ====================
        let endpoint_id = {
            let mut guard = plugin.lock().await;
            let raw = guard
                .invoke_command("ws-register-endpoint", r#"{"path":"echo","maxClients":1}"#)
                .expect("register-endpoint");
            serde_json::from_str::<serde_json::Value>(&raw).expect("register json")["endpointId"]
                .as_str()
                .expect("endpointId")
                .to_string()
        };
        assert!(endpoint_id.starts_with("wse-"), "端点句柄前缀，got: {endpoint_id}");
        plugin
            .lock()
            .await
            .invoke_command("ws-endpoint-echo", r#"{"enabled":true}"#)
            .expect("echo on");

        // 端点清单：注册即可见（clientCount 0）
        {
            let raw = plugin
                .lock()
                .await
                .invoke_command("ws-list-endpoints", "{}")
                .expect("list-endpoints");
            let listed: serde_json::Value = serde_json::from_str(&raw).expect("list json");
            let entries = listed["endpoints"].as_array().expect("endpoints array");
            assert_eq!(entries.len(), 1, "本插件恰好一个端点，got: {listed}");
            assert_eq!(entries[0]["path"], "echo");
            assert_eq!(entries[0]["clientCount"], 0);
        }

        // ==================== 2. 客户端连入 → 接入事件 + 认证态 ====================
        let url = format!("ws://127.0.0.1:{port}/ws/plugin/{PLUGIN_ID}/echo");
        let (mut client_a, _) = tokio_tungstenite::connect_async(&url)
            .await
            .expect("client A connect via host route");

        let clients = ws_wait_clients(&plugin, &endpoint_id, 1).await;
        assert_eq!(clients.len(), 1, "client A 应出现在 list-clients");
        let client_a_id = clients[0]["clientId"].as_str().expect("clientId").to_string();
        assert_eq!(
            clients[0]["authenticated"], false,
            "auth:none 下注册表认证态保持 false（连接可用 ≠ 已认证）"
        );
        assert!(clients[0]["addr"].as_str().is_some_and(|a| !a.is_empty()));

        client_a
            .send(Message::Text("ordering-frame".to_string()))
            .await
            .expect("send ordering frame");
        let state = ws_poll_state(
            &plugin,
            |s| {
                let has_event = s["events"].as_array().is_some_and(|events| {
                    events.iter().any(|event| {
                        event["topic"] == connect_topic.as_str()
                            && event["payload"]["endpointId"] == endpoint_id
                            && event["payload"]["clientId"] == client_a_id
                    })
                });
                let has_frame = s["frames"].as_array().is_some_and(|frames| {
                    frames.iter().any(|frame| {
                        frame["kind"] == "text"
                            && frame["text"] == "ordering-frame"
                            && frame["target"] == format!("{endpoint_id}/{client_a_id}")
                    })
                });
                has_event && has_frame
            },
            std::time::Duration::from_secs(5),
        )
        .await;
        let connect = ws_event_payload(&state, &connect_topic).expect("ws:client-connect 事件");
        assert_eq!(connect["endpointId"], endpoint_id);
        assert_eq!(connect["clientId"], client_a_id, "事件标识与 list-clients 同源");
        let trace = state["trace"].as_array().expect("fixture trace");
        let event_index = trace
            .iter()
            .position(|entry| entry == &format!("event:{}", connect_topic))
            .expect("connect event trace");
        let frame_index = trace
            .iter()
            .position(|entry| entry == &format!("frame:text:{endpoint_id}/{client_a_id}"))
            .expect("first frame trace");
        assert!(event_index < frame_index, "client-connect 必须先于首帧回调: {trace:?}");
        match ws_client_recv(&mut client_a, std::time::Duration::from_secs(5)).await {
            Some(Message::Text(text)) => assert_eq!(text, "ordering-frame"),
            other => panic!("期望 ordering-frame 回显，got: {other:?}"),
        }

        // ==================== 3. 入站帧 → 插件回显（文本 + 二进制） ====================
        client_a
            .send(Message::Text("hello-endpoint".to_string()))
            .await
            .expect("send text");
        let state = ws_poll_state(
            &plugin,
            |s| ws_has_frame(s, "text", Some("hello-endpoint")),
            std::time::Duration::from_secs(5),
        )
        .await;
        let frame = state["frames"]
            .as_array()
            .and_then(|f| f.iter().find(|f| f["text"] == "hello-endpoint"))
            .cloned()
            .expect("fixture 应收到入站文本帧");
        assert!(
            frame["target"]
                .as_str()
                .is_some_and(|t| t.starts_with(&format!("{endpoint_id}/"))),
            "服务端域帧标识应为 endpoint/client，got: {}",
            frame["target"]
        );
        match ws_client_recv(&mut client_a, std::time::Duration::from_secs(5)).await {
            Some(Message::Text(text)) => assert_eq!(text, "hello-endpoint", "插件回显原样回客户端"),
            other => panic!("期望文本回显，got: {other:?}"),
        }

        let binary_payload: Vec<u8> = vec![0x00, 0xff, 0x7f, 0x41];
        client_a
            .send(Message::Binary(binary_payload.clone()))
            .await
            .expect("send binary");
        let state = ws_poll_state(
            &plugin,
            |s| ws_has_frame(s, "binary", None),
            std::time::Duration::from_secs(5),
        )
        .await;
        assert_eq!(
            ws_frame_len(&state, "binary"),
            Some(binary_payload.len() as u64),
            "非 UTF-8 二进制帧长度必须一致（零 JSON 转义）"
        );
        match ws_client_recv(&mut client_a, std::time::Duration::from_secs(5)).await {
            Some(Message::Binary(bytes)) => assert_eq!(bytes, binary_payload, "二进制原样回显"),
            other => panic!("期望二进制回显，got: {other:?}"),
        }

        // ==================== 4. 广播 + 上限（升级前 503） ====================
        let sent = {
            let args = serde_json::json!({ "endpointId": endpoint_id, "text": "broadcast" }).to_string();
            let raw = plugin
                .lock()
                .await
                .invoke_command("ws-broadcast-text", &args)
                .expect("broadcast");
            serde_json::from_str::<serde_json::Value>(&raw).expect("broadcast json")["sent"]
                .as_u64()
                .expect("sent")
        };
        assert_eq!(sent, 1, "广播成功入队数 = 在线客户端数");
        match ws_client_recv(&mut client_a, std::time::Duration::from_secs(5)).await {
            Some(Message::Text(text)) => assert_eq!(text, "broadcast"),
            other => panic!("期望广播文本，got: {other:?}"),
        }

        // maxClients=1 已满：第二条连接在协议升级前被拒（503，不产生连接事件）
        let rejected = tokio_tungstenite::connect_async(&url).await;
        assert!(rejected.is_err(), "超限连接必须在升级前被拒");

        // ==================== 5. 踢出（缺省 4004）====================
        let hit = {
            let args = serde_json::json!({ "endpointId": endpoint_id, "clientId": client_a_id }).to_string();
            let raw = plugin
                .lock()
                .await
                .invoke_command("ws-close-client", &args)
                .expect("close-client");
            serde_json::from_str::<serde_json::Value>(&raw).expect("close json")["hit"]
                .as_bool()
                .expect("hit")
        };
        assert!(hit, "踢出应命中在线客户端");
        match ws_client_recv(&mut client_a, std::time::Duration::from_secs(5)).await {
            Some(Message::Close(Some(frame))) => {
                assert_eq!(u16::from(frame.code), 4004, "踢出缺省关闭码 4004（spec §4.5）");
            }
            other => panic!("期望 Close(4004)，got: {other:?}"),
        }
        let state = ws_poll_state(
            &plugin,
            |s| ws_event_payload(s, &disconnect_topic).is_some(),
            std::time::Duration::from_secs(5),
        )
        .await;
        let disconnect = ws_event_payload(&state, &disconnect_topic).expect("ws:client-disconnect 事件");
        assert_eq!(disconnect["clientId"], client_a_id);
        assert_eq!(disconnect["code"], 4004, "宿主主动断开须上报关闭码");
        assert_eq!(
            disconnect["wasClean"], false,
            "宿主主动断开恒 wasClean=false（spec §4.5）"
        );
        assert!(
            ws_wait_clients(&plugin, &endpoint_id, 0).await.is_empty(),
            "踢出后句柄已回收"
        );
        // 每次断开恰好一条 disconnect 事件（再做一次投递等待后计数）
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        let state = ws_fixture_state(&plugin).await;
        let disconnect_events = state["events"]
            .as_array()
            .map(|events| {
                events
                    .iter()
                    .filter(|e| e["topic"] == disconnect_topic.as_str())
                    .count()
            })
            .unwrap_or(0);
        assert_eq!(disconnect_events, 1, "断开事件每连接恰好一次");

        // ==================== 6. 注销端点（4005）→ 握手 404 ====================
        let (mut client_b, _) = tokio_tungstenite::connect_async(&url)
            .await
            .expect("client B connect after slot freed");
        let clients = ws_wait_clients(&plugin, &endpoint_id, 1).await;
        assert_eq!(clients.len(), 1, "腾出名额后新连接可接入");

        let unregistered = {
            let args = serde_json::json!({ "endpointId": endpoint_id }).to_string();
            let raw = plugin
                .lock()
                .await
                .invoke_command("ws-unregister-endpoint", &args)
                .expect("unregister-endpoint");
            serde_json::from_str::<serde_json::Value>(&raw).expect("unregister json")["hit"]
                .as_bool()
                .expect("hit")
        };
        assert!(unregistered, "注销命中已注册端点");
        match ws_client_recv(&mut client_b, std::time::Duration::from_secs(5)).await {
            Some(Message::Close(Some(frame))) => {
                assert_eq!(u16::from(frame.code), 4005, "端点注销关闭码 4005");
            }
            other => panic!("期望 Close(4005)，got: {other:?}"),
        }

        // 端点已摘除 → 清单空 + 新握手 404（未注册端点）
        let raw = plugin
            .lock()
            .await
            .invoke_command("ws-list-endpoints", "{}")
            .expect("list-endpoints");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&raw).expect("list json")["endpoints"],
            serde_json::json!([]),
            "注销后本插件无端点"
        );
        assert!(
            tokio_tungstenite::connect_async(&url).await.is_err(),
            "未注册端点的握手必须被拒（404）"
        );

        // ==================== 7. 属主停用路径（4005 + 恰好一次）====================
        // 重挂同一后缀端点 → 新客户端连入 → 模拟宿主停用回收
        // （生产调用点：`PluginHost::deactivate_plugin_inner` → `ws::purge_for_plugin`）
        let endpoint_id = {
            let mut guard = plugin.lock().await;
            let raw = guard
                .invoke_command("ws-register-endpoint", r#"{"path":"echo"}"#)
                .expect("re-register endpoint after unregister");
            serde_json::from_str::<serde_json::Value>(&raw).expect("register json")["endpointId"]
                .as_str()
                .expect("endpointId")
                .to_string()
        };
        plugin
            .lock()
            .await
            .invoke_command("ws-endpoint-echo", r#"{"enabled":true}"#)
            .expect("echo on");
        let (mut client_c, _) = tokio_tungstenite::connect_async(&url)
            .await
            .expect("client C connect before deactivation");
        let clients = ws_wait_clients(&plugin, &endpoint_id, 1).await;
        assert_eq!(clients.len(), 1, "client C 已登记");
        let client_c_id = clients[0]["clientId"].as_str().expect("clientId").to_string();

        crate::wasm_core::host_api::ws::purge_for_plugin(PLUGIN_ID);

        match ws_client_recv(&mut client_c, std::time::Duration::from_secs(5)).await {
            Some(Message::Close(Some(frame))) => {
                assert_eq!(u16::from(frame.code), 4005, "属主停用关闭码 4005");
            }
            other => panic!("期望 Close(4005)，got: {other:?}"),
        }
        assert!(
            crate::server::websocket::endpoint::get(&endpoint_id).is_none(),
            "停用回收端点表条目（只碰本人）"
        );
        // 停用路径同样恰好一条 disconnect 事件
        //
        // 同一 topic 上多条事件共存（A 踢出 4004 / B 注销 4005 / C 停用 4005），
        // 轮询与计数都必须按 clientId 收敛：只按 code 计数会被前一条同码事件
        // （B 的注销）提前满足，形成竞态误判
        let disconnect_events_for = |s: &serde_json::Value, client_id: &str| -> Vec<serde_json::Value> {
            s["events"]
                .as_array()
                .map(|events| {
                    events
                        .iter()
                        .filter(|e| e["topic"] == disconnect_topic.as_str() && e["payload"]["clientId"] == client_id)
                        .cloned()
                        .collect()
                })
                .unwrap_or_default()
        };
        let state = ws_poll_state(
            &plugin,
            |s| disconnect_events_for(s, &client_c_id).len() == 1,
            std::time::Duration::from_secs(5),
        )
        .await;
        let deactivated_disconnects = disconnect_events_for(&state, &client_c_id);
        assert_eq!(deactivated_disconnects.len(), 1, "停用断开事件恰好一次，got: {state}");
        assert_eq!(
            deactivated_disconnects[0]["payload"]["code"], 4005,
            "属主停用关闭码 4005，got: {}",
            deactivated_disconnects[0]
        );
        assert_eq!(
            deactivated_disconnects[0]["payload"]["wasClean"], false,
            "宿主主动断开恒 wasClean=false（spec §4.5）"
        );

        // ==================== 8. auth:"jwt"：成功、丢弃与失败 ====================
        let secure_endpoint = {
            let mut guard = plugin.lock().await;
            let raw = guard
                .invoke_command(
                    "ws-register-endpoint",
                    r#"{"path":"secure","auth":"jwt","maxClients":2}"#,
                )
                .expect("register jwt endpoint");
            serde_json::from_str::<serde_json::Value>(&raw).expect("register json")["endpointId"]
                .as_str()
                .expect("endpointId")
                .to_string()
        };
        let secure_url = format!("ws://127.0.0.1:{port}/ws/plugin/{PLUGIN_ID}/secure");
        let token = JwtService::new()
            .generate_token("ws-test-device".to_string(), Some("Phone".to_string()), Some("fp-ws".to_string()))
            .expect("mint jwt");
        let (mut client_d, _) = tokio_tungstenite::connect_async(&secure_url)
            .await
            .expect("jwt endpoint connect");
        let clients = ws_wait_clients(&plugin, &secure_endpoint, 1).await;
        let client_d_id = clients[0]["clientId"].as_str().expect("clientId").to_string();
        assert_eq!(clients[0]["authenticated"], false, "未认证期注册表认证态为 false");

        client_d
            .send(Message::Text(format!(r#"{{"type":"auth","token":"{token}"}}"#)))
            .await
            .expect("send valid auth frame");
        let state = ws_poll_state(
            &plugin,
            |s| {
                s["events"].as_array().is_some_and(|events| {
                    events.iter().any(|event| {
                        event["topic"] == connect_topic.as_str()
                            && event["payload"]["endpointId"] == secure_endpoint
                            && event["payload"]["clientId"] == client_d_id
                            && event["payload"]["authenticated"] == true
                    })
                })
            },
            std::time::Duration::from_secs(5),
        )
        .await;
        assert!(ws_event_payload(&state, &connect_topic).is_some(), "JWT 成功必须产生接入事件");
        let clients = ws_wait_clients(&plugin, &secure_endpoint, 1).await;
        assert_eq!(clients[0]["authenticated"], true, "认证成功后注册表必须可见");
        client_d
            .send(Message::Text("jwt-ok".to_string()))
            .await
            .expect("send authenticated frame");
        let state = ws_poll_state(
            &plugin,
            |s| ws_has_frame(s, "text", Some("jwt-ok")),
            std::time::Duration::from_secs(5),
        )
        .await;
        assert!(ws_has_frame(&state, "text", Some("jwt-ok")), "认证后帧必须可达");
        match ws_client_recv(&mut client_d, std::time::Duration::from_secs(5)).await {
            Some(Message::Text(text)) => assert_eq!(text, "jwt-ok"),
            other => panic!("期望 JWT 成功回显，got: {other:?}"),
        }

        let close_args = serde_json::json!({ "endpointId": secure_endpoint, "clientId": client_d_id }).to_string();
        let raw = plugin
            .lock()
            .await
            .invoke_command("ws-close-client", &close_args)
            .expect("close authenticated client");
        assert_eq!(serde_json::from_str::<serde_json::Value>(&raw).unwrap()["hit"], true);
        assert!(matches!(
            ws_client_recv(&mut client_d, std::time::Duration::from_secs(5)).await,
            Some(Message::Close(Some(frame))) if u16::from(frame.code) == 4004
        ));
        let _ = ws_poll_state(
            &plugin,
            |s| s["events"].as_array().is_some_and(|events| events.iter().any(|event| {
                event["topic"] == disconnect_topic.as_str()
                    && event["payload"]["clientId"] == client_d_id
            })),
            std::time::Duration::from_secs(5),
        )
        .await;
        assert!(ws_wait_clients(&plugin, &secure_endpoint, 0).await.is_empty());

        let (mut client_e, _) = tokio_tungstenite::connect_async(&secure_url)
            .await
            .expect("invalid jwt endpoint connect");
        let clients = ws_wait_clients(&plugin, &secure_endpoint, 1).await;
        let client_e_id = clients[0]["clientId"].as_str().expect("clientId").to_string();
        assert_eq!(clients[0]["authenticated"], false);
        client_e
            .send(Message::Text("before-auth".to_string()))
            .await
            .expect("send pre-auth frame");
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        let state = ws_fixture_state(&plugin).await;
        assert!(!ws_has_frame(&state, "text", Some("before-auth")));

        client_e
            .send(Message::Text(r#"{"type":"auth","token":"not-a-jwt"}"#.to_string()))
            .await
            .expect("send bad auth frame");
        match ws_client_recv(&mut client_e, std::time::Duration::from_secs(5)).await {
            Some(Message::Close(Some(frame))) => {
                assert_eq!(u16::from(frame.code), 4001, "认证失败关闭码 4001");
            }
            other => panic!("期望 Close(4001)，got: {other:?}"),
        }
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        let state = ws_fixture_state(&plugin).await;
        let secure_connect_events = state["events"]
            .as_array()
            .map(|events| {
                events
                    .iter()
                    .filter(|e| {
                        e["topic"] == connect_topic.as_str()
                            && e["payload"]["endpointId"] == secure_endpoint
                            && e["payload"]["clientId"] == client_e_id
                    })
                    .count()
            })
            .unwrap_or(0);
        assert_eq!(secure_connect_events, 0, "认证失败连接不得产生接入事件");

        // ==================== 收尾：优雅停机 + 实例停用 ====================
        server_handle.stop(true).await;
        server_task.abort();
        plugin.lock().await.deactivate().expect("deactivate = 0");
        // 全局端点表在本进程内跨用例共享：显式清理（deactivate 不触达宿主侧回收）
        crate::server::websocket::endpoint::purge_for_plugin(PLUGIN_ID);
    }));
}

/// 票据 06 双 fixture 隔离 demo：A 挂入站端点、B 连外部服务
///
/// 一次贯通两组隔离断言：
///
/// - **零可见**：B 的 `list-endpoints` / `list-clients` 看不到 A 的端点；
///   A 对 B 的出站句柄调用被拒（跨插件属主仲裁）；
/// - **零影响**：A 停用回收（`purge_for_plugin(A)`）后 B 的外部连接仍然在线，
///   而 A 的入站对端收到 4005 下线关闭帧。
#[test]

fn test_ws_two_plugin_isolation() {
    // `setup_wasm_runtime` 内部自建 runtime 并 block_on（建库/建上下文），
    // 必须在 `rt.block_on` **之外**调用：嵌套 block_on 会 panic
    // `Cannot start a runtime from within a runtime`
    let (runtime_a, ctx_a) = setup_wasm_runtime();
    let (runtime_b, ctx_b) = setup_wasm_runtime();
    let _e2e_guard = lock_ws_fixture_e2e();
    let rt = tokio::runtime::Runtime::new().expect("tokio multi-thread runtime");
    rt.block_on(ws_e2e_guard("ws 双 fixture 隔离 e2e", async {
        use futures_util::StreamExt;
        use tokio_tungstenite::tungstenite::Message;

        const PLUGIN_A: &str = "com.bedcode.ws-test";
        // 第二个实例用独立 id：属主域完全隔离（端点命名空间 / 句柄 / 事件 topic）
        const PLUGIN_B: &str = "com.bedcode.ws-test.peer";

        // ==================== B 的外部对端（进程内 mock echo） ====================
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock peer");
        let peer_port = listener.local_addr().expect("peer addr").port();
        let peer = tokio::spawn(async move {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let Ok(mut ws) = tokio_tungstenite::accept_async(stream).await else {
                return;
            };
            while let Some(Ok(msg)) = ws.next().await {
                if matches!(msg, Message::Close(_)) {
                    break;
                }
            }
        });

        // ==================== 两个独立宿主上下文（各自的 bus 与实例表） ====================
        // A 两个域都授权：跨插件负向断言必须先过权限门，才落到属主仲裁
        ctx_a.permission.grant_permissions(
            PLUGIN_A,
            &["storage".to_string(), "ws:server".to_string(), "ws:client".to_string()],
        );
        let component = runtime_a
            .compile_component(&build_ws_test_component())
            .expect("compile ws fixture component");
        let plugin_a = Arc::new(Mutex::new(
            runtime_a
                .instantiate_component(&component, PLUGIN_A, ctx_a.clone(), &[], None)
                .expect("instantiate plugin A"),
        ));
        ctx_a
            .message_bus
            .set_dispatcher(Arc::new(TestInstanceDispatcher {
                instances: Arc::new(RwLock::new(HashMap::from([(PLUGIN_A.to_string(), plugin_a.clone())]))),
            }))
            .await;
        plugin_a.lock().await.activate().expect("activate A");

        // B 同样两个域都授权：跨插件负向断言必须落在**属主仲裁**而非权限门，
        // 否则 B 对 A 端点的调用会被 `permission denied: ws:server` 短路，
        // 证明不了属主隔离（B 实际只需出站能力，此处为断言口径而授权）
        ctx_b.permission.grant_permissions(
            PLUGIN_B,
            &["storage".to_string(), "ws:client".to_string(), "ws:server".to_string()],
        );
        // B 必须用 runtime_b 自己编译的组件：wasmtime 不支持跨 `Engine` 实例化
        let component_b = runtime_b
            .compile_component(&build_ws_test_component())
            .expect("compile ws fixture component for B");
        let plugin_b = Arc::new(Mutex::new(
            runtime_b
                .instantiate_component(&component_b, PLUGIN_B, ctx_b.clone(), &[], None)
                .expect("instantiate plugin B"),
        ));
        ctx_b
            .message_bus
            .set_dispatcher(Arc::new(TestInstanceDispatcher {
                instances: Arc::new(RwLock::new(HashMap::from([(PLUGIN_B.to_string(), plugin_b.clone())]))),
            }))
            .await;
        plugin_b.lock().await.activate().expect("activate B");

        // ==================== A 挂端点 + B 连外部服务（互不相干） ====================
        let (server_handle, server_task, port) = {
            let config = crate::system::config::AppConfig::default().network;
            let port = ws_pick_free_port();
            let (handle, server) = crate::server::core::app::start_http_server(port, &config)
                .await
                .expect("start host http+ws server");
            (handle, tokio::spawn(server), port)
        };

        let endpoint_a = {
            let mut guard = plugin_a.lock().await;
            let raw = guard
                .invoke_command("ws-register-endpoint", r#"{"path":"iso"}"#)
                .expect("register endpoint on A");
            serde_json::from_str::<serde_json::Value>(&raw).expect("register json")["endpointId"]
                .as_str()
                .expect("endpointId")
                .to_string()
        };
        let handle_b = {
            let args = serde_json::json!({ "url": format!("ws://127.0.0.1:{peer_port}/") }).to_string();
            let raw = plugin_b
                .lock()
                .await
                .invoke_command("ws-connect", &args)
                .expect("B connect outbound");
            serde_json::from_str::<serde_json::Value>(&raw).expect("connect json")["handle"]
                .as_str()
                .expect("handle")
                .to_string()
        };

        let url = format!("ws://127.0.0.1:{port}/ws/plugin/{PLUGIN_A}/iso");
        let (mut client_a, _) = tokio_tungstenite::connect_async(&url)
            .await
            .expect("inbound client connects to A endpoint");
        let clients = ws_wait_clients(&plugin_a, &endpoint_a, 1).await;
        assert_eq!(clients.len(), 1, "A 的端点客户端已登记");

        // ==================== 零可见 ====================
        assert_eq!(
            crate::wasm_core::host_api::ws::ws_list_endpoints(ctx_b.as_ref(), PLUGIN_B).unwrap(),
            "[]",
            "B 看不到 A 的端点"
        );
        assert_eq!(
            crate::wasm_core::host_api::ws::ws_list_clients(ctx_b.as_ref(), PLUGIN_B, &endpoint_a)
                .unwrap_err(),
            "not owner of ws endpoint",
            "B 不得查询 A 的端点客户端"
        );
        assert_eq!(
            crate::wasm_core::host_api::ws::ws_is_connected(ctx_a.as_ref(), PLUGIN_A, &handle_b)
                .unwrap_err(),
            "not owner of ws handle",
            "A 不得操作 B 的出站句柄"
        );

        // ==================== A 停用回收：零影响 B；A 的对端收到 4005 ====================
        crate::wasm_core::host_api::ws::purge_for_plugin(PLUGIN_A);
        match ws_client_recv(&mut client_a, std::time::Duration::from_secs(5)).await {
            Some(Message::Close(Some(frame))) => {
                assert_eq!(u16::from(frame.code), 4005, "属主停用关闭码 4005");
            }
            other => panic!("期望 Close(4005)，got: {other:?}"),
        }
        assert!(
            crate::server::websocket::endpoint::get(&endpoint_a).is_none(),
            "A 的端点随停用回收"
        );
        assert!(
            crate::wasm_core::host_api::ws::ws_is_connected(ctx_b.as_ref(), PLUGIN_B, &handle_b)
                .expect("B 句柄仍可查询"),
            "A 停用不得影响 B 的外部连接"
        );
        // B 的对端仍在线：可继续发送（fail-visible 之外的正向断言）
        assert!(
            crate::wasm_core::host_api::ws::ws_send_text(
                ctx_b.as_ref(),
                PLUGIN_B,
                &handle_b,
                "still-alive"
            )
            .is_ok(),
            "A 停用后 B 仍可发送"
        );

        // ==================== 收尾 ====================
        server_handle.stop(true).await;
        server_task.abort();
        plugin_a.lock().await.deactivate().expect("deactivate A");
        plugin_b.lock().await.deactivate().expect("deactivate B");
        crate::wasm_core::host_api::ws::purge_for_plugin(PLUGIN_A);
        crate::wasm_core::host_api::ws::purge_for_plugin(PLUGIN_B);
        crate::server::websocket::endpoint::purge_for_plugin(PLUGIN_A);
        crate::server::websocket::endpoint::purge_for_plugin(PLUGIN_B);
        peer.abort();
    }));
}

/// 票 09b：会话控制声明端点的直连往返（真实 terminal-session 插件产物）
///
/// 旧 `/ws/event` `Message::SessionControl` 协议之外的声明式路由：客户端直连
/// `/ws/plugin/com.bedcode.terminal-session/session-control`（manifest
/// `contributes.wsEndpoints` 声明 + 激活期登记，auth=jwt）→ 首消息 JWT 认证 →
/// 文本动作帧 → 插件 `ws_control` 域分派 → 响应动作帧回包。宿主只做认证与
/// 帧转发（PluginChannel），动作「是什么业务」的解释在插件。
///
/// 覆盖：jwt 认证成功（fixture 用例遗留的「需真实签发 token」分支在此补上）、
/// list_sessions 空登记域回包形状、未知动作 fail-visible 错误帧。
#[test]
fn test_session_control_endpoint_direct_roundtrip() {
    use crate::server::websocket::services::session_control as sc;
    use crate::utils::auth::jwt::JwtService;
    use bedcode_plugin_api::EndpointAuth;
    use futures_util::SinkExt;
    use tokio_tungstenite::tungstenite::Message;

    // `setup_wasm_runtime` 内部自建 runtime 并 block_on（建库/建上下文），
    // 必须在 `rt.block_on` **之外**调用：嵌套 block_on 会 panic
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let _ws_guard = lock_ws_fixture_e2e();
    // 会话插件私有库进程级共享：与会话闭环用例串行（见锁文档）
    let _serial = session_plugin_db_guard();
    // 声明闸门用例（services/session_control.rs）与本用例共用全局端点表：
    // 同一把锁串行化
    let _endpoint_guard = sc::SESSION_CONTROL_ENDPOINT_TEST_LOCK.lock().unwrap();

    const PLUGIN_ID: &str = "com.bedcode.terminal-session";
    let wasm_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../resources/plugins/desktop/com.bedcode.terminal-session/bedcode_plugin_terminal_session.wasm");
    if !wasm_path.exists() {
        eprintln!("[skip] session wasip3 artifact not built");
        return;
    }

    let rt = tokio::runtime::Runtime::new().expect("tokio multi-thread runtime");
    rt.block_on(ws_e2e_guard("session/control 直连 e2e", async {
        // 独立私有库根目录（进程级共享根被并行测试写同一 SQLite 文件 → BUSY）
        let mut host_ctx = host_ctx;
        if let Some(ctx) = Arc::get_mut(&mut host_ctx) {
            ctx.set_plugin_db_root(Some(std::env::temp_dir().join(format!(
                "bedcode_plugin_dbs_wsctl_e2e_{}_{}",
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
                "broadcast".to_string(),
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

        // 激活期等价登记（PluginHost::activate_plugin 激活成功分支的对应物）：
        // 声明端点在端点表落地后，插件端点路由才可达
        let entry = crate::server::websocket::endpoint::register(
            PLUGIN_ID,
            "session-control",
            EndpointAuth::Jwt,
            None,
            None,
            host_ctx.message_bus.clone(),
        )
        .expect("register declared session/control endpoint");
        assert_eq!(entry.mount_path, "/ws/plugin/com.bedcode.terminal-session/session-control");

        // 真实 JWT（同一进程 secret-store：generate/verify 同密钥）
        let token = JwtService::new()
            .generate_token("dev-direct-1".to_string(), Some("Phone".to_string()), Some("fp-1".to_string()))
            .expect("mint jwt");

        let url = format!("ws://127.0.0.1:{port}/ws/plugin/{PLUGIN_ID}/session-control");
        let (mut client, _) = tokio_tungstenite::connect_async(&url)
            .await
            .expect("client connect via host route");

        // ==================== 1. 首消息 JWT 认证（auth=jwt 成功分支） ====================
        client
            .send(Message::Text(format!(r#"{{"type":"auth","token":"{token}"}}"#)))
            .await
            .expect("send auth frame");
        // 认证通过 → 接入事件发布；无显式 auth_ok 回帧（端点协议不定义回执），
        // 用「认证后业务帧可达」作为可达性判据：直接发动作帧

        // ==================== 2. list_sessions → 响应动作帧 ====================
        client
            .send(Message::Text(r#"{"type":"list_sessions"}"#.to_string()))
            .await
            .expect("send list action");
        match ws_client_recv(&mut client, std::time::Duration::from_secs(5)).await {
            Some(Message::Text(text)) => {
                let reply: serde_json::Value =
                    serde_json::from_str(&text).unwrap_or_else(|e| panic!("reply 非 JSON: {text}: {e}"));
                assert_eq!(reply["type"], "session_list", "响应动作标签, got: {reply}");
                assert_eq!(
                    reply["sessions"],
                    serde_json::json!([]),
                    "空登记域 → 空列表（SessionSummary 形状, got: {reply}"
                );
            }
            other => panic!("期望 list_sessions 响应帧，got: {other:?}"),
        }

        // ==================== 3. 未知动作 fail-visible：错误帧 ====================
        client
            .send(Message::Text(r#"{"type":"launch_missiles"}"#.to_string()))
            .await
            .expect("send unknown action");
        match ws_client_recv(&mut client, std::time::Duration::from_secs(5)).await {
            Some(Message::Text(text)) => {
                let reply: serde_json::Value =
                    serde_json::from_str(&text).unwrap_or_else(|e| panic!("reply 非 JSON: {text}: {e}"));
                assert_eq!(reply["type"], "error", "未知动作 → error 帧, got: {reply}");
                assert!(
                    reply["message"]
                        .as_str()
                        .is_some_and(|m| m.contains("unknown session control action")),
                    "错误文案点明未知动作, got: {reply}"
                );
            }
            other => panic!("期望 error 帧，got: {other:?}"),
        }

        // ==================== 4. 二进制帧非本端点协议：忽略 + 连接保持 ====================
        client
            .send(Message::Binary(vec![1, 2, 3]))
            .await
            .expect("send binary frame");
        // 连接不应被关闭：再发一个合法动作确认链路仍活
        client
            .send(Message::Text(r#"{"type":"list_sessions"}"#.to_string()))
            .await
            .expect("send list action again");
        match ws_client_recv(&mut client, std::time::Duration::from_secs(5)).await {
            Some(Message::Text(text)) => {
                let reply: serde_json::Value = serde_json::from_str(&text).expect("reply json");
                assert_eq!(reply["type"], "session_list", "二进制帧后链路仍活, got: {reply}");
            }
            other => panic!("期望 list_sessions 响应帧，got: {other:?}"),
        }

        // ==================== 5. 真实 start/stop/remove 闭环（票 03） ====================
        // 种子配置 → start_session 经端点直发（真实 PTY spawn）→ list 可见 →
        // stop_session → pty:exit 终态 → remove_session → 列表清空。
        // 发起者身份经 connection-context 解析（JWT deviceName=Phone），
        // 会话登记域的正统端初始归属因此固定为移动端。
        let config_out = plugin
            .lock()
            .await
            .invoke_command(
                "session.config.upsert",
                &serde_json::json!({
                    "name": "wsctl-e2e",
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

        client
            .send(Message::Text(format!(
                r#"{{"type":"start_session","config_id":"{config_id}"}}"#
            )))
            .await
            .expect("send start action");
        let started_id = match ws_client_recv(&mut client, std::time::Duration::from_secs(5)).await {
            Some(Message::Text(text)) => {
                let reply: serde_json::Value =
                    serde_json::from_str(&text).unwrap_or_else(|e| panic!("start 回包非 JSON: {text}: {e}"));
                assert_eq!(reply["type"], "start_session", "start 回显动作标签, got: {reply}");
                reply["session_id"].as_str().expect("start 回包 session_id").to_string()
            }
            other => panic!("期望 start_session 回包，got: {other:?}"),
        };
        assert!(
            !started_id.is_empty() && started_id.len() >= 8,
            "插件自产会话 id（UUID v4 形态）, got: {started_id}"
        );

        // list 应能看到新会话（真实 PTY 已 spawn，状态 running）
        client
            .send(Message::Text(r#"{"type":"list_sessions"}"#.to_string()))
            .await
            .expect("send list action");
        match ws_client_recv(&mut client, std::time::Duration::from_secs(5)).await {
            Some(Message::Text(text)) => {
                let reply: serde_json::Value = serde_json::from_str(&text).expect("reply json");
                assert_eq!(reply["type"], "session_list", "got: {reply}");
                let sessions = reply["sessions"].as_array().expect("sessions array");
                assert_eq!(sessions.len(), 1, "start 后列表应有 1 条, got: {reply}");
                assert_eq!(sessions[0]["id"], started_id, "列表命中新会话, got: {reply}");
                assert_eq!(sessions[0]["status"], "running", "spawn 即 running, got: {reply}");
            }
            other => panic!("期望 list_sessions 响应帧，got: {other:?}"),
        }

        // stop_session → pty kill → pty:exit 终态（轮询至 stopped）
        client
            .send(Message::Text(format!(
                r#"{{"type":"stop_session","session_id":"{started_id}"}}"#
            )))
            .await
            .expect("send stop action");
        match ws_client_recv(&mut client, std::time::Duration::from_secs(5)).await {
            Some(Message::Text(text)) => {
                let reply: serde_json::Value = serde_json::from_str(&text).expect("reply json");
                assert_eq!(reply["type"], "stop_session", "stop 回显动作标签, got: {reply}");
            }
            other => panic!("期望 stop_session 回包，got: {other:?}"),
        }
        // pty:exit 终态收尾是异步的（退出事件驱动）；轮询 list 直到 stopped
        let mut stopped = false;
        for _ in 0..40 {
            client
                .send(Message::Text(r#"{"type":"list_sessions"}"#.to_string()))
                .await
                .expect("send list");
            if let Some(Message::Text(text)) = ws_client_recv(&mut client, std::time::Duration::from_secs(3)).await {
                let reply: serde_json::Value = serde_json::from_str(&text).expect("reply json");
                if reply["type"] == "session_list" {
                    if let Some(status) = reply["sessions"].as_array()
                        .and_then(|a| a.first())
                        .and_then(|s| s["status"].as_str())
                    {
                        if status == "stopped" {
                            stopped = true;
                            break;
                        }
                    }
                }
            }
        }
        assert!(stopped, "stop 后会话应进入 stopped 终态（pty:exit 驱动）");

        // remove_session → 摘记录 → 列表清空
        client
            .send(Message::Text(format!(
                r#"{{"type":"remove_session","session_id":"{started_id}"}}"#
            )))
            .await
            .expect("send remove action");
        match ws_client_recv(&mut client, std::time::Duration::from_secs(5)).await {
            Some(Message::Text(text)) => {
                let reply: serde_json::Value = serde_json::from_str(&text).expect("reply json");
                assert_eq!(reply["type"], "remove_session", "remove 回显动作标签, got: {reply}");
            }
            other => panic!("期望 remove_session 回包，got: {other:?}"),
        }
        client
            .send(Message::Text(r#"{"type":"list_sessions"}"#.to_string()))
            .await
            .expect("send list action");
        match ws_client_recv(&mut client, std::time::Duration::from_secs(5)).await {
            Some(Message::Text(text)) => {
                let reply: serde_json::Value = serde_json::from_str(&text).expect("reply json");
                let sessions = reply["sessions"].as_array().expect("sessions array");
                assert_eq!(sessions.len(), 0, "remove 后列表应清空, got: {reply}");
            }
            other => panic!("期望 list_sessions 响应帧，got: {other:?}"),
        }

        // ==================== 收尾：优雅停机 + 清理 ====================
        server_handle.stop(true).await;
        server_task.abort();
        crate::server::websocket::endpoint::purge_for_plugin(PLUGIN_ID);
        plugin.lock().await.deactivate().expect("deactivate = 0");
    }));
}
