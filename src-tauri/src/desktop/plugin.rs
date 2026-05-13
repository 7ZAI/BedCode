//! Plugin Session Management
//!
//! 提供插件会话管理功能
//!
//! 注意：Plugin 会话通过文件系统监控 Claude Code 日志文件，
//! 将 JSONL 日志内容转换为 PTY 输出事件转发到客户端
//!
//! 输入数据通过写入 `.claude/bedcode-pending-input.txt` 文件由插件读取

pub mod jsonl;

use crate::shared::db::Database;
use crate::desktop::pty::PtyOutputEvent;
use crate::Result;
use base64::Engine;
use chrono::Utc;
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, Mutex, RwLock};

/// 插件会话状态
#[derive(Debug)]
pub struct PluginSessionState {
    pub session_id: String,
    pub project_name: String,
    pub project_path: PathBuf,
    pub jsonl_path: PathBuf,
    pub watcher: Option<RecommendedWatcher>,
    pub last_heartbeat: Instant,
    pub file_position: u64,
}

impl Clone for PluginSessionState {
    fn clone(&self) -> Self {
        Self {
            session_id: self.session_id.clone(),
            project_name: self.project_name.clone(),
            project_path: self.project_path.clone(),
            jsonl_path: self.jsonl_path.clone(),
            watcher: None, // 监听器不可克隆，新实例初始化为 None
            last_heartbeat: self.last_heartbeat,
            file_position: self.file_position,
        }
    }
}

/// 插件会话状态枚举（用于状态和显示）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginSessionStatus {
    Starting,
    Running,
    Disconnected,
    Stopped,
}

impl Default for PluginSessionStatus {
    fn default() -> Self {
        Self::Starting
    }
}

/// 插件管理器
///
/// 管理多个插件会话，每个会话对应一个独立的项目
/// 通过文件系统监听和心跳机制监控会话健康状态
pub struct PluginManager {
    /// 活动会话状态（ID → 状态）
    sessions: Arc<RwLock<HashMap<String, PluginSessionState>>>,
    /// 会话信息（与 session 模块兼容）
    session_info: Arc<RwLock<HashMap<String, crate::desktop::session::SessionInfo>>>,
    /// 输出事件广播
    output_tx: broadcast::Sender<PtyOutputEvent>,
    /// 数据库连接
    db: Arc<Mutex<Database>>,
}

