//! Unit tests for WebSocket connection manager

#[cfg(test)]
mod tests {
    use bedcode_lib::shared::websocket::server::connection_manager::{
        Connection, ConnectionEvent, ConnectionId, ConnectionManager,
    };
    use bedcode_lib::shared::websocket::server::server_config::{IpFilter, WsServerConfig};
    use std::net::SocketAddr;
    use tokio::sync::mpsc;

    fn create_test_config() -> WsServerConfig {
        WsServerConfig {
            port: 0,
            max_connections: 10,
            heartbeat_interval_secs: 30,
            heartbeat_timeout_secs: 90,
            message_queue_size: 256,
            ip_filter: IpFilter::default(),
            response_handler: None,
        }
    }

    mod connection {
        use super::*;

        #[test]
        fn test_new_connection() {
            let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
            let conn = Connection::new(1, addr);

            assert_eq!(conn.id, 1);
            assert_eq!(conn.addr, addr);
            assert!(conn.client_id.is_none());
            assert!(!conn.authenticated);
            assert!(conn.tags.is_empty());
        }

        #[test]
        fn test_add_tag() {
            let conn = &mut Connection::new(1, "127.0.0.1:8080".parse().unwrap());

            conn.add_tag("group1");
            conn.add_tag("group2");
            conn.add_tag("group1");

            assert_eq!(conn.tags.len(), 2);
            assert!(conn.has_tag("group1"));
            assert!(conn.has_tag("group2"));
            assert!(!conn.has_tag("group3"));
        }

        #[test]
        fn test_remove_tag() {
            let conn = &mut Connection::new(1, "127.0.0.1:8080".parse().unwrap());

            conn.add_tag("group1");
            conn.add_tag("group2");
            conn.remove_tag("group1");

            assert!(!conn.has_tag("group1"));
            assert!(conn.has_tag("group2"));
        }

        #[test]
        fn test_has_tag() {
            let conn = &mut Connection::new(1, "127.0.0.1:8080".parse().unwrap());

            conn.add_tag("important");

            assert!(conn.has_tag("important"));
            assert!(!conn.has_tag("nonexistent"));
        }
    }

    mod connection_manager {
        use super::*;

        #[tokio::test]
        async fn test_new_manager() {
            let config = create_test_config();
            let manager = ConnectionManager::new(&config);

            assert_eq!(manager.count().await, 0);
            assert_eq!(manager.authenticated_count().await, 0);
        }

        #[tokio::test]
        async fn test_register_connection() {
            let config = create_test_config();
            let manager = ConnectionManager::new(&config);

            let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
            let (tx, _rx) = mpsc::channel(256);

            let id = manager.register(addr, tx).await;

            assert!(id.is_some());
            assert_eq!(manager.count().await, 1);
        }

        #[tokio::test]
        async fn test_unregister_connection() {
            let config = create_test_config();
            let manager = ConnectionManager::new(&config);

            let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
            let (tx, _rx) = mpsc::channel(256);

            let id = manager.register(addr, tx).await.unwrap();
            assert_eq!(manager.count().await, 1);

            let removed_addr = manager.unregister(id).await;
            assert!(removed_addr.is_some());
            assert_eq!(manager.count().await, 0);
        }

        #[tokio::test]
        async fn test_get_connection() {
            let config = create_test_config();
            let manager = ConnectionManager::new(&config);

            let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
            let (tx, _rx) = mpsc::channel(256);

            let id = manager.register(addr, tx).await.unwrap();

            let conn = manager.get(id).await;
            assert!(conn.is_some());
            assert_eq!(conn.unwrap().id, id);
        }

        #[tokio::test]
        async fn test_get_connection_not_found() {
            let config = create_test_config();
            let manager = ConnectionManager::new(&config);

            let conn = manager.get(999).await;
            assert!(conn.is_none());
        }

        #[tokio::test]
        async fn test_set_client_id() {
            let config = create_test_config();
            let manager = ConnectionManager::new(&config);

            let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
            let (tx, _rx) = mpsc::channel(256);

            let id = manager.register(addr, tx).await.unwrap();

            let conn = manager.get(id).await.unwrap();
            assert!(!conn.authenticated);

            manager.set_client_id(id, Some("device_001".to_string())).await;

            let conn = manager.get(id).await.unwrap();
            assert!(conn.authenticated);
            assert_eq!(conn.client_id, Some("device_001".to_string()));
        }

        #[tokio::test]
        async fn test_add_remove_tag() {
            let config = create_test_config();
            let manager = ConnectionManager::new(&config);

            let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
            let (tx, _rx) = mpsc::channel(256);

            let id = manager.register(addr, tx).await.unwrap();

            manager.add_tag(id, "mobile").await;
            let conn = manager.get(id).await.unwrap();
            assert!(conn.has_tag("mobile"));

            manager.remove_tag(id, "mobile").await;
            let conn = manager.get(id).await.unwrap();
            assert!(!conn.has_tag("mobile"));
        }

