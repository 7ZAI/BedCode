//! Tauri Commands
//!
//! 前端 invoke 命令层 - 系统命令、会话命令、PTY 命令等。
//!
//! 2026-09-22 由 `commands/` 目录多文件合并为单一文件（session / pty_input /
//! system / opener / settings / devices / dev_logs / plugin / server 九域）。
//! 域之间用 `// ====================` 分隔注释分组，与 lib.rs `generate_handler!`
//! 的注册顺序解耦——按域就近维护即可。

use crate::db::Database;
use crate::server::core::link_crypto::{self, LinkCryptoConfig};
use crate::server::core::metrics::ServerMetrics;
use crate::server::core::supervisor::{ServerStatusInfo, ServerSupervisor};
use crate::session::{RendererSource, ResizeOutcome, SessionManager};
use crate::system::config::{AppConfig, NetworkConfig};
use crate::system::constants::terminal::{TERMINAL_BG_EXTENSIONS, TERMINAL_BG_FILE_PREFIX, TERMINAL_BG_MAX_BYTES};
use crate::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{Manager, State};
use tracing_subscriber::filter::EnvFilter;

// ==================== Session Commands ====================

// 宿主命令面**只保留终端渲染管道与引擎事实**（ADR 0022 裁剪线 + 终端红线）：
// - `list_sessions` / `get_session`：引擎记录（+ 注解槽任务字段），终端窗口与
//   通知种子化的读取面
// - `resize_session`：尺寸裁决（插件裁决 + 内核登记/执行，保留宿主降级执行器）
//
// 会话**编排**命令（`start_session` / `create_session_no_start` /
// `start_existing_session` / `kill_session` / `delete_session` / `restart_session`）
// 已按 2026-09-21 命令面收敛注销（`.scratch/2026-09-21-host-rust-residue/issues/05`）：
// 创建/停止/移除/重启的业务面归 `com.bedcode.terminal-session` 插件命令面
// （`session.create` / `session.close` / `session.action.*`），宿主只经
// `host-session` 原语执行（见 `plugin/manager/wasm_runtime/host_impl/session.rs`）。
// 两阶段启动（建而不启 + 后续 `start_existing_session`）随之退役——v21 起唯一
// 生产者（会话中心插件与移动端 HTTP/WS 线）一律 `start = true`。

/// 列出会话（对外视图：引擎记录 + 注解槽任务字段，票 12）
///
/// 返回类型 `SessionInfoView` 的 JSON 形状与迁移前 `SessionInfo` 逐字段一致
/// （记录字段 + `taskStatus` / `taskReason` / `taskUpdatedAt` / `taskQuestions`，
/// 缺省不出现）——前端契约不变，变的是取值来源（注解槽）。
#[tauri::command]
pub async fn list_sessions(
    session_manager: State<'_, Arc<SessionManager>>,
) -> Result<Vec<crate::session::SessionInfoView>> {
    Ok(session_manager.session_views().await)
}

/// 获取单个会话（对外视图，同 `list_sessions`）
#[tauri::command]
pub async fn get_session(
    session_manager: State<'_, Arc<SessionManager>>,
    session_id: String,
) -> Result<Option<crate::session::SessionInfoView>> {
    Ok(session_manager.session_view(&session_id).await)
}

