//! Database Settings Commands

use crate::Result;
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub async fn get_all_db_settings(
    db: State<'_, Arc<tokio::sync::Mutex<crate::db::Database>>>,
) -> Result<Vec<crate::db::Setting>> {
    let db = db.lock().await;
    db.get_all_settings()
}

#[tauri::command]
pub async fn set_db_setting(
    db: State<'_, Arc<tokio::sync::Mutex<crate::db::Database>>>,
    key: String,
    value: String,
) -> Result<()> {
    let db = db.lock().await;
    db.set_setting(&key, &value)
}
