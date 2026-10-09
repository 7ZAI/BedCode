//! File Transfer 双端共享业务核。
//!
//! ## 为什么存在
//!
//! 双端 file-transfer 是同一产品在两种宿主形态上的两个 wasm 应用，业务实现此前各持一份
//! （桌面 4.4k 行 / 移动 3.4k 行），可共享面约 70%，改一处漏一处即行为漂移。本 crate 把
//! **可共享面**收敛为一份，把**双端差异面**全部推到端口 trait（`ports`），双端只保留
//! 「适配器（端口实现）+ `WasmPlugin` 外壳」。
//!
//! 形态与 `bedcode-host-api-core`（ADR 0040 第二步）同族：双端共享的实现层，零 WIT、
//! 零任一端 SDK、零平台依赖。
//!
//! ## 依赖纪律（改动前先读）
//!
//! - 依赖只有 `serde` / `serde_json`。引入任何其他依赖前先问「这还是共享核吗」。
//! - **核内零宿主 SDK 引用**：与宿主交互一律经 `ports` 的 trait。反向污染（把某端专属
//!   逻辑塞进核、或在核里 `use` 某端 SDK）会被边界锁测红。
//! - **核内零产品身份字面量**：插件 id / 事件命名空间经 [`identity::PluginIdentity`]
//!   运行期注入（宿主侧语义锁 `capability_crates_no_product_ids` 的 C-4 同源判据）。
//!
//! ## 双端差异面索引（端口 ↔ 现实差异）
//!
//! | 端口 | 桌面实现 | 移动实现 |
//! | --- | --- | --- |
//! | [`ports::RootsStore`] | `host-plugin-database` 表 `shared_roots` | `host-storage` 键 `shared_roots` |
//! | [`ports::RootWireCodec`] | `{ id, name, path }` | `{ id, name, safTreeUri }` |
//! | [`ports::NodePower`] | 插件显式请求起停节点 | 宿主外壳驱动（默认实现即拒绝） |
//! | [`ports::PlatformPort::platform_pick_folders`] | 支持 | 不支持（默认实现拒绝） |
//! | [`ports::PlatformPort::platform_reveal_in_dir`] | 支持 | 不支持（默认实现拒绝） |
//! | [`ports::ConsentGate`] | 互调认证中心 + 双轨降级 | 直答宿主原语 |
//! | [`ports::PluginProfile::uses_legacy_snapshot`] | 双写期仍订阅旧快照 topic | 已整条退役 |
//!
//! ## 模块
//!
//! - [`domain`]：双端同形领域类型（wire 形状即契约，serde 属性不可随手改）
//! - [`identity`]：运行期注入的插件身份
//! - [`ports`]：端口 trait 集合（差异面唯一落点）
//! - [`settings`]：接收策略与落点设置
//! - [`roots`]：共享目录注册表
//! - [`sessions`]：endpoint memo + session 句柄表 + 设备快照
//! - [`transfer`]：传输任务台账（事件归约 + 重试/闸门/意图队列判据单点）
//!
//! 票据与逐票实施记录：`.scratch/2026-10-09-file-transfer-shared-core/`。

// ==================== 反漂移锁（测试期门禁） ====================
//
// 两条锁放在 `src/` 而非 crate 根 `tests/`：治理面按 `packages/bedcode-*` 目录约定推导，
// crate 根出现 `tests/` 目录即被宿主侧 `capability_crates_unit_tests_only.rs` 判红
// （crate 根 `tests/` = 独立测试二进制 = 只能经 `pub` API 访问的对外行为面）。
#[cfg(test)]
mod boundary_lock;
#[cfg(test)]
mod wiring_lock;

pub mod domain;
pub mod identity;
pub mod ports;
pub mod roots;
pub mod sessions;
pub mod settings;
pub mod transfer;

pub use identity::PluginIdentity;
pub use ports::{
    BusPort, ConsentGate, EventPort, KvStore, LogPort, MdnsPort, NodePower, PeerPort, PlatformPort,
    PluginProfile, PortError, PortResult, RootWireCodec, RootsStore,
};
pub use transfer::{RetryMeta, RetryRefusal, TransferEntry};
