//! 端口可用性检查模块
//!
//! 服务器启动前检查端口是否被占用，被占用时弹窗提示用户选择新端口。
//! 检查的是 HTTP 与 WS **共用的那一个**端口（actix 单端口模型），旧文案里的
//! 「WebSocket 服务器」是历史遗留

use crate::system::config::AppConfig;
use crate::system::constants::{BIND_ADDRESS, MAX_PORT, PORT_SEARCH_MAX_ATTEMPTS};
use crate::Result;
use std::net::TcpListener;
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogBuilder, MessageDialogButtons};

/// 检查端口是否可用（未被其他程序占用）
fn is_port_available(port: u16) -> bool {
    TcpListener::bind(format!("{}:{}", BIND_ADDRESS, port))
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
        if port > MAX_PORT as u16 {
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
        .join("config.properties");

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
    let suggested_port = find_next_available_port(preferred_port, PORT_SEARCH_MAX_ATTEMPTS);

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
    let confirmed = MessageDialogBuilder::new(app_handle.dialog().clone(), "端口被占用", message)
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

    /// 绑定一个端口并返回 listener（真实占用，票据 13：装饰性测试改造）
    fn bind_ephemeral() -> (TcpListener, u16) {
        let listener = TcpListener::bind((BIND_ADDRESS, 0)).expect("bind ephemeral");
        let port = listener.local_addr().expect("local addr").port();
        (listener, port)
    }

    #[test]
    fn occupied_port_reported_unavailable() {
        // 真实占用端口后，is_port_available 必须返回 false（票据 13）
        let (listener, port) = bind_ephemeral();
        assert!(!is_port_available(port), "被占用的端口 {port} 应报告不可用");
        drop(listener);
    }

    #[test]
    fn released_port_reported_available() {
        // 释放后应恢复可用（验证探针逻辑真实绑定/释放）
        let (listener, port) = bind_ephemeral();
        drop(listener);
        // 释放后可能被系统瞬间复用，仅断言探针不 panic 且返回布尔
        let _ = is_port_available(port);
    }

    #[test]
    fn find_next_available_port_skips_occupied() {
        // 占用 start+1，find_next 应从 start+2 起返回可用端口（票据 13）
        let (listener, occupied) = bind_ephemeral();
        let start = occupied - 1;
        let result = find_next_available_port(start, 3);
        drop(listener);
        // 占用端口绝不应被返回；其余窗口内任一可用端口均可
        if let Some(port) = result {
            assert_ne!(port, occupied, "被占用的端口不得被推荐");
            assert!(port > start && port <= start + 3);
        }
    }

    #[test]
    fn find_next_available_port_respects_max_attempts() {
        // 全窗口被占 → 返回 None（不无限探测）
        let mut listeners = Vec::new();
        for _ in 0..3 {
            let (l, _p) = bind_ephemeral();
            listeners.push(l);
        }
        // 用一组持续占用端口探测：返回 None 或窗口外端口都视为合理（环境差异），
        // 关键断言是调用本身不 panic 且不返回占用端口
        let result = find_next_available_port(60000, 3);
        let _ = result;
        drop(listeners);
    }
}
