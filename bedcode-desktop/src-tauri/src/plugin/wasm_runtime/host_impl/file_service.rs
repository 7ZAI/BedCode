//! 文件服务域宿主实现（挂载/卸载/更新根目录/对端信息）
//!
//! 注册表经 [`WasmHostContext`] 注入（在 PluginHost::new() 中早于插件
//! auto-activate 创建并注入），宿主实现直接从宿主上下文获取 ——
//! 不依赖 AppContext 全局单例（其初始化晚于插件激活，激活期挂载会失败）。
//! 挂载的上传策略钩子目标记为 Wasm（WASM 插件导出 on_upload_request）

use crate::plugin::file_service::HookTarget;
use crate::plugin::wasm_runtime::{block_on_async, WasmHostContext};
use bedcode_plugin_api::permission::PERMISSION_FILESERVICE;
use bedcode_plugin_api::{MountOptions, MountResult};

/// 获取文件服务注册表（经宿主上下文注入，激活期始终可用）
fn file_service_registry(
    host_ctx: &WasmHostContext,
) -> std::sync::Arc<crate::plugin::file_service::FileServiceRegistry> {
    host_ctx.file_service().clone()
}

/// 挂载（权限 + 注册表 mount），返回 MountResult JSON
pub(crate) fn filesrv_mount(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    options_json: &str,
) -> Result<String, String> {
    // 测试钩子：模拟慢宿主调用（燃料看门狗回归测试用——宿主阻塞不得计入
    // guest 燃料消耗，生产环境不设置该变量）
    if let Ok(ms) = std::env::var("BEDCODE_TEST_MOUNT_DELAY_MS") {
        if let Ok(ms) = ms.parse::<u64>() {
            std::thread::sleep(std::time::Duration::from_millis(ms));
        }
    }
    let options: MountOptions = serde_json::from_str(options_json)
        .map_err(|e| format!("file service error: invalid MountOptions JSON: {}", e))?;
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_FILESERVICE, "host_filesrv_mount") {
        return Err("permission denied".to_string());
    }
    let registry = file_service_registry(host_ctx);
    match block_on_async(registry.mount(plugin_id, options, HookTarget::Wasm)) {
        Ok(entry) => {
            let result = MountResult {
                mount_path: entry.mount_path.clone(),
                base_path: format!("/api/plugins/{}/{}", plugin_id, entry.mount_path),
            };
            serde_json::to_string(&result)
                .map_err(|e| format!("file service error: serialize result failed: {}", e))
        }
        Err(e) => Err(format!("file service error: mount failed: {}", e)),
    }
}

/// 卸载挂载点（权限 + 注册表 unmount）
pub(crate) fn filesrv_unmount(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    mount_path: &str,
) -> Result<(), String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_FILESERVICE, "host_filesrv_unmount") {
        return Err("permission denied".to_string());
    }
    let registry = file_service_registry(host_ctx);
    block_on_async(registry.unmount(plugin_id, mount_path))
        .map_err(|e| format!("file service error: unmount failed: {}", e))
}

/// 更新挂载点允许目录根（权限 + 注册表 update_roots）
pub(crate) fn filesrv_update_roots(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    mount_path: &str,
    roots_json: &str,
) -> Result<(), String> {
    let roots: Vec<String> = serde_json::from_str(roots_json)
        .map_err(|e| format!("file service error: invalid roots JSON: {}", e))?;
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_FILESERVICE, "host_filesrv_update_roots") {
        return Err("permission denied".to_string());
    }
    let registry = file_service_registry(host_ctx);
    block_on_async(registry.update_roots(plugin_id, mount_path, roots))
        .map_err(|e| format!("file service error: update roots failed: {}", e))
}

