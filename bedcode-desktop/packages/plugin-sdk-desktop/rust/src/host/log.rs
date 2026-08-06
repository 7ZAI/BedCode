//! 宿主能力：日志（转发到宿主 tracing，自动附加 plugin_id 前缀）

/// 日志输出（fire-and-forget，无失败语义）
///
/// 四个级别对应宿主 tracing 的 info / debug / warn / error。
///
/// `#[track_caller]` 捕获插件侧调用点（file:line），宿主据此在日志中
/// 记录真实插件代码位置，而非宿主 host function 的实现位置。
pub trait HostLog {
    /// info 级别：关键信息（连接建立、会话创建、用户操作）
    #[track_caller]
    fn log_info(&self, message: &str);

    /// debug 级别：常规操作日志（函数调用、流程步骤）
    #[track_caller]
    fn log_debug(&self, message: &str);

    /// warn 级别：重试、超时、降级处理
    #[track_caller]
    fn log_warn(&self, message: &str);

    /// error 级别：失败与异常处理
    #[track_caller]
    fn log_error(&self, message: &str);

    /// 标记插件自身为错误状态
    ///
    /// 用于配置失败提示（如 hooks 脚本拷贝失败）。宿主仅弹窗提示前端，
    /// 插件保持激活、会话照常运行。
    fn mark_plugin_error(&self, error: &str);
}
