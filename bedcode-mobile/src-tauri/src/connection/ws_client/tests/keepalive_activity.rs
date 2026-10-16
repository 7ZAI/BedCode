//! keepalive —— 任意入站帧（Text / Binary / Ping）刷新心跳判活基准的接线测试
//!
//! 背景（2026-10-16 定位）：移动端 events WS 每 2.5 分钟判死重连，根因是
//! Pong 通路不可达（`[WsClient] Received pong` 两天 0 次）且 receiver 循环
//! 此前**只认 Pong** 刷新心跳活动（`on_activity` 的注释承诺「任意入站帧都
//! 证明链路还通」，但 Text/Binary/Ping 分支未兑现调用）。
//!
//! 修复：Text / Binary / Ping 分支均调 `heartbeat.on_activity()`。本文件验证
//! 三处接线 + 对照组证明「无入站帧时短窗口必然判死」——后者保证这些用例对
//! 「接线被删」的变异是敏感的（不是恒真测试）。

use super::*;

use futures_util::{SinkExt, StreamExt};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::protocol::Message as ServerMsg;

/// 无操作 handler：Text 帧若无 handler 会走 receiver 的 Unroutable warn 分支，
/// 本测试只关心心跳接线，避免噪音
struct NoopHandler;

impl MessageHandler for NoopHandler {
    fn handle(
        &self,
        _raw_message: WsMsg,
        _addr: std::net::SocketAddr,
        _client_id: Option<&str>,
        _sender: Option<tokio::sync::mpsc::Sender<WsMsg>>,
    ) {
    }
}

/// 短心跳窗口配置：interval=1s → timeout=3s（×3 倍）、max_timeouts=3。
/// `from_client_config` 的超时=interval×3，故无任何刷新时约 5s 判死（Error 事件）。
fn keepalive_config(addr: std::net::SocketAddr) -> WsClientConfig {
    WsClientConfig {
        address: "127.0.0.1".to_string(),
        port: addr.port(),
        path: "/".to_string(),
        heartbeat_interval_secs: 1,
        message_queue_size: 1024,
        connect_timeout_ms: 3000,
    }
}

/// server：每 500ms 向客户端推一帧指定类型，直到客户端断开。
/// 500ms << 3s 超时窗口——接线正确时心跳基准每轮都被刷新，永不判死。
async fn spawn_frame_server(frame: fn() -> ServerMsg) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = accept_async(stream).await.unwrap();
        loop {
            if ws.send(frame()).await.is_err() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    });
    (addr, handle)
}

/// server：accept 后静默持有连接（TCP 打开但**不推任何帧**）——对照组，
/// 验证「无刷新必判死」，保证保活用例对变异敏感
async fn spawn_silent_server() -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = accept_async(stream).await.unwrap();
        // 持有连接 10s（期间无任何帧），模拟「TCP 通但无业务/心跳应答」的场景
        let _ = tokio::time::timeout(Duration::from_secs(10), ws.next()).await;
    });
    (addr, handle)
}

/// 观察客户端事件流 `watch_secs`：期间出现断连类事件（Error / ServerClosed /
/// Disconnected）→ `Err(事件)`；全程无事件（超时）→ `Ok(())` = 连接保持
async fn wait_no_disconnect_until(client: &Arc<WsClient>, watch_secs: u64) -> std::result::Result<(), WsClientEvent> {
    let mut rx = client.subscribe();
    let feed = async {
        loop {
            match rx.recv().await {
                Ok(WsClientEvent::Error { message }) => return Err(WsClientEvent::Error { message }),
                Ok(WsClientEvent::ServerClosed { code, reason }) => {
                    return Err(WsClientEvent::ServerClosed { code, reason })
                }
                Ok(WsClientEvent::Disconnected) => return Err(WsClientEvent::Disconnected),
                Ok(_) => continue,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(e) => {
                    return Err(WsClientEvent::Error {
                        message: format!("recv error: {e:?}"),
                    })
                }
            }
        }
    };
    match tokio::time::timeout(Duration::from_secs(watch_secs), feed).await {
        Err(_) => Ok(()),
        Ok(Err(ev)) => Err(ev),
        Ok(Ok(())) => unreachable!("feed 循环不会正常返回"),
    }
}