        #[tokio::test]
        async fn test_ids_by_tag() {
            let config = create_test_config();
            let manager = ConnectionManager::new(&config);

            let (tx1, _rx) = mpsc::channel(256);
            let (tx2, _rx) = mpsc::channel(256);

            let id1 = manager.register("127.0.0.1:8081".parse().unwrap(), tx1).await.unwrap();
            let id2 = manager.register("127.0.0.1:8082".parse().unwrap(), tx2).await.unwrap();

            manager.add_tag(id1, "mobile").await;
            manager.add_tag(id2, "mobile").await;

            let ids = manager.ids_by_tag("mobile").await;
            assert_eq!(ids.len(), 2);
            assert!(ids.contains(&id1));
            assert!(ids.contains(&id2));
        }

        #[tokio::test]
        async fn test_all_ids() {
            let config = create_test_config();
            let manager = ConnectionManager::new(&config);

            let (tx1, _rx) = mpsc::channel(256);
            let (tx2, _rx) = mpsc::channel(256);

            let id1 = manager.register("127.0.0.1:8081".parse().unwrap(), tx1).await.unwrap();
            let id2 = manager.register("127.0.0.1:8082".parse().unwrap(), tx2).await.unwrap();

            let all_ids = manager.all_ids().await;
            assert_eq!(all_ids.len(), 2);
            assert!(all_ids.contains(&id1));
            assert!(all_ids.contains(&id2));
        }

        #[tokio::test]
        async fn test_authenticated_ids() {
            let config = create_test_config();
            let manager = ConnectionManager::new(&config);

            let (tx1, _rx) = mpsc::channel(256);
            let (tx2, _rx) = mpsc::channel(256);

            let id1 = manager.register("127.0.0.1:8081".parse().unwrap(), tx1).await.unwrap();
            let _id2 = manager.register("127.0.0.1:8082".parse().unwrap(), tx2).await.unwrap();

            manager.set_client_id(id1, Some("device_001".to_string())).await;

            let authenticated = manager.authenticated_ids().await;
            assert_eq!(authenticated.len(), 1);
            assert!(authenticated.contains(&id1));
        }

        #[tokio::test]
        async fn test_ip_filter_blocks_connection() {
            let mut config = create_test_config();
            config.ip_filter.blacklist = vec!["127.0.0.1".parse().unwrap()];

            let manager = ConnectionManager::new(&config);

            let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
            let (tx, _rx) = mpsc::channel(256);

            let id = manager.register(addr, tx).await;
            assert!(id.is_none());
            assert_eq!(manager.count().await, 0);
        }

        #[tokio::test]
        async fn test_max_connections() {
            let mut config = create_test_config();
            config.max_connections = 2;

            let manager = ConnectionManager::new(&config);

            let (tx1, _rx) = mpsc::channel(256);
            let (tx2, _rx) = mpsc::channel(256);
            let (tx3, _rx) = mpsc::channel(256);

            let id1 = manager.register("127.0.0.1:8081".parse().unwrap(), tx1).await;
            let id2 = manager.register("127.0.0.1:8082".parse().unwrap(), tx2).await;
            let id3 = manager.register("127.0.0.1:8083".parse().unwrap(), tx3).await;

            assert!(id1.is_some());
            assert!(id2.is_some());
            assert!(id3.is_none());
            assert_eq!(manager.count().await, 2);
        }

        #[tokio::test]
        async fn test_broadcast() {
            let config = create_test_config();
            let manager = ConnectionManager::new(&config);

            let (tx1, mut rx1) = mpsc::channel(256);
            let (tx2, mut rx2) = mpsc::channel(256);

            manager.register("127.0.0.1:8081".parse().unwrap(), tx1).await;
            manager.register("127.0.0.1:8082".parse().unwrap(), tx2).await;

            let msg = tokio_tungstenite::tungstenite::Message::Text("hello".to_string());
            manager.broadcast(&msg).await;

            let msg1 = tokio::time::timeout(std::time::Duration::from_millis(100), rx1.recv()).await;
            let msg2 = tokio::time::timeout(std::time::Duration::from_millis(100), rx2.recv()).await;

            assert!(msg1.is_ok());
            assert!(msg2.is_ok());
        }

        #[tokio::test]
        async fn test_connection_event_connected() {
            let config = create_test_config();
            let manager = ConnectionManager::new(&config);

            let mut rx = manager.subscribe();

            let (tx, _rx) = mpsc::channel(256);
            let _id = manager.register("127.0.0.1:8080".parse().unwrap(), tx).await;

            let event = rx.recv().await.unwrap();
            match event {
                ConnectionEvent::Connected { id: _, addr } => {
                    assert_eq!(addr.to_string(), "127.0.0.1:8080");
                }
                _ => panic!("Expected Connected event"),
            }
        }

        #[tokio::test]
        async fn test_connection_event_disconnected() {
            let config = create_test_config();
            let manager = ConnectionManager::new(&config);

            let (tx, _rx) = mpsc::channel(256);
            let id = manager.register("127.0.0.1:8080".parse().unwrap(), tx).await.unwrap();

            let mut rx = manager.subscribe();
            manager.unregister(id).await;

            let event = rx.recv().await.unwrap();
            match event {
                ConnectionEvent::Disconnected { id: event_id, .. } => {
                    assert_eq!(event_id, id);
                }
                _ => panic!("Expected Disconnected event"),
            }
        }
    }
}