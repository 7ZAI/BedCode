//! Session Config Commands

use crate::session::SessionConfigManager;
use crate::Result;
use std::sync::Arc;
use tauri::State;

#[tauri::command(rename_all = "snake_case")]
pub async fn create_session_config(
    config_manager: State<'_, Arc<SessionConfigManager>>,
    name: String,
    environment: String,
    working_dir: String,
    command: String,
    wsl_distro: Option<String>,
) -> Result<crate::db::SessionConfig> {
    tracing::info!("create_session_config called: name={}, environment={}", name, environment);

    let result = config_manager
        .create_config_full(
            name,
            environment,
            wsl_distro,
            working_dir,
            command,
            false,
        )
        .await;

    match &result {
        Ok(config) => tracing::info!("create_session_config success: id={}", config.id),
        Err(e) => tracing::error!("create_session_config failed: {:?}", e),
    }

    result
}

#[tauri::command]
pub async fn list_session_configs(
    config_manager: State<'_, Arc<SessionConfigManager>>,
) -> Result<Vec<crate::db::SessionConfig>> {
    config_manager.list_configs().await
}

#[tauri::command]
pub async fn get_session_config(
    config_manager: State<'_, Arc<SessionConfigManager>>,
    id: String,
) -> Result<Option<crate::db::SessionConfig>> {
    config_manager.get_config(&id).await
}

#[tauri::command]
pub async fn delete_session_config(
    config_manager: State<'_, Arc<SessionConfigManager>>,
    id: String,
) -> Result<()> {
    config_manager.delete_config(&id).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn update_session_config(
    config_manager: State<'_, Arc<SessionConfigManager>>,
    id: String,
    name: String,
    environment: String,
    working_dir: String,
    command: String,
    wsl_distro: Option<String>,
    auto_start: Option<bool>,
) -> Result<crate::db::SessionConfig> {
    config_manager
        .update_config(
            &id,
            Some(name),
            Some(environment),
            wsl_distro,
            Some(working_dir),
            Some(command),
            auto_start,
        )
        .await
}
