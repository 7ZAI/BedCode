//! WSL Commands

use crate::Result;

#[tauri::command]
pub async fn list_wsl_distributions() -> Result<Vec<crate::desktop::pty::WslDistro>> {
    crate::desktop::pty::list_distributions()
}

#[tauri::command]
pub fn is_wsl_available() -> bool {
    crate::desktop::pty::is_wsl_available()
}