/// 调整会话终端大小（桌面本地路径，正统渲染端身份恒为 Desktop）
///
/// force 置位表示覆盖确认已通过（前端弹窗确认后重发）；返回 ResizeOutcome
/// 供前端判断是否需要弹窗确认（NeedsConfirmation 时未应用任何改动）。
///
/// 票 10：**裁决规则**下沉会话中心插件（正统端判定 + 覆盖确认策略在插件侧），
/// 内核只提供登记与执行；插件不可用时降级内核执行器（含内核裁决分支），
/// 对外行为（返回形状与 NeedsConfirmation 语义）不变。
#[tauri::command]
pub async fn resize_session(
    host: State<'_, Arc<crate::plugin::PluginHost>>,
    session_manager: State<'_, Arc<SessionManager>>,
    session_id: String,
    cols: u16,
    rows: u16,
    force: Option<bool>,
) -> Result<ResizeOutcome> {
    let force = force.unwrap_or(false);
    if let Some(outcome) = crate::utils::session_action_bridge::resize_session_via_plugin(
        host.wasm_host_ctx(),
        &session_id,
        cols,
        rows,
        &RendererSource::Desktop,
        force,
    )
    .await?
    {
        return Ok(outcome);
    }
    session_manager
        .resize_session(&session_id, cols, rows, RendererSource::Desktop, force)
        .await
}

// ==================== PTY Input Commands ====================

#[tauri::command]
pub async fn write_to_session(
    session_manager: State<'_, Arc<SessionManager>>,
    session_id: String,
    data: String,
) -> Result<()> {
    session_manager.write_input(&session_id, &data).await
}

#[tauri::command]
pub async fn send_special_key(
    session_manager: State<'_, Arc<SessionManager>>,
    session_id: String,
    key: String,
) -> Result<()> {
    session_manager.send_special_key(&session_id, &key).await
}

// ==================== Shared System Commands ====================

/// 运行中会话摘要信息，用于窗口关闭确认弹窗
#[derive(Debug, Clone, Serialize)]
pub struct RunningSessionInfo {
    /// 会话 ID
    pub id: String,
    /// 会话名称
    pub name: String,
    /// 会话状态（Running / Starting / WaitingInput）
    pub status: String,
}

// ==================== Settings Commands ====================

/// 获取应用设置
#[tauri::command]
pub async fn get_app_settings(app_handle: tauri::AppHandle) -> crate::Result<crate::system::config::AppConfig> {
    let config_path = app_handle
        .path()
        .app_data_dir()
        .map(|p| p.join("config.properties"))
        .map_err(|e: tauri::Error| crate::AppError::Config(e.to_string()))?;

    crate::system::config::AppConfig::load(&config_path).map_err(|e| crate::AppError::Config(e.to_string()))
}

/// 保存应用设置
#[tauri::command]
pub async fn save_app_settings(
    app_handle: tauri::AppHandle,
    settings: crate::system::config::AppConfig,
) -> crate::Result<()> {
    let config_path = app_handle
        .path()
        .app_data_dir()
        .map(|p| p.join("config.properties"))
        .map_err(|e: tauri::Error| crate::AppError::Config(e.to_string()))?;

    // 同步 PowerManager 开关状态
    crate::system::power::power_manager().set_enabled(settings.network.prevent_sleep);

    settings.save(&config_path)?;

    tracing::info!("App settings saved to {:?}", config_path);
    Ok(())
}

/// 保存日志配置（设置页「日志设置」区）
///
/// 只替换现有配置的 log 段并持久化（避免前端整表保存时丢 log 字段）；
/// 除 file_level 热调外的项（format/rotation/max_files/capacity_bytes）重启后生效。
#[tauri::command]
pub async fn save_log_settings(app_handle: tauri::AppHandle, log: crate::system::config::LogConfig) -> Result<()> {
    let config_path = app_handle
        .path()
        .app_data_dir()
        .map(|p| p.join("config.properties"))
        .map_err(|e: tauri::Error| crate::AppError::Config(e.to_string()))?;
    let mut config =
        crate::system::config::AppConfig::load(&config_path).map_err(|e| crate::AppError::Config(e.to_string()))?;
    config.log = log;
    config.save(&config_path)?;
    tracing::info!("Log settings saved to {:?}", config_path);
    Ok(())
}

