//! 会话创建命令桥接（票 09）：创建编排下沉 `com.bedcode.session` 插件
//!
//! 模式与 `utils/auth/auth_center.rs` / `utils/session_config_bridge.rs` 同构：
//! 探活锚点（会话中心互调面已登记）→ JSON-RPC 互调 `session-create` → 插件完成
//! 命名唯一化 + config→launch spec 映射 + 两阶段启动决策，再经 `host-session`
//! `create-with-spec` 原语交内核执行。
//!
//! ## 降级语义（无单点）
//!
//! - 插件未激活（锚点不在注册表）或互调失败（超时 / 响应损坏 / 插件侧显性报错）
//!   → 返回 `Ok(None)`，调用方（`commands/session.rs`）走宿主旧路径
//!   （读主库投影 + 命名服务 + 配置映射服务），行为与迁移前逐字一致。
//!   与 auth 桥接同判据：插件不是权威失败方，插件故障不断用户启动会话路径。
//! - 插件编排成功 → `Ok(Some(session_id))`（预生成 id，实际创建宿主异步执行，
//!   与 `host_session_create` 同语义）。
//!
//! 票 09 契约——`create-with-spec` 落定后投影与旧表一并退役（siegfried
//! `session_config_bridge.rs` 头部注释），届时降级轨只剩「插件未激活」。
//!
//! ## 两阶段启动编排
//!
//! 宿主命令面 `start_session`（创建即启动）与 `create_session_no_start`（只创建
//! 不启动）分别以 `start=true / false` 转发：是否启动由插件在 spec 里决策
//! （spec D3「两阶段启动的编排决策」），内核 `create_session_from_spec` 按
//! `start` 分支执行（Running / Starting + 输出注册时机），行为与两条旧路径等价。

use crate::plugin::manager::wasm_runtime::WasmHostContext;
use crate::utils::auth::auth_center::{call_api, session_active};
use crate::{AppError, Result};

/// 插件互调 api（短名由 `#[plugin_api]` 宏按 manifest.api 比对防漂移）
const API_CREATE: &str = "com.bedcode.session.session-create";

/// 插件侧不可用时记录降级（结构化字段；双轨期未激活是常态）
fn log_fallback(err: &AppError) {
    tracing::warn!(
        api = %API_CREATE,
        error = %err,
        "session create plugin surface unavailable, fallback to host orchestration"
    );
}

/// 经会话中心插件编排创建会话
///
/// - `Ok(Some(session_id))`：插件编排成功（命名唯一化 + launch spec + 创建执行）
/// - `Ok(None)`：插件不可用 / 互调失败 → 调用方降级宿主旧路径（无单点）
/// - `Err`：入参级校验失败（本函数不做此类失败，保留签名兜底）
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
) -> Result<Option<String>> {
    if !session_active(host_ctx) {
        return Ok(None);
    }
    let params = serde_json::json!({
        "configId": config_id,
        "cols": cols,
        "rows": rows,
        "start": start,
    });
    match call_api(host_ctx, API_CREATE, params) {
        Ok(v) => {
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
            Ok(Some(sid))
        }
        Err(e) => {
            log_fallback(&e);
            Ok(None)
        }
    }
}
