//! 宿主能力：日志（转发到宿主 tracing，自动附加 plugin_id 前缀）

/// 日志输出（fire-and-forget，无失败语义）
///
/// 四个级别对应宿主 tracing 的 info / debug / warn / error
pub trait HostLog {
    /// info 级别：关键信息（连接建立、会话创建、用户操作）
    fn log_info(&self, message: &str);

    /// debug 级别：常规操作日志（函数调用、流程步骤）
    fn log_debug(&self, message: &str);

    /// warn 级别：重试、超时、降级处理
    fn log_warn(&self, message: &str);

    /// error 级别：失败与异常处理
    fn log_error(&self, message: &str);
}
