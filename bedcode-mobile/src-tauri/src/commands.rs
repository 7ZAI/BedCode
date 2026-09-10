//! Mobile Commands Module
//!
//! Tauri 命令，按业务域拆分

pub mod android;
pub mod auth;
pub mod connection;
pub mod dev_logs;
pub mod mdns;
pub mod mobile_commands;
pub mod session;
pub mod terminal;

// Re-export all commands for easy registration
pub use android::{keep_screen_awake, open_url_in_browser, set_screen_orientation};
pub use auth::{
    ws_authenticate, ws_authenticate_with_qr, ws_get_auth_status, ws_request_pairing, ws_verify_pairing_code,
};
pub use connection::{
    ws_clear_token, ws_connect, ws_disconnect, ws_get_status, ws_get_token, ws_is_connected, ws_reconnect, ws_set_token,
};
pub use mobile_commands::{
    get_all_db_settings_mobile, get_session_config_mobile, list_session_configs_mobile, set_db_setting_mobile,
};
pub use session::{
    get_terminal_ws_info, ws_join_session, ws_load_session_configs, ws_load_sessions, ws_remove_session,
    ws_start_session, ws_stop_session,
};
pub use terminal::{ws_resize_terminal, ws_send_and_wait, ws_send_input_async, ws_send_message};
