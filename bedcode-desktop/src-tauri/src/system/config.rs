//! 应用配置管理
//!
//! 提供桌面端应用的参数化配置，配置文件位于应用数据目录下的 config.properties
//! 使用 properties 格式支持注释，方便用户理解和修改配置

use std::collections::HashMap;
use std::path::PathBuf;

/// 全局配置单例
static CONFIG_INSTANCE: std::sync::OnceLock<AppConfig> = std::sync::OnceLock::new();

/// 配置文件中每个 key 对应的注释说明
static PROPERTY_COMMENTS: &[(&str, &str)] = &[
    ("network.port", "WebSocket 服务器端口"),
    ("network.auto_start", "应用启动时是否自动开启服务器"),
    ("network.prevent_sleep", "服务器运行时阻止系统休眠"),
    ("network.workers", "Actix Web worker 线程数（0 = CPU 核心数）"),
    ("network.keep_alive_secs", "HTTP Keep-Alive 超时秒数（0 = 禁用）"),
    ("network.client_request_timeout_secs", "客户端请求头读取超时秒数"),
    ("network.client_disconnect_timeout_secs", "客户端断开连接等待超时秒数"),
    ("network.max_connections", "每 worker 最大并发连接数"),
    ("network.backlog", "TCP 半连接队列上限"),
    ("network.tcp_nodelay", "启用 TCP_NODELAY（禁用 Nagle 算法，降低小包延迟）"),
    ("network.shutdown_timeout_secs", "优雅停机超时秒数"),
    ("network.ws_max_frame_size_kb", "WebSocket 单帧最大大小（KB）"),
    ("network.ws_max_message_size_mb", "WebSocket 单消息最大大小（MB，可跨多帧）"),
    ("session.default_environment", "默认执行环境（windows / wsl2）"),
    ("session.default_wsl_distro", "默认 WSL 发行版（仅 wsl2 环境有效，留空则使用默认发行版）"),
    ("session.default_working_dir", "默认工作目录（留空则使用用户主目录）"),
    ("session.default_command", "默认启动命令"),
    ("session.session_timeout", "会话超时时间（秒）- 无活动自动关闭"),
    ("ui.theme", "主题（light / dark / system）"),
    ("ui.terminal_font_size", "终端字体大小"),
    ("ui.terminal_font_family", "终端字体名称"),
    ("ui.terminal_theme", "终端配色主题名"),
    ("ui.show_preview", "是否显示终端预览"),
    ("ui.language", "语言偏好（zh-CN / en）"),
    ("channels.output_broadcast_capacity", "PTY 输出事件广播容量 - 用于转发终端输出到前端"),
    ("channels.status_broadcast_capacity", "会话状态变更广播容量 - 用于通知状态更新"),
    ("channels.restart_broadcast_capacity", "会话重启事件广播容量 - 用于通知会话重启"),
    ("channels.event_broadcast_capacity", "统一事件广播容量 - 整合所有事件类型"),
    ("channels.pty_subscription_capacity", "PTY 订阅广播容量 - 用于移动端订阅输出"),
    ("channels.global_queue_capacity", "全局输出队列容量 - 存储历史输出供移动端回放"),
    ("channels.ws_event_capacity", "WebSocket 事件广播容量 - 业务层事件分发"),
    ("channels.lifecycle_capacity", "生命周期事件广播容量 - PTY 进程状态变更"),
    ("terminal.default_cols", "默认终端列数"),
    ("terminal.default_rows", "默认终端行数"),
    ("terminal.flush_interval_ms", "输出缓冲刷新间隔（毫秒）- 合并多条输出减少 WebSocket 消息数"),
    ("terminal.max_buffer_size", "最大输出缓冲大小（字节）- 达到此大小立即刷新"),
    ("terminal.read_buffer_size", "PTY 读取缓冲区大小（字节）- 单次读取的最大字节数"),
    ("plugin.token", "HTTP API 认证 token - 插件推送任务状态时需携带此 token，为空时跳过验证（开发模式）"),
];

