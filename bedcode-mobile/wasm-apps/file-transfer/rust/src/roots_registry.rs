//! 共享目录注册表 —— 纯函数与存取编排**已迁双端共享核**
//! （`bedcode-file-transfer-core::roots`，ADR 0044），本文件保留移动端宿主 I/O 包装。
//!
//! 移动端形态差异全部落在适配器（`adapters.rs`）：持久化走 `host-storage` 单键
//! `shared_roots`（差异面①），推送载荷字段名 `safTreeUri`（差异面③）。
//!
//! 内置条目 `local-downloads` 不进注册表（引擎自行注入，get-settings 时以只读形状合并展示）。

use crate::adapters::MobilePorts;
use bedcode_file_transfer_core::ports::RootsStore as _;
use bedcode_file_transfer_core::roots as core_roots;
use bedcode_plugin_api_mobile::host::{HostPeer, HostStorage};

pub(crate) use bedcode_file_transfer_core::domain::SharedRoot;
pub(crate) use bedcode_file_transfer_core::roots::{remove, root_id, upsert};

/// 注册表存储键（适配器经它读写；双端键名一致是**迁移约束**，改键 = 用户已配置的共享根凭空消失）
pub(crate) const ROOTS_KEY: &str = "shared_roots";

/// 读取全部条目（无值/损坏回空表）
pub(crate) fn load_all<H: HostStorage + ?Sized>(h: &H) -> anyhow::Result<Vec<SharedRoot>> {
    MobilePorts(h).load_roots().map_err(anyhow::Error::new)
}

/// 变更应用 + 全量推送 + 失败回滚：`mutate` 在快照副本上执行增删；推送成功才落存储，
/// 失败恢复原内容并上抛（引擎拒绝 = 注册表无效）。返回变更后的全量注册表。
pub(crate) fn apply_and_push<H: HostStorage + HostPeer + ?Sized>(
    h: &H,
    mutate: impl FnOnce(&mut Vec<SharedRoot>),
) -> anyhow::Result<Vec<SharedRoot>> {
    core_roots::apply_and_push(&MobilePorts(h), mutate).map_err(anyhow::Error::new)
}
