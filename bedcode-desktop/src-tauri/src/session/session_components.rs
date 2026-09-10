//! Session Components
//!
//! 会话管理器的内部组件：注册表、命名服务、配置映射、状态检测
//! 这些组件各自只有一个实现，trait 已内联到此文件

use crate::db::SessionConfig;
use crate::enums::SessionStatus;
use crate::pty::{ExecutionEnvironment, PtySession, SessionLaunchConfig, WindowsShell};
use crate::session::SessionInfo;
use crate::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

// ==================== PTY Registry ====================

/// PTY 会话注册表 - 负责 PTY 会话的存储和基本操作
pub trait PtyRegistry: Send + Sync {
    async fn insert(&self, id: String, session: PtySession);
    async fn remove(&self, id: &str) -> Option<PtySession>;
    async fn get(&self, id: &str) -> Option<PtySession>;
    async fn list(&self) -> Vec<PtySession>;
    async fn list_ids(&self) -> Vec<String>;
    async fn write_input(&self, id: &str, data: &str) -> Result<()>;
    async fn send_special_key(&self, id: &str, key: &str) -> Result<()>;
    async fn resize(&self, id: &str, cols: u16, rows: u16) -> Result<()>;
    async fn kill(&self, id: &str) -> Result<()>;
    async fn kill_all(&self) -> Result<()>;
}

pub struct DefaultPtyRegistry {
    sessions: Arc<RwLock<HashMap<String, PtySession>>>,
}

impl DefaultPtyRegistry {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for DefaultPtyRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl PtyRegistry for DefaultPtyRegistry {
    async fn insert(&self, id: String, session: PtySession) {
        let mut sessions = self.sessions.write().await;
        sessions.insert(id, session);
    }

    async fn remove(&self, id: &str) -> Option<PtySession> {
        let mut sessions = self.sessions.write().await;
        sessions.remove(id)
    }

    async fn get(&self, id: &str) -> Option<PtySession> {
        let sessions = self.sessions.read().await;
        sessions.get(id).cloned()
    }

    async fn list(&self) -> Vec<PtySession> {
        let sessions = self.sessions.read().await;
        sessions.values().cloned().collect()
    }

    async fn list_ids(&self) -> Vec<String> {
        let sessions = self.sessions.read().await;
        sessions.keys().cloned().collect()
    }

    async fn write_input(&self, id: &str, data: &str) -> Result<()> {
        let sessions = self.sessions.read().await;
        let session = sessions
            .get(id)
            .ok_or_else(|| crate::AppError::NotFound(format!("Session not found: {}", id)))?;
        session.write_str(data).await
    }

    async fn send_special_key(&self, id: &str, key: &str) -> Result<()> {
        let sessions = self.sessions.read().await;
        let session = sessions
            .get(id)
            .ok_or_else(|| crate::AppError::NotFound(format!("Session not found: {}", id)))?;
        session.send_special_key(key).await
    }

    async fn resize(&self, id: &str, cols: u16, rows: u16) -> Result<()> {
        let sessions = self.sessions.read().await;
        let session = sessions
            .get(id)
            .ok_or_else(|| crate::AppError::NotFound(format!("Session not found: {}", id)))?;
        session.resize(cols, rows).await
    }

    async fn kill(&self, id: &str) -> Result<()> {
        let session = self.remove(id).await;
        if let Some(s) = session {
            s.kill().await?;
        }
        Ok(())
    }