/// 收到 Text 帧应刷新心跳活动：观察窗 6s 内连接保持（无刷新则 ~5s 判死）
#[tokio::test]
async fn text_frames_keep_connection_alive() {
    let (addr, _server) = spawn_frame_server(|| ServerMsg::Text("keepalive".to_string())).await;
    let client = WsClient::new(keepalive_config(addr));
    client.set_handler(Arc::new(NoopHandler)).await;
    client.connect().await.unwrap();

    let result = wait_no_disconnect_until(&client, 6).await;
    assert!(result.is_ok(), "Text 帧未刷新心跳活动，连接被误判死: {:?}", result);
    assert!(client.is_connected().await, "连接应保持 Connected");
}

/// 收到 Binary 帧应刷新心跳活动：观察窗 6s 内连接保持
#[tokio::test]
async fn binary_frames_keep_connection_alive() {
    let (addr, _server) = spawn_frame_server(|| ServerMsg::Binary(vec![0x01, 0x02, 0x03].into())).await;
    let client = WsClient::new(keepalive_config(addr));
    client.set_handler(Arc::new(NoopHandler)).await;
    client.connect().await.unwrap();

    let result = wait_no_disconnect_until(&client, 6).await;
    assert!(result.is_ok(), "Binary 帧未刷新心跳活动，连接被误判死: {:?}", result);
    assert!(client.is_connected().await, "连接应保持 Connected");
}

/// 收到服务端 Ping 帧应刷新心跳活动：观察窗 6s 内连接保持
/// （服务端每 5s 一发的 Ping 是最密的保活信号，Pong 通路异常时的判活兜底）
#[tokio::test]
async fn server_ping_frames_keep_connection_alive() {
    let (addr, _server) = spawn_frame_server(|| ServerMsg::Ping(vec![].into())).await;
    let client = WsClient::new(keepalive_config(addr));
    client.set_handler(Arc::new(NoopHandler)).await;
    client.connect().await.unwrap();

    let result = wait_no_disconnect_until(&client, 6).await;
    assert!(result.is_ok(), "服务端 Ping 未刷新心跳活动，连接被误判死: {:?}", result);
    assert!(client.is_connected().await, "连接应保持 Connected");
}

/// 对照组：TCP 连接保持打开但无任何入站帧 → 短窗口（3s/3 次）内必须判死。
/// 本用例证明保活用例对「接线缺失」变异敏感：若 Text/Binary/Ping 全不刷新
/// 而保活用例仍通过，说明断言失效
#[tokio::test]
async fn silent_connection_still_dies_without_inbound_frames() {
    let (addr, _server) = spawn_silent_server().await;
    let client = WsClient::new(keepalive_config(addr));
    client.connect().await.unwrap();

    // 无刷新：约 t=3s 起计数、约 t=5s 判死；8s 内必出现断连事件
    let result = wait_no_disconnect_until(&client, 8).await;
    assert!(result.is_err(), "短窗口无刷新竟未判死——对照组失效，保活用例不可信");
}

/// server：accept 一次，记录收到的 Ping 数并回 Pong，其余文本收集；
/// 直到连接关闭返回 `(文本列表, Ping 计数)`
async fn spawn_ping_counting_server() -> (std::net::SocketAddr, tokio::task::JoinHandle<(Vec<String>, usize)>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = accept_async(stream).await.unwrap();
        let mut received = Vec::new();
        let mut pings = 0usize;
        loop {
            match ws.next().await {
                Some(Ok(ServerMsg::Text(t))) => received.push(t.to_string()),
                Some(Ok(ServerMsg::Ping(d))) => {
                    pings += 1;
                    let _ = ws.send(ServerMsg::Pong(d)).await;
                }
                Some(Ok(ServerMsg::Close(_))) | None => break,
                _ => {}
            }
        }
        (received, pings)
    });
    (addr, handle)
}

/// 客户端心跳 Ping 必须真正上环（sender 任务不再吞控制帧）：
/// 短心跳窗口下服务端应能收到多个 Ping——这是「服务端回 Pong 的前提」，
/// 也是 2026-10-16 断线根因的直接回归锁
#[tokio::test]
async fn client_ping_frames_reach_server() {
    let (addr, server) = spawn_ping_counting_server().await;
    let client = WsClient::new(keepalive_config(addr)); // interval=1s
    client.connect().await.unwrap();

    // 覆盖 ≥3 个心跳周期（首 ping 在第 1s 末；稍留余量避免时钟抖动）
    tokio::time::sleep(Duration::from_secs(4)).await;
    client.disconnect().await;

    let (_, pings) = tokio::time::timeout(Duration::from_secs(10), server)
        .await
        .expect("server should finish")
        .unwrap();
    assert!(
        pings >= 2,
        "客户端 Ping 必须到达服务端，实际收到 {pings} 个（sender 吞帧变异未锁住?）"
    );
}
