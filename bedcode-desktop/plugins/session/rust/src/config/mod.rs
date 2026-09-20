//! 会话配置域（票 08）——配置真源在本插件私有库
//!
//! 职责边界（spec D3/D5）：
//! - **在插件**：配置校验规则（环境取值 / WSL 分支合法性 / 空命令兜底）、业务排序与
//!   展示组织、真源读写与一次性幂等迁移
//! - **留宿主**：主库旧表作为「引擎输入投影」（`session/session_manager.rs` 启动会话时
//!   读它；票 09 的 `create-with-spec` 落地后该项依赖消失、旧表可退役）、schema 级
//!   最终仲裁（NOT NULL / CHECK）、同步事件广播（移动端可见形状不变）
//!
//! 插件读 legacy 主库的唯一合法通道是宿主 `host-session` 配置面（票 07）——主库表名强制
//! `plugin_<id>_` 前缀，插件直连会被前缀校验拒。
//!
//! 模块构成：
//! - [`model`]：wire 模型（camelCase，与宿主 DTO 同形）+ 时钟 / ID 值对象
//! - [`store`]：存储端口（wasm = 插件私有库；native 单测注入内存实现）
//! - [`ops`]：校验与归一化、业务排序、写入、迁移编排（真源策略全在此层）

pub mod model;
pub mod ops;
pub mod store;

// ==================== api / 命令面入口（cfg 分流，native 显性失败） ====================
//
// lib.rs 的互调 api 面与命令面只调这些入口；native（cargo test）下 `WasmHost` 没有
// `ConfigStore` / `LegacyConfigSource` impl（wasm 专属 import 符号不在 native 链接），
// 因此这里只是 wasm 运行时的薄包装（同 `trust/ops.rs` 与 `pairing/keys.rs` 的模式）。

#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::wasm_host::WasmHost;

/// 建表（幂等；activate 调用）
#[cfg(target_arch = "wasm32")]
pub fn ensure_schema_via_host() -> Result<(), String> {
    store::ConfigStore::ensure_schema(&WasmHost)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn ensure_schema_via_host() -> Result<(), String> {
    Err("config store unavailable outside wasm runtime".to_string())
}

/// 配置列表（真源 + 插件侧排序）→ `SessionConfig[]`（camelCase）
#[cfg(target_arch = "wasm32")]
pub fn list_via_host() -> Result<serde_json::Value, String> {
    let configs = ops::list(&WasmHost)?;
    serde_json::to_value(configs).map_err(|e| format!("config serialize failed: {}", e))
}

/// HTTP wire 列表（票 02）：真源 → `ConfigItem[]`（只含 6 个 wire 字段，
/// `wslDistro` 显式 null）——网关 /api/configs 的插件侧应答形状
#[cfg(target_arch = "wasm32")]
pub fn list_http_items_via_host() -> Result<Vec<serde_json::Value>, String> {
    let configs = ops::list(&WasmHost)?;
    Ok(configs.iter().map(model::to_http_item).collect())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn list_http_items_via_host() -> Result<Vec<serde_json::Value>, String> {
    Err("config http list unavailable outside wasm runtime".to_string())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn list_via_host() -> Result<serde_json::Value, String> {
    Err("config list unavailable outside wasm runtime".to_string())
}

/// 配置写入（`id` 缺省 → 新建；命中 → 覆盖；非空未命中 → 显性报错）→ 写入后的配置
#[cfg(target_arch = "wasm32")]
pub fn upsert_via_host(draft: serde_json::Value) -> Result<serde_json::Value, String> {
    let draft: model::ConfigDraft =
        serde_json::from_value(draft).map_err(|e| format!("invalid config draft: {}", e))?;
    let written = ops::upsert(&WasmHost, &draft)?;
    serde_json::to_value(written).map_err(|e| format!("config serialize failed: {}", e))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn upsert_via_host(_draft: serde_json::Value) -> Result<serde_json::Value, String> {
    Err("config upsert unavailable outside wasm runtime".to_string())
}

/// 配置删除 → 是否命中（未知 id 幂等 false）
#[cfg(target_arch = "wasm32")]
pub fn delete_via_host(id: &str) -> Result<bool, String> {
    ops::delete(&WasmHost, id)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn delete_via_host(_id: &str) -> Result<bool, String> {
    Err("config delete unavailable outside wasm runtime".to_string())
}

/// 一次性幂等迁移：legacy 主库（经 host-session 配置面读）→ 本插件私有库
#[cfg(target_arch = "wasm32")]
pub fn migrate_via_host() -> Result<ops::MigrationReport, String> {
    ops::migrate(&WasmHost, &WasmHost)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn migrate_via_host() -> Result<ops::MigrationReport, String> {
    Err("config migration unavailable outside wasm runtime".to_string())
}
