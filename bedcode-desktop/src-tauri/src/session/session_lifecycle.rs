//! Session Lifecycle Listener
//!
//! 会话生命周期事件与监听器机制
//! SessionManager 在关键生命周期节点分发事件，外部通过注册监听器实现解耦扩展

/// 会话生命周期事件
#[derive(Debug, Clone)]
pub enum SessionLifecycleEvent {
    /// 会话创建前（PTY 启动前，同步阻塞）
    ///
    /// 监听器可在此阶段执行前置准备工作（如 hooks 设置）
    Creating {
        config_id: String,
        command: String,
        working_dir: String,
        source_device: Option<String>,
    },
    /// 会话创建后（PTY 启动后，异步通知）
    Created {
        session_id: String,
        config_id: String,
        name: String,
        working_dir: String,
    },
    /// 会话停止前（异步通知）
    Stopping {
        session_id: String,
        source_device: Option<String>,
    },
    /// 会话停止后（异步通知）
    Stopped {
        session_id: String,
        source_device: Option<String>,
    },
}

/// 会话生命周期监听器
///
/// 外部模块实现此 trait 并注册到 SessionManager，即可感知会话生命周期变化。
/// 注册方式：`session_manager.register_lifecycle_listener(Arc::new(MyListener))`
///
/// 调用保证：
/// - `Creating` 事件同步阻塞分发，监听器处理完成后 SessionManager 才继续创建 PTY
/// - 其余事件 fire-and-forget，不阻塞主流程
/// - 监听器实现应避免长时间阻塞（Creating 除外），否则影响会话操作响应
pub trait SessionLifecycleListener: Send + Sync + 'static {
    /// 处理会话生命周期事件
    fn on_session_lifecycle(&self, event: &SessionLifecycleEvent);

    /// 返回关联的插件 ID（如果有）
    ///
    /// 用于按插件 ID 移除监听器，非插件监听器返回 None
    fn plugin_id(&self) -> Option<&str> {
        None
    }
}
