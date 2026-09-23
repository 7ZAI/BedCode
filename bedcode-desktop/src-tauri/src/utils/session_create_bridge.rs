//! 会话创建命令桥接（票 09）：创建编排下沉 `com.bedcode.terminal-session` 插件
//!
//! 模式与 `utils/auth/auth_center.rs` / `utils/session_config_bridge.rs` 同构：
//! 探活锚点（会话中心互调面已登记）→ JSON-RPC 互调 `session-create` → 插件完成
//! 命名唯一化 + config→launch spec 映射 + 两阶段启动决策，再经 `host-session`
//! `create-with-spec` 原语交内核执行。
//!
//! ## 插件必需（host-business-decarriage 收尾）
//!
//! 本桥接**不再有宿主降级轨**：创建编排（命名唯一化 / config→launch / 何时启动）
//! 与配置真源（插件私有库）都在插件侧，宿主旧路径只读主库投影——**对 票 08 之后
//! 新建的配置本就无法启动会话**，那条「降级」是伪降级（对老库配置可用、对新配置
//! 必失败）。按无业务内核口径（ADR 0022：业务数据真源进插件后宿主不留副本），
//! 会话创建统一走插件编排；插件未激活时**显性报错**，不回退宿主。
//!
//! 内核遗留通道已于 v21 退役：`host-session.create(config-id)` 与内核
//! `create_session_with_id` / `create_session_with_source_and_id` 随最后一个
//! 消费者（定时任务域，见 `plugins/terminal-session/rust/src/task/scheduled.rs`）改走
//! `create-with-spec` 一并删除。会话创建自此只有一条编排入口：本桥接。
//!
//! ## 两阶段启动编排
//!
//! 宿主命令面 `start_session`（创建即启动）与 `create_session_no_start`（只创建
//! 不启动）分别以 `start=true / false` 转发：是否启动由插件在 spec 里决策
//! （spec D3「两阶段启动的编排决策」），内核 `create_session_from_spec` 按
//! `start` 分支执行（Running / Starting + 输出注册时机）。

use crate::wasm_core::manager::runtime::WasmHostContext;
use crate::utils::auth::auth_center::{call_api, session_active};
use crate::{AppError, Result};

/// 插件互调 api（短名由 `#[plugin_api]` 宏按 manifest.api 比对防漂移）
const API_CREATE: &str = "com.bedcode.terminal-session.session-create";

/// 插件不可用时的显性错误（无降级；文案面向用户可见的错误通道）
fn plugin_required_error() -> AppError {
    AppError::Plugin("session plugin not active: session create requires com.bedcode.terminal-session".to_string())
}

/// 经会话中心插件编排创建会话（插件必需，无宿主降级）
///
/// - `Ok(session_id)`：插件编排成功（命名唯一化 + launch spec + 创建执行，预生成 id）
/// - `Err`：插件未激活（锚点不在注册表）或互调失败（超时 / 响应损坏 / 插件侧显性报错）
///
/// 入参 `start` = 两阶段启动编排决策：`true` 创建即启动（`start_session` 语义），
/// `false` 只创建不启动（`create_session_no_start` 语义，后续
/// `start_existing_session` 接管启动）。
pub async fn create_session_via_plugin(
    host_ctx: &WasmHostContext,
    config_id: &str,
    cols: Option<u16>,
    rows: Option<u16>,
    start: bool,
    source_device: Option<&str>,
) -> Result<String> {
    if !session_active(host_ctx) {
        tracing::warn!(
            config_id = %config_id,
            "session create refused: session plugin not active (plugin required, no host fallback)"
        );
        return Err(plugin_required_error());
    }
    let params = serde_json::json!({
        "configId": config_id,
        "cols": cols,
        "rows": rows,
        "start": start,
        // 启动端事实透传（移动端 HTTP/WS 启动携带设备名 → 正统端初始归属该端）
        "sourceDevice": source_device,
    });
    let v = call_api(host_ctx, API_CREATE, params).map_err(|e| {
        tracing::error!(
            config_id = %config_id,
            error = %e,
            "session create failed via plugin orchestration"
        );
        AppError::Plugin(format!("session create failed (plugin error): {e}"))
    })?;
    let sid = v
        .get("sessionId")
        .and_then(|s| s.as_str())
        .ok_or_else(|| AppError::Plugin(format!("session-create reply missing sessionId: {v}")))?
        .to_string();
    tracing::info!(
        config_id = %config_id,
        session_id = %sid,
        start,
        "session create orchestrated via plugin"
    );
    Ok(sid)
}
