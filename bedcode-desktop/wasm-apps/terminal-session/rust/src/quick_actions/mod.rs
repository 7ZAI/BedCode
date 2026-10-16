//! 快捷指令域（票 02）——第 4 域：私有库持久化 + HTTP 查询面 + 迁移导入
//!
//! 职责边界（spec 决策 3/4）：
//! - **在插件**：真源读写（私有库 `quick_actions` 表）、业务排序（`sort_order`
//!   升序）、一次性幂等迁移（marker 语义）、HTTP 查询面（`configs` / `quick-actions`
//!   两个业务端点的插件侧实现，wire 形状与宿主 DTO 逐字节一致）
//! - **留宿主**：legacy 主库旧表（handoff 读它推送）、网关别名路由（Forward /
//!   PluginRequired 判定）、HTTP 中间件验签
//!
//! 模块构成：
//! - [`model`]：wire 模型（camelCase，与宿主 `QuickActionItem` 同形）+ 内部字段
//! - [`store`]：存储端口（wasm = 插件私有库；native 单测注入内存实现）
//! - [`ops`]：业务排序、一次性幂等导入、HTTP wire 装配
//!
//! 对外面：
//! - 互调 api `com.bedcode.terminal-session.quick-actions-import`（宿主 handoff 推送通道，
//!   见 `ops::import`；宿主侧模块 `src-tauri/src/plugin/quick_actions_migration.rs`）
//! - HTTP `GET quick-actions`（网关别名 `/api/quick-actions` 的插件目标）

pub mod model;
pub mod ops;
pub mod store;

// ==================== api / 命令面入口（cfg 分流，native 显性失败） ====================
//
// lib.rs 的互调 api 面与 HTTP 分派只调这些入口；native（cargo test）下 `WasmHost`
// 没有 `QuickActionStore` impl（wasm 专属 import 符号不在 native 链接），因此这里
// 只是 wasm 运行时的薄包装（同 `config/mod.rs` 与 `trust/ops.rs` 的模式）。

#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::wasm_host::WasmHost;

/// 建表（幂等；activate 调用）
#[cfg(target_arch = "wasm32")]
pub fn ensure_schema_via_host() -> Result<(), String> {
    store::QuickActionStore::ensure_schema(&WasmHost)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn ensure_schema_via_host() -> Result<(), String> {
    Err("quick action store unavailable outside wasm runtime".to_string())
}

/// 全量列表（真源 + 业务排序）→ `QuickAction[]`（camelCase，含内部字段；
/// HTTP wire 装配时再去内部字段）
#[cfg(target_arch = "wasm32")]
pub fn list_via_host() -> Result<serde_json::Value, String> {
    let actions = ops::list(&WasmHost)?;
    serde_json::to_value(actions).map_err(|e| format!("quick action serialize failed: {}", e))
}

/// HTTP wire 列表（票 02）：真源 → `QuickActionItem[]`（只含 5 个 wire 字段，
/// `icon` / `color` 显式 null）——网关 /api/quick-actions 的插件侧应答形状
#[cfg(target_arch = "wasm32")]
pub fn list_http_items_via_host() -> Result<Vec<serde_json::Value>, String> {
    let actions = ops::list(&WasmHost)?;
    Ok(actions.iter().map(model::to_http_item).collect())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn list_http_items_via_host() -> Result<Vec<serde_json::Value>, String> {
    Err("quick action http list unavailable outside wasm runtime".to_string())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn list_via_host() -> Result<serde_json::Value, String> {
    Err("quick action list unavailable outside wasm runtime".to_string())
}

/// 一次性幂等导入（宿主 handoff 推送）→ `ImportReport`（camelCase）
#[cfg(target_arch = "wasm32")]
pub fn import_via_host(rows: serde_json::Value) -> Result<serde_json::Value, String> {
    let rows: Vec<model::QuickActionRow> =
        serde_json::from_value(rows).map_err(|e| format!("invalid quick action rows: {}", e))?;
    let report = ops::import(&WasmHost, &rows)?;
    serde_json::to_value(report).map_err(|e| format!("quick action import serialize failed: {}", e))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn import_via_host(_rows: serde_json::Value) -> Result<serde_json::Value, String> {
    Err("quick action import unavailable outside wasm runtime".to_string())
}