/// 配置 key 的分组顺序，控制写入文件时的排列
static PROPERTY_GROUPS: &[(&str, &[&str])] = &[
    ("网络配置", &[
        "network.port",
        "network.auto_start",
        "network.prevent_sleep",
        "network.workers",
        "network.keep_alive_secs",
        "network.client_request_timeout_secs",
        "network.client_disconnect_timeout_secs",
        "network.max_connections",
        "network.backlog",
        "network.tcp_nodelay",
        "network.shutdown_timeout_secs",
        "network.ws_max_frame_size_kb",
        "network.ws_max_message_size_mb",
    ]),
    ("会话默认配置", &[
        "session.default_environment",
        "session.default_wsl_distro",
        "session.default_working_dir",
        "session.default_command",
        "session.session_timeout",
    ]),
    ("UI 界面配置", &[
        "ui.theme",
        "ui.terminal_font_size",
        "ui.terminal_font_family",
        "ui.terminal_theme",
        "ui.show_preview",
        "ui.language",
    ]),
    ("Channel 容量配置", &[
        "channels.output_broadcast_capacity",
        "channels.status_broadcast_capacity",
        "channels.restart_broadcast_capacity",
        "channels.event_broadcast_capacity",
        "channels.pty_subscription_capacity",
        "channels.global_queue_capacity",
        "channels.ws_event_capacity",
        "channels.lifecycle_capacity",
    ]),
    ("终端配置", &[
        "terminal.default_cols",
        "terminal.default_rows",
        "terminal.flush_interval_ms",
        "terminal.max_buffer_size",
        "terminal.read_buffer_size",
    ]),
    ("插件配置", &[
        "plugin.token",
    ]),
];

/// 应用配置
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AppConfig {
    /// 网络配置
    pub network: NetworkConfig,
    /// 会话默认配置
    #[serde(default)]
    pub session: SessionConfig,
    /// UI 界面配置
    #[serde(default)]
    pub ui: UiConfig,
    /// Channel 容量配置
    #[serde(default)]
    pub channels: ChannelsConfig,
    /// 终端配置
    #[serde(default)]
    pub terminal: TerminalConfig,
    /// 插件配置
    #[serde(default)]
    pub plugin: PluginConfig,
}

/// 网络配置
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NetworkConfig {
    /// WebSocket 服务器端口
    pub port: u16,
    /// 应用启动时是否自动开启服务器
    pub auto_start: bool,
    /// 服务器运行时阻止系统休眠
    #[serde(default = "default_prevent_sleep")]
    pub prevent_sleep: bool,
    /// Actix Web worker 线程数（0 = CPU 核心数）
    #[serde(default)]
    pub workers: usize,
    /// HTTP Keep-Alive 超时秒数（0 = 禁用）
    #[serde(default = "default_keep_alive_secs")]
    pub keep_alive_secs: u64,
    /// 客户端请求头读取超时秒数
    #[serde(default = "default_client_request_timeout_secs")]
    pub client_request_timeout_secs: u64,
    /// 客户端断开连接等待超时秒数
    #[serde(default = "default_client_disconnect_timeout_secs")]
    pub client_disconnect_timeout_secs: u64,
    /// 每 worker 最大并发连接数
    #[serde(default = "default_max_connections")]
    pub max_connections: usize,
    /// TCP 半连接队列上限
    #[serde(default = "default_backlog")]
    pub backlog: u32,
    /// 启用 TCP_NODELAY
    #[serde(default = "default_tcp_nodelay")]
    pub tcp_nodelay: bool,
    /// 优雅停机超时秒数
    #[serde(default = "default_shutdown_timeout_secs")]
    pub shutdown_timeout_secs: u64,
    /// WebSocket 单帧最大大小（KB）
    #[serde(default = "default_ws_max_frame_size_kb")]
    pub ws_max_frame_size_kb: usize,
    /// WebSocket 单消息最大大小（MB）
    #[serde(default = "default_ws_max_message_size_mb")]
    pub ws_max_message_size_mb: usize,
}

fn default_prevent_sleep() -> bool {
    true
}

