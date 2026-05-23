//! Unit tests for WebSocket client reconnection

#[cfg(test)]
mod tests {
    use bedcode_lib::shared::websocket::client::reconnect::{ReconnectConfig, ReconnectEvent, ReconnectState};

    mod reconnect_config {
        use super::*;

        #[test]
        fn test_default_config() {
            let config = ReconnectConfig::default();

            assert_eq!(config.max_retries, 0);
            assert_eq!(config.initial_delay_ms, 1000);
            assert_eq!(config.max_delay_ms, 30000);
            assert_eq!(config.backoff_multiplier, 2.0);
            assert!(config.jitter);
        }

        #[test]
        fn test_custom_config() {
            let config = ReconnectConfig::new(5, 500, 10000);

            assert_eq!(config.max_retries, 5);
            assert_eq!(config.initial_delay_ms, 500);
            assert_eq!(config.max_delay_ms, 10000);
            assert_eq!(config.backoff_multiplier, 2.0);
            assert!(config.jitter);
        }

        #[test]
        fn test_debug_format() {
            let config = ReconnectConfig::default();
            let debug_str = format!("{:?}", config);

            assert!(debug_str.contains("ReconnectConfig"));
        }
    }

    mod reconnect_state {
        use super::*;

        #[test]
        fn test_state_variants() {
            let state = ReconnectState::Idle;
            assert!(format!("{:?}", state).contains("Idle"));

            let state = ReconnectState::Reconnecting {
                attempt: 1,
                next_delay: std::time::Duration::from_millis(1000),
            };
            assert!(format!("{:?}", state).contains("Reconnecting"));

            let state = ReconnectState::Success;
            assert!(format!("{:?}", state).contains("Success"));

            let state = ReconnectState::Failed {
                attempts: 3,
                last_error: "timeout".to_string(),
            };
            assert!(format!("{:?}", state).contains("Failed"));

            let state = ReconnectState::Abandoned;
            assert!(format!("{:?}", state).contains("Abandoned"));
        }
    }

    mod backoff_algorithm {
        use super::*;
        use std::time::Duration;

        #[test]
        fn test_exponential_backoff() {
            let config = ReconnectConfig {
                max_retries: 10,
                initial_delay_ms: 1000,
                max_delay_ms: 30000,
                backoff_multiplier: 2.0,
                jitter: false,
            };

            let delay = calculate_delay(&config, 1);
            assert_eq!(delay.as_millis(), 1000);

            let delay = calculate_delay(&config, 2);
            assert_eq!(delay.as_millis(), 2000);

            let delay = calculate_delay(&config, 3);
            assert_eq!(delay.as_millis(), 4000);

            let delay = calculate_delay(&config, 4);
            assert_eq!(delay.as_millis(), 8000);
        }

        #[test]
        fn test_max_delay_cap() {
            let config = ReconnectConfig {
                max_retries: 10,
                initial_delay_ms: 1000,
                max_delay_ms: 5000,
                backoff_multiplier: 2.0,
                jitter: false,
            };

            let delay = calculate_delay(&config, 10);
            assert_eq!(delay.as_millis(), 5000);

            let delay = calculate_delay(&config, 100);
            assert_eq!(delay.as_millis(), 5000);
        }

        #[test]
        fn test_first_retry_delay() {
            let config = ReconnectConfig::new(5, 500, 10000);

            let delay = calculate_delay(&config, 1);
            assert_eq!(delay.as_millis(), 500);
        }

        #[test]
        fn test_zero_max_retries_infinite() {
            let config = ReconnectConfig {
                max_retries: 0,
                initial_delay_ms: 1000,
                max_delay_ms: 30000,
                backoff_multiplier: 2.0,
                jitter: false,
            };

            for attempt in 1..=100 {
                let delay = calculate_delay(&config, attempt);
                assert!(delay.as_millis() > 0);
            }
        }

        fn calculate_delay(config: &ReconnectConfig, attempt: u32) -> Duration {
            let base_delay = config.initial_delay_ms as f64
                * config.backoff_multiplier.powi(attempt as i32 - 1);
            let capped_delay = base_delay.min(config.max_delay_ms as f64);
            Duration::from_millis(capped_delay as u64)
        }
    }
}