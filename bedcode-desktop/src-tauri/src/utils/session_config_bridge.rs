//! 会话配置命令桥接（票 08）：插件真源为唯一读写面
//!
//! 模式与 `utils/session_create_bridge.rs` / `utils/session_action_bridge.rs` 同构
//! （探活锚点 + JSON-RPC 互调），但**只有一条路径**：
//!
//! ## 为什么不再有宿主降级与主库投影（v21，host-business-decarriage 收尾）
//!
//! 票 08 期初的设计是「插件真源 + 主库投影」双写：投影存在的唯一理由是**内核会话
//! 启动/重启路径直接读主库配置表**（`create_session_with_source_and_id` /
//! `restart_session` → `storage.get_config`）。v21 起内核创建与重启执行器全部退役
//! （`host-session.create` / `restart` 删除，创建统一走 `create-with-spec`、重启
//! 由插件用 remove + create-with-spec 编排），**投影已无任何读者** → 投影写入、
//! 降级读取与主库业务副本一并退役。
//!
//! 保留（不属于本文件）：内核 `session_configs` 表与 `host-session.config-*` 原语
//! 仍是插件**一次性迁移通道**（老库存量配置 → 插件私有库，marker 幂等），其退役
//! 需先确认各安装点迁移已跑过（见 `.scratch/2026-09-21-host-rust-residue/`）。
//!
//! ## 插件必需
//!
//! 插件未激活 / 互调失败 → 显性报错（业务数据真源在插件侧，宿主不留副本，
//! 不回退假数据）。

use crate::db::SessionConfig;
use crate::plugin::manager::wasm_runtime::WasmHostContext;
use crate::utils::auth::auth_center::{call_api, session_active};
use crate::{AppError, Result};

/// 插件互调 api（短名由 `#[plugin_api]` 宏按 manifest.api 比对防漂移）
const API_LIST: &str = "com.bedcode.session.config-list";
const API_UPSERT: &str = "com.bedcode.session.config-upsert";
const API_DELETE: &str = "com.bedcode.session.config-delete";

/// 插件不可用时的显性错误（无降级；文案面向用户可见的错误通道）
fn plugin_required_error(op: &str) -> AppError {
    AppError::Plugin(format!(
        "session plugin not active: {op} requires com.bedcode.session"
    ))
}

/// 探活 + 报错（统一两处判据：锚点在注册表且插件已激活）
fn ensure_plugin(host_ctx: &WasmHostContext, op: &str) -> Result<()> {
    if !session_active(host_ctx) {
        tracing::warn!(op = %op, "session config refused: session plugin not active (plugin required)");
        return Err(plugin_required_error(op));
    }
    Ok(())
}

/// 配置列表：插件真源（含插件侧业务排序）
pub async fn list_configs(host_ctx: &WasmHostContext) -> Result<Vec<SessionConfig>> {
    ensure_plugin(host_ctx, "session.config.list")?;
    match call_api(host_ctx, API_LIST, serde_json::Value::Null) {
        Ok(value) => serde_json::from_value(value).map_err(AppError::Serialization),
        Err(e) => {
            tracing::error!(error = %e, "session config list failed via plugin");
            Err(AppError::Plugin(format!("session config list failed (plugin error): {e}")))
        }
    }
}

/// 单条配置：插件侧没有 per-id api（配置量级为十位），列表过滤
pub async fn get_config(host_ctx: &WasmHostContext, id: &str) -> Result<Option<SessionConfig>> {
    let configs = list_configs(host_ctx).await?;
    Ok(configs.into_iter().find(|c| c.id == id))
}

/// 新建：插件生成 id 并落私有库（真源），返回写入结果
pub async fn create_config(
    host_ctx: &WasmHostContext,
    name: String,
    environment: String,
    wsl_distro: Option<String>,
    working_dir: String,
    command: String,
) -> Result<SessionConfig> {
    ensure_plugin(host_ctx, "session.config.upsert")?;
    let mut draft = serde_json::json!({
        "name": name,
        "environment": environment,
        "workingDir": working_dir,
        "command": command,
    });
    if let Some(distro) = wsl_distro.as_deref() {
        draft["wslDistro"] = serde_json::json!(distro);
    }
    match call_api(host_ctx, API_UPSERT, draft) {
        Ok(value) => serde_json::from_value(value).map_err(AppError::Serialization),
        Err(e) => {
            tracing::error!(error = %e, "session config create failed via plugin");
            Err(AppError::Plugin(format!("session config create failed (plugin error): {e}")))
        }
    }
}

/// 更新：插件真源覆盖（缺省字段回落既有值）
#[allow(clippy::too_many_arguments)]
pub async fn update_config(
    host_ctx: &WasmHostContext,
    id: &str,
    name: String,
    environment: String,
    wsl_distro: Option<String>,
    working_dir: String,
    command: String,
    auto_start: Option<bool>,
) -> Result<SessionConfig> {
    ensure_plugin(host_ctx, "session.config.upsert")?;
    let mut draft = serde_json::json!({
        "id": id,
        "name": name,
        "environment": environment,
        "workingDir": working_dir,
        "command": command,
    });
    if let Some(distro) = wsl_distro.as_deref() {
        draft["wslDistro"] = serde_json::json!(distro);
    }
    if let Some(auto) = auto_start {
        draft["autoStart"] = serde_json::json!(auto);
    }
    match call_api(host_ctx, API_UPSERT, draft) {
        Ok(value) => serde_json::from_value(value).map_err(AppError::Serialization),
        Err(e) => {
            tracing::error!(config_id = %id, error = %e, "session config update failed via plugin");
            Err(AppError::Plugin(format!("session config update failed (plugin error): {e}")))
        }
    }
}

/// 删除：插件真源删除（幂等：未知 id 在插件侧 no-op）
pub async fn delete_config(host_ctx: &WasmHostContext, id: &str) -> Result<()> {
    ensure_plugin(host_ctx, "session.config.delete")?;
    match call_api(host_ctx, API_DELETE, serde_json::json!(id)) {
        Ok(_) => Ok(()),
        Err(e) => {
            tracing::error!(config_id = %id, error = %e, "session config delete failed via plugin");
            Err(AppError::Plugin(format!("session config delete failed (plugin error): {e}")))
        }
    }
}