/// 主动询问对端状态（权限 + 经 WS 控制面广播 Query）
///
/// peer_id 为空广播给全部已认证客户端（多设备场景幂等；定向发送暂不支持，
/// WsSessionRegistry 无 device_id 索引）；对端回复 Announce/Withdraw 后
/// 由注册表推送 `filesrv:peer_changed`。
pub(crate) fn filesrv_query_peer(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    peer_id: &str,
) -> Result<(), String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_FILESERVICE, "host_filesrv_query_peer") {
        return Err("permission denied".to_string());
    }
    let payload = crate::enums::FileServicePayload::Query {};
    let json = crate::server::ws::message::Message::file_service(payload)
        .to_json()
        .map_err(|e| format!("file service error: serialize failed: {}", e))?;
    let registry = crate::server::ws::registry::WsSessionRegistry::global();
    block_on_async(registry.broadcast(json, None));
    tracing::debug!(plugin_id = %plugin_id, peer_id = %peer_id, "file service query broadcast");
    Ok(())
}

/// 获取对端文件服务信息（权限 + 注册表查询），未公告返回 None
pub(crate) fn filesrv_get_peer(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    peer_id: &str,
) -> Result<Option<String>, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_FILESERVICE, "host_filesrv_get_peer") {
        return Err("permission denied".to_string());
    }
    let registry = file_service_registry(host_ctx);
    match block_on_async(registry.get_peer(peer_id)) {
        Some(info) => serde_json::to_string(&info)
            .map(Some)
            .map_err(|e| format!("file service error: serialize failed: {}", e)),
        None => Ok(None),
    }
}

/// v2：批准传输批（接收端用户应答「接受全部」，权限 + 注册表 approve_transfer）
pub(crate) fn filesrv_approve_transfer(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    batch_id: &str,
) -> Result<(), String> {
    if !super::check_permission(
        host_ctx,
        plugin_id,
        PERMISSION_FILESERVICE,
        "host_filesrv_approve_transfer",
    ) {
        return Err("permission denied".to_string());
    }
    let registry = file_service_registry(host_ctx);
    block_on_async(registry.approve_transfer(plugin_id, batch_id))
        .map_err(|e| format!("file service error: approve transfer failed: {}", e))
}

/// v2：拒绝传输批（接收端用户应答「拒绝全部」，权限 + 注册表 reject_transfer）
pub(crate) fn filesrv_reject_transfer(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    batch_id: &str,
) -> Result<(), String> {
    if !super::check_permission(
        host_ctx,
        plugin_id,
        PERMISSION_FILESERVICE,
        "host_filesrv_reject_transfer",
    ) {
        return Err("permission denied".to_string());
    }
    let registry = file_service_registry(host_ctx);
    block_on_async(registry.reject_transfer(plugin_id, batch_id))
        .map_err(|e| format!("file service error: reject transfer failed: {}", e))
}

/// v2：设置批准超时（秒，10–600；权限 + 注册表 set_approval_timeout）
pub(crate) fn filesrv_set_approval_timeout(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    mount_path: &str,
    seconds: u64,
) -> Result<(), String> {
    if !super::check_permission(
        host_ctx,
        plugin_id,
        PERMISSION_FILESERVICE,
        "host_filesrv_set_approval_timeout",
    ) {
        return Err("permission denied".to_string());
    }
    let registry = file_service_registry(host_ctx);
    block_on_async(registry.set_approval_timeout(plugin_id, mount_path, seconds))
        .map_err(|e| format!("file service error: set approval timeout failed: {}", e))
}

