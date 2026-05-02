//! WebSocket Message Types
//!
//! 定义移动端和桌面端之间的通信协议

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// WebSocket 消息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Message {
    /// 输出消息 (服务端 → 客户端)
    #[serde(rename = "output")]
    Output {
        /// Unique message ID for request-response tracking
        #[serde(default = "generate_message_id")]
        message_id: String,
        session_id: String,
        timestamp: i64,
        payload: OutputPayload,
    },

    /// 输入消息 (客户端 → 服务端)
    #[serde(rename = "input")]
    Input {
        /// Unique message ID for request-response tracking
        #[serde(default = "generate_message_id")]
        message_id: String,
        session_id: String,
        timestamp: i64,
        payload: InputPayload,
    },

    /// 认证消息 (双向)
    #[serde(rename = "auth")]
    Auth {
        /// Unique message ID for request-response tracking
        #[serde(default = "generate_message_id")]
        message_id: String,
        session_id: Option<String>,
        timestamp: i64,
        payload: AuthPayload,
    },

    /// 控制消息 (双向)
    #[serde(rename = "control")]
    Control {
        /// Unique message ID for request-response tracking
        #[serde(default = "generate_message_id")]
        message_id: String,
        session_id: Option<String>,
        timestamp: i64,
        payload: ControlPayload,
    },

    /// 错误消息 (服务端 → 客户端)
    #[serde(rename = "error")]
    Error {
        /// Message ID this error relates to (if any)
        #[serde(skip_serializing_if = "Option::is_none")]
        message_id: Option<String>,
        code: String,
        message: String,
    },

    /// 心跳消息 (双向)
    #[serde(rename = "heartbeat")]
    Heartbeat {
        timestamp: i64,
    },
}

/// Generate a unique message ID
fn generate_message_id() -> String {
    Uuid::new_v4().to_string()
}

impl Message {
    /// 创建输出消息
    pub fn output(session_id: &str, data: &[u8], is_waiting: bool) -> Self {
        Message::Output {
            message_id: generate_message_id(),
            session_id: session_id.to_string(),
            timestamp: Utc::now().timestamp_millis(),
            payload: OutputPayload {
                data: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, data),
                is_waiting,
            },
        }
    }

    /// 创建输入消息
    pub fn input(session_id: &str, data: &str, special_key: Option<SpecialKey>) -> Self {
        Message::Input {
            message_id: generate_message_id(),
            session_id: session_id.to_string(),
            timestamp: Utc::now().timestamp_millis(),
            payload: InputPayload {
                data: data.to_string(),
                special_key,
            },
        }
    }

    /// 创建控制消息
    pub fn control(action: ControlAction, session_id: Option<&str>) -> Self {
        Message::Control {
            message_id: generate_message_id(),
            session_id: session_id.map(|s| s.to_string()),
            timestamp: Utc::now().timestamp_millis(),
            payload: ControlPayload { action },
        }
    }

    /// 创建错误消息
    pub fn error(code: &str, message: &str) -> Self {
        Message::Error {
            message_id: None,
            code: code.to_string(),
            message: message.to_string(),
        }
    }

    /// 创建错误消息（关联到特定消息ID）
    pub fn error_with_id(message_id: &str, code: &str, message: &str) -> Self {
        Message::Error {
            message_id: Some(message_id.to_string()),
            code: code.to_string(),
            message: message.to_string(),
        }
    }

    /// 创建心跳消息
    pub fn heartbeat() -> Self {
        Message::Heartbeat {
            timestamp: Utc::now().timestamp_millis(),
        }
    }

    /// 获取消息ID
    pub fn message_id(&self) -> Option<&str> {
        match self {
            Message::Output { message_id, .. } => Some(message_id),
            Message::Input { message_id, .. } => Some(message_id),
            Message::Auth { message_id, .. } => Some(message_id),
            Message::Control { message_id, .. } => Some(message_id),
            Message::Error { message_id, .. } => message_id.as_deref(),
            Message::Heartbeat { .. } => None,
        }
    }

    /// 序列化为 JSON
    pub fn to_json(&self) -> crate::Result<String> {
        Ok(serde_json::to_string(self)?)
    }

    /// 从 JSON 反序列化
    pub fn from_json(json: &str) -> crate::Result<Self> {
        Ok(serde_json::from_str(json)?)
    }
}

