//! Mobile-to-Desktop Request Builders
//!
//! 封装移动端向桌面端发送的各类请求，按业务类型组织：
//! - AuthRequest: 认证相关请求
//! - SessionRequest: 会话管理请求
//! - TerminalRequest: 终端操作请求
//! - ConfigRequest: 配置查询请求

use std::time::Duration;

use crate::enums::auth::{AuthPayload, AuthStage};
use crate::enums::control::{SessionConfigAction, SessionControlAction};
use crate::enums::special_key::KeyCombo;
use crate::model::message::Message;
use crate::state::get_global_token;

/// 获取当前全局 Token 并应用到消息
fn with_token(message: Message) -> Message {
    let token = get_global_token();
    if token.is_empty() {
        message
    } else {
        message.with_token(&token)
    }
}

// ==================== Auth Requests ====================

/// 认证相关请求构建器
pub struct AuthRequest;

impl AuthRequest {
    /// 构建 JWT Token 重新认证消息（WS 首消息 JWT 认证）
    ///
    /// 认证已 HTTP 化后，移动端正常路径不再经 WS 握手——本构造器服务：
    /// ① 04 常驻事件 WS 建连后的首消息 JWT 认证；
    /// ② 集成测试驱动真实 WsClient→router→handler 链路。
    pub fn reauthenticate(device_id: &str, fingerprint: &str, session_token: &str) -> Message {
        Self::reauthenticate_with_crypto(device_id, fingerprint, session_token, None)
    }

    /// 构建 JWT 重新认证消息（可携链路加密提案，issue 09）
    ///
    /// `crypto` 为 Some 时首消息附带客户端临时 X25519 公钥；桌面端接受协商后
    /// 在 auth 响应明文回执服务端临时公钥，此后帧进入加密模式。
    pub fn reauthenticate_with_crypto(
        device_id: &str,
        fingerprint: &str,
        session_token: &str,
        crypto: Option<crate::enums::auth::CryptoProposal>,
    ) -> Message {
        with_token(Message::Auth {
            message_id: uuid::Uuid::new_v4().to_string(),
            expect_response: true,
            timestamp: chrono::Utc::now().timestamp_millis(),
            session_id: None,
            token: String::new(),
            payload: AuthPayload {
                stage: AuthStage::Reauthenticate,
                device_id: Some(device_id.to_string()),
                device_fingerprint: Some(fingerprint.to_string()),
                session_token: Some(session_token.to_string()),
                crypto,
                ..Default::default()
            },
        })
    }

    /// 构建生物凭证绑定/解绑消息
    ///
    /// 在已认证连接上注册公钥（绑定）或清空公钥（解绑，public_key 传空串）。
    /// ExchangeCertificate 无 HTTP 端点，绑定需已认证持久 WS（04 事件 WS 落地
    /// 后接线；此前的绑定调用报 Not connected 属预期过渡回归）。
    pub fn exchange_biometric_credential(fingerprint: &str, public_key: &str) -> Message {
        with_token(Message::Auth {
            message_id: uuid::Uuid::new_v4().to_string(),
            expect_response: true,
            timestamp: chrono::Utc::now().timestamp_millis(),
            session_id: None,
            token: String::new(),
            payload: AuthPayload {
                stage: AuthStage::ExchangeCertificate,
                device_fingerprint: Some(fingerprint.to_string()),
                public_key: Some(public_key.to_string()),
                ..Default::default()
            },
        })
    }
}

// ==================== Session Control Requests ====================

/// 会话管理请求构建器
pub struct SessionRequest;

impl SessionRequest {
    /// 默认请求超时
    const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);

    /// 构建获取会话列表消息
    pub fn list_sessions() -> Message {
        with_token(Message::session_control_with_response(
            SessionControlAction::ListSessions,
            None,
        ))
    }

    /// 构建启动会话消息
    pub fn start_session(config_id: &str) -> Message {
        with_token(Message::session_control_with_response(
            SessionControlAction::StartSession {
                config_id: config_id.to_string(),
            },
            None,
        ))
    }

    /// 构建停止会话消息
    pub fn stop_session(session_id: &str) -> Message {
        with_token(Message::session_control_with_response(
            SessionControlAction::StopSession {
                session_id: session_id.to_string(),
            },
            Some(session_id),
        ))
    }

    /// 构建删除会话消息
    pub fn remove_session(session_id: &str) -> Message {
        with_token(Message::session_control_with_response(
            SessionControlAction::RemoveSession {
                session_id: session_id.to_string(),
            },
            Some(session_id),
        ))
    }

    /// 构建调整会话终端大小消息
    ///
    /// 移动端屏幕尺寸变化时通知桌面端调整 PTY 大小。force=false：非正统端首次
    /// 请求会收到 NeedsConfirmation（服务端裁决），由前端弹窗确认后经 HTTP
    /// resize（force=true）覆盖重发；WS 路径保持简单，不带强制。
    pub fn resize_session(session_id: &str, cols: u16, rows: u16) -> Message {
        with_token(Message::session_control(
            SessionControlAction::ResizeSession {
                session_id: session_id.to_string(),
                cols,
                rows,
                force: false,
            },
            Some(session_id),
        ))
    }
}

// ==================== Terminal Requests ====================

/// 终端操作请求构建器
pub struct TerminalRequest;

impl TerminalRequest {
    /// 默认订阅超时
    const SUBSCRIBE_TIMEOUT: Duration = Duration::from_secs(10);