/// 运行时热调日志文件级别（不落盘；重启后回落到持久化 `log.file_level`）
///
/// 仅允许标准五级之一；非法值返回配置错误。
#[tauri::command]
pub fn set_log_level(level: String) -> Result<()> {
    match level.as_str() {
        "trace" | "debug" | "info" | "warn" | "error" => {}
        other => {
            return Err(crate::AppError::Config(format!(
                "invalid log level '{other}' (expected trace/debug/info/warn/error)"
            )));
        }
    }
    let setup = crate::system::logging::global_setup()
        .ok_or_else(|| crate::AppError::Config("logging not initialized yet".to_string()))?;
    setup
        .file_level_reload
        .reload(EnvFilter::new(&level))
        .map_err(|e| crate::AppError::Config(format!("log level reload failed: {e}")))?;
    tracing::info!("Log file level hot-reloaded to {level} (restart falls back to config)");
    Ok(())
}

// ==================== Terminal Background Image ====================

/// 设置终端背景图片
///
/// 传入源图片路径时，将图片复制到应用数据目录（统一命名为 `terminal_bg.<ext>`）并返回文件名；
/// 传入 `None` 时移除已有背景图片文件。选择复制而非直接引用源路径，
/// 避免用户移动/删除原图后背景失效。
#[tauri::command]
pub fn set_terminal_bg_image(app_handle: tauri::AppHandle, source_path: Option<String>) -> Result<Option<String>> {
    let data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e: tauri::Error| crate::AppError::Config(e.to_string()))?;

    // 清理已有背景图片（每次选择扩展名可能不同，避免残留旧文件）
    if data_dir.exists() {
        let entries = std::fs::read_dir(&data_dir)
            .map_err(|e| crate::AppError::Config(format!("读取应用数据目录失败 {}: {e}", data_dir.display())))?;
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(ext) = name.strip_prefix(&format!("{TERMINAL_BG_FILE_PREFIX}.")) {
                if TERMINAL_BG_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()) {
                    if let Err(e) = std::fs::remove_file(entry.path()) {
                        tracing::warn!("删除旧终端背景图片失败 {}: {e}", entry.path().display());
                    }
                }
            }
        }
    }

    let Some(source) = source_path.filter(|p| !p.is_empty()) else {
        // 仅移除背景图片
        return Ok(None);
    };

    // 校验源路径：绝对路径 + 扩展名（§8 输入校验在 Rust 端；相对路径可越界复制）
    let (src, ext) = validate_terminal_bg_source(&source)?;
    let file_name = format!("{TERMINAL_BG_FILE_PREFIX}.{ext}");
    let dest = data_dir.join(&file_name);
    std::fs::copy(src, &dest)
        .map_err(|e| crate::AppError::Config(format!("复制背景图片 {source} 到 {} 失败: {e}", dest.display())))?;

    tracing::info!("终端背景图片已更新: {}", dest.display());
    Ok(Some(file_name))
}

/// 校验终端背景图片来源路径（纯函数，可单测）
///
/// 约束：绝对路径（相对路径可越界复制到应用数据目录）、扩展名在白名单内、
/// 文件存在且大小不超过上限。返回 `(源路径, 小写扩展名)`。
fn validate_terminal_bg_source(source: &str) -> Result<(std::path::PathBuf, String)> {
    // 绝对路径校验，防止相对路径穿越
    let src = std::path::Path::new(source);
    if !src.is_absolute() {
        return Err(crate::AppError::InvalidInput(format!(
            "背景图片必须是绝对路径: {source}"
        )));
    }

    // 校验扩展名，防止复制任意文件
    let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .ok_or_else(|| crate::AppError::InvalidInput(format!("文件缺少扩展名，无法识别图片格式: {source}")))?;
    if !TERMINAL_BG_EXTENSIONS.contains(&ext.as_str()) {
        return Err(crate::AppError::InvalidInput(format!("不支持的图片格式: {ext}")));
    }

    // 限制文件大小，避免超大图片占用过多存储
    let metadata =
        std::fs::metadata(src).map_err(|e| crate::AppError::Config(format!("读取图片文件信息失败 {source}: {e}")))?;
    if metadata.len() > TERMINAL_BG_MAX_BYTES {
        return Err(crate::AppError::InvalidInput(format!(
            "图片文件过大（{} 字节），上限 {} 字节",
            metadata.len(),
            TERMINAL_BG_MAX_BYTES
        )));
    }

    Ok((src.to_path_buf(), ext))
}