/// 输出载荷
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputPayload {
    /// Base64 编码的输出数据
    pub data: String,
    /// 是否等待输入
    pub is_waiting: bool,
}

/// 输入载荷
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputPayload {
    /// 输入数据
    pub data: String,
    /// 特殊键
    #[serde(skip_serializing_if = "Option::is_none")]
    pub special_key: Option<SpecialKey>,
}

/// 特殊键
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpecialKey {
    Tab,
    Enter,
    Escape,
    CtrlC,
    CtrlD,
    CtrlZ,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Backspace,
}

impl SpecialKey {
    pub fn as_str(&self) -> &'static str {
        match self {
            SpecialKey::Tab => "tab",
            SpecialKey::Enter => "enter",
            SpecialKey::Escape => "escape",
            SpecialKey::CtrlC => "ctrl_c",
            SpecialKey::CtrlD => "ctrl_d",
            SpecialKey::CtrlZ => "ctrl_z",
            SpecialKey::ArrowUp => "arrow_up",
            SpecialKey::ArrowDown => "arrow_down",
            SpecialKey::ArrowLeft => "arrow_left",
            SpecialKey::ArrowRight => "arrow_right",
            SpecialKey::Backspace => "backspace",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "tab" => Some(SpecialKey::Tab),
            "enter" => Some(SpecialKey::Enter),
            "escape" | "esc" => Some(SpecialKey::Escape),
            "ctrl_c" | "ctrlc" => Some(SpecialKey::CtrlC),
            "ctrl_d" | "ctrld" => Some(SpecialKey::CtrlD),
            "ctrl_z" | "ctrlz" => Some(SpecialKey::CtrlZ),
            "arrow_up" | "up" => Some(SpecialKey::ArrowUp),
            "arrow_down" | "down" => Some(SpecialKey::ArrowDown),
            "arrow_left" | "left" => Some(SpecialKey::ArrowLeft),
            "arrow_right" | "right" => Some(SpecialKey::ArrowRight),
            "backspace" => Some(SpecialKey::Backspace),
            _ => None,
        }
    }
}

/// 认证载荷
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthPayload {
    /// 认证阶段
    pub stage: AuthStage,
    /// 设备 ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    /// 设备名称
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_name: Option<String>,
    /// 设备指纹
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_fingerprint: Option<String>,
    /// 配对码
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pairing_code: Option<String>,
    /// 会话令牌
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_token: Option<String>,
    /// 错误消息
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// 认证阶段
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthStage {
    /// 请求配对
    RequestPairing,
    /// 配对码验证
    VerifyCode,
    /// 交换证书
    ExchangeCertificate,
    /// 认证成功
    Authenticated,
    /// 认证失败
    Failed,
}

/// 控制载荷
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlPayload {
    /// 控制动作
    pub action: ControlAction,
}

/// 控制动作
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControlAction {
    /// 列出会话
    ListSessions,
    /// 会话列表响应
    SessionList { sessions: Vec<SessionSummary> },
    /// 列出会话配置
    ListSessionConfigs,
    /// 会话配置列表响应
    SessionConfigList { configs: Vec<SessionConfigSummary> },
    /// 启动会话
    StartSession { config_id: String },
    /// 停止会话
    StopSession { session_id: String },
    /// 调整终端大小
    ResizeSession { session_id: String, cols: u16, rows: u16 },
    /// 列出快捷指令
    ListQuickActions,
    /// 快捷指令列表响应
    QuickActionList { actions: Vec<QuickActionSummary> },
    /// 加入会话，开始接收输出
    JoinSession { session_id: String },
    /// 离开会话，停止接收输出
    LeaveSession { session_id: String },
}

/// 会话摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    pub name: String,
    pub status: String,
}

/// 会话配置摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfigSummary {
    pub id: String,
    pub name: String,
    pub environment: String,
    pub wsl_distro: Option<String>,
    pub working_dir: String,
    pub command: String,
}

/// 快捷指令摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickActionSummary {
    pub id: String,
    pub name: String,
    pub content: String,
    pub icon: Option<String>,
    pub color: Option<String>,
}