impl PluginManager {
    /// 创建新的 PluginManager
    ///
    /// # Arguments
    /// * `output_tx` - 全局输出事件发送器，用于向下游客户端转发消息
    /// * `db` - 数据库连接（可选，用于持久化会话信息）
    ///
    /// # Returns
    /// 新创建的 PluginManager 实例
    pub fn new(output_tx: broadcast::Sender<PtyOutputEvent>, db: Arc<Mutex<Database>>) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            session_info: Arc::new(RwLock::new(HashMap::new())),
            output_tx,
            db,
        }
    }

    /// 从数据库创建 PluginManager
    pub fn from_database(db: Database, output_tx: broadcast::Sender<PtyOutputEvent>) -> Self {
        Self::new(output_tx, Arc::new(Mutex::new(db)))
    }

    /// 注册新的插件会话
    ///
    /// 当 Claude Code 插件首次连接时调用，注册项目路径和日志文件
    ///
    /// # Arguments
    /// * `session_id` - 会话 UUID（由插件生成）
    /// * `project_name` - 项目名称
    /// * `project_path` - 项目根目录
    /// * `jsonl_path` - Claude Code 消息日志文件路径
    ///
    /// # Returns
    /// Ok(()) 注册成功，Err 注册失败
    pub async fn register_session(
        &self,
        session_id: String,
        project_name: String,
        project_path: String,
        jsonl_path: String,
    ) -> Result<()> {
        let jsonl_path = PathBuf::from(&jsonl_path);
        let project_path = PathBuf::from(&project_path);

        // 验证文件路径存在且可读
        if !jsonl_path.exists() {
            tracing::warn!("JSONL file not found: {}", jsonl_path.display());
        }

        // 创建 SessionInfo（可与 PTY 会话共存）
        let mut info =
            crate::desktop::session::SessionInfo::new_plugin(&project_name, &project_path.to_string_lossy());
        info.id = session_id.clone();
        info.status = crate::desktop::session::SessionStatus::Running;

        // 保存会话信息到内存
        {
            let mut info_map = self.session_info.write().await;
            info_map.insert(session_id.clone(), info);
        }

        // 创建插件会话状态
        let state = PluginSessionState {
            session_id: session_id.clone(),
            project_name,
            project_path,
            jsonl_path: jsonl_path.clone(),
            watcher: None,
            last_heartbeat: Instant::now(),
            file_position: 0,
        };

        // 保存会话状态
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(session_id.clone(), state);
        }

        // 启动文件监听器读取日志更新
        // 返回监听器和初始位置，然后在 async 上下文中更新会话状态
        let (watcher, initial_pos) = self
            .start_file_watcher(session_id.clone(), jsonl_path.clone())
            .await?;

        // 更新会话状态，写入文件位置和监听器
        {
            let mut sessions = self.sessions.write().await;
            if let Some(state) = sessions.get_mut(&session_id) {
                state.file_position = initial_pos;
                state.watcher = Some(watcher);

                // 读取并转发初始位置到结尾的内容
                if let Ok((lines, new_pos)) = jsonl::read_new_lines(&jsonl_path, initial_pos)
                {
                    state.file_position = new_pos;
                    for line in lines {
                        if line.trim().is_empty() {
                            continue;
                        }
                        if let Some(output) = jsonl::ClaudeEntry::parse_line(&line) {
                            if !output.text.is_empty() {
                                let event = PtyOutputEvent {
                                    session_id: session_id.clone(),
                                    data: Engine::encode(
                                        &base64::engine::general_purpose::STANDARD,
                                        output.text.as_bytes(),
                                    ),
                                    timestamp: chrono::Utc::now(),
                                    is_waiting: output.is_waiting,
                                };
                                let _ = self.output_tx.send(event);
                            }
                        }
                    }
                }
            }
        }

        tracing::info!(
            "Plugin session registered: {} (jsonl: {})",
            session_id,
            jsonl_path.display()
        );
        Ok(())
    }

    /// 启动文件监听进程
    ///
    /// 使用 notify 库监听 JSONL 文件的修改事件，当文件更新时
    /// 读取新内容并转发到输出通道
    async fn start_file_watcher(
        &self,
        session_id: String,
        jsonl_path: PathBuf,
    ) -> Result<(RecommendedWatcher, u64)> {
        let sessions = self.sessions.clone();
        let output_tx = self.output_tx.clone();

        // 获取初始文件位置（从文件结尾开始，忽略历史内容）
        let initial_pos = tokio::fs::metadata(&jsonl_path)
            .await
            .map(|m| m.len())
            .unwrap_or(0);

        // 创建 notify 监听器
        // 使用 500ms 轮询间隔平衡响应速度和 CPU 占用
        let mut watcher = {
            let sessions = sessions.clone();
            let output_tx = output_tx.clone();
            let session_id = session_id.clone();

            RecommendedWatcher::new(
                move |res: std::result::Result<notify::Event, notify::Error>| {
                    if let Ok(event) = res {
                        if event.kind.is_modify() {
                            // 使用 tokio::spawn 调用 async 方法处理文件更新
                            // 将同步的 notify 回调与异步的 JSONL 处理解耦
                            let sessions = sessions.clone();
                            let output_tx = output_tx.clone();
                            let session_id = session_id.clone();
                            tokio::spawn(async move {
                                Self::read_jsonl_updates(
                                    &sessions,
                                    &output_tx,
                                    &session_id,
                                ).await;
                            });
                        }
                    }
                },
                Config::default().with_poll_interval(Duration::from_millis(500)),
            )?
        };

        // 开始监听（非递归，仅监听单个文件）
        watcher.watch(&jsonl_path, RecursiveMode::NonRecursive)?;

        // 返回监听器和初始文件位置，由 register_session 负责异步写入会话状态
        Ok((watcher, initial_pos))
    }

    /// 读取并处理 JSONL 文件更新
    ///
    /// 从文件中读取新行（从上次读取位置开始），解析后转发为 PTY 输出事件
    ///
    /// # Arguments
    /// * `sessions` - 会话状态映射
    /// * `output_tx` - 输出事件发送器
    /// * `session_id` - 目标会话 ID
    async fn read_jsonl_updates(
        sessions: &Arc<RwLock<HashMap<String, PluginSessionState>>>,
        output_tx: &broadcast::Sender<PtyOutputEvent>,
        session_id: &str,
    ) {
        // 获取当前文件路径和读取位置
        let (jsonl_path, last_pos) = {
            let sessions = sessions.read().await;
            match sessions.get(session_id) {
                Some(s) => (s.jsonl_path.clone(), s.file_position),
                None => {
                    tracing::warn!("Session not found: {}", session_id);
                    return;
                }
            }
        };

        // 读取新行
        let (lines, new_pos) = match jsonl::read_new_lines(&jsonl_path, last_pos) {
            Ok(result) => result,
            Err(e) => {
                tracing::error!("Failed to read JSONL file: {}", e);
                return;
            }
        };

        // 更新读取位置
        {
            let mut sessions = sessions.write().await;
            if let Some(state) = sessions.get_mut(session_id) {
                state.file_position = new_pos;
            }
        }

        // 解析并转发每行
        for line in lines {
            if line.trim().is_empty() {
                continue;
            }
            if let Some(output) = jsonl::ClaudeEntry::parse_line(&line) {
                if output.text.is_empty() {
                    continue;
                }

                let event = PtyOutputEvent {
                    session_id: session_id.to_string(),
                    data: base64::Engine::encode(
                        &base64::engine::general_purpose::STANDARD,
                        output.text.as_bytes(),
                    ),
                    timestamp: chrono::Utc::now(),
                    is_waiting: output.is_waiting,
                };

                let _ = output_tx.send(event);
            }
        }
    }

    /// 处理来自插件的心跳消息
    ///
    /// 插件每 30 秒发送一次心跳，若 90 秒内未收到心跳则认为连接断开
    pub async fn handle_heartbeat(&self, session_id: &str) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        if let Some(state) = sessions.get_mut(session_id) {
            state.last_heartbeat = Instant::now();
            tracing::debug!("Heartbeat received for plugin session: {}", session_id);
        } else {
            tracing::warn!(
                "Heartbeat received for unknown session: {}",
                session_id
            );
        }
        Ok(())
    }

    /// 注销插件会话
    ///
    /// 当 Claude Code 退出或用户手动断开时调用
    /// 停止文件监听并清理会话资源
    pub async fn unregister_session(&self, session_id: &str) -> Result<()> {
        // 停止文件监听并清理会话状态
        {
            let mut sessions = self.sessions.write().await;
            if let Some(mut state) = sessions.remove(session_id) {
                // 释放监听器（drop watcher）
                state.watcher = None;
                tracing::debug!("File watcher stopped for session: {}", session_id);
            }
        }

        // 更新会话信息，标记为已停止
        {
            let mut info_map = self.session_info.write().await;
            if let Some(info) = info_map.get_mut(session_id) {
                info.status = crate::desktop::session::SessionStatus::Stopped;
                info.stopped_at = Some(Utc::now());
            }
        }

        tracing::info!("Plugin session unregistered: {}", session_id);
        Ok(())
    }

    /// 写入用户输入到等待文件
    ///
    /// 插件通过轮询 `.claude/bedcode-pending-input.txt` 文件获取用户输入
    ///
    /// # Arguments
    /// * `session_id` - 目标会话 ID
    /// * `data` - 用户输入内容（通常是单条消息）
    pub async fn write_input(&self, session_id: &str, data: &str) -> Result<()> {
        // 获取项目路径
        let project_path = {
            let sessions = self.sessions.read().await;
            sessions
                .get(session_id)
                .map(|s| s.project_path.clone())
                .ok_or_else(|| crate::AppError::NotFound(format!("Session not found: {}", session_id)))?
        };

        let pending_file = project_path.join(".claude").join("bedcode-pending-input.txt");

        // 确保 .claude 目录存在
        if let Some(parent) = pending_file.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // 写入输入内容
        tokio::fs::write(&pending_file, data).await?;

        tracing::info!("Input written to pending file: {}", pending_file.display());
        Ok(())
    }

    /// 获取会话信息
    pub async fn get_session(&self, session_id: &str) -> Option<crate::desktop::session::SessionInfo> {
        let info_map = self.session_info.read().await;
        info_map.get(session_id).cloned()
    }

    /// 列出所有会话
    pub async fn list_sessions(&self) -> Vec<crate::desktop::session::SessionInfo> {
        let info_map = self.session_info.read().await;
        info_map.values().cloned().collect()
    }

    /// 完全删除会话
    ///
    /// 从所有数据结构移除此会话，与 SessionManager 接口一致
    pub async fn remove_session(&self, session_id: &str) -> Result<()> {
        // 检查会话是否存在
        let sessions = self.sessions.read().await;
        let exists = sessions.contains_key(session_id);
        drop(sessions);

        if exists {
            // 先注销
            self.unregister_session(session_id).await?;
        }

        // 从会话信息映射中完全移除
        {
            let mut info_map = self.session_info.write().await;
            info_map.remove(session_id);
        }

        tracing::info!("Plugin session fully removed: {}", session_id);
        Ok(())
    }

    /// 检查超时并返回断开连接的会话 ID
    ///
    /// 心跳超时时间：90 秒
    /// 建议每分钟调用一次此函数进行清理
    ///
    /// # Returns
    /// 断开连接的会话 ID 列表
    pub async fn check_timeouts(&self) -> Vec<String> {
        let now = Instant::now();
        let timeout = Duration::from_secs(90);
        let mut disconnected = vec![];

        let sessions = self.sessions.write().await;
        for (id, state) in sessions.iter() {
            if now.duration_since(state.last_heartbeat) > timeout {
                disconnected.push(id.clone());
            }
        }
        drop(sessions);

        // 更新断开连接会话的状态
        {
            let mut info_map = self.session_info.write().await;
            for id in &disconnected {
                if let Some(info) = info_map.get_mut(id) {
                    info.status = crate::desktop::session::SessionStatus::Error;
                }
            }
        }

        // 清理掉断开连接的会话状态
        if !disconnected.is_empty() {
            let mut sessions = self.sessions.write().await;
            for id in &disconnected {
                // 这里只移除会话，不更新 info_map（因为上面已经更新了）
                if let Some(mut state) = sessions.remove(id) {
                    state.watcher = None;
                }
            }
        }

        if !disconnected.is_empty() {
            tracing::info!("Detected {} disconnected plugin sessions", disconnected.len());
        }

        disconnected
    }

    /// 获取会话状态
    pub async fn get_session_status(&self, session_id: &str) -> Option<PluginSessionStatus> {
        let info_map = self.session_info.read().await;
        info_map.get(session_id).map(|session| match session.status {
            crate::desktop::session::SessionStatus::Running => PluginSessionStatus::Running,
            crate::desktop::session::SessionStatus::Starting => PluginSessionStatus::Starting,
            crate::desktop::session::SessionStatus::Stopped => PluginSessionStatus::Stopped,
            crate::desktop::session::SessionStatus::Error => PluginSessionStatus::Disconnected,
            crate::desktop::session::SessionStatus::WaitingInput => PluginSessionStatus::Running,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_session_status_default() {
        let status: PluginSessionStatus = Default::default();
        assert_eq!(status, PluginSessionStatus::Starting);
    }

    #[tokio::test]
    async fn test_plugin_manager_list_sessions_empty() {
        let (tx, _rx) = broadcast::channel(100);
        let db = Database::new(std::path::Path::new(":memory:")).unwrap();
        db.init_schema().unwrap();

        let manager = PluginManager::from_database(db, tx);
        let sessions = manager.list_sessions().await;
        assert!(sessions.is_empty());
    }

    // 注意：完整测试需要模拟文件系统，建议在实际项目路径上手动测试
}
