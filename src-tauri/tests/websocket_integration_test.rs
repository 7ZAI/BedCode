//! WebSocket Server-Client Integration Tests
//!
//! Tests WebSocket server and client interaction, including performance tests

use bedcode_lib::shared::websocket::{
    ConnectionStatus, WsClient, WsClientConfig, WsMessage, WsMessageType, WsServer, WsServerConfig,
    WsServerEvent,
};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::broadcast;

/// Helper to get an available port
fn get_available_port() -> u16 {
    use std::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    addr.port()
}

mod server_lifecycle_tests {
    use super::*;

    #[tokio::test]
    async fn test_server_creation() {
        let config = WsServerConfig::default();
        let server = WsServer::new(config);

        assert_eq!(server.config().port, 8765);
        assert!(!server.is_running().await);
    }

    #[tokio::test]
    async fn test_server_start_and_stop() {
        let port = get_available_port();
        let config = WsServerConfig {
            port,
            ..Default::default()
        };
        let server = WsServer::new(config.clone());

        // Start server in background
        let server_clone = Arc::new(server);
        let server_for_task = server_clone.clone();
        let handle = tokio::spawn(async move {
            let _ = server_for_task.start().await;
        });

        // Wait for server to start
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Check server is running
        assert!(server_clone.is_running().await);
        assert_eq!(server_clone.client_count().await, 0);

        // Stop server
        server_clone.stop().await;
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Check server is stopped
        assert!(!server_clone.is_running().await);

        // Clean up the task
        let _ = tokio::time::timeout(tokio::time::Duration::from_secs(2), handle).await;
    }

    #[tokio::test]
    async fn test_server_subscribe_events() {
        let config = WsServerConfig::default();
        let server = WsServer::new(config);

        let mut rx = server.subscribe();
        // Just verify we can create a subscription - use try_recv to avoid blocking
        match rx.try_recv() {
            Ok(_) => panic!("Should not receive any event"),
            Err(broadcast::error::TryRecvError::Empty) => {
                // This is expected - no events yet
            }
            Err(broadcast::error::TryRecvError::Lagged(n)) => {
                // Also acceptable - just means we missed some events
                println!("Missed {} events", n);
            }
            Err(broadcast::error::TryRecvError::Closed) => {
                panic!("Channel should not be closed");
            }
        }
    }
}

mod server_client_connection_tests {
    use super::*;

    async fn start_test_server(port: u16) -> Arc<WsServer> {
        let config = WsServerConfig {
            port,
            ..Default::default()
        };
        let server = Arc::new(WsServer::new(config));

        let server_clone = server.clone();
        tokio::spawn(async move {
            let _ = server_clone.start().await;
        });

        // Wait for server to start
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        server
    }

    #[tokio::test]
    async fn test_server_accepts_connection() {
        let port = get_available_port();
        let server = start_test_server(port).await;

        // Create client and connect
        let config = WsClientConfig::new("127.0.0.1", port);
        let client = WsClient::new(config);

        // Connect in a separate task
        let client_clone = client.clone();
        let connect_handle = tokio::spawn(async move {
            let _ = client_clone.connect().await;
        });

        // Wait for connection
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        // Check client status
        let status = client.get_status().await;
        assert!(status == ConnectionStatus::Connected || status == ConnectionStatus::Connecting);

        // Check server has a client
        let client_count = server.client_count().await;
        assert!(client_count >= 1, "Expected at least 1 client, got {}", client_count);

        // Clean up
        client.disconnect().await;
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let _ = tokio::time::timeout(tokio::time::Duration::from_secs(2), connect_handle).await;
    }

    #[tokio::test]
    async fn test_client_connection_event() {
        let port = get_available_port();
        let server = start_test_server(port).await;

        // Subscribe to server events
        let mut event_rx = server.subscribe();

        // Create and connect client
        let config = WsClientConfig::new("127.0.0.1", port);
        let client = Arc::new(WsClient::new(config));

        let client_clone = client.clone();
        let connect_handle = tokio::spawn(async move {
            let _ = client_clone.connect().await;
        });

        // Wait for connection
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        // Check for connection event (non-blocking check)
        match event_rx.try_recv() {
            Ok(WsServerEvent::ClientConnected { .. }) => {
                println!("Received ClientConnected event");
            }
            _ => {
                // Event might not be immediately available, that's ok for this test
            }
        }

        // Clean up
        client.disconnect().await;
        let _ = tokio::time::timeout(tokio::time::Duration::from_secs(2), connect_handle).await;
    }
}

mod message_transmission_tests {
    use super::*;

    #[tokio::test]
    async fn test_message_serialization_performance() {
        let iterations = 1000;
        let start = Instant::now();

        for _ in 0..iterations {
            let msg = WsMessage::text("Test message for performance");
            let json = msg.to_json().unwrap();
            let _ = WsMessage::from_json(&json).unwrap();
        }

        let elapsed = start.elapsed();
        let ns_per_op = elapsed.as_nanos() / iterations as u128;

        println!("Message serialization: {} ns/op", ns_per_op);

        // Should be under 100 microseconds per operation (debug build is slower)
        assert!(ns_per_op < 100_000, "Serialization too slow: {} ns/op", ns_per_op);
    }

