//! Session Config Commands
//!
//! 票 08：配置真源迁入会话中心插件私有库——本文件四个命令改为**薄转发**
//! （经 `utils::session_config_bridge`）。命令签名与返回形状保持不变
//! （前端与移动端零改判）；插件不可用时桥接自动降级宿主 `SessionConfigManager`
//! （迁移前行为，无单点）。

use crate::session::SessionConfigManager;
use crate::Result;
use std::sync::Arc;
use tauri::State;

#[tauri::command(rename_all = "snake_case")]
pub async fn create_session_config(
    host: State<'_, Arc<crate::plugin::PluginHost>>,
    config_manager: State<'_, Arc<SessionConfigManager>>,
    name: String,
    environment: String,
    working_dir: String,
    command: String,
    wsl_distro: Option<String>,
) -> Result<crate::db::SessionConfig> {
    let result = crate::utils::session_config_bridge::create_config(
        host.wasm_host_ctx(),
        &config_manager,
        name,
        environment,
        wsl_distro,
        working_dir,
        command,
    )
    .await;

    match &result {
        Ok(config) => tracing::info!(config_id = %config.id, "create_session_config success"),
        Err(e) => tracing::error!(error = %e, "create_session_config failed"),
    }

    result
}

#[tauri::command]
pub async fn list_session_configs(
    host: State<'_, Arc<crate::plugin::PluginHost>>,
    config_manager: State<'_, Arc<SessionConfigManager>>,
) -> Result<Vec<crate::db::SessionConfig>> {
    crate::utils::session_config_bridge::list_configs(host.wasm_host_ctx(), &config_manager).await
}

#[tauri::command]
pub async fn get_session_config(
    host: State<'_, Arc<crate::plugin::PluginHost>>,
    config_manager: State<'_, Arc<SessionConfigManager>>,
    id: String,
) -> Result<Option<crate::db::SessionConfig>> {
    crate::utils::session_config_bridge::get_config(host.wasm_host_ctx(), &config_manager, &id).await
}

#[tauri::command]
pub async fn delete_session_config(
    host: State<'_, Arc<crate::plugin::PluginHost>>,
    config_manager: State<'_, Arc<SessionConfigManager>>,
    id: String,
) -> Result<()> {
    crate::utils::session_config_bridge::delete_config(host.wasm_host_ctx(), &config_manager, &id).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn update_session_config(
    host: State<'_, Arc<crate::plugin::PluginHost>>,
    config_manager: State<'_, Arc<SessionConfigManager>>,
    id: String,
    name: String,
    environment: String,
    working_dir: String,
    command: String,
    wsl_distro: Option<String>,
    auto_start: Option<bool>,
) -> Result<crate::db::SessionConfig> {
    crate::utils::session_config_bridge::update_config(
        host.wasm_host_ctx(),
        &config_manager,
        &id,
        name,
        environment,
        wsl_distro,
        working_dir,
        command,
        auto_start,
    )
    .await
}