    async fn kill_all(&self) -> Result<()> {
        let sessions: Vec<(String, PtySession)> = {
            let mut map = self.sessions.write().await;
            map.drain().collect()
        };
        for (id, session) in sessions {
            if let Err(e) = session.kill().await {
                tracing::error!(session_id = %id, error = %e, "Failed to kill session");
            }
        }
        Ok(())
    }
}

// ==================== Session Info Registry ====================

/// 会话信息注册表 - 负责会话元数据的存储和状态管理
pub trait SessionInfoRegistry: Send + Sync {
    async fn insert(&self, info: SessionInfo);
    async fn remove(&self, id: &str) -> Option<SessionInfo>;
    async fn get(&self, id: &str) -> Option<SessionInfo>;
    async fn list(&self) -> Vec<SessionInfo>;
    async fn update_status(&self, id: &str, status: SessionStatus);
    async fn update_status_with_time(&self, id: &str, status: SessionStatus);
    async fn get_status(&self, id: &str) -> Option<SessionStatus>;
    async fn filter_by_config(&self, config_id: &str) -> Vec<SessionInfo>;
    async fn filter_active_by_config(&self, config_id: &str) -> Vec<SessionInfo>;
}

pub struct DefaultSessionInfoRegistry {
    info: Arc<RwLock<HashMap<String, SessionInfo>>>,
}

impl DefaultSessionInfoRegistry {
    pub fn new() -> Self {
        Self {
            info: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for DefaultSessionInfoRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionInfoRegistry for DefaultSessionInfoRegistry {
    async fn insert(&self, info: SessionInfo) {
        let mut map = self.info.write().await;
        map.insert(info.id.clone(), info);
    }

    async fn remove(&self, id: &str) -> Option<SessionInfo> {
        let mut map = self.info.write().await;
        map.remove(id)
    }

    async fn get(&self, id: &str) -> Option<SessionInfo> {
        let map = self.info.read().await;
        map.get(id).cloned()
    }

    async fn list(&self) -> Vec<SessionInfo> {
        let map = self.info.read().await;
        map.values().cloned().collect()
    }

    async fn update_status(&self, id: &str, status: SessionStatus) {
        let mut map = self.info.write().await;
        if let Some(info) = map.get_mut(id) {
            info.status = status;
        }
    }

    async fn update_status_with_time(&self, id: &str, status: SessionStatus) {
        let mut map = self.info.write().await;
        if let Some(info) = map.get_mut(id) {
            info.status = status.clone();
            match status {
                SessionStatus::Running => {
                    if info.started_at.is_none() {
                        info.started_at = Some(chrono::Utc::now());
                    }
                }
                SessionStatus::Stopped | SessionStatus::Error(_) => {
                    if info.stopped_at.is_none() {
                        info.stopped_at = Some(chrono::Utc::now());
                    }
                }
                _ => {}
            }
        }
    }

    async fn get_status(&self, id: &str) -> Option<SessionStatus> {
        let map = self.info.read().await;
        map.get(id).map(|i| i.status.clone())
    }

    async fn filter_by_config(&self, config_id: &str) -> Vec<SessionInfo> {
        let map = self.info.read().await;
        map.values().filter(|s| s.config_id == config_id).cloned().collect()
    }

    async fn filter_active_by_config(&self, config_id: &str) -> Vec<SessionInfo> {
        let map = self.info.read().await;
        map.values()
            .filter(|s| s.config_id == config_id && s.status != SessionStatus::Stopped)
            .cloned()
            .collect()
    }
}

// ==================== Naming Service ====================

/// 会话命名服务 - 生成唯一的会话名称
pub trait NamingService: Send + Sync {
    fn generate_unique_name(&self, config_id: &str, base_name: &str, sessions: &[SessionInfo]) -> String;
}

pub struct DefaultNamingService;

impl DefaultNamingService {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DefaultNamingService {
    fn default() -> Self {
        Self::new()
    }
}

impl NamingService for DefaultNamingService {
    fn generate_unique_name(&self, config_id: &str, base_name: &str, sessions: &[SessionInfo]) -> String {
        // 从同配置的活跃会话名称中提取最大编号，避免删除后编号回退导致重名
        let max_index = sessions
            .iter()
            .filter(|s| s.config_id == config_id && s.status != SessionStatus::Stopped)
            .filter_map(|s| {
                // 匹配 "baseName(N)" 格式，提取数字 N
                let name = &s.name;
                if name == base_name {
                    Some(0)
                } else if let Some(rest) = name.strip_prefix(base_name) {
                    rest.strip_prefix('(')
                        .and_then(|r| r.strip_suffix(')'))
                        .and_then(|n| n.parse::<usize>().ok())
                } else {
                    None
                }
            })
            .max();

        match max_index {
            None => base_name.to_string(),
            Some(0) => format!("{}(1)", base_name),
            Some(n) => format!("{}({})", base_name, n + 1),
        }
    }
}

// ==================== Config Mapper ====================

/// 配置映射服务 - 将数据库配置转换为启动配置
pub trait ConfigMapper: Send + Sync {
    fn to_launch_config(&self, config: &SessionConfig) -> Result<SessionLaunchConfig>;
}

pub struct DefaultConfigMapper;

impl DefaultConfigMapper {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DefaultConfigMapper {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigMapper for DefaultConfigMapper {
    fn to_launch_config(&self, config: &SessionConfig) -> Result<SessionLaunchConfig> {
        let environment = match config.environment.as_str() {
            "wsl2" => ExecutionEnvironment::Wsl2 {
                distro: config.wsl_distro.clone().unwrap_or_else(|| "Ubuntu".to_string()),
            },
            // Linux 原生环境：直接跑 bash，不带 distro
            "linux" => ExecutionEnvironment::Linux,
            _ => ExecutionEnvironment::Windows {
                shell: WindowsShell::PowerShell,
            },
        };

        Ok(SessionLaunchConfig {
            name: config.name.clone(),
            environment,
            working_dir: config.working_dir.clone(),
            command: config.command.clone(),
            env_vars: std::collections::HashMap::new(),
            cols: 120,
            rows: 40,
        })
    }
}

// ==================== Status Detector ====================

/// 状态检测服务 - 检测会话状态（如等待输入）
pub trait StatusDetector: Send + Sync {
    fn detect_waiting_input(&self, output: &str) -> bool;
}

pub struct DefaultStatusDetector;

impl DefaultStatusDetector {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DefaultStatusDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl StatusDetector for DefaultStatusDetector {
    fn detect_waiting_input(&self, output: &str) -> bool {
        crate::utils::parser::detect_waiting_input(output)
    }
}

// ==================== Canonical Renderer Registry ====================

/// 正统渲染端身份：当前 PTY 网格尺寸的权威归属端
///
/// 桌面端与移动端同时查看同一会话时 PTY 只能有一个尺寸，输出格式必须
/// 匹配实际渲染的那个端。每次 resize 后归属即确立为请求方，其他端再
/// 调整需先确认覆盖（见 SessionManager::resize_session 裁决）。
///
/// serde 注意：容器级 rename_all 只作用于变体名（tag 值），字段名需另用
/// rename_all_fields（serde ≥1.0.186）转为 camelCase，与两端前端的 TS 类型
/// （`{ kind: 'mobile'; deviceName }` / `{ status: 'needsConfirmation'; currentCanonical }`）
/// 对齐——曾因字段保持 snake_case 导致前端读到 undefined 崩溃、确认弹窗不显示。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum RendererSource {
    /// 桌面端（会话宿主：本地命令 / 本地环回 WS）
    Desktop,
    /// 移动端设备（device_name 来自 JWT claims）
    Mobile { device_name: String },
}

impl RendererSource {
    /// 是否为桌面端（桌面本地路径恒为 Desktop）
    pub fn is_desktop(&self) -> bool {
        matches!(self, RendererSource::Desktop)
    }
}

/// resize 裁决结果（统一输出给所有 entry：桌面命令 / 移动端 HTTP / WS 控制）
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "status", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ResizeOutcome {
    /// 已应用：请求方就是正统端，或强制覆盖已确认
    Applied { canonical: RendererSource },
    /// 需要确认：另一个端正在渲染输出，本次未应用；客户端弹窗确认后带 force 重发
    NeedsConfirmation { current_canonical: RendererSource },
}

/// 启动初始网格解析：启动端携带且合法（>0）时覆盖配置默认尺寸
///
/// 各端终端组件按自身窗口/字体预算出默认网格随启动请求传入，PTY openpty
/// 直接以该尺寸创建，避免「先 80x24 启动 → 挂载后再 resize」的首帧回绕。
pub fn resolve_initial_size(base_cols: u16, base_rows: u16, initial: Option<(u16, u16)>) -> (u16, u16) {
    match initial {
        Some((cols, rows)) if cols > 0 && rows > 0 => (cols, rows),
        _ => (base_cols, base_rows),
    }
}

/// 正统渲染端注册表 - 每会话记录当前 PTY 尺寸归属端
pub trait CanonicalRendererRegistry: Send + Sync {
    async fn get(&self, session_id: &str) -> Option<RendererSource>;
    async fn set(&self, session_id: &str, source: RendererSource);
    async fn clear(&self, session_id: &str);
}

pub struct DefaultCanonicalRendererRegistry {
    map: Arc<RwLock<HashMap<String, RendererSource>>>,
}

impl DefaultCanonicalRendererRegistry {
    pub fn new() -> Self {
        Self {
            map: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for DefaultCanonicalRendererRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl CanonicalRendererRegistry for DefaultCanonicalRendererRegistry {
    async fn get(&self, session_id: &str) -> Option<RendererSource> {
        let map = self.map.read().await;
        map.get(session_id).cloned()
    }

    async fn set(&self, session_id: &str, source: RendererSource) {
        let mut map = self.map.write().await;
        map.insert(session_id.to_string(), source);
    }

    async fn clear(&self, session_id: &str) {
        let mut map = self.map.write().await;
        map.remove(session_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// serde 形状回归：字段必须输出 camelCase（与两端前端 TS 类型对齐）。
    /// 曾因容器级 rename_all 只转换变体名、current_canonical/device_name 保持
    /// snake_case，导致前端读 currentCanonical 为 undefined 崩溃且确认弹窗不显示。
    #[test]
    fn test_resize_outcome_and_renderer_source_json_shape_is_camel_case() {
        let outcome = ResizeOutcome::NeedsConfirmation {
            current_canonical: RendererSource::Mobile {
                device_name: "Pixel-9".to_string(),
            },
        };
        let json: serde_json::Value = serde_json::to_value(&outcome).unwrap();
        assert_eq!(json["status"], "needsConfirmation");
        assert!(json.get("currentCanonical").is_some(), "field must be camelCase: {json}");
        assert!(json.get("current_canonical").is_none());
        assert_eq!(json["currentCanonical"]["kind"], "mobile");
        assert_eq!(json["currentCanonical"]["deviceName"], "Pixel-9");

        let applied = ResizeOutcome::Applied {
            canonical: RendererSource::Desktop,
        };
        let json: serde_json::Value = serde_json::to_value(&applied).unwrap();
        assert_eq!(json["status"], "applied");
        assert_eq!(json["canonical"]["kind"], "desktop");

        // 反序列化回环（HTTP/命令边界双向兼容）
        let back: ResizeOutcome = serde_json::from_value(json).unwrap();
        assert_eq!(
            back,
            ResizeOutcome::Applied {
                canonical: RendererSource::Desktop
            }
        );
    }

    /// 启动初始网格解析：合法尺寸覆盖默认值，非法（0）或缺省回退配置默认
    #[test]
    fn test_resolve_initial_size_overrides_only_when_valid() {
        assert_eq!(resolve_initial_size(80, 24, Some((120, 40))), (120, 40));
        assert_eq!(resolve_initial_size(80, 24, None), (80, 24));
        // 0 尺寸（隐藏容器误传）不生效
        assert_eq!(resolve_initial_size(80, 24, Some((0, 40))), (80, 24));
        assert_eq!(resolve_initial_size(80, 24, Some((120, 0))), (80, 24));
    }
}
