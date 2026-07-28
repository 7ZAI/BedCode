//! 宿主能力：会话查询与生命周期监听

use super::HostError;

/// 会话信息与生命周期
///
/// 查询类方法需要 `session:read` 权限（会话配置列表含 working_dir 等路径信息）。
/// 生命周期事件通过 [`WasmPlugin::on_session_lifecycle`](crate::wasm::WasmPlugin::on_session_lifecycle)
/// 回调接收，不走消息总线。
pub trait HostSession {
    /// 列出所有会话（JSON 数组）
    fn session_list(&self) -> Result<Option<serde_json::Value>, HostError>;

    /// 获取单个会话；不存在返回 `Ok(None)`
    fn session_get(&self, session_id: &str) -> Result<Option<serde_json::Value>, HostError>;

    /// 列出所有会话配置的精简列表（仅 id / workingDir / command），
    /// 供插件遍历项目目录（如批量清理 hooks）
    fn session_config_list(&self) -> Result<Option<serde_json::Value>, HostError>;

    /// 注册会话生命周期监听器
    ///
    /// 调用后宿主为该插件创建监听器并注册到 SessionManager，
    /// 事件（creating / created / stopping / stopped）通过
    /// `on_session_lifecycle` 回调投递
    fn session_lifecycle_register(&self) -> Result<(), HostError>;
}
