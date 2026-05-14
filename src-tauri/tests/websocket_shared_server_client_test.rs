//! Unit tests for the shared WebSocket server module
//!
//! Tests WsServer and WsServerConfig

#[cfg(test)]
mod ws_server_config_tests {
    use bedcode_lib::shared::websocket::WsServerConfig;

    #[test]
    fn test_ws_server_config_default() {
        let config = WsServerConfig::default();

        assert_eq!(config.port, 8765);
        assert_eq!(config.heartbeat_interval_secs, 30);
        assert_eq!(config.heartbeat_timeout_secs, 90);
        assert_eq!(config.message_queue_size, 256);
    }

    #[test]
    fn test_ws_server_config_custom() {
        let config = WsServerConfig {
            port: 9000,
            heartbeat_interval_secs: 60,
            heartbeat_timeout_secs: 180,
            message_queue_size: 512,
        };

        assert_eq!(config.port, 9000);
        assert_eq!(config.heartbeat_interval_secs, 60);
        assert_eq!(config.heartbeat_timeout_secs, 180);
        assert_eq!(config.message_queue_size, 512);
    }

    #[test]
    fn test_ws_server_config_clone() {
        let config = WsServerConfig::default();
        let cloned = config.clone();

        assert_eq!(cloned.port, config.port);
        assert_eq!(cloned.heartbeat_interval_secs, config.heartbeat_interval_secs);
    }
}

#[cfg(test)]
mod ws_server_event_tests {
    use bedcode_lib::shared::websocket::WsServerEvent;
    use std::net::SocketAddr;

    #[test]
    fn test_ws_server_event_connected() {
        let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let event = WsServerEvent::ClientConnected {
            addr,
            client_id: Some("client-1".to_string()),
        };

        match event {
            WsServerEvent::ClientConnected { addr: a, client_id } => {
                assert_eq!(a, addr);
                assert_eq!(client_id, Some("client-1".to_string()));
            }
            _ => panic!("Expected ClientConnected"),
        }
    }

    #[test]
    fn test_ws_server_event_disconnected() {
        let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let event = WsServerEvent::ClientDisconnected {
            addr,
            client_id: None,
        };

        match event {
            WsServerEvent::ClientDisconnected { addr: a, client_id } => {
                assert_eq!(a, addr);
                assert!(client_id.is_none());
            }
            _ => panic!("Expected ClientDisconnected"),
        }
    }

    #[test]
    fn test_ws_server_event_text_message() {
        let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let event = WsServerEvent::TextMessage {
            addr,
            client_id: Some("client-1".to_string()),
            message_id: Some("msg-123".to_string()),
            content: "Hello".to_string(),
        };

        match event {
            WsServerEvent::TextMessage { content, .. } => {
                assert_eq!(content, "Hello");
            }
            _ => panic!("Expected TextMessage"),
        }
    }

    #[test]
    fn test_ws_server_event_server_closed() {
        let event = WsServerEvent::ServerClosed {
            reason: "Shutdown".to_string(),
        };

        match event {
            WsServerEvent::ServerClosed { reason } => {
                assert_eq!(reason, "Shutdown");
            }
            _ => panic!("Expected ServerClosed"),
        }
    }
}

#[cfg(test)]
mod ws_client_config_tests {
    use bedcode_lib::shared::websocket::WsClientConfig;

    #[test]
    fn test_ws_client_config_default() {
        let config = WsClientConfig::default();

        assert_eq!(config.address, "127.0.0.1");
        assert_eq!(config.port, 8765);
        assert_eq!(config.heartbeat_interval_secs, 30);
        assert_eq!(config.message_queue_size, 256);
        assert_eq!(config.connect_timeout_ms, 10000);
    }

    #[test]
    fn test_ws_client_config_new() {
        let config = WsClientConfig::new("192.168.1.1", 8080);

        assert_eq!(config.address, "192.168.1.1");
        assert_eq!(config.port, 8080);
    }

    #[test]
    fn test_ws_client_config_url() {
        let config = WsClientConfig::new("example.com", 8080);
        assert_eq!(config.url(), "ws://example.com:8080");

        let config2 = WsClientConfig::default();
        assert_eq!(config2.url(), "ws://127.0.0.1:8765");
    }

    #[test]
    fn test_ws_client_config_clone() {
        let config = WsClientConfig::default();
        let cloned = config.clone();

        assert_eq!(cloned.address, config.address);
        assert_eq!(cloned.port, config.port);
    }
}

#[cfg(test)]
mod connection_status_tests {
    use bedcode_lib::shared::websocket::ConnectionStatus;

    #[test]
    fn test_connection_status_variants() {
        let disconnected = ConnectionStatus::Disconnected;
        assert_eq!(format!("{:?}", disconnected), "Disconnected");

        let connecting = ConnectionStatus::Connecting;
        assert_eq!(format!("{:?}", connecting), "Connecting");

        let connected = ConnectionStatus::Connected;
        assert_eq!(format!("{:?}", connected), "Connected");

        let error = ConnectionStatus::Error("test error".to_string());
        assert_eq!(format!("{:?}", error), "Error(\"test error\")");
    }

    #[test]
    fn test_connection_status_equality() {
        assert_eq!(ConnectionStatus::Disconnected, ConnectionStatus::Disconnected);
        assert_eq!(ConnectionStatus::Connected, ConnectionStatus::Connected);
        assert_ne!(ConnectionStatus::Connected, ConnectionStatus::Disconnected);
    }

    #[test]
    fn test_connection_status_error_equality() {
        let error1 = ConnectionStatus::Error("test".to_string());
        let error2 = ConnectionStatus::Error("test".to_string());
        assert_eq!(error1, error2);

        let error3 = ConnectionStatus::Error("different".to_string());
        assert_ne!(error1, error3);
    }
}

#[cfg(test)]
mod ws_client_event_tests {
    use bedcode_lib::shared::websocket::WsClientEvent;

    #[test]
    fn test_ws_client_event_connected() {
        let event = WsClientEvent::Connected;

        match event {
            WsClientEvent::Connected => {}
            _ => panic!("Expected Connected"),
        }
    }

    #[test]
    fn test_ws_client_event_disconnected() {
        let event = WsClientEvent::Disconnected;

        match event {
            WsClientEvent::Disconnected => {}
            _ => panic!("Expected Disconnected"),
        }
    }

    #[test]
    fn test_ws_client_event_text_message() {
        let event = WsClientEvent::TextMessage {
            message_id: Some("msg-123".to_string()),
            content: "Hello".to_string(),
        };

        match event {
            WsClientEvent::TextMessage { content, .. } => {
                assert_eq!(content, "Hello");
            }
            _ => panic!("Expected TextMessage"),
        }
    }

    #[test]
    fn test_ws_client_event_error() {
        let event = WsClientEvent::Error {
            message: "Connection failed".to_string(),
        };

        match event {
            WsClientEvent::Error { message } => {
                assert_eq!(message, "Connection failed");
            }
            _ => panic!("Expected Error"),
        }
    }

    #[test]
    fn test_ws_client_event_server_closed() {
        let event = WsClientEvent::ServerClosed {
            reason: "Server shutdown".to_string(),
        };

        match event {
            WsClientEvent::ServerClosed { reason } => {
                assert_eq!(reason, "Server shutdown");
            }
            _ => panic!("Expected ServerClosed"),
        }
    }
}