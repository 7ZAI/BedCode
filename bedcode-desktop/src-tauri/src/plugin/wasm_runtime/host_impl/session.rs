//! 会话域宿主实现（会话查询、配置列表与会话创建）

use crate::plugin::permission::{PERMISSION_SESSION_READ, PERMISSION_SESSION_WRITE};
use crate::plugin::wasm_runtime::{block_on_async, WasmHostContext};
use crate::system::error_boundary::spawn_with_error_boundary;
use uuid::Uuid;

/// 列出所有会话（权限 + SessionManager 查询），返回 JSON 数组字符串
pub(crate) fn session_list(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
) -> Result<Option<String>, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_SESSION_READ, "host_session_list") {
        return Err("permission denied".to_string());
    }
    let sm = host_ctx.session_manager.clone();
    let sessions = block_on_async(sm.list_sessions());
    serde_json::to_string(&sessions)
        .map(Some)
        .map_err(|e| format!("session error: JSON serialization failed: {}", e))
}

/// 获取单个会话（权限 + 查询），不存在返回 None
pub(crate) fn session_get(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    session_id: &str,
) -> Result<Option<String>, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_SESSION_READ, "host_session_get") {
        return Err("permission denied".to_string());
    }
    let sm = host_ctx.session_manager.clone();
    match block_on_async(sm.get_session(session_id)) {
        Some(info) => serde_json::to_string(&info)
            .map(Some)
            .map_err(|e| format!("session error: JSON serialization failed: {}", e)),
        None => Ok(None),
    }
}

/// 列出会话配置精简列表（id/name/workingDir/command）
pub(crate) fn session_config_list(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
) -> Result<Option<String>, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_SESSION_READ, "host_session_config_list") {
        return Err("permission denied".to_string());
    }
    let cm = host_ctx.config_manager.clone();
    let configs = block_on_async(cm.list_configs())
        .map_err(|e| format!("session error: {}", e))?;
    // 精简输出：仅包含插件需要的字段，避免传输不必要的数据
    // （name 供插件 UI 展示会话配置选择列表，如定时任务选配置）
    let simplified: Vec<serde_json::Value> = configs.iter().map(|c| {
        serde_json::json!({
            "id": c.id,
            "name": c.name,
            "workingDir": c.working_dir,
            "command": c.command,
        })
    }).collect();
    serde_json::to_string(&simplified)
        .map(Some)
        .map_err(|e| format!("session error: JSON serialization failed: {}", e))
}

/// 按配置创建新会话（v6，ADR 0003），返回预生成的 session_id
///
/// 创建为宿主异步执行：wasm 调用栈内同步创建会死锁 —— `create_session`
/// 会同步分发 Creating/Created 生命周期事件，而事件回灌同一插件实例需要
/// 重新获取 `wasm_plugins` 写锁（该锁正被当前 wasm 调用持有，tokio RwLock
/// 不可重入），且 wasmtime Store 不可重入。因此此处预生成会话 ID 立即返回，
/// 实际创建在宿主上下文异步执行：事件分发发生在 wasm 调用返回（锁释放）后，
/// hooks 仍先于 PTY 启动就位。
pub(crate) fn session_create(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    config_id: &str,
) -> Result<String, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_SESSION_WRITE, "host_session_create") {
        return Err("permission denied".to_string());
    }
    if config_id.is_empty() {
        return Err("session error: empty config_id".to_string());
    }
    let sm = host_ctx.session_manager.clone();
    // 预生成会话 ID 并异步创建：插件侧照常将 job 置 creating 并记录 session_id，
    // 等待 Created 事件（携带同一 session_id）完成匹配，语义与同步创建一致
    let session_id = Uuid::new_v4().to_string();
    let cid = config_id.to_string();
    let sid = session_id.clone();
    let pid = plugin_id.to_string();
    spawn_with_error_boundary("host_session_create", async move {
        match sm.create_session_with_id(&cid, &sid).await {
            Ok(_) => {
                tracing::info!(
                    plugin_id = %pid,
                    config_id = %cid,
                    session_id = %sid,
                    "host_session_create: session created (async)"
                );
            }
            Err(e) => {
                // 创建失败无同步返回通道：插件侧由 creating 超时看门狗置 failed
                tracing::error!(
                    plugin_id = %pid,
                    config_id = %cid,
                    session_id = %sid,
                    error = %e,
                    "host_session_create: create_session failed (async)"
                );
            }
        }
    });
    Ok(session_id)
}