// ==================== Utility Commands ====================

/// 测试命令
#[tauri::command]
pub fn ping() -> String {
    "pong".to_string()
}

/// 获取应用版本
#[tauri::command]
pub fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// 获取自应用启动以来的耗时（毫秒）
#[tauri::command]
pub fn get_startup_time(start_time: State<'_, crate::AppStartTime>) -> u64 {
    start_time.0.elapsed().as_millis() as u64
}

// ==================== Window Close Commands ====================

/// 用户确认关闭窗口（前端确认弹窗后调用）
///
/// 使用 destroy() 直接销毁窗口，不再次触发 CloseRequested
#[tauri::command]
pub fn confirm_window_close(app_handle: tauri::AppHandle) -> Result<()> {
    if let Some(window) = app_handle.get_webview_window("main") {
        window.destroy().map_err(|e| crate::AppError::Internal(e.to_string()))?;
    }
    Ok(())
}

// ==================== Opener Commands ====================

// 宿主设置页的「打开日志目录」入口（外壳命令，不经插件权限链）。
//
// 平台定位实现本体在引擎模块 [`crate::system::opener`]——同一份实现也被插件
// 原语 `host-platform.reveal-in-dir` 使用（ABI v22）。原
// `plugin_reveal_in_dir` 命令 + `system:open` 权限 + 前端 `context.system`
// 插件 API 桥已随该原语化一并退役（见
// `.scratch/2026-09-21-host-rust-residue/issues/04`）。

/// 打开日志目录（设置页「打开日志目录」按钮；独立命令，无需插件权限）
///
/// 复用 `system::opener::reveal_in_dir` 的平台分发（Windows COM / macOS Finder /
/// Linux xdg-open）
#[tauri::command]
pub fn open_log_dir() -> Result<()> {
    let setup = crate::system::logging::global_setup()
        .ok_or_else(|| crate::AppError::Config("logging not initialized yet".to_string()))?;
    let dir = &setup.log_dir;
    if !dir.exists() {
        return Err(crate::AppError::NotFound(format!(
            "log directory not found: {}",
            dir.display()
        )));
    }
    crate::system::opener::reveal_in_dir(dir)
        .map_err(|e| crate::AppError::Internal(format!("open log directory '{}' failed: {e}", dir.display())))
}

// ==================== Device Connection Commands ====================

/// 已连接设备清单（**引擎事实**：连接注册表原始记录）
///
/// host-business-decarriage 收尾：本命令只回连接注册表事实（addr / device_id /
/// fingerprint），不再拼装「设备派生视图」（在线判定 + 真实会话数 + 任务状态合并）。
/// 派生视图是业务，归属 `com.bedcode.terminal-session` 插件（api `devices-connect-list` /
/// 命令面 `session.devices.connect-list`，供插件设备中心消费）；宿主侧的消费方
/// （`useGlobalNotifications` 启动期指纹种子化）只需要事实字段，`session_count`
/// 是已退役设备页的产物。
///
/// 因此本命令**无插件依赖**：插件未激活时依然可用（`session_count` 字段不再
/// 由本路径产出，保持 0 以维持前端类型形状）。
#[tauri::command]
pub async fn get_connected_devices(
    _host: State<'_, Arc<crate::plugin::PluginHost>>,
) -> Result<Vec<crate::server::DeviceConnectionInfo>> {
    let manager = crate::server::websocket::WebSocketManager::global();
    let clients = manager.list_clients().await;
    let devices = clients
        .into_iter()
        .map(|c| crate::server::DeviceConnectionInfo {
            addr: c.addr,
            device_id: c.client_id,
            fingerprint: c.fingerprint,
            // 派生计数在插件侧；本命令只回引擎事实
            session_count: 0,
        })
        .collect();
    Ok(devices)
}

