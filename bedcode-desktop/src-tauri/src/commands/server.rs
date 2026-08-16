//! Server Control Commands
//!
//! Tauri commands for server lifecycle management and metrics query

use crate::server::supervisor::{ServerStatusInfo, ServerSupervisor};
use crate::server::metrics::ServerMetrics;
use crate::system::config::{AppConfig, NetworkConfig};
use crate::Result;
use tauri::Manager;

/// 启动服务器
#[tauri::command]
pub async fn server_start(
    app_handle: tauri::AppHandle,
    port: u16,
) -> Result<()> {
    let supervisor = ServerSupervisor::global();
    supervisor.start(port).await?;

    // 写入端口文件
    let port_file = app_handle
        .path()
        .app_data_dir()
        .ok()
        .map(|p| p.join("bedcode-port.txt"));
    if let Some(port_file) = port_file {
        if let Some(parent) = port_file.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }
        let _ = tokio::fs::write(&port_file, port.to_string()).await;
    }

    Ok(())
}

/// 停止服务器
#[tauri::command]
pub async fn server_stop() -> Result<()> {
    let supervisor = ServerSupervisor::global();
    supervisor.stop().await
}

/// 重启服务器
#[tauri::command]
pub async fn server_restart() -> Result<()> {
    let supervisor = ServerSupervisor::global();
    supervisor.restart().await
}

/// 获取服务器状态信息
#[tauri::command]
pub async fn get_server_status() -> Result<ServerStatusInfo> {
    let supervisor = ServerSupervisor::global();
    Ok(supervisor.get_status_info().await)
}

/// 获取本地 WS 通道短期一次性令牌
///
/// 桌面端 WebView 连接 /ws/terminal/local 时携带（?token=），
/// 环回 IP 校验之外的第二道防线（防本机其他进程免认证订阅 PTY 输出）
#[tauri::command]
pub fn get_local_ws_token() -> String {
    crate::server::local_token::LocalTokenManager::global().issue()
}

/// 获取网络配置
#[tauri::command]
pub async fn get_server_network_config() -> Result<NetworkConfig> {
    let config = AppConfig::global();
    Ok(config.network.clone())
}

/// 获取服务器性能指标
///
/// 采集总开关（network.metrics_enabled）默认关闭，关闭时返回错误；
/// 前端轮询静默忽略（metrics 保持空），页面显示占位符
#[tauri::command]
pub async fn get_server_metrics() -> Result<ServerMetrics> {
    if !AppConfig::global().network.metrics_enabled {
        return Err(crate::AppError::Config(
            "服务器性能监控已关闭（network.metrics_enabled=false）".to_string(),
        ));
    }
    let supervisor = ServerSupervisor::global();
    Ok(supervisor.get_metrics().await)
}

/// 更新服务器端口配置
#[tauri::command]
pub async fn update_server_port(
    app_handle: tauri::AppHandle,
    port: u16,
) -> Result<()> {
    // 保存到配置文件
    let config_path = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| crate::AppError::Config(format!("Failed to get app data dir: {}", e)))?
        .join("config.properties");

    let mut config = AppConfig::load(&config_path)?;
    config.network.port = port;
    config.save(&config_path)?;

    // 更新 supervisor 内存中的端口
    let supervisor = ServerSupervisor::global();
    supervisor.update_port(port).await?;

    tracing::info!("Server port updated to {}", port);
    Ok(())
}

/// 更新自启动配置
#[tauri::command]
pub async fn update_server_auto_start(
    app_handle: tauri::AppHandle,
    auto_start: bool,
) -> Result<()> {
    let config_path = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| crate::AppError::Config(format!("Failed to get app data dir: {}", e)))?
        .join("config.properties");

    let mut config = AppConfig::load(&config_path)?;
    config.network.auto_start = auto_start;
    config.save(&config_path)?;

    let supervisor = ServerSupervisor::global();
    supervisor.update_auto_start(auto_start).await;

    tracing::info!("Server auto_start updated to {}", auto_start);
    Ok(())
}

/// 更新服务器网络配置（Actix Web + WebSocket 参数）
///
/// 仅更新配置文件，需重启服务器生效
#[tauri::command]
pub async fn update_server_network_config(
    app_handle: tauri::AppHandle,
    network_config: NetworkConfig,
) -> Result<()> {
    let config_path = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| crate::AppError::Config(format!("Failed to get app data dir: {}", e)))?
        .join("config.properties");

    let mut config = AppConfig::load(&config_path)?;
    let auto_start = network_config.auto_start;
    let port = network_config.port;
    config.network = network_config;
    config.save(&config_path)?;

    // 更新 supervisor 内存中的端口和自启动
    let supervisor = ServerSupervisor::global();
    supervisor.update_port(port).await?;
    supervisor.update_auto_start(auto_start).await;

    tracing::info!("Server network config updated");
    Ok(())
}

/// 重置服务器网络配置为默认值
///
/// 仅更新配置文件，需重启服务器生效
#[tauri::command]
pub async fn reset_server_network_config(
    app_handle: tauri::AppHandle,
) -> Result<NetworkConfig> {
    let config_path = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| crate::AppError::Config(format!("Failed to get app data dir: {}", e)))?
        .join("config.properties");

    let mut config = AppConfig::load(&config_path)?;
    let default_config = NetworkConfig::default();
    let auto_start = default_config.auto_start;
    let port = default_config.port;
    config.network = default_config.clone();
    config.save(&config_path)?;

    // 更新 supervisor 内存中的端口和自启动
    let supervisor = ServerSupervisor::global();
    supervisor.update_port(port).await?;
    supervisor.update_auto_start(auto_start).await;

    tracing::info!("Server network config reset to defaults");
    Ok(default_config)
}
