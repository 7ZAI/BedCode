//! 共享目录注册表 —— 纯函数与存取编排**已迁双端共享核**
//! （`bedcode-file-transfer-core::roots`，ADR 0044），本文件保留桌面端宿主 I/O 包装。
//!
//! 桌面形态差异全部落在适配器（`adapters.rs`）：持久化走 `host-plugin-database` 独立表
//! `shared_roots`（差异面①），推送载荷字段名 `path`（差异面③）。
//!
//! 内置条目 `local-downloads` 不进注册表（引擎自行注入，UI 只读展示）。

use crate::adapters::DesktopPorts;
use bedcode_file_transfer_core::ports::RootsStore as _;
use bedcode_file_transfer_core::roots as core_roots;
use bedcode_plugin_api::host::{HostPeer, HostPluginDatabase};

pub(crate) use bedcode_file_transfer_core::domain::SharedRoot;
pub(crate) use bedcode_file_transfer_core::roots::{remove, root_id, upsert};

/// 建表（幂等；activate 时调用一次以尽早暴露持久化故障）
pub(crate) fn ensure_table<H: HostPluginDatabase + ?Sized>(h: &H) -> anyhow::Result<()> {
    DesktopPorts(h)
        .ensure_roots_table()
        .map_err(anyhow::Error::new)
}

/// 读取全部条目（created_at 升序 = 加入顺序）
pub(crate) fn load_all<H: HostPluginDatabase + ?Sized>(h: &H) -> anyhow::Result<Vec<SharedRoot>> {
    DesktopPorts(h).load_roots().map_err(anyhow::Error::new)
}

/// 变更应用 + 全量推送 + 失败回滚：`mutate` 在快照副本上执行增删；推送成功才落库，
/// 失败恢复原库内容并上抛（引擎拒绝 = 注册表无效，如路径已不存在）。
/// 返回变更后的全量注册表。
pub(crate) fn apply_and_push<H: HostPluginDatabase + HostPeer + ?Sized>(
    h: &H,
    mutate: impl FnOnce(&mut Vec<SharedRoot>),
) -> anyhow::Result<Vec<SharedRoot>> {
    core_roots::apply_and_push(&DesktopPorts(h), mutate).map_err(anyhow::Error::new)
}
