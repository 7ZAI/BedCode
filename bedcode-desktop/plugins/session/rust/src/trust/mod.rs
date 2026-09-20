//! trust 模块（票 05 自认证中心搬入会话中心）— 设备信任列表统一视图
//!
//! 语义：把「内核 `pairings`（已配对设备）+ 宿主 peer trust_store（可信对端）」
//! 两套宿主事实映射为单一统一视图，列表 / 撤销行为与宿主实现等价（对照测试）。
//!
//! ## 统一视图 = pairing（内核 `pairings` 表） + peer（可信对端）
//!
//! | 宿主概念 | 宿主实现（行为基准） | 插件映射 |
//! | --- | --- | --- |
//! | 已配对设备 | `pairings` 表（`get_pairings`：仅 `is_active=1`，`ORDER BY paired_at DESC`；`remove_pairing`：软删 `is_active=0` + 删连接历史） | host-auth `trusted-devices-list` 读原始记录，[`source`] 适配 + [`ops`] 过滤排序 |
//! | 可信对端 | peer trust_store（`list_trusted_peers` → `TrustedPeerDto`；`revoke_trusted_peer` → bool） | 经 host-peer `list-trusted` / `revoke-trusted` 原语原样映射 |
//!
//! 行为等价约束（对照测试锚点，逐条对应宿主实现）：
//! - **列表**：pairing 段只含活跃记录，按 `paired_at` DESC（= 宿主
//!   `get_pairings` 的 `WHERE is_active=1 ORDER BY paired_at DESC`）；
//!   peer 段透传宿主 `list_trusted_peers` 的条目序。两段合并为统一数组，
//!   每条带 `kind` 判别（`pairing` | `peer`）。
//! - **撤销**：pairing 经 host-auth `trusted-device-revoke`（宿主 `remove_pairing`
//!   语义：软删 + 删连接历史），撤销后立即从列表消失；未命中 id 幂等
//!   `removed=false` 不报错。peer 经 host-peer `revoke-trusted`。
//! - **持久化**：真源是内核表（重启一致），插件不持账本——票 05 前的
//!   host-storage `trust.pairings` 镜像已删除（消灭宿主/插件两套账本的口径漂移）。
//!
//! ## 降级（peer 侧不可用时）
//!
//! 无头/peer-net 未启动时 host-peer `list-trusted` 报错（宿主原语显性失败），
//! 本模块不静默吞掉：`list` 仍返回 pairing 段 + `peerError` 字段透出错误；
//! `revoke` 对 peer 目标直接上抛错误（不做「看似成功」的假撤销）。
//!
//! ## 命令面（lib.rs）
//!
//! - `session.trust.list` → `{ devices: TrustedDeviceDto[], peerError: string|null }`
//! - `session.trust.revoke` `{id}` → `{ removed, kind }`

pub mod model;
pub mod ops;
pub mod source;

pub use ops::{list_via_host, revoke_via_host};
