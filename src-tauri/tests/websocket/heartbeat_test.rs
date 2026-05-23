//! Unit tests for heartbeat module

#[cfg(test)]
mod tests {
    use bedcode_lib::shared::websocket::server::heartbeat::{HeartbeatConfig, HeartbeatEvent};
    use bedcode_lib::shared::websocket::client::heartbeat::{
        HeartbeatConfig as ClientHeartbeatConfig,
    };

    mod server_heartbeat_config {
        use super::*;

        #[test]
        fn test_default_config() {
            let config = HeartbeatConfig::default();

            assert_eq!(config.check_interval, std::time::Duration::from_secs(30));
            assert_eq!(config.timeout, std::time::Duration::from_secs(90));
        }

        #[test]
        fn test_custom_config() {
            let config = HeartbeatConfig::new(60, 180);

            assert_eq!(config.check_interval, std::time::Duration::from_secs(60));
            assert_eq!(config.timeout, std::time::Duration::from_secs(180));
        }

        #[test]
        fn test_debug_format() {
            let config = HeartbeatConfig::default();
            let debug_str = format!("{:?}", config);

            assert!(debug_str.contains("HeartbeatConfig"));
            assert!(debug_str.contains("check_interval"));
        }
    }

    mod client_heartbeat_config {
        use super::*;

        #[test]
        fn test_default_config() {
            let config = ClientHeartbeatConfig::default();

            assert_eq!(config.interval, std::time::Duration::from_secs(30));
            assert_eq!(config.timeout, std::time::Duration::from_secs(90));
            assert_eq!(config.max_timeouts, 3);
        }

        #[test]
        fn test_custom_config() {
            let config = ClientHeartbeatConfig {
                interval: std::time::Duration::from_secs(45),
                timeout: std::time::Duration::from_secs(120),
                max_timeouts: 5,
            };

            assert_eq!(config.interval, std::time::Duration::from_secs(45));
            assert_eq!(config.timeout, std::time::Duration::from_secs(120));
            assert_eq!(config.max_timeouts, 5);
        }

        #[test]
        fn test_constructor() {
            let config = ClientHeartbeatConfig::new(60, 180);

            assert_eq!(config.interval, std::time::Duration::from_secs(60));
            assert_eq!(config.timeout, std::time::Duration::from_secs(180));
            assert_eq!(config.max_timeouts, 3);
        }
    }

    mod heartbeat_event {
        use super::*;

        #[test]
        fn test_server_event_variants() {
            let event = HeartbeatEvent::Timeout {
                id: 1,
                addr: "127.0.0.1:8080".parse().unwrap(),
            };
            let debug_str = format!("{:?}", event);
            assert!(debug_str.contains("Timeout"));

            let event = HeartbeatEvent::Authenticated {
                id: 1,
                client_id: "device_001".to_string(),
            };
            let debug_str = format!("{:?}", event);
            assert!(debug_str.contains("Authenticated"));

            let event = HeartbeatEvent::Disconnected {
                id: 1,
                addr: "127.0.0.1:8080".parse().unwrap(),
            };
            let debug_str = format!("{:?}", event);
            assert!(debug_str.contains("Disconnected"));
        }
    }
}