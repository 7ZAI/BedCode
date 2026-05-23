//! Integration tests for WebSocket client - Simplified

#[cfg(test)]
mod tests {
    use futures_util::{SinkExt, StreamExt};
    use tokio::net::TcpListener;
    use tokio_tungstenite::{connect_async, tungstenite::Message};

    async fn get_available_port() -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        addr.port()
    }

    #[tokio::test]
    async fn test_client_connect() {
        let port = get_available_port().await;
        let addr = format!("ws://127.0.0.1:{}", port);

        let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).await.unwrap();

        let server = tokio::spawn(async move {
            if let Ok((stream, _)) = listener.accept().await {
                let _ws = tokio_tungstenite::accept_async(stream).await;
            }
        });

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            connect_async(&addr)
        ).await;

        assert!(result.is_ok());
        let (ws_stream, _) = result.unwrap().unwrap();
        drop(ws_stream);

        server.await.unwrap();
    }

    #[tokio::test]
    async fn test_client_send_message() {
        let port = get_available_port().await;
        let addr = format!("ws://127.0.0.1:{}", port);

        let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).await.unwrap();
        let received = std::sync::Arc::new(std::sync::Mutex::new(None::<String>));
        let received_clone = received.clone();

        let server = tokio::spawn(async move {
            if let Ok((stream, _)) = listener.accept().await {
                let ws = tokio_tungstenite::accept_async(stream).await.unwrap();
                let (_write, mut read) = ws.split();

                if let Some(Ok(Message::Text(text))) = read.next().await {
                    *received_clone.lock().unwrap() = Some(text);
                }
            }
        });

        let (mut ws_stream, _) = connect_async(&addr).await.unwrap();
        ws_stream.send(Message::Text("test message".to_string())).await.unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        server.abort();

        assert_eq!(*received.lock().unwrap(), Some("test message".to_string()));
    }
}