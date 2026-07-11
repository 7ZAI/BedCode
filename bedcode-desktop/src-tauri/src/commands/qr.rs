//! QR Token Commands

use crate::system::constants::network::{LOCALHOST_IP, IP_LOOPBACK_PREFIX, IP_LINK_LOCAL_PREFIX};
use crate::Result;
use std::sync::Arc;
use tauri::{Manager, State};

#[derive(Debug, Clone, serde::Serialize)]
pub struct QrConnectionInfo {
    pub token: String,
    pub host: String,
    pub port: u16,
    /// 剩余有效时间（秒）
    pub remaining_secs: u64,
}

#[tauri::command]
pub async fn generate_qr_code(
    qr_manager: State<'_, Arc<crate::utils::auth::QrTokenManager>>,
    db: State<'_, Arc<tokio::sync::Mutex<crate::db::Database>>>,
) -> Result<String> {
    let ttl = {
        let db = db.lock().await;
        db.get_setting("qr_token_ttl")
            .ok()
            .flatten()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(300)
    };

    let token = qr_manager.generate(ttl).await;
    tracing::info!("QR code generated, TTL: {}s", ttl);
    Ok(token)
}

#[tauri::command]
pub async fn clear_qr_code(
    qr_manager: State<'_, Arc<crate::utils::auth::QrTokenManager>>,
) -> Result<()> {
    qr_manager.clear().await;
    tracing::info!("QR code cleared");
    Ok(())
}

#[tauri::command]
pub async fn get_qr_connection_info(
    qr_manager: State<'_, Arc<crate::utils::auth::QrTokenManager>>,
    app_handle: tauri::AppHandle,
    host: Option<String>,
) -> Result<Option<QrConnectionInfo>> {
    let active = qr_manager.get_active().await;
    match active {
        None => Ok(None),
        Some((token, _ttl, remaining)) => {
            let host = host.or_else(|| {
                crate::commands::system::get_local_ip_addresses()
                    .into_iter()
                    .find(|ip| !ip.starts_with(IP_LOOPBACK_PREFIX) && !ip.starts_with(IP_LINK_LOCAL_PREFIX))
            }).unwrap_or_else(|| LOCALHOST_IP.to_string());

            let config = crate::system::config::AppConfig::load(
                &app_handle.path().app_data_dir()
                    .unwrap_or_default()
                    .join("config.properties")
            ).unwrap_or_default();
            let port = config.network.port;

            Ok(Some(QrConnectionInfo { token, host, port, remaining_secs: remaining }))
        }
    }
}

#[tauri::command]
pub async fn get_qr_token_ttl(
    db: State<'_, Arc<tokio::sync::Mutex<crate::db::Database>>>,
) -> Result<u64> {
    let db = db.lock().await;
    match db.get_setting("qr_token_ttl") {
        Ok(Some(value)) => value.parse::<u64>().map_err(|e| crate::AppError::Config(e.to_string())),
        _ => Ok(300),
    }
}

#[tauri::command]
pub async fn set_qr_token_ttl(
    db: State<'_, Arc<tokio::sync::Mutex<crate::db::Database>>>,
    seconds: u64,
) -> Result<()> {
    let db = db.lock().await;
    db.set_setting("qr_token_ttl", &seconds.to_string())
        .map_err(|e| crate::AppError::Config(e.to_string()))
}
