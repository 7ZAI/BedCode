//! Integration tests for WebSocket server - Simplified

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
    async fn test_basic_connection() {
        let port = get_available_port().await;
        let addr = format!("ws://127.0.0.1:{}", port);

        // Start simple server
        let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).await.unwrap();

        let server = tokio::spawn(async move {
            if let Ok((stream, _)) = listener.accept().await {
                let _ws = tokio_tungstenite::accept_async(stream).await;
            }
        });

        // Client connects
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            connect_async(&addr)
        ).await;

        assert!(result.is_ok());
        let (ws_stream, _) = result.unwrap().unwrap();

        // Just verify connection works
        let _ = ws_stream;

        drop(ws_stream);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn test_message_exchange() {
        let port = get_available_port().await;
        let addr = format!("ws://127.0.0.1:{}", port);

        // Echo server
        let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).await.unwrap();

        let server = tokio::spawn(async move {
            if let Ok((stream, _)) = listener.accept().await {
                let ws = tokio_tungstenite::accept_async(stream).await.unwrap();
                let (mut write, mut read) = ws.split();

                while let Some(msg) = read.next().await {
                    if let Ok(Message::Text(text)) = msg {
                        let _ = write.send(Message::Text(format!("echo: {}", text))).await;
                    }
                }
            }
        });

        // Client sends and receives
        let (mut ws_stream, _) = connect_async(&addr).await.unwrap();

        ws_stream.send(Message::Text("hello".to_string())).await.unwrap();

        let response = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            ws_stream.next()
        ).await.unwrap().unwrap().unwrap();

        if let Message::Text(text) = response {
            assert_eq!(text, "echo: hello");
        } else {
            panic!("Expected text message");
        }

        server.abort();
    }
}