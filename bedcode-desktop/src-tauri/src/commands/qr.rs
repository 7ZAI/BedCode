//! QR Token Commands

use crate::system::constants::network::{IP_LINK_LOCAL_PREFIX, IP_LOOPBACK_PREFIX, LOCALHOST_IP};
use crate::Result;
use std::sync::Arc;
use tauri::{Manager, State};

/// QR token TTL 上限（24 小时），防超大值误配
pub const QR_TOKEN_TTL_MAX_SECS: u64 = 86_400;

/// 校验 QR token TTL 边界（纯函数，可单测）
///
/// `0` 会生成立即过期的 token（移动端扫码即失败），超大值无意义，均拒绝。
pub fn validate_qr_token_ttl(ttl: u64) -> std::result::Result<(), crate::AppError> {
    if ttl == 0 || ttl > QR_TOKEN_TTL_MAX_SECS {
        return Err(crate::AppError::InvalidInput(format!(
            "qr_token_ttl 必须在 1..={QR_TOKEN_TTL_MAX_SECS} 秒之间，收到 {ttl}"
        )));
    }
    Ok(())
}

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
    host: State<'_, Arc<crate::plugin::PluginHost>>,
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

    // 票 11 命令面桥接：认证中心激活时经互调生成（TTL 传入宿主配置值），否则降级
    let token = crate::utils::auth::auth_center::generate_qr_code(host.wasm_host_ctx(), &qr_manager, ttl).await?;
    tracing::info!("QR code generated, TTL: {}s", ttl);
    Ok(token)
}

#[tauri::command]
pub async fn clear_qr_code(
    host: State<'_, Arc<crate::plugin::PluginHost>>,
    qr_manager: State<'_, Arc<crate::utils::auth::QrTokenManager>>,
) -> Result<()> {
    crate::utils::auth::auth_center::clear_qr_code(host.wasm_host_ctx(), &qr_manager).await?;
    tracing::info!("QR code cleared");
    Ok(())
}

#[tauri::command]
pub async fn get_qr_connection_info(
    host: State<'_, Arc<crate::plugin::PluginHost>>,
    qr_manager: State<'_, Arc<crate::utils::auth::QrTokenManager>>,
    app_handle: tauri::AppHandle,
    host_override: Option<String>,
) -> Result<Option<QrConnectionInfo>> {
    // 票 11 命令面桥接：认证中心激活时经互调取 token 状态，否则降级宿主；
    // 宿主/端口组装（局域网 IP / 端口配置）保留在命令层（宿主引擎配置域）
    let active = crate::utils::auth::auth_center::qr_conn_info(host.wasm_host_ctx(), &qr_manager).await?;
    match active {
        None => Ok(None),
        Some((token, remaining)) => {
            let host_addr = host_override
                .or_else(|| {
                    crate::commands::system::get_local_ip_addresses()
                        .into_iter()
                        .find(|ip| !ip.starts_with(IP_LOOPBACK_PREFIX) && !ip.starts_with(IP_LINK_LOCAL_PREFIX))
                })
                .unwrap_or_else(|| LOCALHOST_IP.to_string());

            let config = crate::system::config::AppConfig::load(
                &app_handle
                    .path()
                    .app_data_dir()
                    .unwrap_or_default()
                    .join("config.properties"),
            )
            .unwrap_or_default();
            let port = config.network.port;

            Ok(Some(QrConnectionInfo {
                token,
                host: host_addr,
                port,
                remaining_secs: remaining,
            }))
        }
    }
}

#[tauri::command]
pub async fn get_qr_token_ttl(db: State<'_, Arc<tokio::sync::Mutex<crate::db::Database>>>) -> Result<u64> {
    let db = db.lock().await;
    match db.get_setting("qr_token_ttl") {
        Ok(Some(value)) => value.parse::<u64>().map_err(|e| crate::AppError::Config(e.to_string())),
        _ => Ok(300),
    }
}

#[tauri::command]
pub async fn set_qr_token_ttl(db: State<'_, Arc<tokio::sync::Mutex<crate::db::Database>>>, ttl: u64) -> Result<()> {
    validate_qr_token_ttl(ttl)?;
    let db = db.lock().await;
    db.set_setting("qr_token_ttl", &ttl.to_string())
        .map_err(|e| crate::AppError::Config(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_qr_token_ttl_accepts_bounds() {
        assert!(validate_qr_token_ttl(1).is_ok());
        assert!(validate_qr_token_ttl(300).is_ok());
        assert!(validate_qr_token_ttl(QR_TOKEN_TTL_MAX_SECS).is_ok());
    }

    #[test]
    fn validate_qr_token_ttl_rejects_zero() {
        let err = validate_qr_token_ttl(0).unwrap_err();
        assert!(err.to_string().contains("qr_token_ttl 必须在"), "unexpected: {err}");
    }

    #[test]
    fn validate_qr_token_ttl_rejects_overflow() {
        let err = validate_qr_token_ttl(QR_TOKEN_TTL_MAX_SECS + 1).unwrap_err();
        assert!(err.to_string().contains("qr_token_ttl 必须在"), "unexpected: {err}");
    }
}
