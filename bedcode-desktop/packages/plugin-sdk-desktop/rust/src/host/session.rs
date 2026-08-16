//! 宿主能力：会话查询与生命周期/输入监听

use super::HostError;

/// 会话信息与生命周期
///
/// 查询类方法需要 `session:read` 权限（会话配置列表含 working_dir 等路径信息）。
/// 生命周期事件通过 [`WasmPlugin::on_session_lifecycle`](crate::wasm::WasmPlugin::on_session_lifecycle)
/// 回调接收，不走消息总线。
/// 提交输入行事件通过 [`WasmPlugin::on_input_submitted`](crate::wasm::WasmPlugin::on_input_submitted)
/// 回调接收，注册需要 `terminal:observe` 权限（见 ADR 0001）。
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

    /// 注册提交输入行监听器
    ///
    /// 调用后宿主为该插件创建监听器并注册到 SessionManager，
    /// 用户提交输入（回车触发）时通过 `on_input_submitted` 回调
    /// 异步投递重建后的完整输入行。需要 `terminal:observe` 权限，
    /// 未授权时返回错误
    fn session_input_register(&self) -> Result<(), HostError>;

    /// 按会话配置创建新会话（v6，需要 `session:write` 权限）
    ///
    /// 成功返回新会话的 session_id；会话创建完成后宿主分发 `Created`
    /// 生命周期事件（带 session_id + config_id），已注册生命周期监听器的
    /// 插件可据此感知新会话就绪（定时自动任务的会话就绪信号，见 ADR 0003）
    fn session_create(&self, config_id: &str) -> Result<String, HostError>;

    /// 关闭（终止）会话（v7，需要 `session:write` 权限）
    ///
    /// 停止会话 PTY 并置为 Stopped，会话记录保留（与用户手动关闭一致）；
    /// 宿主分发 `Stopping` / `Stopped` 生命周期事件。用于插件在任务
    /// 执行完毕后清理自己创建的会话（如定时自动任务会话）
    fn session_close(&self, session_id: &str) -> Result<(), HostError>;
}
