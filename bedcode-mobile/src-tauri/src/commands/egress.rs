//! Egress 授权命令面：弹窗回执 + 设置页查看/撤销

use crate::Result;

// ==================== 弹窗回执 ====================

/// 前端声明/更新桌面端目标（setApiBaseUrl 时调用；L1 放行的 host:port 集合）
///
/// httpProbe 在 ws_connect 前执行，此时 ConnectionManager.target 尚未设置——
/// 前端在 setApiBaseUrl(address, port) 时即声明目标，probe/会话内请求经 L1 放行。
#[tauri::command]
pub fn egress_declare_desktop_target(address: String, port: u16) -> Result<()> {
    crate::egress::policy().add_desktop_target(&address, port);
    Ok(())
}

/// 前端授权弹窗回执（EgressConsentDialog 确认/拒绝时调用）
///
/// `persist` = 用户勾选「不再询问」→ 持久记忆落盘（spec §9 D7）。
/// request_id 不存在（超时已清）时静默成功——弹窗超时兜底在前端也可展示失败。
#[tauri::command]
pub async fn egress_consent_resolve(request_id: String, allow: bool, persist: bool) -> Result<()> {
    let handled = crate::egress::policy()
        .resolve_consent(&request_id, allow, persist)
        .await;
    if !handled {
        tracing::warn!(request_id = %request_id, "egress: consent resolve for unknown request_id");
    }
    Ok(())
}

// ==================== 设置页授权管理（D8） ====================

/// 列出全部授权记录（会话级 + 持久；设置页展示）
#[tauri::command]
pub fn egress_list_grants() -> Result<Vec<crate::egress::PersistentGrant>> {
    Ok(crate::egress::policy().list_grants())
}

/// 撤销全部授权（会话级 + 持久文件清空；设置页「撤销」）
#[tauri::command]
pub fn egress_revoke_grants() -> Result<()> {
    crate::egress::policy().revoke_all_grants();
    Ok(())
}
