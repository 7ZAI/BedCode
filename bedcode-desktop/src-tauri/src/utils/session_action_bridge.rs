//! 会话动作命令桥接（票 10）：重启 / 移除 / 改名 / 尺寸裁决下沉 `com.bedcode.terminal-session`
//!
//! 模式与 `session_create_bridge.rs` / `session_config_bridge.rs` 同构：探活锚点
//! （会话中心互调面已登记）→ JSON-RPC 互调 → 插件完成编排（存在性预检、调用顺序、
//! 失败可见）与**尺寸裁决**（正统端判定 + 覆盖确认策略），再经 `host-session` 的
//! `restart` / `remove` / `rename` / `resize` 四原语交内核执行。
//!
//! ## 降级语义（无单点）
//!
//! 插件未激活（锚点不在注册表）或互调失败（超时 / 响应损坏 / 插件侧显性报错）→
//! 返回 `Ok(None)`，调用方（`commands/session.rs`）走宿主旧路径（直接调
//! `SessionManager` 执行器，含内核侧裁决分支），行为与迁移前逐字一致。
//!
//! ## 尺寸裁决的取舍（spec D3）
//!
//! 规则搬入插件后，**内核 `SessionManager::resize_session` 的裁决分支保留**：
//! 移动端 HTTP / WS 路径（`server/services/session_control.rs`）与插件不可用时的
//! 降级轨仍直连它——这是「宿主执行器保留」的口径，也是移动端零改动的前提。
//! 桌面命令面在插件可用时经本桥接（裁决在插件侧），两条路径的对外行为等价
//! （同一裁决规则、同一 `ResizeOutcome` 形状，见 `plugins/terminal-session/rust/src/actions.rs`
//! 与 `session/session_manager.rs` 的四态对照测试）。

use crate::plugin::manager::wasm_runtime::WasmHostContext;
use crate::session::{RendererSource, ResizeOutcome};
use crate::utils::auth::auth_center::{call_api, session_active};
use crate::{AppError, Result};

/// 插件互调 api（短名由 `#[plugin_api]` 宏按 manifest.api 比对防漂移）
const API_RESTART: &str = "com.bedcode.terminal-session.session-restart";
const API_REMOVE: &str = "com.bedcode.terminal-session.session-remove";
const API_RENAME: &str = "com.bedcode.terminal-session.session-rename";
const API_RESIZE: &str = "com.bedcode.terminal-session.session-resize";

/// 插件侧不可用时记录降级（结构化字段；双轨期未激活是常态）
fn log_fallback(api: &str, err: &AppError) {
    tracing::warn!(
        api = %api,
        error = %err,
        "session action plugin surface unavailable, fallback to host executor"
    );
}

/// 经会话中心插件重启会话（同一 session id 重建并启动）——**插件必需**
///
/// v21 起内核重启执行器退役（`host-session.restart` 一并删除），重启 = 插件编排
/// 「`remove` + 同 id `create-with-spec`」；故本桥接不再有宿主降级轨：插件未激活 /
/// 互调失败一律显性报错。
pub async fn restart_session_via_plugin(host_ctx: &WasmHostContext, session_id: &str) -> Result<String> {
    if !session_active(host_ctx) {
        tracing::warn!(
            session_id = %session_id,
            "session restart refused: session plugin not active (plugin required, kernel executor retired)"
        );
        return Err(AppError::Plugin(
            "session plugin not active: session restart requires com.bedcode.terminal-session".to_string(),
        ));
    }
    let params = serde_json::json!({ "sessionId": session_id });
    match call_api(host_ctx, API_RESTART, params) {
        Ok(v) => {
            let sid = v
                .get("sessionId")
                .and_then(|s| s.as_str())
                .ok_or_else(|| AppError::Plugin(format!("session-restart reply missing sessionId: {v}")))?
                .to_string();
            tracing::info!(session_id = %sid, "session restart orchestrated via plugin");
            Ok(sid)
        }
        Err(e) => {
            tracing::error!(session_id = %session_id, error = %e, "session restart failed via plugin");
            Err(AppError::Plugin(format!("session restart failed (plugin error): {e}")))
        }
    }
}

/// 经会话中心插件移除会话
///
/// - `Ok(Some(()))`：插件编排成功（存在性预检通过 + 移除完成，失败在插件侧可见）
/// - `Ok(None)`：插件不可用 / 互调失败 → 调用方降级宿主执行器
pub async fn remove_session_via_plugin(host_ctx: &WasmHostContext, session_id: &str) -> Result<Option<()>> {
    if !session_active(host_ctx) {
        return Ok(None);
    }
    let params = serde_json::json!({ "sessionId": session_id });
    match call_api(host_ctx, API_REMOVE, params) {
        Ok(_) => {
            tracing::info!(session_id = %session_id, "session remove orchestrated via plugin");
            Ok(Some(()))
        }
        Err(e) => {
            log_fallback(API_REMOVE, &e);
            Ok(None)
        }
    }
}

/// 经会话中心插件改名会话 → 返回改名前的名字
///
/// - `Ok(Some(previous_name))`：插件编排成功
/// - `Ok(None)`：插件不可用 / 互调失败 → 调用方降级宿主执行器
pub async fn rename_session_via_plugin(
    host_ctx: &WasmHostContext,
    session_id: &str,
    name: &str,
) -> Result<Option<String>> {
    if !session_active(host_ctx) {
        return Ok(None);
    }
    let params = serde_json::json!({ "sessionId": session_id, "name": name });
    match call_api(host_ctx, API_RENAME, params) {
        Ok(v) => {
            let previous = v
                .get("previousName")
                .and_then(|s| s.as_str())
                .ok_or_else(|| AppError::Plugin(format!("session-rename reply missing previousName: {v}")))?
                .to_string();
            tracing::info!(session_id = %session_id, "session rename orchestrated via plugin");
            Ok(Some(previous))
        }
        Err(e) => {
            log_fallback(API_RENAME, &e);
            Ok(None)
        }
    }
}

/// 经会话中心插件做尺寸裁决 + 执行
///
/// - `Ok(Some(outcome))`：插件裁决完成（`applied` 已执行并登记归属；
///   `needsConfirmation` 零改动，调用方弹窗确认后带 `force` 重发）
/// - `Ok(None)`：插件不可用 / 互调失败 → 调用方降级内核执行器（含内核裁决分支）
pub async fn resize_session_via_plugin(
    host_ctx: &WasmHostContext,
    session_id: &str,
    cols: u16,
    rows: u16,
    requester: &RendererSource,
    force: bool,
) -> Result<Option<ResizeOutcome>> {
    if !session_active(host_ctx) {
        return Ok(None);
    }
    let params = serde_json::json!({
        "sessionId": session_id,
        "cols": cols,
        "rows": rows,
        "requester": requester,
        "force": force,
    });
    match call_api(host_ctx, API_RESIZE, params) {
        Ok(v) => {
            let outcome: ResizeOutcome = serde_json::from_value(v.clone()).map_err(|e| {
                AppError::Plugin(format!("session-resize reply is not a ResizeOutcome: {e} (reply: {v})"))
            })?;
            tracing::debug!(
                session_id = %session_id,
                cols,
                rows,
                force,
                "session resize decided via plugin"
            );
            Ok(Some(outcome))
        }
        Err(e) => {
            log_fallback(API_RESIZE, &e);
            Ok(None)
        }
    }
}