// ==================== Dev Console Log Relay ====================

// 开发者前端控制台日志转发（仅 `tauri dev` 注册，release 不包含）。
//
// 前端在 dev 构建下将 `console.*` 输出经 IPC 批量转发到此命令，写进
// runtime.*.log（target=`frontend`），与 Rust 日志合并同一份文件——
// AI agent 排查前端问题时直接 grep 日志目录即可，无需独立 CLI。
//
// 前端实现见 `src/utils/frontendLogger.ts`（loglevel methodFactory 接管，dev 下先落
// DevTools 控制台再批量转发；旧 devConsoleRelay 覆盖方案已随 loglevel 迁移移除）。

/// 单条日志的最大字符数，超过部分截断（防止异常超长消息撑爆日志文件）
pub const MAX_MESSAGE_LEN: usize = 16 * 1024;

/// 前端 console 转发的单条日志记录
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontendLogEntry {
    /// 日志级别：debug / info / warn / error（前端已归一化）
    pub level: String,
    /// 日志内容（前端多参数已拼接为单条字符串）
    pub message: String,
}

/// 将前端日志级别归一化为 tracing 级别；未知级别回退 debug
///
/// 回退 debug 而不是丢弃，保证信息不丢——dev 构建 runtime 日志 file_level
/// 强制 debug，trace 级不会落盘，故 console.log 也下沉到 debug 而非 trace。
pub fn normalized_level(level: &str) -> &'static str {
    match level {
        "info" => "info",
        "warn" => "warn",
        "error" => "error",
        // "log" / "debug" / 未知级别统一走 debug
        _ => "debug",
    }
}

/// 超长消息截断（按字符边界回溯，避免切在多字节 UTF-8 中间 panic）
pub fn truncate_message(message: &str) -> &str {
    if message.len() <= MAX_MESSAGE_LEN {
        return message;
    }
    let mut end = MAX_MESSAGE_LEN;
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    &message[..end]
}

/// 接收前端批量转发的 console 日志并写入 tracing 落盘（runtime.*.log 与 frontend.*.log）
#[cfg(debug_assertions)]
#[tauri::command]
pub fn report_frontend_log(logs: Vec<FrontendLogEntry>) {
    for entry in logs {
        if entry.message.is_empty() {
            continue;
        }
        let message = truncate_message(&entry.message);
        match normalized_level(&entry.level) {
            "info" => tracing::info!(target: "frontend", "{message}"),
            "warn" => tracing::warn!(target: "frontend", "{message}"),
            "error" => tracing::error!(target: "frontend", "{message}"),
            _ => tracing::debug!(target: "frontend", "{message}"),
        }
    }
}

// ==================== Plugin Commands ====================

// 插件系统 Tauri 命令 — 重新导出 api_bridge 中的所有命令

pub use crate::plugin::api_bridge::*;

// ==================== Server Control Commands ====================

// Tauri commands for server lifecycle management and metrics query