    /// 构建订阅会话输出消息
    ///
    /// 开始接收指定会话的终端输出
    pub fn subscribe(session_id: &str, start_seq: Option<u64>) -> Message {
        with_token(Message::subscribe_with_response(session_id, start_seq))
    }

    /// 构建取消订阅消息
    ///
    /// 停止接收指定会话的终端输出
    pub fn unsubscribe(session_id: &str) -> Message {
        with_token(Message::unsubscribe_with_response(session_id))
    }

    /// 构建终端输入消息
    ///
    /// 发送用户输入到终端，支持特殊按键
    /// 使用带响应期望的模式，确保桌面端确认收到
    pub fn input(session_id: &str, data: &str, special_key: Option<KeyCombo>) -> Message {
        with_token(Message::input_with_response(session_id, data, special_key))
    }

    /// 解析特殊按键字符串
    pub fn parse_special_key(key: &str) -> Option<KeyCombo> {
        KeyCombo::parse(key)
    }
}

// ==================== Config Requests ====================

/// 配置查询请求构建器
pub struct ConfigRequest;

impl ConfigRequest {
    /// 配置请求超时
    const CONFIG_TIMEOUT: Duration = Duration::from_secs(30);

    /// 构建获取会话配置列表消息
    pub fn list_session_configs() -> Message {
        with_token(Message::session_config_with_response(
            SessionConfigAction::ListSessionConfigs,
            None,
        ))
    }

    /// 构建获取快捷操作列表消息
    pub fn list_quick_actions() -> Message {
        with_token(Message::session_config_with_response(
            SessionConfigAction::ListQuickActions,
            None,
        ))
    }
}

// ==================== Response Parsers ====================

/// 响应解析器
pub struct ResponseParser;

impl ResponseParser {
    /// 解析会话列表响应
    ///
    /// 从 SessionControl 响应中提取会话列表
    pub fn parse_session_list(response: &Message) -> Option<Vec<serde_json::Value>> {
        if let Message::SessionControl { payload, .. } = response {
            if let SessionControlAction::SessionList { sessions } = &payload.action {
                let value = serde_json::to_value(sessions).ok()?;
                return value.as_array().cloned();
            }
        }
        None
    }

    /// 解析会话配置列表响应
    ///
    /// 从 SessionConfig 响应中提取配置列表
    pub fn parse_config_list(response: &Message) -> Option<Vec<serde_json::Value>> {
        if let Message::SessionConfig { payload, .. } = response {
            if let SessionConfigAction::SessionConfigList { configs } = &payload.action {
                let value = serde_json::to_value(configs).ok()?;
                return value.as_array().cloned();
            }
        }
        None
    }

    /// 解析认证响应
    ///
    /// 检查认证是否成功，返回认证阶段
    pub fn parse_auth_response(response: &Message) -> Option<AuthStage> {
        if let Message::Auth { payload, .. } = response {
            Some(payload.stage.clone())
        } else {
            None
        }
    }

    /// 解析启动会话响应
    ///
    /// 从 SessionControl 响应中提取 session_id
    pub fn parse_start_session_response(response: &Message) -> Option<String> {
        if let Message::SessionControl {
            session_id, payload, ..
        } = response
        {
            if matches!(payload.action, SessionControlAction::StartSession { .. }) {
                return session_id.clone();
            }
        }
        None
    }
}

// ==================== Constants ====================

/// 默认超时常量
pub mod timeouts {
    use std::time::Duration;

    /// 认证请求超时
    pub const AUTH: Duration = Duration::from_secs(30);
    /// 生物认证请求超时（含系统生物识别弹窗，需更长等待）
    pub const BIO_AUTH: Duration = Duration::from_secs(120);
    /// 会话控制请求超时
    pub const SESSION_CONTROL: Duration = Duration::from_secs(15);
    /// 终端订阅超时
    pub const TERMINAL_SUBSCRIBE: Duration = Duration::from_secs(10);
    /// 配置请求超时
    pub const CONFIG: Duration = Duration::from_secs(30);
    /// 默认通用超时
    pub const DEFAULT: Duration = Duration::from_secs(30);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_request_list() {
        let msg = SessionRequest::list_sessions();
        if let Message::SessionControl { payload, .. } = &msg {
            assert!(matches!(payload.action, SessionControlAction::ListSessions));
        } else {
            panic!("Expected SessionControl message");
        }
    }

    #[test]
    fn test_terminal_parse_special_key() {
        use crate::enums::special_key::KeyCode;
        let combo = TerminalRequest::parse_special_key("enter").unwrap();
        assert_eq!(combo.key, KeyCode::Enter);

        let combo = TerminalRequest::parse_special_key("ctrl_c").unwrap();
        assert!(combo.ctrl());
        assert_eq!(combo.key, KeyCode::Char('c'));

        let combo = TerminalRequest::parse_special_key("ctrl_l").unwrap();
        assert!(combo.ctrl());
        assert_eq!(combo.key, KeyCode::Char('l'));

        // 新格式
        let combo = TerminalRequest::parse_special_key("ctrl+a").unwrap();
        assert!(combo.ctrl());
        assert_eq!(combo.key, KeyCode::Char('a'));

        // 方向键
        let combo = TerminalRequest::parse_special_key("up").unwrap();
        assert_eq!(combo.key, KeyCode::Up);

        let combo = TerminalRequest::parse_special_key("arrow_up").unwrap();
        assert_eq!(combo.key, KeyCode::Up);

        // 无效键
        assert!(TerminalRequest::parse_special_key("unknown").is_none());
    }
}
