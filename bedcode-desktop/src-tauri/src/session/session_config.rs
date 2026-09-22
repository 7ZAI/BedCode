//! Session Config Manager（v24 壳：装配链占位）
//!
//! 2026-09-22 认证记录下沉 + `session_configs` 表退役后，本管理器不再提供任何
//! 数据库访问——`get_config` / `list_configs` 读表方法及全部测试随表删除
//! （config 读取面 `host_impl/session.rs::session_config_*` 一并退役，插件私有库
//! 是会话配置唯一真源）。
//!
//! 类型保留仅因装配链引用（`WasmHostContext.config_manager` / `AppContext` /
//! `PluginHost::new` 参数）：`new` 构造与 `db()` 句柄访问符保留，供 e2e 播种
//! 与注入；**不含任何业务方法**。待装配链随 config 域彻底清空后可整体删除。

use crate::db::Database;
use std::sync::Arc;
use tokio::sync::Mutex;

/// 会话配置管理器（v24 起为装配占位壳，无业务方法）
pub struct SessionConfigManager {
    db: Arc<Mutex<Database>>,
}

impl SessionConfigManager {
    /// 创建配置管理器（保留：装配链注入点）
    pub fn new(db: Arc<Mutex<Database>>) -> Self {
        Self { db }
    }

    /// 所持引擎库句柄（crate 内可见；仅测试播种 legacy 行用）
    pub(crate) fn db(&self) -> Arc<Mutex<Database>> {
        Arc::clone(&self.db)
    }
}