fn default_keep_alive_secs() -> u64 { 5 }
fn default_client_request_timeout_secs() -> u64 { 5 }
fn default_client_disconnect_timeout_secs() -> u64 { 5 }
fn default_max_connections() -> usize { 25000 }
fn default_backlog() -> u32 { 2048 }
fn default_tcp_nodelay() -> bool { true }
fn default_shutdown_timeout_secs() -> u64 { 30 }
fn default_ws_max_frame_size_kb() -> usize { 64 }
fn default_ws_max_message_size_mb() -> usize { 16 }

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            port: 8765,
            auto_start: true,
            prevent_sleep: true,
            workers: 0,
            keep_alive_secs: default_keep_alive_secs(),
            client_request_timeout_secs: default_client_request_timeout_secs(),
            client_disconnect_timeout_secs: default_client_disconnect_timeout_secs(),
            max_connections: default_max_connections(),
            backlog: default_backlog(),
            tcp_nodelay: default_tcp_nodelay(),
            shutdown_timeout_secs: default_shutdown_timeout_secs(),
            ws_max_frame_size_kb: default_ws_max_frame_size_kb(),
            ws_max_message_size_mb: default_ws_max_message_size_mb(),
        }
    }
}

/// 会话默认配置
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionConfig {
    /// 默认执行环境（windows/wsl2）
    pub default_environment: String,
    /// 默认 WSL 发行版（仅 wsl2 环境有效）
    pub default_wsl_distro: Option<String>,
    /// 默认工作目录
    pub default_working_dir: Option<String>,
    /// 默认启动命令
    pub default_command: Option<String>,
    /// 会话超时时间（秒）
    pub session_timeout: u64,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            default_environment: "windows".to_string(),
            default_wsl_distro: None,
            default_working_dir: None,
            default_command: Some("claude".to_string()),
            session_timeout: 3600,
        }
    }
}

/// UI 界面配置
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UiConfig {
    /// 主题（light/dark/system）
    pub theme: String,
    /// 终端字体大小
    pub terminal_font_size: u8,
    /// 终端字体名称
    pub terminal_font_family: String,
    /// 终端配色主题名
    #[serde(default = "default_terminal_theme")]
    pub terminal_theme: String,
    /// 是否显示终端预览
    pub show_preview: bool,
    /// 语言偏好（zh-CN / en）
    #[serde(default = "default_language")]
    pub language: String,
}

fn default_terminal_theme() -> String {
    "dracula".to_string()
}

fn default_language() -> String {
    "zh-CN".to_string()
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: "system".to_string(),
            terminal_font_size: 12,
            terminal_font_family: "Consolas".to_string(),
            terminal_theme: default_terminal_theme(),
            show_preview: true,
            language: default_language(),
        }
    }
}

/// Channel 容量配置
///
/// 控制 Tokio broadcast/mpsc channel 的缓冲区大小，
/// 影响高负载场景下的消息处理能力和内存占用
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChannelsConfig {
    /// PTY 输出事件广播容量 - 用于转发终端输出到前端
    pub output_broadcast_capacity: usize,
    /// 会话状态变更广播容量 - 用于通知状态更新
    pub status_broadcast_capacity: usize,
    /// 会话重启事件广播容量 - 用于通知会话重启
    pub restart_broadcast_capacity: usize,
    /// 统一事件广播容量 - 整合所有事件类型
    pub event_broadcast_capacity: usize,
    /// PTY 订阅广播容量 - 用于移动端订阅输出
    pub pty_subscription_capacity: usize,
    /// 全局输出队列容量 - 存储历史输出供移动端回放
    pub global_queue_capacity: usize,
    /// WebSocket 事件广播容量 - 业务层事件分发
    pub ws_event_capacity: usize,
    /// 生命周期事件广播容量 - PTY 进程状态变更
    pub lifecycle_capacity: usize,
}

impl Default for ChannelsConfig {
    fn default() -> Self {
        Self {
            output_broadcast_capacity: 2048,
            status_broadcast_capacity: 64,
            restart_broadcast_capacity: 64,
            event_broadcast_capacity: 256,
            pty_subscription_capacity: 1024,
            global_queue_capacity: 50000,
            ws_event_capacity: 1024,
            lifecycle_capacity: 16,
        }
    }
}

/// 终端配置
///
/// 控制 PTY 终端的默认参数和输出处理行为
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TerminalConfig {
    /// 默认终端列数
    pub default_cols: u16,
    /// 默认终端行数
    pub default_rows: u16,
    /// 输出缓冲刷新间隔（毫秒）- 合并多条输出减少 WebSocket 消息数
    pub flush_interval_ms: u64,
    /// 最大输出缓冲大小（字节）- 达到此大小立即刷新
    pub max_buffer_size: usize,
    /// PTY 读取缓冲区大小（字节）- 单次读取的最大字节数
    pub read_buffer_size: usize,
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            default_cols: 120,
            default_rows: 40,
            flush_interval_ms: 100,
            max_buffer_size: 64 * 1024,
            read_buffer_size: 4096,
        }
    }
}