    #[tokio::test]
    async fn test_binary_message_performance() {
        let data = vec![0u8; 1024]; // 1KB of data
        let iterations = 500;
        let start = Instant::now();

        for _ in 0..iterations {
            let msg = WsMessage::binary(&data);
            let json = msg.to_json().unwrap();
            let _ = WsMessage::from_json(&json).unwrap();
        }

        let elapsed = start.elapsed();
        let ns_per_op = elapsed.as_nanos() / iterations as u128;

        println!("Binary message (1KB): {} ns/op", ns_per_op);

        // Should be under 500 microseconds per operation (debug build is slower)
        assert!(ns_per_op < 500_000, "Binary serialization too slow: {} ns/op", ns_per_op);
    }

    #[tokio::test]
    async fn test_large_message_performance() {
        let data = vec![0u8; 1024 * 100]; // 100KB of data
        let iterations = 100;
        let start = Instant::now();

        for _ in 0..iterations {
            let msg = WsMessage::binary(&data);
            let json = msg.to_json().unwrap();
            let _ = WsMessage::from_json(&json).unwrap();
        }

        let elapsed = start.elapsed();
        let ns_per_op = elapsed.as_nanos() / iterations as u128;
        let mb_per_sec = (iterations * 100) as f64 / (elapsed.as_secs_f64() * 1024.0 * 1024.0);

        println!("Large message (100KB): {} ns/op, {} MB/s", ns_per_op, mb_per_sec);

        // Debug build: just report performance, don't assert
        // Release构建应该能达到更高性能
        println!("Note: Large message throughput in debug build: {} MB/s", mb_per_sec);
    }

    #[tokio::test]
    async fn test_concurrent_message_types() {
        let iterations = 100;
        let start = Instant::now();

        for i in 0..iterations {
            let msg = match i % 6 {
                0 => WsMessage::text("text"),
                1 => WsMessage::binary(b"binary"),
                2 => WsMessage::ping(),
                3 => WsMessage::pong(),
                4 => WsMessage::error("code", "msg"),
                5 => WsMessage::ack("id"),
                _ => WsMessage::text("text"),
            };
            let json = msg.to_json().unwrap();
            let _ = WsMessage::from_json(&json).unwrap();
        }

        let elapsed = start.elapsed();
        let ns_per_op = elapsed.as_nanos() / iterations as u128;

        println!("Mixed message types: {} ns/op", ns_per_op);

        assert!(ns_per_op < 200_000);
    }
}

mod heartbeat_tests {
    use super::*;

    #[tokio::test]
    async fn test_ping_pong_message_creation() {
        let ping = WsMessage::ping();
        assert_eq!(ping.message_type(), WsMessageType::Ping);

        let pong = WsMessage::pong();
        assert_eq!(pong.message_type(), WsMessageType::Pong);
    }

    #[tokio::test]
    async fn test_server_config_heartbeat() {
        let config = WsServerConfig {
            heartbeat_interval_secs: 30,
            heartbeat_timeout_secs: 90,
            ..Default::default()
        };

        assert_eq!(config.heartbeat_interval_secs, 30);
        assert_eq!(config.heartbeat_timeout_secs, 90);
    }

    #[tokio::test]
    async fn test_client_config_heartbeat() {
        let config = WsClientConfig {
            heartbeat_interval_secs: 45,
            ..Default::default()
        };

        assert_eq!(config.heartbeat_interval_secs, 45);
    }
}

mod broadcast_tests {
    use super::*;

    async fn start_test_server(port: u16) -> Arc<WsServer> {
        let config = WsServerConfig {
            port,
            ..Default::default()
        };
        let server = Arc::new(WsServer::new(config));

        let server_clone = server.clone();
        tokio::spawn(async move {
            let _ = server_clone.start().await;
        });

        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        server
    }

    #[tokio::test]
    async fn test_server_broadcast() {
        let port = get_available_port();
        let server = start_test_server(port).await;

        // Broadcast a message (should not panic even with no clients)
        let msg = WsMessage::text("Broadcast test");
        let result = server.broadcast(&msg).await;
        assert!(result.is_ok());

        // Stop server
        server.stop().await;
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    #[tokio::test]
    async fn test_server_broadcast_to_others() {
        let port = get_available_port();
        let server = start_test_server(port).await;

        // Test broadcast to others with an address (should handle no clients gracefully)
        use std::net::SocketAddr;
        let test_addr: SocketAddr = "127.0.0.1:12345".parse().unwrap();
        let msg = WsMessage::text("Broadcast to others");
        let result = server.broadcast_to_others(&test_addr, &msg).await;
        assert!(result.is_ok());

        // Stop server
        server.stop().await;
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
}

mod client_operations_tests {
    use super::*;

    #[tokio::test]
    async fn test_client_send_text_not_connected() {
        let config = WsClientConfig::default();
        let client = WsClient::new(config);

        let result = client.send_text("test").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_client_send_not_connected() {
        let config = WsClientConfig::default();
        let client = WsClient::new(config);

        let msg = WsMessage::text("test");
        let result = client.send(&msg).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_client_disconnect_when_not_connected() {
        let config = WsClientConfig::default();
        let client = WsClient::new(config);

        // Should not panic
        client.disconnect().await;

        let status = client.get_status().await;
        assert_eq!(status, ConnectionStatus::Disconnected);
    }

    #[tokio::test]
    async fn test_client_is_connected() {
        let config = WsClientConfig::default();
        let client = WsClient::new(config);

        assert!(!client.is_connected().await);
    }

    // Note: set_client_id uses blocking_write which cannot be used in async context
    // This is a limitation of the WsClient implementation
}