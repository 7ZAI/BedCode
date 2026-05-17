//! ConnectionManager Unit Tests
//!
//! Tests for the simplified ConnectionManager that delegates to WsClient
//!
//! Run with: cargo test --test connection_manager_test --target x86_64-pc-windows-msvc

#![cfg(any(target_os = "android", target_os = "ios"))]

use bedcode_lib::mobile::ConnectionManager;
use bedcode_lib::shared::websocket::{ConnectionStatus, WsMessage, WsServer, WsServerConfig};
use std::sync::Arc;
use std::time::Duration;

/// Helper to get an available port
fn get_available_port() -> u16 {
    use std::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    addr.port()
}

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

mod connection_manager_tests {
    use super::*;

    #[tokio::test]
    async fn test_connection_manager_creation() {
        let cm = ConnectionManager::new();

        // Should start in disconnected state
        let status = cm.get_status().await;
        assert_eq!(status, ConnectionStatus::Disconnected);

        // Should not be connected
        assert!(!cm.is_connected().await);

        println!("[PASS] ConnectionManager creation works");
    }

    #[tokio::test]
    async fn test_connection_manager_get_status() {
        let cm = ConnectionManager::new();

        // Initial status should be Disconnected
        let status = cm.get_status().await;
        assert_eq!(status, ConnectionStatus::Disconnected);

        println!("[PASS] ConnectionManager get_status works");
    }

    #[tokio::test]
    async fn test_connection_manager_is_connected() {
        let cm = ConnectionManager::new();

        // Initially not connected
        assert!(!cm.is_connected().await);

        println!("[PASS] ConnectionManager is_connected works");
    }

    #[tokio::test]
    async fn test_connection_manager_disconnect_when_not_connected() {
        let cm = ConnectionManager::new();

        // Should not panic when disconnecting without connection
        cm.disconnect().await;

        // Status should still be disconnected
        let status = cm.get_status().await;
        assert_eq!(status, ConnectionStatus::Disconnected);

        println!("[PASS] ConnectionManager disconnect when not connected works");
    }

    #[tokio::test]
    async fn test_connection_manager_subscribe() {
        let cm = ConnectionManager::new();

        // Should be able to subscribe to events
        let _rx = cm.subscribe();

        println!("[PASS] ConnectionManager subscribe works");
    }

    #[tokio::test]
    async fn test_connection_manager_set_paired() {
        let cm = ConnectionManager::new();

        // set_paired should not panic even without a client
        cm.set_paired().await;

        // With no client, status should still be disconnected
        let status = cm.get_status().await;
        assert_eq!(status, ConnectionStatus::Disconnected);

        println!("[PASS] ConnectionManager set_paired when not connected works");
    }

    #[tokio::test]
    async fn test_connection_manager_connect_to_server() {
        let port = get_available_port();

        // Start server first
        let server = start_test_server(port).await;

        // Create connection manager
        let cm = ConnectionManager::new();

        // Connect to the server (using test-friendly method)
        let result = cm.connect_without_emit(
            "127.0.0.1".to_string(),
            port,
            None,
        ).await;

        // Connection should succeed
        assert!(result.is_ok(), "Connection should succeed: {:?}", result.err());

        // Status should be Connected
        let status = cm.get_status().await;
        assert_eq!(status, ConnectionStatus::Connected, "Status should be Connected");

        // Should be connected
        assert!(cm.is_connected().await, "Should be connected");

        // Cleanup
        cm.disconnect().await;
        let _ = server.stop().await;
        tokio::time::sleep(Duration::from_millis(100)).await;

        println!("[PASS] ConnectionManager can connect to server");
    }

    #[tokio::test]
    async fn test_connection_manager_reconnect() {
        let port = get_available_port();

        // Start server
        let server = start_test_server(port).await;

        // First connection
        let cm = ConnectionManager::new();
        cm.connect_without_emit(
            "127.0.0.1".to_string(),
            port,
            None,
        ).await.unwrap();

        assert!(cm.is_connected().await);

        // Disconnect
        cm.disconnect().await;
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Should be disconnected
        assert!(!cm.is_connected().await);

        // Reconnect with new ConnectionManager instance
        let cm2 = ConnectionManager::new();
        let result = cm2.connect_without_emit(
            "127.0.0.1".to_string(),
            port,
            None,
        ).await;

        assert!(result.is_ok(), "Reconnect should succeed");

        // Cleanup
        cm2.disconnect().await;
        let _ = server.stop().await;
        tokio::time::sleep(Duration::from_millis(100)).await;

        println!("[PASS] ConnectionManager can reconnect after disconnect");
    }

    #[tokio::test]
    async fn test_connection_manager_send_message() {
        let port = get_available_port();

        // Start server
        let server = start_test_server(port).await;

        // Create and connect
        let cm = ConnectionManager::new();
        cm.connect_without_emit(
            "127.0.0.1".to_string(),
            port,
            None,
        ).await.unwrap();

        // Send a text message
        let msg = WsMessage::text(r#"{"type":"test","content":"hello"}"#);
        let result = cm.send(&msg).await;

        // Should succeed (message sent to server)
        assert!(result.is_ok(), "Should be able to send message: {:?}", result.err());

        // Cleanup
        cm.disconnect().await;
        let _ = server.stop().await;
        tokio::time::sleep(Duration::from_millis(100)).await;

        println!("[PASS] ConnectionManager can send messages");
    }

    #[tokio::test]
    async fn test_connection_manager_get_target() {
        let cm = ConnectionManager::new();

        // Initially no target
        let target = cm.get_target().await;
        assert!(target.is_none());

        // After connecting, target should be set
        let port = get_available_port();
        let server = start_test_server(port).await;

        cm.connect_without_emit(
            "127.0.0.1".to_string(),
            port,
            Some("test-device".to_string()),
        ).await.unwrap();

        let target = cm.get_target().await;
        assert!(target.is_some());
        assert_eq!(target.unwrap().name, Some("test-device".to_string()));

        // Cleanup
        cm.disconnect().await;
        let _ = server.stop().await;

        println!("[PASS] ConnectionManager get_target works");
    }
}