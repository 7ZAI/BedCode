//! trust 模块（票 08 B3）— 设备信任列表统一视图
//!
//! 语义从宿主「已配对设备 + 可信对端」两套宿主实现映射为单一统一视图，
//! 列表 / 撤销行为与宿主实现等价（对照测试），数据持久化。
//!
//! ## 统一视图 = pairing（已配对设备） + peer（可信对端）映射
//!
//! | 宿主概念 | 宿主实现（行为基准） | 插件映射 |
//! | --- | --- | --- |
//! | 已配对设备 | `pairings` 表（`get_pairings`：仅 `is_active=1`，`ORDER BY paired_at DESC`；`remove_pairing`：软删 `is_active=0`） | [`PairingRecord`]（host-storage 持久化，字段对齐宿主 `Pairing`） |
//! | 可信对端 | peer trust_store（`list_trusted_peers` → `TrustedPeerDto`；`revoke_trusted_peer` → bool） | 经 host-peer `list-trusted` / `revoke-trusted` 原语原样映射 |
//!
//! 行为等价约束（对照测试锚点，逐条对应宿主实现）：
//! - **列表**：pairing 段只含 active 记录，按 `paired_at` DESC（= 宿主
//!   `get_pairings` 的 `WHERE is_active=1 ORDER BY paired_at DESC`）；
//!   peer 段透传宿主 `list_trusted_peers` 的条目序。两段合并为统一数组，
//!   每条带 `kind` 判别（`pairing` | `peer`）。
//! - **撤销**：pairing 软删（`active=false`，保留记录）= 宿主 `remove_pairing`
//!   的 `is_active=0` 语义，撤销后立即从列表消失且持久化；未命中 id 幂等
//!   返回 `removed=false` 不报错（宿主对不存在 id 的 UPDATE 影响 0 行）。
//!   peer 经 host-peer `revoke-trusted`（宿主 `revoke_trusted_peer` 语义，
//!   返回是否删除）。
//! - **持久化**：pairing 记录存 host-storage 键 `trust.pairings`（宿主
//!   `plugin_storage` 表，重启一致）；撤销软删即时写回（重启后仍不可见）。
//!
//! ## 降级（peer 侧不可用时）
//!
//! 无头/peer-net 未启动时 host-peer `list-trusted` 报错（宿主原语显性失败），
//! 本模块不静默吞掉：`list` 仍返回 pairing 段 + `peerError` 字段透出错误；
//! `revoke` 对 peer 目标直接上抛错误（不做「看似成功」的假撤销）。
//!
//! ## 命令面（lib.rs）
//!
//! - `devices.trust.list` → `{ devices: TrustedDeviceDto[], peerError: string|null }`
//! - `devices.trust.revoke` `{id}` → `{ removed, kind }`
//! - `devices.trust.add-pairing` `{deviceName, deviceFingerprint, address?}`
//!   → `{ id, pairedAt, connectCount }`（配对完成流/闭环测试的写入入口）

pub mod model;
pub mod ops;
pub mod store;

pub use ops::{add_pairing_via_host, list_via_host, revoke_via_host};
