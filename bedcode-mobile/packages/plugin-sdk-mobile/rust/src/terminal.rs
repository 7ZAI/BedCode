//! Terminal Handler (Mobile)
//!
//! 终端扩展点 trait

pub trait TerminalHandler: Send + Sync + 'static {
    fn on_input(&self, session_id: &str, text: &str) -> Option<String> { let _ = (session_id, text); None }
    fn on_output(&self, session_id: &str, data: &str) -> Option<String> { let _ = (session_id, data); None }
}
