//! 一次性数据搬运模块（core-legacy）
//!
//! 宿主侧 legacy 表 / 私有库 id 路径 → 插件私有库的一次性迁移：
//! 各迁移幂等（账本已落即跳过）、失败不阻断启动，跑在 `PluginHost::new` 之后
//! （插件已激活、互调面已登记、目标库已建表）。
//!
//! - [`auth_records_migration`]：认证记录（pairings / connection_history）下沉
//!   认证中心插件私有库（2026-09-22 用户裁定；宿主侧 handoff，插件 marker 幂等）
//! - [`quick_actions_migration`]：快捷指令 legacy 主库 → session 插件私有库（票 02）
//! - [`session_db_migration`]：终端会话中心私有库 id 路径迁移（票 07 B2：
//!   com.bedcode.session → com.bedcode.terminal-session）
//! - [`task_data_migration`]：旧 auto-task 私有库任务数据一次性搬运（票 17）

pub mod auth_records_migration;
pub mod quick_actions_migration;
pub mod session_db_migration;
pub mod task_data_migration;