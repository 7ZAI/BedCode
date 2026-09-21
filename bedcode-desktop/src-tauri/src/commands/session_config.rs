//! Session Config Commands
//!
//! 票 08 + v21：配置真源在会话中心插件私有库——本文件五个命令是**薄转发**
//! （经 `utils::session_config_bridge`），命令签名与返回形状保持不变（前端与
//! 移动端零改判）。插件未激活时桥接**显性报错**（无宿主降级：主库投影随内核
//! 创建/重启执行器退役一并停写，宿主不再持有业务副本）。

use crate::Result;
use std::sync::Arc;
use tauri::State;

#[tauri::command(rename_all = "snake_case")]
pub async fn create_session_config(
    host: State<'_, Arc<crate::plugin::PluginHost>>,
    name: String,
    environment: String,
    working_dir: String,
    command: String,
    wsl_distro: Option<String>,
) -> Result<crate::db::SessionConfig> {
    let result = crate::utils::session_config_bridge::create_config(
        host.wasm_host_ctx(),
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
) -> Result<Vec<crate::db::SessionConfig>> {
    crate::utils::session_config_bridge::list_configs(host.wasm_host_ctx()).await
}

#[tauri::command]
pub async fn get_session_config(
    host: State<'_, Arc<crate::plugin::PluginHost>>,
    id: String,
) -> Result<Option<crate::db::SessionConfig>> {
    crate::utils::session_config_bridge::get_config(host.wasm_host_ctx(), &id).await
}

#[tauri::command]
pub async fn delete_session_config(
    host: State<'_, Arc<crate::plugin::PluginHost>>,
    id: String,
) -> Result<()> {
    crate::utils::session_config_bridge::delete_config(host.wasm_host_ctx(), &id).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn update_session_config(
    host: State<'_, Arc<crate::plugin::PluginHost>>,
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
