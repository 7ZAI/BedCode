//! Unit tests for WebSocket server configuration

#[cfg(test)]
mod tests {
    use bedcode_lib::shared::websocket::server::server_config::{IpFilter, WsServerConfig};
    use std::net::IpAddr;

    mod ip_filter {
        use super::*;

        #[test]
        fn test_empty_filter_allows_all() {
            let filter = IpFilter::default();
            let ip: IpAddr = "192.168.1.1".parse().unwrap();
            assert!(filter.is_allowed(&ip));
        }

        #[test]
        fn test_whitelist_only_allows_listed() {
            let mut filter = IpFilter::default();
            filter.whitelist = vec!["192.168.1.10".parse().unwrap()];

            assert!(filter.is_allowed(&"192.168.1.10".parse().unwrap()));
            assert!(!filter.is_allowed(&"192.168.1.20".parse().unwrap()));
            assert!(!filter.is_allowed(&"10.0.0.1".parse().unwrap()));
        }

        #[test]
        fn test_blacklist_blocks_listed() {
            let mut filter = IpFilter::default();
            filter.blacklist = vec!["192.168.1.10".parse().unwrap()];

            assert!(!filter.is_allowed(&"192.168.1.10".parse().unwrap()));
            assert!(filter.is_allowed(&"192.168.1.20".parse().unwrap()));
            assert!(filter.is_allowed(&"10.0.0.1".parse().unwrap()));
        }

        #[test]
        fn test_whitelist_takes_precedence() {
            let mut filter = IpFilter::default();
            filter.whitelist = vec!["192.168.1.10".parse().unwrap()];
            filter.blacklist = vec!["192.168.1.10".parse().unwrap()];

            // Whitelist takes precedence even if also in blacklist
            assert!(filter.is_allowed(&"192.168.1.10".parse().unwrap()));
        }

        #[test]
        fn test_ipv4_and_ipv6() {
            let mut filter = IpFilter::default();
            filter.whitelist = vec![
                "192.168.1.1".parse().unwrap(),
                "::1".parse().unwrap(),
            ];

            assert!(filter.is_allowed(&"192.168.1.1".parse().unwrap()));
            assert!(filter.is_allowed(&"::1".parse().unwrap()));
            assert!(!filter.is_allowed(&"127.0.0.1".parse().unwrap()));
        }
    }

    mod ws_server_config {
        use super::*;

        #[test]
        fn test_default_config() {
            let config = WsServerConfig::default();

            assert_eq!(config.port, 8765);
            assert_eq!(config.max_connections, 0);
            assert_eq!(config.heartbeat_interval_secs, 30);
            assert_eq!(config.heartbeat_timeout_secs, 90);
            assert_eq!(config.message_queue_size, 256);
        }

        #[test]
        fn test_custom_config() {
            let config = WsServerConfig {
                port: 9000,
                max_connections: 100,
                heartbeat_interval_secs: 60,
                heartbeat_timeout_secs: 180,
                message_queue_size: 512,
                ip_filter: IpFilter::default(),
                response_handler: None,
            };

            assert_eq!(config.port, 9000);
            assert_eq!(config.max_connections, 100);
            assert_eq!(config.heartbeat_interval_secs, 60);
            assert_eq!(config.heartbeat_timeout_secs, 180);
            assert_eq!(config.message_queue_size, 512);
        }

        #[test]
        fn test_debug_format() {
            let config = WsServerConfig::default();
            let debug_str = format!("{:?}", config);

            assert!(debug_str.contains("WsServerConfig"));
            assert!(debug_str.contains("port"));
        }
    }
}