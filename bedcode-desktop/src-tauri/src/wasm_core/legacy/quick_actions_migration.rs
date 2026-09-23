//! 快捷指令 legacy 主库 → session 插件私有库的一次性搬运（票 02）
//!
//! **为什么需要它**：快捷指令真源下沉 session 插件第 4 域（私有库 `quick_actions`
//! 表）后，历史数据还在宿主主库旧表里。会话配置的迁移有 `host-session` 配置面可
//! 供插件拉取（票 08），快捷指令没有对应 face——按 spec 决策 6「不新增 host 原语、
//! 不触发 ABI bump」，迁移由**宿主侧**完成：本模块读取 legacy 主库 `quick_actions`
//! 行，经互调 api `com.bedcode.terminal-session.quick-actions-import`（JSON-RPC 2.0 over
//! host-bus，与 `auth_center::call_api` 同一通道）推给 session 插件；插件侧按
//! marker 一次性语义幂等落库（重复推送整体跳过）。
//!
//! **触发时机**：`lib.rs` setup 阶段 `PluginHost::new()` 之后（与
//! [`crate::wasm_core::task_data_migration`] 同一位置）——此刻插件已按持久化状态
//! 自动激活，互调面已登记。插件未激活 → 本轮跳过（legacy 数据留在主库，双轨期
//! 由宿主旧面继续服务）；下次启动插件激活后再搬。已迁移（插件 marker 在）→
//! 插件直接回 already_migrated，宿主只记日志，不重复搬运。
//!
//! **已知边界（本票记录）**：插件在运行中途被启用（非启动时激活）时，本模块不会
//! 感知——legacy 数据仍留在主库（零丢失），插件面直到下次启动才补齐。契约退役
//! （删表）前需确认 handoff 已跑过（验收含「旧库重跑 + 历史零丢失」）。
//!
//! **与 [`crate::wasm_core::task_data_migration`] 的区别**：那边是「旧插件私有库文件
//! → 新插件私有库文件」的直拷；这里是「宿主主库 → 插件私有库」的经互调 api 推送
//! （插件保持自身 schema 的唯一写者，宿主不触碰插件私有库表结构）。

use crate::db::{Database, LegacyQuickActionRow};
use crate::wasm_core::manager::runtime::WasmHostContext;
use crate::system::app_context::AppContext;
use crate::utils::auth::auth_center::{call_api, session_active};
use crate::{AppError, Result};

/// 插件互调 api（短名由 `#[plugin_api]` 宏按 manifest.api 比对防漂移）
pub const API_QUICK_ACTIONS_IMPORT: &str = "com.bedcode.terminal-session.quick-actions-import";

/// 搬运结果（宿主日志与测试断言的外部可见面）
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuickActionsMigrationReport {
    /// 整体未执行搬运的原因（插件未激活 / legacy 表不存在 / 数据为空）
    pub skipped: Option<String>,
    /// 插件侧回执：`{alreadyMigrated, imported, skippedExisting}`（camelCase）
    pub plugin_report: Option<serde_json::Value>,
}

impl QuickActionsMigrationReport {
    pub fn skipped(reason: impl Into<String>) -> Self {
        Self {
            skipped: Some(reason.into()),
            plugin_report: None,
        }
    }
}

/// 迁移入口（setup 阶段调用一次；失败只记日志，绝不阻断启动）
pub fn run() {
    let Some(ctx) = AppContext::try_global() else {
        tracing::warn!("quick actions migration skipped (AppContext not ready)");
        return;
    };
    let host_ctx = ctx.plugin_host().wasm_host_ctx().clone();
    let db = ctx.db().clone();
    match tauri::async_runtime::block_on(async move {
        let db_guard = db.lock().await;
        migrate(&host_ctx, &db_guard).await
    }) {
        Ok(report) => {
            if let Some(reason) = &report.skipped {
                tracing::info!(plugin_id = "com.bedcode.terminal-session", "快捷指令搬运跳过: {reason}");
            } else {
                tracing::info!(
                    plugin_id = "com.bedcode.terminal-session",
                    plugin_report = ?report.plugin_report,
                    "legacy 主库快捷指令已推入 session 插件私有库"
                );
            }
        }
        Err(e) => tracing::warn!(
            error = %e,
            "快捷指令搬运整体失败（不阻断启动；下次启动重试）"
        ),
    }
}

/// 搬运主体（host_ctx + legacy 主库注入，便于无头/闭环测试）
pub async fn migrate(host_ctx: &WasmHostContext, db: &Database) -> Result<QuickActionsMigrationReport> {
    if !session_active(host_ctx) {
        return Ok(QuickActionsMigrationReport::skipped(
            "session 插件未激活（互调面未登记）；legacy 数据留在主库，下次启动重试",
        ));
    }

    // 读 legacy 主库（表不存在 → 已清理/全新安装，无历史可搬）
    let Some(rows) = db.list_legacy_quick_action_rows()? else {
        return Ok(QuickActionsMigrationReport::skipped(
            "legacy quick_actions 表不存在（契约已退役或从未有历史数据）",
        ));
    };

    // 即使为空也调插件 api（插件侧落 marker，后续启动整体跳过；空表无成本）
    let payload = serde_json::to_value(&rows).map_err(AppError::Serialization)?;
    let reply = call_api(host_ctx, API_QUICK_ACTIONS_IMPORT, payload)?;
    Ok(QuickActionsMigrationReport {
        skipped: None,
        plugin_report: Some(reply),
    })
}

/// 插件互调失败时统一包装（供测试断言与日志）
pub fn map_api_error(api: &str, e: &AppError) -> AppError {
    AppError::Plugin(format!("quick actions migration api '{api}' failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::LegacyQuickActionRow;

    /// legacy 行 → 插件 api 入参 JSON 的形状锁定（camelCase + created_at 原样）
    #[test]
    fn legacy_row_serializes_camel_case_for_plugin() {
        let rows = vec![LegacyQuickActionRow {
            id: "qa-1".into(),
            name: "部署".into(),
            content: "pnpm deploy".into(),
            icon: Some("rocket".into()),
            color: None,
            category: Some("dev".into()),
            sort_order: 2,
            created_at: "2026-09-20T00:00:00Z".into(),
        }];
        let v = serde_json::to_value(&rows).expect("serialize");
        assert_eq!(
            v,
            serde_json::json!([{
                "id": "qa-1",
                "name": "部署",
                "content": "pnpm deploy",
                "icon": "rocket",
                "color": null,
                "category": "dev",
                "sortOrder": 2,
                "createdAt": "2026-09-20T00:00:00Z"
            }]),
            "与插件 QuickActionRow 反序列化形状逐字一致（颜色 null 显式）"
        );
    }

    /// api 常量与插件 manifest 声明一致（防漂移的第一道闸，闭环测试兜底真实调用）
    #[test]
    fn import_api_matches_plugin_manifest() {
        let manifest_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../plugins/terminal-session/plugin.json");
        let raw = std::fs::read_to_string(&manifest_path).expect("session plugin.json 可读");
        let manifest: serde_json::Value = serde_json::from_str(&raw).expect("manifest JSON");
        assert!(
            manifest["api"]
                .as_array()
                .expect("api 数组")
                .iter()
                .any(|v| v.as_str() == Some(API_QUICK_ACTIONS_IMPORT)),
            "宿主 handoff api 常量必须与 session 插件 manifest.api 一致"
        );
    }
}
