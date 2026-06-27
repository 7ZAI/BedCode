//! 应用配置管理
//!
//! 提供桌面端应用的参数化配置，配置文件位于应用数据目录下的 config.json

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 全局配置单例
static CONFIG_INSTANCE: std::sync::OnceLock<AppConfig> = std::sync::OnceLock::new();

/// 应用配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// 网络配置
    pub network: NetworkConfig,
    /// 会话默认配置
    pub session: SessionConfig,
    /// UI 界面配置
    pub ui: UiConfig,
    /// Channel 容量配置
    #[serde(default)]
    pub channels: ChannelsConfig,
    /// 终端配置
    #[serde(default)]
    pub terminal: TerminalConfig,
    /// 输出历史配置
    #[serde(default)]
    pub output_history: OutputHistoryConfig,
    /// 插件配置
    #[serde(default)]
    pub plugin: PluginConfig,
    /// 显示配置
    #[serde(default)]
    pub display: DisplayConfig,
}

/// 网络配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// WebSocket 服务器端口
    pub port: u16,
    /// 心跳间隔（秒）- 客户端发送心跳的频率
    pub heartbeat_interval_secs: u64,
    /// 心跳超时（秒）- 超过此时间未收到心跳则断开连接
    pub heartbeat_timeout_secs: u64,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            port: 8765,
            heartbeat_interval_secs: 30,
            heartbeat_timeout_secs: 90,
        }
    }
}

/// 会话默认配置
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    /// 主题（light/dark/system）
    pub theme: String,
    /// 终端字体大小
    pub terminal_font_size: u8,
    /// 终端字体名称
    pub terminal_font_family: String,
    /// 是否显示终端预览
    pub show_preview: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: "system".to_string(),
            terminal_font_size: 12,
            terminal_font_family: "Consolas".to_string(),
            show_preview: true,
        }
    }
}

/// Channel 容量配置
///
/// 控制 Tokio broadcast/mpsc channel 的缓冲区大小，
/// 影响高负载场景下的消息处理能力和内存占用
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
            flush_interval_ms: 30,
            max_buffer_size: 64 * 1024,
            read_buffer_size: 4096,
        }
    }
}

/// 输出历史配置
///
/// 控制终端输出历史的存储参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputHistoryConfig {
    /// 环形缓冲区容量 - 存储最近 N 条 PTY 输出供历史回放
    pub ring_buffer_capacity: usize,
}

impl Default for OutputHistoryConfig {
    fn default() -> Self {
        Self {
            ring_buffer_capacity: 10000,
        }
    }
}

/// 插件配置
///
/// 控制 Claude Code 插件会话的行为参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    /// 文件轮询间隔（毫秒）- 监听 JSONL 日志文件的频率
    pub file_poll_interval_ms: u64,
    /// 心跳超时（秒）- 超过此时间未收到心跳则判定插件断开
    pub heartbeat_timeout_secs: u64,
    /// HTTP API 认证 token - 插件推送任务状态时需携带此 token
    /// 为空时跳过验证（开发模式）
    #[serde(default)]
    pub token: String,
}

impl Default for PluginConfig {
    fn default() -> Self {
        Self {
            file_poll_interval_ms: 500,
            heartbeat_timeout_secs: 90,
            token: String::new(),
        }
    }
}

/// 显示配置
///
/// 控制 Claude Code 消息日志的显示截断参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayConfig {
    /// 工具输入最大显示长度（字符）- 超出部分截断
    pub max_tool_input_display: usize,
    /// 工具结果最大显示长度（字符）- 超出部分截断
    pub max_tool_result_display: usize,
    /// 思考过程最大显示长度（字符）- 超出部分截断
    pub max_thinking_display: usize,
    /// JSONL 文件最大读取大小（字节）- 防止内存溢出
    pub max_read_size: u64,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            max_tool_input_display: 200,
            max_tool_result_display: 500,
            max_thinking_display: 300,
            max_read_size: 1024 * 1024,
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
            output_history: OutputHistoryConfig::default(),
            plugin: PluginConfig::default(),
            display: DisplayConfig::default(),
        }
    }
}

impl AppConfig {
    /// 从文件加载配置
    ///
    /// 文件不存在时返回默认配置
    pub fn load(path: &PathBuf) -> crate::Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(path)?;
        let config: Self = serde_json::from_str(&content)?;
        Ok(config)
    }

    /// 保存配置到文件
    pub fn save(&self, path: &PathBuf) -> crate::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(self)?;
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
}
