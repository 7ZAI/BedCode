//! Mobile Plugin System
//!
//! 插件系统入口 — WASM 动态加载 + 前端插件管理
//!
//! **票 17 批次 2b（垫片形态）+ 票 06 批次 03/04（单一 crate）**：机制面真源在
//! 仓库根单一 wasm-core `packages/bedcode-wasm-core` 的 `mobile-host` 面
//! （manager::runtime / host_api / bus / storage / security::fs_auth /
//! manager::{types,validation,downloader} / terminal_stream_gateway；
//! `bedcode-wasm-core-mobile` 只是 package rename 别名，fork crate 已退役）
//! ——本模块对 `crate::plugin::*` 的历史路径做
//! 转发垫片（76+ 处宿主引用零改动）；宿主自持面（android_plugins / saf_io /
//! saf_path / commands / db_schema / loader / manager / registry / approval /
//! fs_auth / types / host_ports）仍在宿主。

pub mod android_plugins;
pub mod approval;
pub mod commands;
pub mod db_schema;
pub mod downloader;
pub mod fs_auth;
pub mod host_ports;
pub mod loader;
pub mod manager;
pub mod registry;
pub mod saf_io;
pub mod saf_path;
pub mod types;

// ==================== 机制面垫片（真源 = bedcode-wasm-core-mobile） ====================

/// 终端输出流窄转发表（纯机制；Tauri 命令薄壳在 lib.rs 注册面，经此调用）
pub use bedcode_wasm_core_mobile::terminal_stream_gateway;

/// 插件 KV 存储（主库 plugin_storage 表；crate Database wrapper 形状）
pub use bedcode_wasm_core_mobile::storage;

/// 插件间消息总线（dispatcher async 形状，ADR 0029）
pub mod message_bus {
    pub use bedcode_wasm_core_mobile::bus::*;
}

/// 插件身份校验（目录名与 manifest id 一致性，防冒名）
pub use bedcode_wasm_core_mobile::manager::validation;


/// WASM 运行时与 16 域 host 原语（Engine/Store/组件绑定/host_impl；批次 1b 迁入）
pub use bedcode_wasm_core_mobile::manager::runtime as wasm_runtime;

/// WASM Host Function 通用工具（HTTP 代理执行 + SQL 表名前缀护栏；
/// 原宿主 wasm_host.rs 拆分迁入 crate host_api——符号面逐字保真）
pub mod wasm_host {
    pub use bedcode_wasm_core_mobile::host_api::http_engine::*;
    pub use bedcode_wasm_core_mobile::host_api::sql_guard::*;
}

pub use android_plugins::{asset_extractor_plugin, foreground_service_plugin};
pub use registry::builtin_manifests;
