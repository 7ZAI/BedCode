//! 共享类型化载荷
//!
//! 宿主 ↔ 插件之间的事件载荷定义。两端引用同一份类型，
//! serde 表示即线协议 —— 新增/修改事件时编译器强制两端同步，
//! 杜绝字符串契约漂移（如历史上 TaskQueueChanged 广播静默丢失）。

use serde::{Deserialize, Serialize};

/// 会话生命周期事件（宿主 → 插件）
///
/// 插件通过 `session_lifecycle_register()` 注册后，经
/// [`WasmPlugin::on_session_lifecycle`](crate::wasm::WasmPlugin::on_session_lifecycle)
/// 回调接收，不走消息总线。
///
/// 线协议：`{ "event_type": "creating" | "created" | "stopping" | "stopped", ...字段 }`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type", rename_all = "snake_case")]
pub enum SessionLifecycleEvent {
    /// 会话创建前（PTY 启动前，同步阻塞）— 用于前置准备（如 hooks 设置）
    Creating {
        /// 会话配置 ID
        config_id: String,
        /// 启动命令（如 claude）
        command: String,
        /// 工作目录
        working_dir: String,
        /// 触发操作的设备名称（桌面本地操作为 None）
        #[serde(default)]
        source_device: Option<String>,
        /// 插件安装目录（宿主注入），含 hook 脚本等资源文件
        #[serde(default)]
        resource_dir: String,
    },
    /// 会话创建后（PTY 启动后，异步通知）
    Created {
        /// PTY 会话 ID
        session_id: String,
        /// 会话配置 ID
        config_id: String,
        /// 会话名称
        name: String,
        /// 工作目录
        working_dir: String,
        /// 插件安装目录（宿主注入）
        #[serde(default)]
        resource_dir: String,
    },
    /// 会话停止前（异步通知）
    Stopping {
        /// PTY 会话 ID
        session_id: String,
        /// 触发操作的设备名称（桌面本地操作为 None）
        #[serde(default)]
        source_device: Option<String>,
        /// 插件安装目录（宿主注入）
        #[serde(default)]
        resource_dir: String,
    },
    /// 会话停止后（异步通知）
    Stopped {
        /// PTY 会话 ID
        session_id: String,
        /// 触发操作的设备名称（桌面本地操作为 None）
        #[serde(default)]
        source_device: Option<String>,
        /// 插件安装目录（宿主注入）
        #[serde(default)]
        resource_dir: String,
    },
}

/// 提交输入行事件（宿主 → 插件）
///
/// 用户在终端会话中完成输入并提交（回车触发）时，由宿主 SessionManager
/// 从原始输入字节流重建出完整文本行后分发。插件通过
/// `session_input_register()` 注册后，经
/// [`WasmPlugin::on_input_submitted`](crate::wasm::WasmPlugin::on_input_submitted)
/// 回调接收，不走消息总线。
///
/// 纯观察通知：异步分发、无顺序保证，回调出错或超时不影响输入本身。
/// 宿主不做语义过滤——空提交（空行回车）同样触发，是否忽略由插件决定。
/// 注册需要 `terminal:observe` 权限。见 ADR 0001。
///
/// 线协议：`{ "session_id": "...", "text": "..." }`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct InputSubmittedEvent {
    /// PTY 会话 ID
    pub session_id: String,
    /// 提交的输入行内容（仅普通输入；多行粘贴时含换行符）
    pub text: String,
}

/// 同步事件（插件 → 宿主 → 移动端客户端）
///
/// 通过 `HostEvents::broadcast_sync` 发布，宿主转发给所有已认证的
/// WebSocket 客户端（移动端）。
///
/// 线协议：`{ "type": "TaskStatusChanged" | "SessionModeChanged" | "TaskQueueChanged", ...字段 }`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SyncEvent {
    /// 任务状态变更
    TaskStatusChanged {
        /// BedCode PTY 会话 ID
        session_id: String,
        /// 任务状态：idle / in_progress / asking / completed / interrupted
        task_status: String,
        /// 状态原因说明
        #[serde(default, skip_serializing_if = "Option::is_none")]
        task_reason: Option<String>,
        /// 等待用户回答的问题列表（asking 状态）
        #[serde(default, skip_serializing_if = "Option::is_none")]
        task_questions: Option<Vec<PluginQuestion>>,
    },
    /// 会话自动授权模式变更
    SessionModeChanged {
        /// BedCode PTY 会话 ID
        session_id: String,
        /// 是否自动授权
        auto_approve: bool,
    },
    /// 会话任务队列变更
    TaskQueueChanged {
        /// BedCode PTY 会话 ID
        session_id: String,
        /// 变更后的待执行任务数量
        queue_count: i64,
        /// 触发动作：add / remove / clear / dequeue
        action: String,
    },
}

/// 插件推送的问题结构（任务询问，随 TaskStatusChanged 同步到移动端）
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct PluginQuestion {
    /// 问题文本
    pub question: String,
    /// 问题简短标题
    pub header: String,
    /// 是否多选
    #[serde(default)]
    pub multi_select: bool,
    /// 选项列表
    #[serde(default)]
    pub options: Vec<PluginQuestionOption>,
}

/// 插件推送的问题选项
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct PluginQuestionOption {
    /// 选项标签
    pub label: String,
    /// 选项描述
    #[serde(default)]
    pub description: String,
}
