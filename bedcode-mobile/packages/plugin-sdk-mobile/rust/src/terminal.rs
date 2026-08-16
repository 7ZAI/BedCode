//! Terminal Handler (Mobile)
//!
//! 终端扩展点 trait

pub trait TerminalHandler: Send + Sync + 'static {
    fn on_input(&self, session_id: &str, text: &str) -> Option<String> { let _ = (session_id, text); None }
    fn on_output(&self, session_id: &str, data: &str) -> Option<String> { let _ = (session_id, data); None }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 不实现任何方法的处理器：全部走默认实现
    struct NoopHandler;
    impl TerminalHandler for NoopHandler {}

    /// 实现 on_input 的处理器：验证自定义实现可覆盖默认的 None
    struct InputHandler;
    impl TerminalHandler for InputHandler {
        fn on_input(&self, _session_id: &str, text: &str) -> Option<String> {
            Some(format!("[wrapped] {}", text))
        }
    }

    #[test]
    fn test_default_handlers_are_pass_through() {
        // 默认行为 = 不修改管道（None）——宿主按 None 语义原样放行
        let h = NoopHandler;
        assert_eq!(h.on_input("s1", "ls -la"), None);
        assert_eq!(h.on_output("s1", "output"), None);
    }

    #[test]
    fn test_custom_on_input_overrides_default() {
        // 插件可选择性覆盖单个方法，其余仍走默认
        let h = InputHandler;
        assert_eq!(h.on_input("s1", "cmd"), Some("[wrapped] cmd".to_string()));
        assert_eq!(h.on_output("s1", "out"), None);
    }
}
