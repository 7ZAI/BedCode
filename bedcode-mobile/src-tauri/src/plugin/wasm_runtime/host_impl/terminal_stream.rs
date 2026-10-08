//! host-terminal-stream —— 终端输出流窄转发（逻辑层，ABI v15 · 票 12）
//!
//! 纯传输原语（ADR 0022 四类薄壳④）：把插件交来的输出**裸字节**按
//! session-id 转发到已登记的前端页面通道。宿主不读内容、不缓存、不解析
//! 终端协议（C3 红线：字节全程零 JSON / 零 base64）。
//!
//! 表与 Tauri 命令面在 `crate::terminal_stream_gateway`（Channel 是 Tauri
//! IPC 机制，登记与转发共用同一张表）；本层只加权限门与插件态适配。

use super::super::WasmPluginState;
use crate::terminal_stream_gateway::terminal_stream_gateway;

/// 权限门统一拒绝文案（既有词汇 `terminal:output` 复用，fail-closed）
fn denied() -> String {
    format!(
        "permission denied: {}",
        bedcode_plugin_api_mobile::permission::PERMISSION_TERMINAL_OUTPUT
    )
}

/// 逻辑层：转发一段输出字节到前端页面通道
///
/// 通道未登记 / 发送失败 → Err（字节由调用方插件丢弃——页面重进经重订阅
/// 回放补齐；宿主不缓存、不补发，与退役前 `terminal_link` 门控丢弃语义一致）
pub(crate) fn terminal_stream_forward_output(
    state: &WasmPluginState,
    session_id: &str,
    data: Vec<u8>,
) -> Result<(), String> {
    if !state
        .granted_permissions
        .contains(bedcode_plugin_api_mobile::permission::PERMISSION_TERMINAL_OUTPUT)
    {
        return Err(denied());
    }
    if session_id.is_empty() {
        return Err("terminal-stream forward-output: session-id is empty".to_string());
    }
    terminal_stream_gateway().forward_output(session_id, &data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::fs_auth::FsAuthChecker;
    use crate::plugin::message_bus::MessageBus;
    use crate::plugin::storage::PluginStorage;
    use crate::plugin::wasm_runtime::WasmHostContext;
    use std::collections::HashSet;
    use std::sync::Arc;

    /// 构造指定权限集的 state（plugin id 用 uuid 隔离，避免并行测试互踩）
    fn ts_state(plugin_id: &str, permissions: &[&str], handle: &tokio::runtime::Handle) -> WasmPluginState {
        let db = Arc::new(std::sync::Mutex::new(
            rusqlite::Connection::open_in_memory().expect("open in-memory db"),
        ));
        let storage = PluginStorage::test_storage();
        let fs_auth = Arc::new(FsAuthChecker::new(storage.clone(), None));
        let status_reporter: Arc<dyn Fn(&str, &str) + Send + Sync> = Arc::new(|_, _| {});
        WasmPluginState {
            plugin_id: plugin_id.to_string(),
            host_ctx: Arc::new(WasmHostContext::new(
                db,
                storage,
                None,
                fs_auth,
                Arc::new(MessageBus::new()),
                status_reporter,
            )),
            runtime_handle: handle.clone(),
            granted_permissions: permissions.iter().map(|p| p.to_string()).collect::<HashSet<_>>(),
            on_message_binary: None,
        }
    }

    /// 权限门 fail-closed：未声明 `terminal:output` 一律拒（错误文本含词汇名）
    #[test]
    fn permission_gate_rejects_without_terminal_output() {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let state = ts_state(&format!("com.bedcode.ts-deny-{}", uuid::Uuid::new_v4()), &[], rt.handle());
        let err = terminal_stream_forward_output(&state, "s1", vec![1])
            .expect_err("must be denied without permission");
        assert!(err.contains("permission denied: terminal:output"), "{err}");
    }

    /// 空 session-id 显性拒绝（不进 gateway，不留「成功但没人收到」假象）
    #[test]
    fn empty_session_id_rejected() {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let state = ts_state(
            &format!("com.bedcode.ts-empty-{}", uuid::Uuid::new_v4()),
            &[bedcode_plugin_api_mobile::permission::PERMISSION_TERMINAL_OUTPUT],
            rt.handle(),
        );
        let err = terminal_stream_forward_output(&state, "", vec![1])
            .expect_err("empty session id must be rejected");
        assert!(err.contains("session-id is empty"), "{err}");
    }

    /// 已授权但通道未登记 → gateway 显性报错（fail-visible，非「成功但没人收到」）
    #[test]
    fn forward_without_channel_fails_visibly() {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let state = ts_state(
            &format!("com.bedcode.ts-fwd-{}", uuid::Uuid::new_v4()),
            &[bedcode_plugin_api_mobile::permission::PERMISSION_TERMINAL_OUTPUT],
            rt.handle(),
        );
        let err = terminal_stream_forward_output(&state, "s-no-channel", vec![1])
            .expect_err("no registered channel must fail");
        assert!(err.contains("no page channel"), "{err}");
    }
}
