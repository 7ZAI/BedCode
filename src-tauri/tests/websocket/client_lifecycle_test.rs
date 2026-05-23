//! Unit tests for WebSocket client lifecycle

#[cfg(test)]
mod tests {
    use bedcode_lib::shared::websocket::client::lifecycle::{ConnectionStatus, LifecycleEvent, LifecycleManager};
    use std::sync::Arc;

    fn create_lifecycle_manager() -> Arc<LifecycleManager> {
        LifecycleManager::new()
    }

    mod connection_status {
        use super::*;

        #[test]
        fn test_status_variants() {
            let status = ConnectionStatus::Disconnected;
            assert_eq!(format!("{:?}", status), "Disconnected");

            let status = ConnectionStatus::Connecting;
            assert_eq!(format!("{:?}", status), "Connecting");

            let status = ConnectionStatus::Connected;
            assert_eq!(format!("{:?}", status), "Connected");

            let status = ConnectionStatus::Paired;
            assert_eq!(format!("{:?}", status), "Paired");

            let status = ConnectionStatus::Error("test error".to_string());
            assert!(format!("{:?}", status).contains("Error"));
        }

        #[test]
        fn test_status_equality() {
            assert_eq!(ConnectionStatus::Disconnected, ConnectionStatus::Disconnected);
            assert_eq!(ConnectionStatus::Connecting, ConnectionStatus::Connecting);
            assert_ne!(ConnectionStatus::Connected, ConnectionStatus::Disconnected);
        }
    }

    mod lifecycle_manager {
        use super::*;

        #[tokio::test]
        async fn test_new_manager() {
            let manager = create_lifecycle_manager();
            let status = manager.get_status().await;

            assert_eq!(status, ConnectionStatus::Disconnected);
        }

        #[tokio::test]
        async fn test_set_status_disconnected_to_connected() {
            let manager = create_lifecycle_manager();
            let mut rx = manager.subscribe();

            manager.set_status(ConnectionStatus::Connected).await;
            let status = manager.get_status().await;

            assert_eq!(status, ConnectionStatus::Connected);

            let event = rx.recv().await.unwrap();
            match event {
                LifecycleEvent::Connected => {}
                _ => panic!("Expected Connected event"),
            }
        }

        #[tokio::test]
        async fn test_set_status_to_paired() {
            let manager = create_lifecycle_manager();
            let mut rx = manager.subscribe();

            manager.set_status(ConnectionStatus::Paired).await;
            let status = manager.get_status().await;

            assert_eq!(status, ConnectionStatus::Paired);

            let event = rx.recv().await.unwrap();
            match event {
                LifecycleEvent::Paired => {}
                _ => panic!("Expected Paired event"),
            }
        }

        #[tokio::test]
        async fn test_set_status_to_error() {
            let manager = create_lifecycle_manager();

            let error_msg = "Connection refused".to_string();
            manager.set_status(ConnectionStatus::Error(error_msg.clone())).await;

            let status = manager.get_status().await;
            match status {
                ConnectionStatus::Error(msg) => {
                    assert_eq!(msg, error_msg);
                }
                _ => panic!("Expected Error status"),
            }
        }

        #[tokio::test]
        async fn test_set_client_id() {
            let manager = create_lifecycle_manager();

            manager.set_client_id("device_001".to_string()).await;
            let client_id = manager.get_client_id().await;

            assert_eq!(client_id, Some("device_001".to_string()));
        }

        #[tokio::test]
        async fn test_subscribe_multiple_receivers() {
            let manager = create_lifecycle_manager();

            let mut rx1 = manager.subscribe();
            let mut rx2 = manager.subscribe();

            manager.set_status(ConnectionStatus::Connected).await;

            let event1 = rx1.recv().await.unwrap();
            let event2 = rx2.recv().await.unwrap();

            match (event1, event2) {
                (LifecycleEvent::Connected, LifecycleEvent::Connected) => {}
                _ => panic!("Expected Connected events"),
            }
        }

        #[tokio::test]
        async fn test_broadcast_dropped_when_no_receiver() {
            let manager = create_lifecycle_manager();

            manager.set_status(ConnectionStatus::Connected).await;

            let status = manager.get_status().await;
            assert_eq!(status, ConnectionStatus::Connected);
        }
    }
}