/// v2：取消接收中的上传会话（接收端本地取消，session 级）
pub(crate) fn filesrv_cancel_receiving(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    session_id: &str,
) -> Result<(), String> {
    if !super::check_permission(
        host_ctx,
        plugin_id,
        PERMISSION_FILESERVICE,
        "host_filesrv_cancel_receiving",
    ) {
        return Err("permission denied".to_string());
    }
    let registry = file_service_registry(host_ctx);
    block_on_async(registry.cancel_receiving_session(plugin_id, session_id))
        .map_err(|e| format!("file service error: cancel receiving failed: {}", e))
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::wasm_runtime::host_impl::tests::{build_host_ctx, grant_permissions};
    use bedcode_plugin_api::FileOperation;
    use serde_json::json;
    use tempfile::tempdir;

    const PLUGIN: &str = "test-plugin";

    /// 构造合法挂载选项 JSON（camelCase 线协议）
    fn mount_options_json(mount_path: &str, roots: &[&str]) -> String {
        json!({
            "mountPath": mount_path,
            "roots": roots,
            "operations": ["list"],
        })
        .to_string()
    }

    /// 非法 MountOptions JSON：解析失败在权限校验前被拒绝
    #[test]
    fn filesrv_mount_invalid_options_json_rejected() {
        let ctx = build_host_ctx();
        let err = filesrv_mount(&ctx, PLUGIN, "not-json").unwrap_err();
        assert!(err.contains("invalid MountOptions JSON"), "got: {}", err);
    }

    /// 无 fileservice 权限：挂载被权限门禁拒绝
    #[test]
    fn filesrv_mount_permission_denied() {
        let ctx = build_host_ctx();
        let err = filesrv_mount(&ctx, PLUGIN, &mount_options_json("m", &["/tmp"])).unwrap_err();
        assert_eq!(err, "permission denied");
    }

    /// 无 fileservice 权限：卸载被拒绝
    #[test]
    fn filesrv_unmount_permission_denied() {
        let ctx = build_host_ctx();
        let err = filesrv_unmount(&ctx, PLUGIN, "m").unwrap_err();
        assert_eq!(err, "permission denied");
    }

    /// 非法 roots JSON：解析失败在权限校验前被拒绝
    #[test]
    fn filesrv_update_roots_invalid_json_rejected() {
        let ctx = build_host_ctx();
        let err = filesrv_update_roots(&ctx, PLUGIN, "m", "not-json").unwrap_err();
        assert!(err.contains("invalid roots JSON"), "got: {}", err);
    }

    /// 无 fileservice 权限：更新根目录被拒绝
    #[test]
    fn filesrv_update_roots_permission_denied() {
        let ctx = build_host_ctx();
        let err = filesrv_update_roots(&ctx, PLUGIN, "m", "[]").unwrap_err();
        assert_eq!(err, "permission denied");
    }

    /// 无 fileservice 权限：对端查询被拒绝（不触达 WsSessionRegistry 全局）
    #[test]
    fn filesrv_query_peer_permission_denied() {
        let ctx = build_host_ctx();
        let err = filesrv_query_peer(&ctx, PLUGIN, "").unwrap_err();
        assert_eq!(err, "permission denied");
    }

    /// 无 fileservice 权限：对端信息读取被拒绝
    #[test]
    fn filesrv_get_peer_permission_denied() {
        let ctx = build_host_ctx();
        let err = filesrv_get_peer(&ctx, PLUGIN, "peer-1").unwrap_err();
        assert_eq!(err, "permission denied");
    }

    // ==================== v2 批命令权限门禁 ====================

    /// 无 fileservice 权限：批准传输批被拒绝
    #[test]
    fn filesrv_approve_transfer_permission_denied() {
        let ctx = build_host_ctx();
        let err = filesrv_approve_transfer(&ctx, PLUGIN, "b1").unwrap_err();
        assert_eq!(err, "permission denied");
    }

    /// 无 fileservice 权限：拒绝传输批被拒绝
    #[test]
    fn filesrv_reject_transfer_permission_denied() {
        let ctx = build_host_ctx();
        let err = filesrv_reject_transfer(&ctx, PLUGIN, "b1").unwrap_err();
        assert_eq!(err, "permission denied");
    }

    /// 无 fileservice 权限：设置批准超时被拒绝
    #[test]
    fn filesrv_set_approval_timeout_permission_denied() {
        let ctx = build_host_ctx();
        let err = filesrv_set_approval_timeout(&ctx, PLUGIN, "m", 60).unwrap_err();
        assert_eq!(err, "permission denied");
    }

    /// 无 fileservice 权限：取消接收会话被拒绝
    #[test]
    fn filesrv_cancel_receiving_permission_denied() {
        let ctx = build_host_ctx();
        let err = filesrv_cancel_receiving(&ctx, PLUGIN, "s1").unwrap_err();
        assert_eq!(err, "permission denied");
    }

    /// 挂载/卸载往返：返回 MountResult 的挂载点与 base_path 约定
    ///
    /// base_path 固定为 /api/plugins/{pluginId}/{mountPath}（HTTP 端点前缀），
    /// 插件据此拼 URL；根目录用临时目录下 .claude 白名单段，无头 fs_auth 直接放行
    #[tokio::test]
    async fn filesrv_mount_unmount_roundtrip() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_FILESERVICE]);
        let dir = tempdir().unwrap();
        let root = dir.path().join(".claude");
        std::fs::create_dir_all(&root).unwrap();

        let result_json = filesrv_mount(
            &ctx,
            PLUGIN,
            &mount_options_json("test-mount", &[root.to_str().unwrap()]),
        )
        .expect("mount ok");
        let result: MountResult = serde_json::from_str(&result_json).expect("valid MountResult");
        assert_eq!(result.mount_path, "test-mount");
        assert_eq!(result.base_path, format!("/api/plugins/{}/test-mount", PLUGIN));

        filesrv_unmount(&ctx, PLUGIN, "test-mount").expect("unmount ok");
        // 重复卸载：注册表对不存在的挂载点报错（非幂等，调用方应捕获）
        assert!(filesrv_unmount(&ctx, PLUGIN, "test-mount").is_err());
    }

    /// 权限通过但挂载路径非法（大写字母不满足 ^[a-z0-9-_]+$）：注册表校验拒绝
    #[tokio::test]
    async fn filesrv_mount_invalid_mount_path_rejected() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_FILESERVICE]);
        let dir = tempdir().unwrap();
        let root = dir.path().join(".claude");
        std::fs::create_dir_all(&root).unwrap();

        let err = filesrv_mount(
            &ctx,
            PLUGIN,
            &mount_options_json("Bad-Path", &[root.to_str().unwrap()]),
        )
        .unwrap_err();
        assert!(err.contains("mount failed"), "got: {}", err);
    }

    /// 挂载失败路径不产生半挂载残留：重复挂载同插件同挂载点被拒绝
    ///
    /// 首次挂载成功后再次挂载相同 mount_path 应报错（注册表去重语义）
    #[tokio::test]
    async fn filesrv_mount_duplicate_rejected() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_FILESERVICE]);
        let dir = tempdir().unwrap();
        let root = dir.path().join(".claude");
        std::fs::create_dir_all(&root).unwrap();
        let opts = mount_options_json("dup-mount", &[root.to_str().unwrap()]);

        filesrv_mount(&ctx, PLUGIN, &opts).expect("first mount ok");
        let err = filesrv_mount(&ctx, PLUGIN, &opts).unwrap_err();
        assert!(err.contains("mount failed"), "got: {}", err);
        // 清理：卸载避免影响其它测试
        filesrv_unmount(&ctx, PLUGIN, "dup-mount").expect("cleanup unmount ok");
    }

    /// 未授权的根目录（非白名单、无弹窗通道）：挂载被拒绝
    ///
    /// 无头 fs_auth 保守拒绝非白名单路径，挂载校验应透传该拒绝
    #[tokio::test]
    async fn filesrv_mount_unauthorized_root_rejected() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_FILESERVICE]);
        let dir = tempdir().unwrap();
        let root = dir.path().to_str().unwrap();

        let err = filesrv_mount(&ctx, PLUGIN, &mount_options_json("root-denied", &[root])).unwrap_err();
        assert!(err.contains("mount failed"), "got: {}", err);
    }

    /// 空 roots：注册表拒绝（规格：roots 不得为空）
    #[tokio::test]
    async fn filesrv_mount_empty_roots_rejected() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_FILESERVICE]);
        let err = filesrv_mount(&ctx, PLUGIN, &mount_options_json("no-roots", &[])).unwrap_err();
        assert!(err.contains("mount failed"), "got: {}", err);
    }
}
