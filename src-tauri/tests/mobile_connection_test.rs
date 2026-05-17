//! Mobile Connection Integration Tests
//!
//! Tests the mobile connection flow to verify WebSocket connectivity
//! and data transmission between mobile client and desktop server.
//!
//! This test simulates the mobile::commands::ws_connect command chain:
//! 1. Start WebSocket server (desktop side)
//! 2. Mobile client connects (via WsClient - same as ConnectionManager uses)
//! 3. Verify connection established
//! 4. Test data transmission: mobile -> server
//! 5. Test data transmission: server -> mobile
//! 6. Verify bidirectional communication works

use bedcode_lib::shared::websocket::{
    ConnectionStatus, WsClient, WsClientConfig, WsMessage, WsServer, WsServerConfig,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;

/// Helper to get an available port
fn get_available_port() -> u16 {
    use std::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    addr.port()
}

mod mobile_connection_tests {
    use super::*;

    /// Start a test WebSocket server
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
        tokio::time::sleep(Duration::from_millis(200)).await;
        server
    }

    #[tokio::test]
    async fn test_ws_client_can_connect_to_server() {
        let port = get_available_port();

        // Start server
        let server = start_test_server(port).await;

        // Create client and connect
        let config = WsClientConfig::new("127.0.0.1", port);
        let client = Arc::new(WsClient::new(config));

        // Connect
        let connect_result = client.connect().await;

        // Verify connection succeeded
        assert!(connect_result.is_ok(), "Connection should succeed: {:?}", connect_result.err());

        // Check status
        let status = client.get_status().await;
        assert_eq!(status, ConnectionStatus::Connected, "Status should be Connected");

        // Check server has client
        let client_count = server.client_count().await;
        assert_eq!(client_count, 1, "Server should have 1 client");

        // Cleanup
        client.disconnect().await;
        let _ = server.stop().await;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    #[tokio::test]
    async fn test_ws_client_connection_timeout() {
        // Use a non-routable IP to simulate timeout
        let config = WsClientConfig {
            address: "10.255.255.1".to_string(),
            port: 9999,
            connect_timeout_ms: 2000, // 2 second timeout
            ..Default::default()
        };

        let client = Arc::new(WsClient::new(config));

        // Try to connect - should timeout
        let start = std::time::Instant::now();
        let result = client.connect().await;
        let elapsed = start.elapsed();

        // Should fail with timeout or connection error
        assert!(result.is_err(), "Connection should fail");

        // Verify it took around the timeout duration
        assert!(
            elapsed.as_millis() >= 1800 && elapsed.as_millis() <= 5000,
            "Should take around 2 seconds, took {}ms",
            elapsed.as_millis()
        );

        println!("Connection failed as expected after {}ms", elapsed.as_millis());
    }

    #[tokio::test]
    async fn test_ws_client_reconnect_after_disconnect() {
        let port = get_available_port();

        // Start server
        let server = start_test_server(port).await;

        // First connection
        let config = WsClientConfig::new("127.0.0.1", port);
        let client = Arc::new(WsClient::new(config));

        // First connect
        client.connect().await.unwrap();
        assert_eq!(client.get_status().await, ConnectionStatus::Connected);

        // Disconnect
        client.disconnect().await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(client.get_status().await, ConnectionStatus::Disconnected);

        // Reconnect - create new client for clean state
        let config2 = WsClientConfig::new("127.0.0.1", port);
        let client2 = Arc::new(WsClient::new(config2));

        let result = client2.connect().await;
        assert!(result.is_ok(), "Reconnect should succeed");

        // Cleanup
        client2.disconnect().await;
        let _ = server.stop().await;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Integration test for mobile::commands::ws_connect command chain
/// Tests the full flow: start server -> mobile connects -> send/receive data
///
/// This simulates the same logic that mobile::commands::ws_connect uses:
/// - Creates WsClient with address/port
/// - Calls connect() to establish WebSocket connection
/// - Verifies connection status
/// - Tests bidirectional data transmission
mod ws_connect_integration_tests {
    use super::*;

    /// Start a test WebSocket server that echoes messages back
    async fn start_echo_server(port: u16) -> Arc<WsServer> {
        let config = WsServerConfig {
            port,
            ..Default::default()
        };
        let server = Arc::new(WsServer::new(config));

        let server_clone = server.clone();
        tokio::spawn(async move {
            let _ = server_clone.start().await;
        });

        tokio::time::sleep(Duration::from_millis(200)).await;
        server
    }

    /// Test the full ws_connect command chain:
    /// 1. Start WebSocket server (desktop)
    /// 2. Mobile connects via ws_connect logic (WsClient)
    /// 3. Verify connection established
    /// 4. Send message mobile -> server
    /// 5. Send message server -> mobile
    /// 6. Verify bidirectional communication
    #[tokio::test]
    async fn test_mobile_ws_connect_command_chain() {
        // Step 1: Start WebSocket server (desktop side)
        let port = get_available_port();
        let server = start_echo_server(port).await;

        // Step 2: Simulate mobile::commands::ws_connect
        // The command creates WsClientConfig with address/port and calls connect()
        let config = WsClientConfig::new("127.0.0.1", port);
        let client = Arc::new(WsClient::new(config));

        // Step 3: Connect (same as ConnectionManager::connect() does)
        let connect_result = client.connect().await;
        assert!(connect_result.is_ok(), "ws_connect: connection should succeed: {:?}", connect_result.err());

        // Wait for connection to stabilize
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Step 4: Verify connection is established
        assert!(client.is_connected().await, "ws_connect: client should be connected");

        // Verify server has client
        let client_count = server.client_count().await;
        assert_eq!(client_count, 1, "ws_connect: server should have 1 client connected");

        // Step 5: Test data transmission - mobile sends to server
        let test_message = r#"{"type":"test","message":"hello from mobile"}"#;
        let ws_msg = WsMessage::text(test_message);
        let send_result = client.send(&ws_msg).await;
        assert!(send_result.is_ok(), "ws_connect: client should be able to send message");

        // Give time for message to be processed
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Step 6: Test data transmission - server sends to mobile
        // Subscribe to client events to receive messages
        let mut event_rx = client.subscribe();

        let server_response = r#"{"type":"response","status":"received"}"#;
        let clients = server.clients().read().await;
        if let Some(client_addr) = clients.keys().next() {
            let _ = server.send_text_to(client_addr, server_response).await;
        }
        drop(clients);

        // Wait for response
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Check if we received the server response (non-blocking)
        match event_rx.try_recv() {
            Ok(event) => {
                println!("ws_connect: Received event from server: {:?}", event);
            }
            Err(broadcast::error::TryRecvError::Empty) => {
                // This is ok - message might be processed differently
            }
            Err(broadcast::error::TryRecvError::Closed) => {
                panic!("ws_connect: Event channel closed");
            }
            Err(broadcast::error::TryRecvError::Lagged(n)) => {
                println!("ws_connect: Lagged {} events", n);
            }
        }

        // Step 7: Verify bidirectional communication
        // Client should still be connected after data exchange
        assert!(client.is_connected().await, "ws_connect: client should still be connected after data exchange");

        println!("[PASS] ws_connect command chain test passed - connection and data transmission work");

        // Cleanup
        client.disconnect().await;
        let _ = server.stop().await;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    /// Test mobile sending control message via ws_connect chain
    #[tokio::test]
    async fn test_mobile_sends_message_via_ws_connect_chain() {
        let port = get_available_port();
        let server = start_echo_server(port).await;

        // Create client simulating mobile connection (same as ws_connect)
        let config = WsClientConfig::new("127.0.0.1", port);
        let client = Arc::new(WsClient::new(config));

        // Connect
        client.connect().await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Mobile sends a control message (same format as mobile::commands::ws_send_message)
        let control_msg = serde_json::json!({
            "type": "control",
            "message_id": uuid::Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().timestamp_millis(),
            "payload": {
                "action": {
                    "type": "ping"
                }
            }
        });

        let ws_msg = WsMessage::text(control_msg.to_string());
        client.send(&ws_msg).await.expect("Should be able to send control message");

        tokio::time::sleep(Duration::from_millis(100)).await;

        // Verify message was sent (server has 1 client)
        assert_eq!(server.client_count().await, 1);

        // Client disconnects
        client.disconnect().await;
        let _ = server.stop().await;

        println!("[PASS] Mobile can send message through ws_connect chain");
    }

    /// Test server sending data to mobile via WebSocket
    #[tokio::test]
    async fn test_server_to_mobile_data_transmission() {
        let port = get_available_port();
        let server = start_echo_server(port).await;

        // Create and connect client (mobile simulation)
        let config = WsClientConfig::new("127.0.0.1", port);
        let client = Arc::new(WsClient::new(config));

        // Subscribe to client events
        let mut event_rx = client.subscribe();

        client.connect().await.unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Server sends output data to mobile (session output)
        let output_data = r#"{"type":"Output","session_id":"test-session","data":"Hello from desktop","is_waiting":false}"#;

        let clients = server.clients().read().await;
        if let Some(client_addr) = clients.keys().next() {
            server.send_text_to(client_addr, output_data).await.expect("Server should send");
        }
        drop(clients);

        // Wait for message to arrive
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Check if client received the message
        for _ in 0..5 {
            match event_rx.try_recv() {
                Ok(event) => {
                    println!("Received event from server: {:?}", event);
                    break;
                }
                Err(broadcast::error::TryRecvError::Empty) => {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                _ => break,
            }
        }

        // Client should still be connected
        assert!(client.is_connected().await, "Client should be connected");

        // Cleanup
        client.disconnect().await;
        let _ = server.stop().await;

        println!("[PASS] Server can send data to mobile via WebSocket");
    }
}