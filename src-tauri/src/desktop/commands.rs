//! Desktop-only Tauri Commands
//!
//! 桌面端专用命令 - 移动端不可用
//! 按领域拆分，与移动端 commands/ 组织方式一致

pub mod session_config;
pub mod session;
pub mod pty_input;
pub mod wsl;
pub mod qr;
pub mod quick_actions;
pub mod settings;
pub mod devices;