/// 启动服务器
#[tauri::command]
pub async fn server_start(app_handle: tauri::AppHandle, port: u16) -> Result<()> {
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

// ==================== 链路加密配置（issue 01） ====================

/// 读取链路加密配置（运行期快照，spec §6）
#[tauri::command]
pub fn get_traffic_encryption_config() -> LinkCryptoConfig {
    link_crypto::current_config()
}

/// 更新链路加密配置：先持久化成功再热更新快照并同步过滤器注册；
/// 落库失败不影响运行态（快照保持原值）
#[tauri::command]
pub async fn set_traffic_encryption_config(
    db: State<'_, Arc<tokio::sync::Mutex<Database>>>,
    config: LinkCryptoConfig,
) -> Result<()> {
    {
        let guard = db.lock().await;
        link_crypto::persist_config_to_db(&guard, &config)?;
    }
    link_crypto::update_config(config);
    link_crypto::sync_registration();
    Ok(())
}

/// 本机链路加密身份指纹（SHA-256 前 16 hex；未初始化时懒生成）
#[tauri::command]
pub async fn get_link_crypto_fingerprint(app_handle: tauri::AppHandle) -> Result<String> {
    link_crypto::ensure_identity_fingerprint(&app_handle).await
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
pub async fn update_server_port(app_handle: tauri::AppHandle, port: u16) -> Result<()> {
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
pub async fn update_server_auto_start(app_handle: tauri::AppHandle, auto_start: bool) -> Result<()> {
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
pub async fn update_server_network_config(app_handle: tauri::AppHandle, network_config: NetworkConfig) -> Result<()> {
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
pub async fn reset_server_network_config(app_handle: tauri::AppHandle) -> Result<NetworkConfig> {
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

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_level_maps_known_and_unknown() {
        assert_eq!(normalized_level("debug"), "debug");
        assert_eq!(normalized_level("info"), "info");
        assert_eq!(normalized_level("warn"), "warn");
        assert_eq!(normalized_level("error"), "error");
        // console.log 与未知级别均下沉 debug，保证 dev 落盘不丢信息
        assert_eq!(normalized_level("log"), "debug");
        assert_eq!(normalized_level("trace"), "debug");
        assert_eq!(normalized_level("whatever"), "debug");
    }

    #[test]
    fn truncate_message_keeps_short_input_unchanged() {
        assert_eq!(truncate_message("short message"), "short message");
        assert_eq!(truncate_message(""), "");
    }

    #[test]
    fn truncate_message_limits_long_input() {
        let long = "x".repeat(MAX_MESSAGE_LEN + 10);
        let truncated = truncate_message(&long);
        assert_eq!(truncated.len(), MAX_MESSAGE_LEN);
    }

    #[test]
    fn truncate_message_does_not_split_multibyte_char() {
        // 构造长度略超上限、且截断点落在多字节字符中间的字符串
        // "a" 占 1 字节；"😀" 占 4 字节；末尾推到 MAX_MESSAGE_LEN+4，使边界切进 emoji
        let s = "a".repeat(MAX_MESSAGE_LEN - 1) + "😀😀";
        let truncated = truncate_message(&s);
        assert!(truncated.len() <= MAX_MESSAGE_LEN);
        assert!(truncated.is_char_boundary(truncated.len()));
        assert!(!truncated.ends_with('\u{FFFD}'));
    }

    #[test]
    fn validate_bg_source_rejects_relative_path() {
        let err = validate_terminal_bg_source("images/bg.png").unwrap_err();
        assert!(err.to_string().contains("绝对路径"), "unexpected: {err}");
    }

    #[test]
    fn validate_bg_source_rejects_bad_extension() {
        let err = validate_terminal_bg_source("/tmp/bg.exe").unwrap_err();
        assert!(err.to_string().contains("不支持的图片格式"), "unexpected: {err}");
    }

    #[test]
    fn validate_bg_source_rejects_missing_file() {
        let err = validate_terminal_bg_source("/tmp/nonexistent-bg-xyz.png").unwrap_err();
        assert!(err.to_string().contains("读取图片文件信息失败"), "unexpected: {err}");
    }

    #[test]
    fn validate_bg_source_accepts_existing_absolute_file() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("bedcode-bg-test-{}.png", std::process::id()));
        std::fs::write(&path, b"fake-png-bytes").unwrap();
        let (src, ext) = validate_terminal_bg_source(path.to_str().unwrap()).unwrap();
        assert_eq!(ext, "png");
        assert!(src.is_absolute());
        std::fs::remove_file(&path).unwrap();
    }
}
