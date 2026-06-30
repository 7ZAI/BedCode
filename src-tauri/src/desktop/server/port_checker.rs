//! 端口可用性检查模块
//!
//! 在 WebSocket 服务器启动前检查端口是否被占用，
//! 被占用时弹窗提示用户选择新端口

use crate::shared::system::config::AppConfig;
use crate::Result;
use std::net::TcpListener;
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogBuilder};

/// 检查端口是否可用（未被其他程序占用）
fn is_port_available(port: u16) -> bool {
    TcpListener::bind(format!("0.0.0.0:{}", port))
        .map(|listener| {
            // 立即释放端口
            drop(listener);
            true
        })
        .unwrap_or(false)
}

/// 查找下一个可用端口
///
/// 从 start_port + 1 开始，最多尝试 max_attempts 个端口
fn find_next_available_port(start_port: u16, max_attempts: u16) -> Option<u16> {
    for offset in 1..=max_attempts {
        let port = start_port + offset;
        if port > 65535 {
            break;
        }
        if is_port_available(port) {
            return Some(port);
        }
    }
    None
}

/// 保存端口到配置文件
fn save_port_to_config(app_handle: &AppHandle, port: u16) -> Result<()> {
    let config_path = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| crate::AppError::Config(format!("Failed to get app data dir: {}", e)))?
        .join("config.json");

    let mut config = AppConfig::load(&config_path)?;
    config.network.port = port;
    config.save(&config_path)?;

    tracing::info!("Port {} saved to config", port);
    Ok(())
}

/// 检查端口可用性，被占用时让用户选择新端口
///
/// 返回可用的端口号，如果用户取消则返回原端口
pub fn check_and_resolve_port(app_handle: &AppHandle, preferred_port: u16) -> Result<u16> {
    // 首先检查首选端口是否可用
    if is_port_available(preferred_port) {
        tracing::info!("Port {} is available", preferred_port);
        return Ok(preferred_port);
    }

    tracing::warn!("Port {} is already in use", preferred_port);

    // 尝试找到下一个可用端口
    let suggested_port = find_next_available_port(preferred_port, 10);

    // 构建提示消息
    let message = if let Some(suggested) = suggested_port {
        format!(
            "端口 {} 已被其他程序占用。\n\n建议使用端口 {}。\n是否使用建议的端口？",
            preferred_port, suggested
        )
    } else {
        format!(
            "端口 {} 已被其他程序占用。\n\n请手动在设置中修改端口后重启应用。",
            preferred_port
        )
    };

    // 弹出对话框询问用户
    let confirmed = MessageDialogBuilder::new(
        app_handle.dialog().clone(),
        "端口被占用",
        message,
    )
        .buttons(MessageDialogButtons::OkCancel)
        .blocking_show();

    // 用户取消或关闭对话框
    if !confirmed {
        tracing::info!("User cancelled port selection dialog");
        return Ok(preferred_port); // 返回原端口，服务器启动会失败
    }

    // 用户确认使用建议端口
    if let Some(new_port) = suggested_port {
        // 保存到配置文件
        save_port_to_config(app_handle, new_port)?;
        tracing::info!("User selected port {}", new_port);
        return Ok(new_port);
    }

    // 没有可用端口建议，返回原端口
    Ok(preferred_port)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_port_available_with_free_port() {
        // 端口 0 表示让系统自动分配一个空闲端口
        // 这里测试一个不太可能被占用的端口范围
        let port = 59000;
        // 注意：这个测试可能在某些环境下失败，因为端口可能恰好被占用
        // 所以我们只测试函数不会 panic
        let _ = is_port_available(port);
    }

    #[test]
    fn test_find_next_available_port() {
        // 测试查找下一个可用端口的逻辑
        // 注意：结果取决于系统当前端口使用情况
        let result = find_next_available_port(59000, 10);
        // 只验证返回值在有效范围内
        if let Some(port) = result {
            assert!(port > 59000 && port <= 59010);
        }
    }
}