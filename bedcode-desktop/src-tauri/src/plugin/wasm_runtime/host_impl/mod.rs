//! WASM 宿主能力实现层（Component Model 绑定调用）
//!
//! 迁移阶段 C 后宿主能力只剩 Component Model 一种形态：
//! 本模块提供 13 组宿主能力的功能域实现（权限校验 + 宿主服务调用），
//! 由 `wasm_runtime::component` 的 Host trait 绑定逐接口调用。
//!
//! 各功能域与 SDK `host/*` trait 一一对应：
//! storage / database / terminal / session / events / http / log / fs / config /
//! bus / lifecycle / file_service / transfer
//!
//! 历史：阶段 A/B 时本目录名为 `host_functions`，包含 core module 胶水层
//! （(ptr,len) 内存搬运 + Linker 注册）；阶段 C 已删除胶水层，仅保留实现层。

pub(super) mod bus;
pub(super) mod config;
pub(super) mod database;
pub(super) mod events;
pub(super) mod file_service;
pub(super) mod fs;
pub(super) mod http;
pub(super) mod lifecycle;
pub(super) mod log;
pub(super) mod session;
pub(super) mod status;
pub(super) mod storage;
pub(super) mod terminal;
pub(super) mod timer;
pub(super) mod transfer;
mod wsl_fs;

use crate::plugin::wasm_runtime::WasmHostContext;

// ==================== Shared Guards ====================

/// 统一权限守卫
///
/// 校验通过返回 true；拒绝时记录结构化错误日志并返回 false，
/// 调用方据此返回 Err。替换原先约 30 处重复的 check/log 三连。
pub(super) fn check_permission(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    permission: &str,
    api: &str,
) -> bool {
    if host_ctx.permission.check(plugin_id, permission) {
        true
    } else {
        tracing::error!(plugin_id = %plugin_id, permission = %permission, "{}: permission denied", api);
        false
    }
}
