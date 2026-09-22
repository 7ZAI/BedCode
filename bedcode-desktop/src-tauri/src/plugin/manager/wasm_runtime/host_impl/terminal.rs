//! 终端域宿主实现（PTY 输入注入）
//!
//! `terminal_send`（权限校验 + 写入）供 Component Model 绑定
//! （`wasm_runtime::component`）调用。

use crate::plugin::manager::wasm_runtime::{block_on_async, WasmHostContext};
use crate::plugin::permission::PERMISSION_TERMINAL_INPUT;

/// 向指定会话注入终端输入（权限校验 + 属主校验 + 写入）
///
/// 属主校验（票 04，P0-3）：`terminal:input` 是「往终端敲键」的能力，只查权限位时
/// 任意持该位的插件都能向**用户正在使用的**交互终端注入命令。与 pty / ws / mdns
/// 一致：先权限门，再属主判定（只有创建该会话的插件可注入）。
pub(crate) fn terminal_send(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    session_id: &str,
    data: &str,
) -> Result<(), String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_TERMINAL_INPUT, "host_terminal_send") {
        return Err("permission denied".to_string());
    }
    super::session::ensure_session_owner(host_ctx, plugin_id, session_id)?;

    let sm = host_ctx.session_manager.clone();
    block_on_async(sm.write_input(session_id, data)).map_err(|e| format!("write failed: {}", e))
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::manager::wasm_runtime::host_impl::tests::{build_host_ctx, grant_permissions};
    use crate::plugin::permission::PERMISSION_SESSION_WRITE;

    /// 无 terminal:input 权限：终端输入注入被权限门禁拒绝
    #[test]
    fn terminal_send_permission_denied() {
        let ctx = build_host_ctx();
        let err = terminal_send(&ctx, "test-plugin", "session-1", "echo hi").unwrap_err();
        assert_eq!(err, "permission denied");
    }

    /// 属主闭环（票 04 红测）：持 `terminal:input` 的他插件不得向别人的会话注入按键
    #[tokio::test]
    async fn terminal_send_by_non_owner_is_denied() {
        use crate::plugin::permission::PERMISSION_SESSION_READ;
        let ctx = build_host_ctx();
        let owner = "com.bedcode.owner-a";
        let intruder = "com.bedcode.intruder-b";
        let sid = seed_session_with_owner(&ctx, owner).await;
        grant_permissions(&ctx, owner, &[PERMISSION_SESSION_WRITE]);
        // 越权方权限齐备——只有属主判定能拦住它
        grant_permissions(
            &ctx,
            intruder,
            &[PERMISSION_TERMINAL_INPUT, PERMISSION_SESSION_READ, PERMISSION_SESSION_WRITE],
        );

        let err = terminal_send(&ctx, intruder, &sid, "rm -rf /").unwrap_err();
        assert!(
            err.contains("not owner"),
            "非属主终端注入必须被拒，got: {err}"
        );
        // 权限门与属主门分档可辨：同一越权方少了 terminal:input 时报的是权限拒绝
        let outsider = "com.bedcode.no-input-perm";
        grant_permissions(&ctx, outsider, &[PERMISSION_SESSION_READ]);
        let err = terminal_send(&ctx, outsider, &sid, "ls").unwrap_err();
        assert_eq!(err, "permission denied", "权限门应先于属主门报出");
    }

    /// 播种一个登记了属主的会话（复用会话域执行端，不 spawn 进程）
    async fn seed_session_with_owner(ctx: &WasmHostContext, owner: &str) -> String {
        use crate::enums::{ExecutionEnvironment, SessionLaunchConfig};
        block_on_async(ctx.session_manager.create_session_from_spec(
            SessionLaunchConfig {
                name: "终端注入用会话".to_string(),
                environment: ExecutionEnvironment::Linux,
                working_dir: "/tmp".to_string(),
                command: "bash".to_string(),
                command_args: None,
                env_vars: std::collections::HashMap::new(),
                cols: 120,
                rows: 40,
            },
            "cfg-terminal-owner".to_string(),
            None,
            false,
            None,
            Some(owner),
        ))
        .expect("seed session with owner")
    }

    // 成功路径（write_input → PTY 写入）依赖 AppContext 全局单例与真实 PTY 会话：
    // AppContext 仅应用启动时初始化（测试环境未初始化会 panic），且无 PTY 运行时
    // 会话不可用 —— 交由集成/手动测试覆盖，此处只测可独立验证的权限门禁与属主门禁
}