/// 关闭（终止）会话（v7，需要 `session:write` 权限）
///
/// 包一层核心已有的 `SessionManager::kill_session_with_source`，供插件
/// （如 auto-task 定时自动任务）在执行完毕后关闭自己创建的会话。
/// 停止 PTY 并置 Stopped，会话记录保留（与用户手动关闭一致）。
///
/// **异步执行**：`kill_session_with_source` 会同步分发 Stopping/Stopped
/// 生命周期事件，事件回灌同一插件实例需要重新获取 `wasm_plugins` 写锁
/// （tokio RwLock 不可重入）——与 `session_create` 同理，此处 spawn 异步执行，
/// wasm 调用立即返回。
pub(crate) fn session_close(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    session_id: &str,
) -> Result<(), String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_SESSION_WRITE, "host_session_close") {
        return Err("permission denied".to_string());
    }
    if session_id.is_empty() {
        return Err("session error: empty session_id".to_string());
    }
    let sm = host_ctx.session_manager.clone();
    let sid = session_id.to_string();
    let pid = plugin_id.to_string();
    spawn_with_error_boundary("host_session_close", async move {
        match sm.kill_session_with_source(&sid, None).await {
            Ok(_) => {
                tracing::info!(
                    plugin_id = %pid,
                    session_id = %sid,
                    "host_session_close: session closed (async)"
                );
            }
            Err(e) => {
                tracing::error!(
                    plugin_id = %pid,
                    session_id = %sid,
                    error = %e,
                    "host_session_close: kill_session failed (async)"
                );
            }
        }
    });
    Ok(())
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::wasm_runtime::host_impl::tests::{build_host_ctx, grant_permissions};

    const PLUGIN: &str = "test-plugin";

    /// 无 session:read 权限：会话列表被拒绝
    #[test]
    fn session_list_permission_denied() {
        let ctx = build_host_ctx();
        let err = session_list(&ctx, PLUGIN).unwrap_err();
        assert_eq!(err, "permission denied");
    }

    /// 无 session:read 权限：单会话查询被拒绝
    #[test]
    fn session_get_permission_denied() {
        let ctx = build_host_ctx();
        let err = session_get(&ctx, PLUGIN, "s1").unwrap_err();
        assert_eq!(err, "permission denied");
    }

    /// 无 session:read 权限：配置列表被拒绝
    #[test]
    fn session_config_list_permission_denied() {
        let ctx = build_host_ctx();
        let err = session_config_list(&ctx, PLUGIN).unwrap_err();
        assert_eq!(err, "permission denied");
    }

    /// 无 session:write 权限：创建会话被拒绝
    #[test]
    fn session_create_permission_denied() {
        let ctx = build_host_ctx();
        let err = session_create(&ctx, PLUGIN, "cfg-1").unwrap_err();
        assert_eq!(err, "permission denied");
    }

    /// 空 config_id：权限通过后参数校验拒绝（避免无效会话创建）
    #[test]
    fn session_create_empty_config_id_rejected() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_SESSION_WRITE]);
        let err = session_create(&ctx, PLUGIN, "").unwrap_err();
        assert_eq!(err, "session error: empty config_id");
    }

    /// 无 session:write 权限：关闭会话被拒绝
    #[test]
    fn session_close_permission_denied() {
        let ctx = build_host_ctx();
        let err = session_close(&ctx, PLUGIN, "s1").unwrap_err();
        assert_eq!(err, "permission denied");
    }

    /// 空 session_id：权限通过后参数校验拒绝（防误杀全量会话）
    #[test]
    fn session_close_empty_session_id_rejected() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_SESSION_WRITE]);
        let err = session_close(&ctx, PLUGIN, "").unwrap_err();
        assert_eq!(err, "session error: empty session_id");
    }

    /// 空会话库：列表返回空 JSON 数组（内存 SessionManager）
    #[tokio::test]
    async fn session_list_empty_ok() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_SESSION_READ]);
        let json = session_list(&ctx, PLUGIN).expect("list ok").expect("some value");
        assert_eq!(json, "[]");
    }

    /// 空配置库：精简配置列表返回空 JSON 数组
    #[tokio::test]
    async fn session_config_list_empty_ok() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_SESSION_READ]);
        let json = session_config_list(&ctx, PLUGIN).expect("list ok").expect("some value");
        assert_eq!(json, "[]");
    }

    /// 不存在的会话：session_get 返回 Ok(None)（非错误）
    #[tokio::test]
    async fn session_get_missing_returns_none() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_SESSION_READ]);
        let result = session_get(&ctx, PLUGIN, "no-such-session").expect("get ok");
        assert!(result.is_none());
    }

    /// 预生成 session_id：同步返回 UUID v4，实际创建在后台异步执行
    ///
    /// 配置不存在时后台创建失败仅记录日志（插件侧由 creating 超时看门狗接管），
    /// 同步路径不受影响 —— 断言返回值的 UUID 形态而非创建结果
    #[tokio::test]
    async fn session_create_returns_pre_generated_uuid() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_SESSION_WRITE]);
        let sid = session_create(&ctx, PLUGIN, "no-such-config").expect("pre-generated id");
        assert_eq!(sid.len(), 36);
        let uuid = Uuid::parse_str(&sid).expect("valid uuid");
        assert_eq!(uuid.get_version_num(), 4);
    }

    /// 关闭不存在的会话：同步返回 Ok（异步 kill 失败仅记录日志）
    #[tokio::test]
    async fn session_close_missing_session_returns_ok() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_SESSION_WRITE]);
        session_close(&ctx, PLUGIN, "no-such-session").expect("close ok");
    }
}
