//! Mobile Token Commands
//!
//! 全局 Token 管理命令

use crate::Result;
use crate::mobile::global::{set_global_token, get_global_token, clear_global_token};

/// 设置全局 Token（前端启动时从 localStorage 读取并调用）
#[tauri::command]
pub fn ws_set_token(token: String) -> Result<()> {
    set_global_token(&token);
    Ok(())
}

/// 获取当前全局 Token
#[tauri::command]
pub fn ws_get_token() -> String {
    get_global_token()
}

/// 清除全局 Token（登出时调用）
#[tauri::command]
pub fn ws_clear_token() -> Result<()> {
    clear_global_token();
    Ok(())
}
