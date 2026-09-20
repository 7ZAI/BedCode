//! 会话配置命令桥接（票 08）：插件真源 ↔ 主库投影
//!
//! 模式与 `utils/auth/auth_center.rs` 同构（探活锚点 + JSON-RPC 互调 + 降级 + warn
//! 留痕），区别只有一点：**转发成功后要把结果投影回主库**。
//!
//! ## 为什么还要主库那一份（本票设计决策）
//!
//! 票 08 之后，产品语义的真源在插件私有库（`com.bedcode.session`），但内核会话启动
//! 路径仍直接读主库配置表（`session/session_manager.rs::create_session_with_source_and_id`
//! → `storage.get_config`）。真源若只活在插件里，「迁移后新建/修改的配置」就无法启动
//! 会话。故本桥接在插件写入成功后，把返回的配置**投影**回主库
//! （[`SessionConfigManager::upsert_config`]，保留插件生成的 id 与时间戳）——
//! 主库那一份自此只是「引擎输入投影」：单写者 = 宿主，插件碰不到它（主库表名强制
//! `plugin_<id>_` 前缀，前缀校验会拒）。
//!
//! 票 09（`create-with-spec`）让内核不再依赖配置表后，投影与旧表一并退役。
//!
//! ## 降级（插件未激活 / 互调失败）
//!
//! 回落宿主 `SessionConfigManager`（迁移前行为），`warn` 留痕：双轨期不允许出现
//! 「配置面单点」——插件不在时前端与移动端仍可读改配置（读投影、写主库）。

use crate::db::SessionConfig;
use crate::plugin::manager::wasm_runtime::WasmHostContext;
use crate::session::SessionConfigManager;
use crate::utils::auth::auth_center::{call_api, session_active};
use crate::{AppError, Result};

/// 插件互调 api（短名由 `#[plugin_api]` 宏按 manifest.api 比对防漂移）
const API_LIST: &str = "com.bedcode.session.config-list";
const API_UPSERT: &str = "com.bedcode.session.config-upsert";
const API_DELETE: &str = "com.bedcode.session.config-delete";

/// 插件配置面是否可用：与 auth 桥接同一判据（会话中心互调面已登记）
fn plugin_available(host_ctx: &WasmHostContext) -> bool {
    session_active(host_ctx)
}

/// 插件侧不可用时记录降级（结构化字段；双轨期未激活是常态）
fn log_fallback(api: &str, err: &AppError) {
    tracing::warn!(
        api = %api,
        error = %err,
        "session config plugin surface unavailable, fallback to host store"
    );
}

/// 配置列表：插件真源（含插件侧业务排序）→ 降级读主库投影
pub async fn list_configs(host_ctx: &WasmHostContext, cm: &SessionConfigManager) -> Result<Vec<SessionConfig>> {
    if plugin_available(host_ctx) {
        match call_api(host_ctx, API_LIST, serde_json::Value::Null) {
            Ok(value) => return serde_json::from_value(value).map_err(AppError::Serialization),
            Err(e) => log_fallback(API_LIST, &e),
        }
    }
    cm.list_configs().await
}

/// 单条配置：插件侧没有 per-id api（配置量级为十位），列表过滤；
/// 降级读主库投影（投影由本桥接与迁移保持同步）
pub async fn get_config(
    host_ctx: &WasmHostContext,
    cm: &SessionConfigManager,
    id: &str,
) -> Result<Option<SessionConfig>> {
    if plugin_available(host_ctx) {
        match call_api(host_ctx, API_LIST, serde_json::Value::Null) {
            Ok(value) => {
                let configs: Vec<SessionConfig> = serde_json::from_value(value).map_err(AppError::Serialization)?;
                return Ok(configs.into_iter().find(|c| c.id == id));
            }
            Err(e) => log_fallback(API_LIST, &e),
        }
    }
    cm.get_config(id).await
}

/// 新建：插件生成 id（真源）→ 投影主库；降级走宿主原路径
pub async fn create_config(
    host_ctx: &WasmHostContext,
    cm: &SessionConfigManager,
    name: String,
    environment: String,
    wsl_distro: Option<String>,
    working_dir: String,
    command: String,
) -> Result<SessionConfig> {
    if plugin_available(host_ctx) {
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
            Ok(value) => {
                let written: SessionConfig = serde_json::from_value(value).map_err(AppError::Serialization)?;
                return cm.upsert_config(written).await;
            }
            Err(e) => log_fallback(API_UPSERT, &e),
        }
    }
    cm.create_config_full(name, environment, wsl_distro, working_dir, command, false)
        .await
}

/// 更新：插件真源覆盖（缺省字段回落既有值）→ 投影主库；降级走宿主原路径
#[allow(clippy::too_many_arguments)]
pub async fn update_config(
    host_ctx: &WasmHostContext,
    cm: &SessionConfigManager,
    id: &str,
    name: String,
    environment: String,
    wsl_distro: Option<String>,
    working_dir: String,
    command: String,
    auto_start: Option<bool>,
) -> Result<SessionConfig> {
    if plugin_available(host_ctx) {
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
            Ok(value) => {
                let written: SessionConfig = serde_json::from_value(value).map_err(AppError::Serialization)?;
                return cm.upsert_config(written).await;
            }
            Err(e) => log_fallback(API_UPSERT, &e),
        }
    }
    cm.update_config(
        id,
        Some(name),
        Some(environment),
        wsl_distro,
        Some(working_dir),
        Some(command),
        auto_start,
    )
    .await
}

/// 删除：插件真源删除 → 投影同步删除（幂等）；降级只删主库
pub async fn delete_config(host_ctx: &WasmHostContext, cm: &SessionConfigManager, id: &str) -> Result<()> {
    if plugin_available(host_ctx) {
        match call_api(host_ctx, API_DELETE, serde_json::json!(id)) {
            Ok(_) => {
                // 投影删除幂等（未知 id 在 DB 层是 no-op）
                cm.delete_config(id).await?;
                return Ok(());
            }
            Err(e) => log_fallback(API_DELETE, &e),
        }
    }
    cm.delete_config(id).await
}