/// 插件配置
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PluginConfig {
    /// HTTP API 认证 token - 插件推送任务状态时需携带此 token
    /// 为空时跳过验证（开发模式）
    #[serde(default)]
    pub token: String,
}

impl Default for PluginConfig {
    fn default() -> Self {
        Self {
            token: String::new(),
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            network: NetworkConfig::default(),
            session: SessionConfig::default(),
            ui: UiConfig::default(),
            channels: ChannelsConfig::default(),
            terminal: TerminalConfig::default(),
            plugin: PluginConfig::default(),
        }
    }
}

impl AppConfig {
    /// 从 properties 文件加载配置
    ///
    /// 文件不存在时返回默认配置
    pub fn load(path: &PathBuf) -> crate::Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(path)?;
        let props = parse_properties(&content);
        Ok(Self::from_properties(&props))
    }

    /// 保存配置到 properties 文件
    pub fn save(&self, path: &PathBuf) -> crate::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = self.to_properties_string();
        std::fs::write(path, content)?;
        Ok(())
    }

    /// 初始化全局配置单例
    ///
    /// 应在应用启动时调用，传入从文件加载的配置
    pub fn init(config: AppConfig) {
        let _ = CONFIG_INSTANCE.set(config);
    }

    /// 确保 plugin token 合法，不合法则生成新 token
    ///
    /// 合法条件：非空、长度 >= 16、纯 ASCII
    /// 返回 true 表示新生成了 token
    pub fn ensure_valid_token(&mut self) -> bool {
        let is_valid = !self.plugin.token.is_empty()
            && self.plugin.token.len() >= 16
            && self.plugin.token.is_ascii();

        if !is_valid {
            self.plugin.token = Self::generate_token();
            tracing::info!(
                "Generated new plugin token (len={})",
                self.plugin.token.len()
            );
            true
        } else {
            false
        }
    }

    /// 生成随机 token（UUID v4 去连字符，32 字符 hex）
    fn generate_token() -> String {
        uuid::Uuid::new_v4().to_string().replace('-', "")
    }

    /// 保存配置到指定路径
    pub fn save_to(&self, path: &PathBuf) -> crate::Result<()> {
        self.save(path)
    }

    /// 获取全局配置实例
    ///
    /// 如果未初始化，返回默认配置
    pub fn global() -> &'static AppConfig {
        static DEFAULT: std::sync::LazyLock<AppConfig> =
            std::sync::LazyLock::new(AppConfig::default);
        CONFIG_INSTANCE.get().unwrap_or(&DEFAULT)
    }

    /// 从 properties map 构建配置
    fn from_properties(props: &HashMap<String, String>) -> Self {
        Self {
            network: NetworkConfig {
                port: parse_value(props, "network.port", 8765),
                auto_start: parse_value(props, "network.auto_start", true),
                prevent_sleep: parse_value(props, "network.prevent_sleep", true),
                workers: parse_value(props, "network.workers", 0),
                keep_alive_secs: parse_value(props, "network.keep_alive_secs", default_keep_alive_secs()),
                client_request_timeout_secs: parse_value(props, "network.client_request_timeout_secs", default_client_request_timeout_secs()),
                client_disconnect_timeout_secs: parse_value(props, "network.client_disconnect_timeout_secs", default_client_disconnect_timeout_secs()),
                max_connections: parse_value(props, "network.max_connections", default_max_connections()),
                backlog: parse_value(props, "network.backlog", default_backlog()),
                tcp_nodelay: parse_value(props, "network.tcp_nodelay", default_tcp_nodelay()),
                shutdown_timeout_secs: parse_value(props, "network.shutdown_timeout_secs", default_shutdown_timeout_secs()),
                ws_max_frame_size_kb: parse_value(props, "network.ws_max_frame_size_kb", default_ws_max_frame_size_kb()),
                ws_max_message_size_mb: parse_value(props, "network.ws_max_message_size_mb", default_ws_max_message_size_mb()),
            },
            session: SessionConfig {
                default_environment: parse_value(props, "session.default_environment", "windows".to_string()),
                default_wsl_distro: parse_optional(props, "session.default_wsl_distro"),
                default_working_dir: parse_optional(props, "session.default_working_dir"),
                default_command: parse_optional(props, "session.default_command"),
                session_timeout: parse_value(props, "session.session_timeout", 3600),
            },
            ui: UiConfig {
                theme: parse_value(props, "ui.theme", "system".to_string()),
                terminal_font_size: parse_value(props, "ui.terminal_font_size", 12),
                terminal_font_family: parse_value(props, "ui.terminal_font_family", "Consolas".to_string()),
                terminal_theme: parse_value(props, "ui.terminal_theme", default_terminal_theme()),
                show_preview: parse_value(props, "ui.show_preview", true),
                language: parse_value(props, "ui.language", default_language()),
            },
            channels: ChannelsConfig {
                output_broadcast_capacity: parse_value(props, "channels.output_broadcast_capacity", 2048),
                status_broadcast_capacity: parse_value(props, "channels.status_broadcast_capacity", 64),
                restart_broadcast_capacity: parse_value(props, "channels.restart_broadcast_capacity", 64),
                event_broadcast_capacity: parse_value(props, "channels.event_broadcast_capacity", 256),
                pty_subscription_capacity: parse_value(props, "channels.pty_subscription_capacity", 1024),
                global_queue_capacity: parse_value(props, "channels.global_queue_capacity", 50000),
                ws_event_capacity: parse_value(props, "channels.ws_event_capacity", 1024),
                lifecycle_capacity: parse_value(props, "channels.lifecycle_capacity", 16),
            },
            terminal: TerminalConfig {
                default_cols: parse_value(props, "terminal.default_cols", 120),
                default_rows: parse_value(props, "terminal.default_rows", 40),
                flush_interval_ms: parse_value(props, "terminal.flush_interval_ms", 100),
                max_buffer_size: parse_value(props, "terminal.max_buffer_size", 65536),
                read_buffer_size: parse_value(props, "terminal.read_buffer_size", 4096),
            },
            plugin: PluginConfig {
                token: parse_value(props, "plugin.token", String::new()),
            },
        }
    }

    /// 将配置序列化为 properties 格式字符串
    fn to_properties_string(&self) -> String {
        let values = self.to_property_values();
        let mut lines = Vec::new();

        for (group_name, keys) in PROPERTY_GROUPS {
            lines.push(format!("# ==================== {} ====================", group_name));
            for key in *keys {
                // 写入注释说明
                if let Some((_, comment)) = PROPERTY_COMMENTS.iter().find(|(k, _)| *k == *key) {
                    lines.push(format!("# {}", comment));
                }
                // 写入 key=value
                let value = values.get(*key).map(|s| s.as_str()).unwrap_or("");
                lines.push(format!("{}={}", key, value));
                lines.push(String::new());
            }
        }

        lines.join("\n")
    }

    /// 将配置转为 key-value map
    fn to_property_values(&self) -> HashMap<String, String> {
        let mut map = HashMap::new();
        map.insert("network.port".to_string(), self.network.port.to_string());
        map.insert("network.auto_start".to_string(), self.network.auto_start.to_string());
        map.insert("network.prevent_sleep".to_string(), self.network.prevent_sleep.to_string());
        map.insert("network.workers".to_string(), self.network.workers.to_string());
        map.insert("network.keep_alive_secs".to_string(), self.network.keep_alive_secs.to_string());
        map.insert("network.client_request_timeout_secs".to_string(), self.network.client_request_timeout_secs.to_string());
        map.insert("network.client_disconnect_timeout_secs".to_string(), self.network.client_disconnect_timeout_secs.to_string());
        map.insert("network.max_connections".to_string(), self.network.max_connections.to_string());
        map.insert("network.backlog".to_string(), self.network.backlog.to_string());
        map.insert("network.tcp_nodelay".to_string(), self.network.tcp_nodelay.to_string());
        map.insert("network.shutdown_timeout_secs".to_string(), self.network.shutdown_timeout_secs.to_string());
        map.insert("network.ws_max_frame_size_kb".to_string(), self.network.ws_max_frame_size_kb.to_string());
        map.insert("network.ws_max_message_size_mb".to_string(), self.network.ws_max_message_size_mb.to_string());
        map.insert("session.default_environment".to_string(), self.session.default_environment.clone());
        map.insert("session.default_wsl_distro".to_string(), self.session.default_wsl_distro.clone().unwrap_or_default());
        map.insert("session.default_working_dir".to_string(), self.session.default_working_dir.clone().unwrap_or_default());
        map.insert("session.default_command".to_string(), self.session.default_command.clone().unwrap_or_default());
        map.insert("session.session_timeout".to_string(), self.session.session_timeout.to_string());
        map.insert("ui.theme".to_string(), self.ui.theme.clone());
        map.insert("ui.terminal_font_size".to_string(), self.ui.terminal_font_size.to_string());
        map.insert("ui.terminal_font_family".to_string(), self.ui.terminal_font_family.clone());
        map.insert("ui.terminal_theme".to_string(), self.ui.terminal_theme.clone());
        map.insert("ui.show_preview".to_string(), self.ui.show_preview.to_string());
        map.insert("ui.language".to_string(), self.ui.language.clone());
        map.insert("channels.output_broadcast_capacity".to_string(), self.channels.output_broadcast_capacity.to_string());
        map.insert("channels.status_broadcast_capacity".to_string(), self.channels.status_broadcast_capacity.to_string());
        map.insert("channels.restart_broadcast_capacity".to_string(), self.channels.restart_broadcast_capacity.to_string());
        map.insert("channels.event_broadcast_capacity".to_string(), self.channels.event_broadcast_capacity.to_string());
        map.insert("channels.pty_subscription_capacity".to_string(), self.channels.pty_subscription_capacity.to_string());
        map.insert("channels.global_queue_capacity".to_string(), self.channels.global_queue_capacity.to_string());
        map.insert("channels.ws_event_capacity".to_string(), self.channels.ws_event_capacity.to_string());
        map.insert("channels.lifecycle_capacity".to_string(), self.channels.lifecycle_capacity.to_string());
        map.insert("terminal.default_cols".to_string(), self.terminal.default_cols.to_string());
        map.insert("terminal.default_rows".to_string(), self.terminal.default_rows.to_string());
        map.insert("terminal.flush_interval_ms".to_string(), self.terminal.flush_interval_ms.to_string());
        map.insert("terminal.max_buffer_size".to_string(), self.terminal.max_buffer_size.to_string());
        map.insert("terminal.read_buffer_size".to_string(), self.terminal.read_buffer_size.to_string());
        map.insert("plugin.token".to_string(), self.plugin.token.clone());
        map
    }
}

