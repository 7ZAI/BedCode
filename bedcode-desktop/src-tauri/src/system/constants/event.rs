//! Tauri 事件名称常量
//!
//! 统一管理所有 emit/listen 的事件字符串，避免拼写错误和重复定义

/// 会话状态变更事件
pub const SESSION_STATUS_CHANGED: &str = "session-status-changed";

/// 会话重启事件
pub const SESSION_RESTARTED: &str = "session-restarted";

/// 设备连接/认证事件
pub const DEVICE_CONNECTED: &str = "device-connected";

/// 设备断开事件（WS 连接关闭时发出，与 `DEVICE_CONNECTED` 对称）
pub const DEVICE_DISCONNECTED: &str = "device-disconnected";

/// 生命周期：应用启动完成
pub const LIFECYCLE_STARTUP: &str = "lifecycle:startup";

/// 生命周期：应用即将关闭
pub const LIFECYCLE_SHUTDOWN: &str = "lifecycle:shutdown";

/// 插件开发模式热重载通知
pub const PLUGIN_DEV_RELOAD: &str = "plugin:dev-reload";

/// 插件自检失败提示（host_mark_plugin_error）— 前端弹窗提示，不改插件状态
pub const PLUGIN_ERROR: &str = "plugin:error";

/// 窗口关闭请求 — 有运行中会话时发送到前端，请求用户确认
pub const WINDOW_CLOSE_REQUESTED: &str = "window-close-requested";
