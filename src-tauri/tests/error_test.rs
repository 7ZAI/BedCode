//! Tests for error types

use bedcode_lib::error::AppError;

#[test]
fn test_error_display() {
    let err = AppError::Pty("test pty error".to_string());
    assert_eq!(format!("{}", err), "PTY error: test pty error");

    let err = AppError::Session("test session error".to_string());
    assert_eq!(format!("{}", err), "Session error: test session error");

    let err = AppError::Auth("test auth error".to_string());
    assert_eq!(format!("{}", err), "Authentication error: test auth error");
}

#[test]
fn test_error_from_io() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
    let app_err: AppError = io_err.into();

    match app_err {
        AppError::Io(e) => assert_eq!(e.kind(), std::io::ErrorKind::NotFound),
        _ => panic!("Expected Io error variant"),
    }
}

#[test]
fn test_error_from_json() {
    let json_err = serde_json::from_str::<i32>("not a number").unwrap_err();
    let app_err: AppError = json_err.into();

    match app_err {
        AppError::Serialization(_) => {}
        _ => panic!("Expected Serialization error variant"),
    }
}

#[test]
fn test_error_chain() {
    fn inner_function() -> Result<(), AppError> {
        Err(AppError::NotFound("resource".to_string()))
    }

    fn outer_function() -> Result<String, AppError> {
        inner_function()?;
        Ok("success".to_string())
    }

    let result = outer_function();
    assert!(result.is_err());

    match result.unwrap_err() {
        AppError::NotFound(s) => assert_eq!(s, "resource"),
        _ => panic!("Expected NotFound error variant"),
    }
}
