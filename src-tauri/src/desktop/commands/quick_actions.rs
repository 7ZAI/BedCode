//! Quick Actions Commands

use crate::Result;
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub async fn list_quick_actions(
    db: State<'_, Arc<tokio::sync::Mutex<crate::shared::db::Database>>>,
) -> Result<Vec<crate::shared::db::QuickAction>> {
    let db = db.lock().await;
    db.get_quick_actions()
}

#[tauri::command]
pub async fn create_quick_action(
    db: State<'_, Arc<tokio::sync::Mutex<crate::shared::db::Database>>>,
    name: String,
    content: String,
    icon: Option<String>,
    color: Option<String>,
) -> Result<crate::shared::db::QuickAction> {
    let mut action = crate::shared::db::QuickAction::new(name, content);
    action.icon = icon;
    action.color = color;

    let db = db.lock().await;
    db.create_quick_action(&action)?;
    Ok(action)
}

#[tauri::command]
pub async fn update_quick_action(
    db: State<'_, Arc<tokio::sync::Mutex<crate::shared::db::Database>>>,
    id: String,
    name: String,
    content: String,
    icon: Option<String>,
    color: Option<String>,
) -> Result<crate::shared::db::QuickAction> {
    let db = db.lock().await;
    let mut action = db.get_quick_actions()?
        .into_iter()
        .find(|a| a.id == id)
        .ok_or_else(|| crate::AppError::NotFound(format!("Quick action not found: {}", id)))?;

    action.name = name;
    action.content = content;
    action.icon = icon;
    action.color = color;

    db.update_quick_action(&action)?;
    Ok(action)
}

#[tauri::command]
pub async fn delete_quick_action(
    db: State<'_, Arc<tokio::sync::Mutex<crate::shared::db::Database>>>,
    id: String,
) -> Result<()> {
    let db = db.lock().await;
    db.delete_quick_action(&id)
}
