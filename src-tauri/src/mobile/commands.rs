//! Mobile Commands Module
//!
//! 移动端专用 Tauri 命令，按业务域拆分
//!
//! 模块划分：
//! - token: Token 管理命令
//! - connection: WebSocket 连接命令
//! - auth: 认证和配对命令
//! - session: 会话管理命令
//! - terminal: 终端操作命令
//! - android: Android 平台专用命令

pub mod token;
pub mod connection;
pub mod auth;
pub mod session;
pub mod terminal;
pub mod android;
pub mod mobile_commands;

// Re-export all commands for easy registration
pub use token::{ws_set_token, ws_get_token, ws_clear_token};
pub use connection::{
    ws_connect, ws_disconnect, ws_get_status, ws_is_connected, ws_reconnect,
};
pub use auth::{
    ws_get_auth_status, ws_authenticate, ws_request_pairing, ws_verify_pairing_code, ws_authenticate_with_qr,
};
pub use session::{
    ws_load_sessions, ws_join_session, ws_leave_session, ws_subscribe_session,
    ws_start_session, ws_stop_session, ws_remove_session, ws_load_session_configs,
};
pub use terminal::{
    ws_send_input_async, ws_send_message, ws_send_and_wait, ws_resize_terminal,
};
pub use android::{
    set_screen_orientation, keep_screen_awake,
    open_url_in_browser,
};
pub use mobile_commands::{
    list_quick_actions_mobile, create_quick_action_mobile, update_quick_action_mobile,
    delete_quick_action_mobile, get_all_db_settings_mobile, set_db_setting_mobile,
    list_session_configs_mobile, get_session_config_mobile,
};