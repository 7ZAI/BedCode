//! Tests for WebSocket handlers - QR Token and Pairing Service
//!
//! 测试重构后的 WebSocket handlers 模块中的 QR 码和配对服务

use bedcode_lib::auth::{PairingService, QrTokenManager};

mod qr_token_tests {
    use bedcode_lib::auth::QrTokenManager;

    #[tokio::test]
    async fn test_qr_token_generate_and_verify() {
        let manager = QrTokenManager::new();

        // Generate token (300 seconds TTL)
        let token_str = manager.generate(300).await;
        assert!(!token_str.is_empty());

        // Verify valid token
        let result = manager.verify(&token_str).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_qr_token_verify_invalid() {
        let manager = QrTokenManager::new();

        // Generate a token first
        let _token = manager.generate(300).await;

        // Verify invalid token
        let result = manager.verify("invalid_token").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_qr_token_generate_multiple() {
        let manager = QrTokenManager::new();

        // Generate multiple tokens
        let token1 = manager.generate(300).await;
        let token2 = manager.generate(300).await;

        // Tokens should be different
        assert_ne!(token1, token2);
    }

    #[tokio::test]
    async fn test_qr_token_verify_after_new_generation() {
        let manager = QrTokenManager::new();

        // Generate and verify first token
        let token1 = manager.generate(300).await;
        let result1 = manager.verify(&token1).await;
        assert!(result1.is_ok());

        // Generate new token, old token should still work
        let _token2 = manager.generate(300).await;
        let result2 = manager.verify(&token1).await;
        // Old token should still be valid (no expiry for test)
        assert!(result2.is_ok());
    }

    #[tokio::test]
    async fn test_qr_token_empty_string() {
        let manager = QrTokenManager::new();

        // Generate a token first
        let _token = manager.generate(300).await;

        // Verify empty string
        let result = manager.verify("").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_qr_token_wrong_format() {
        let manager = QrTokenManager::new();

        // Generate a token first
        let _token = manager.generate(300).await;

        // Verify wrong format token
        let result = manager.verify("not-a-valid-token-format").await;
        assert!(result.is_err());
    }
}

mod pairing_service_tests {
    use bedcode_lib::auth::PairingService;

    #[tokio::test]
    async fn test_pairing_service_code_expiration() {
        let service = PairingService::new();

        let code = service.generate_code().await;

        // Check code is not expired immediately after generation
        assert!(!code.is_expired());

        // Verify the code is valid
        let is_valid = service.verify_code(&code.code).await;
        assert!(is_valid);
    }

    #[tokio::test]
    async fn test_pairing_service_clear_code() {
        let service = PairingService::new();

        // Generate code
        let code = service.generate_code().await;

        // Clear the code
        service.clear_code().await;

        // Verify the code is no longer valid
        let is_valid = service.verify_code(&code.code).await;
        assert!(!is_valid);

        // Verify no current code
        let current = service.get_current_code().await;
        assert!(current.is_none());
    }

    #[tokio::test]
    async fn test_pairing_service_reuse_nonexpired() {
        let service = PairingService::new();

        // Generate first code
        let code1 = service.generate_code().await;
        let code1_str = code1.code.clone();

        // Request another code (should reuse since not expired)
        let code2 = service.generate_code().await;

        // Should be the same code
        assert_eq!(code1_str, code2.code);
    }

    #[tokio::test]
    async fn test_pairing_service_verify_incorrect_code() {
        let service = PairingService::new();

        // Generate a valid code
        let _code = service.generate_code().await;

        // Verify incorrect code
        let is_valid = service.verify_code("000000").await;
        assert!(!is_valid);
    }

    #[tokio::test]
    async fn test_pairing_service_verify_no_code() {
        let service = PairingService::new();

        // No code generated, verify should fail
        let is_valid = service.verify_code("123456").await;
        assert!(!is_valid);
    }

    #[tokio::test]
    async fn test_pairing_service_code_format() {
        let service = PairingService::new();

        let code = service.generate_code().await;

        // Code should be 6 digits
        assert_eq!(code.code.len(), 6);
        assert!(code.code.chars().all(|c| c.is_ascii_digit()));
    }

    #[tokio::test]
    async fn test_pairing_service_get_current_code_none() {
        let service = PairingService::new();

        // No code generated yet
        let current = service.get_current_code().await;
        assert!(current.is_none());
    }

    #[tokio::test]
    async fn test_pairing_service_get_current_code_some() {
        let service = PairingService::new();

        // Generate code
        let code = service.generate_code().await;

        // Get current code
        let current = service.get_current_code().await;
        assert!(current.is_some());
        assert_eq!(current.unwrap().code, code.code);
    }
}