/// 解析 properties 文件内容为 key-value map
///
/// 支持 # 开头的注释行和空行，key=value 格式
/// 值两端的空白会被 trim
fn parse_properties(content: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in content.lines() {
        let trimmed = line.trim();
        // 跳过空行和注释
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        // 解析 key=value
        if let Some((key, value)) = trimmed.split_once('=') {
            let key = key.trim().to_string();
            let value = value.trim().to_string();
            if !key.is_empty() {
                map.insert(key, value);
            }
        }
    }
    map
}

/// 从 properties map 中解析值，不存在时返回默认值
fn parse_value<T: std::str::FromStr>(props: &HashMap<String, String>, key: &str, default: T) -> T {
    props.get(key).and_then(|v| v.parse().ok()).unwrap_or(default)
}

/// 从 properties map 中解析可选值，空字符串返回 None
fn parse_optional<T: std::str::FromStr>(props: &HashMap<String, String>, key: &str) -> Option<T> {
    props.get(key).and_then(|v| {
        if v.is_empty() {
            None
        } else {
            v.parse().ok()
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_properties_basic() {
        let content = r#"
# 这是一个注释
network.port=8765

# 另一个注释
channels.output_broadcast_capacity=2048
"#;
        let props = parse_properties(content);
        assert_eq!(props.get("network.port").unwrap(), "8765");
        assert_eq!(props.get("channels.output_broadcast_capacity").unwrap(), "2048");
    }

    #[test]
    fn test_parse_properties_ignores_comments() {
        let content = "# comment\nkey=value";
        let props = parse_properties(content);
        assert_eq!(props.len(), 1);
        assert_eq!(props.get("key").unwrap(), "value");
    }

    #[test]
    fn test_parse_properties_trims_whitespace() {
        let content = "  key  =  value  ";
        let props = parse_properties(content);
        assert_eq!(props.get("key").unwrap(), "value");
    }

    #[test]
    fn test_from_properties_defaults() {
        let props = HashMap::new();
        let config = AppConfig::from_properties(&props);
        assert_eq!(config.network.port, 8765);
        assert_eq!(config.network.auto_start, true);
        assert_eq!(config.session.default_environment, "windows");
        assert_eq!(config.ui.theme, "system");
        assert_eq!(config.ui.terminal_theme, "dracula");
    }

    #[test]
    fn test_from_properties_override() {
        let mut props = HashMap::new();
        props.insert("network.port".to_string(), "9999".to_string());
        props.insert("network.auto_start".to_string(), "false".to_string());
        props.insert("session.default_environment".to_string(), "wsl2".to_string());
        props.insert("ui.theme".to_string(), "dark".to_string());
        let config = AppConfig::from_properties(&props);
        assert_eq!(config.network.port, 9999);
        assert_eq!(config.network.auto_start, false);
        assert_eq!(config.session.default_environment, "wsl2");
        assert_eq!(config.ui.theme, "dark");
    }

    #[test]
    fn test_to_properties_roundtrip() {
        let config = AppConfig::default();
        let content = config.to_properties_string();
        let props = parse_properties(&content);
        let config2 = AppConfig::from_properties(&props);

        assert_eq!(config.network.port, config2.network.port);
        assert_eq!(config.network.auto_start, config2.network.auto_start);
        assert_eq!(config.session.default_environment, config2.session.default_environment);
        assert_eq!(config.ui.theme, config2.ui.theme);
        assert_eq!(config.ui.terminal_theme, config2.ui.terminal_theme);
        assert_eq!(config.channels.output_broadcast_capacity, config2.channels.output_broadcast_capacity);
        assert_eq!(config.terminal.default_cols, config2.terminal.default_cols);
    }

    #[test]
    fn test_ensure_valid_token() {
        let mut config = AppConfig::default();
        // 空 token 应生成新 token
        assert!(config.ensure_valid_token());
        assert!(config.plugin.token.len() >= 16);

        // 合法 token 不应重新生成
        assert!(!config.ensure_valid_token());
    }

    #[test]
    fn test_save_and_load() {
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let path = tmp_dir.path().join("config.properties");

        let config = AppConfig::default();
        config.save(&path).unwrap();

        let loaded = AppConfig::load(&path).unwrap();
        assert_eq!(config.network.port, loaded.network.port);
        assert_eq!(config.network.auto_start, loaded.network.auto_start);
        assert_eq!(config.channels.output_broadcast_capacity, loaded.channels.output_broadcast_capacity);
    }

    #[test]
    fn test_load_nonexistent_returns_default() {
        let path = PathBuf::from("/nonexistent/config.properties");
        let config = AppConfig::load(&path).unwrap();
        assert_eq!(config.network.port, 8765);
    }

    #[test]
    fn test_properties_output_has_comments() {
        let config = AppConfig::default();
        let content = config.to_properties_string();
        // 验证包含分组标题
        assert!(content.contains("# ==================== 网络配置 ===================="));
        // 验证包含字段注释
        assert!(content.contains("# WebSocket 服务器端口"));
        // 验证包含 key=value
        assert!(content.contains("network.port=8765"));
    }

    #[test]
    fn test_optional_fields_empty() {
        let mut props = HashMap::new();
        props.insert("session.default_wsl_distro".to_string(), String::new());
        props.insert("session.default_command".to_string(), "claude".to_string());
        let config = AppConfig::from_properties(&props);
        assert_eq!(config.session.default_wsl_distro, None);
        assert_eq!(config.session.default_command, Some("claude".to_string()));
    }

    #[test]
    fn test_removed_keys_ignored_on_load() {
        // 验证已移除的配置 key（output_history、display）不影响加载
        let mut props = HashMap::new();
        props.insert("network.port".to_string(), "9999".to_string());
        props.insert("output_history.ring_buffer_capacity".to_string(), "5000".to_string());
        props.insert("display.max_read_size".to_string(), "100".to_string());
        let config = AppConfig::from_properties(&props);
        // 有效的 key 正常读取
        assert_eq!(config.network.port, 9999);
        // 已移除的 key 被忽略，不影响 AppConfig 结构
    }
}
