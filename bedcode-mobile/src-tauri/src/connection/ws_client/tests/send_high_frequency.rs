//! general — crate 内单元测试（自 bedcode-mobile/src-tauri/src/connection/ws_client.rs 迁出）

use super::*;

use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio_tungstenite::accept_async;

/// 高频发送不丢不乱：2000 条消息顺序与服务端收到顺序一致
#[tokio::test]
async fn send_high_frequency_preserves_order() {
    let (addr, server) = spawn_local_server().await;
    let client = WsClient::new(test_config(addr));
    client.connect().await.unwrap();

    const N: usize = 2000;
    for i in 0..N {
        client.send_text(&format!("msg-{i}")).await.unwrap();
    }

    // disconnect 触发连接关闭，服务端收满后返回
    client.disconnect().await;
    let received = tokio::time::timeout(Duration::from_secs(10), server)
        .await
        .expect("server should finish")
        .unwrap();

    assert_eq!(received.len(), N, "all messages must arrive");
    for (i, msg) in received.iter().enumerate() {
        assert_eq!(msg, &format!("msg-{i}"), "order must be preserved at index {}", i);
    }
}
/// 对端关闭后 send 快速失败（channel 关闭），不永久挂起
#[tokio::test]
async fn send_fails_fast_after_peer_close() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    // 服务端完成握手后（通知主测试）再关闭连接
    let (handshake_done_tx, mut handshake_done_rx) = tokio::sync::mpsc::channel::<()>(1);
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let _ws = accept_async(stream).await.unwrap();
        let _ = handshake_done_tx.send(()).await;
        // 等主测试确认已连接后 drop：模拟对端掉线
        tokio::time::sleep(Duration::from_secs(1)).await;
    });

    let client = WsClient::new(test_config(addr));
    client.connect().await.unwrap();
    // 确认服务端握手完成，随后服务端关闭连接
    let _ = tokio::time::timeout(Duration::from_secs(3), handshake_done_rx.recv())
        .await
        .expect("server handshake must complete");
    server.await.unwrap();

    // 等待 receiver/sender task 感知连接关闭
    tokio::time::sleep(Duration::from_millis(300)).await;

    // 发送应最终失败且不挂起（单条 3s 硬超时兜底断言）
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut last_result = Ok(());
    while Instant::now() < deadline {
        last_result = client.send_text("after-close").await;
        if last_result.is_err() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        last_result.is_err(),
        "send must fail after peer close, got: {:?}",
        last_result
    );
}
/// disconnect 后 send 返回错误（ws_sender 已清空），不静默成功
#[tokio::test]
async fn send_after_disconnect_returns_error() {
    let (addr, _server) = spawn_local_server().await;
    let client = WsClient::new(test_config(addr));
    client.connect().await.unwrap();
    client.disconnect().await;

    let result = client.send_text("after-disconnect").await;
    assert!(result.is_err(), "send after disconnect must fail, got: {:?}", result);
}
/// 重连重建通道：disconnect 后再次 connect，新 channel 可正常收发
#[tokio::test]
async fn disconnect_then_reconnect_rebuilds_channel() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    // 服务端接受两次连接，分别收集各自的消息
    let server = tokio::spawn(async move {
        let mut rounds: Vec<Vec<String>> = Vec::new();
        for _ in 0..2 {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(stream).await.unwrap();
            let mut received = Vec::new();
            loop {
                match ws.next().await {
                    Some(Ok(ServerMsg::Text(t))) => received.push(t.to_string()),
                    Some(Ok(ServerMsg::Ping(d))) => {
                        let _ = ws.send(ServerMsg::Pong(d)).await;
                    }
                    Some(Ok(ServerMsg::Close(_))) | None => break,
                    _ => {}
                }
            }
            rounds.push(received);
        }
        rounds
    });

    let client = WsClient::new(test_config(addr));
    client.connect().await.unwrap();
    client.send_text("round-1").await.unwrap();
    client.disconnect().await;

    // 直接再次 connect：应成功（而非 "Already connected"）
    let reconnect_result = tokio::time::timeout(Duration::from_secs(5), client.connect()).await;
    assert!(
        reconnect_result.is_ok(),
        "reconnect must not hang, got: {:?}",
        reconnect_result
    );
    assert!(
        reconnect_result.unwrap().is_ok(),
        "reconnect must succeed, running flag should be reset by disconnect"
    );

    client.send_text("round-2").await.unwrap();
    client.disconnect().await;

    let rounds = tokio::time::timeout(Duration::from_secs(10), server)
        .await
        .expect("server should finish")
        .unwrap();
    assert_eq!(rounds.len(), 2, "server must see two connections");
    assert_eq!(rounds[0], vec!["round-1".to_string()]);
    assert_eq!(rounds[1], vec!["round-2".to_string()]);
}
