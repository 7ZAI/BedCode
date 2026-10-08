//! host_api **共享实现核**（票 18 · ADR 0040 选项 C 第二步）
//!
//! 双端 wasm-core（桌面 `packages/bedcode-wasm-core` / 移动
//! `bedcode-mobile/packages/bedcode-wasm-core`）的 host_api 域中，**不依赖 WIT
//! bindgen trait、不依赖任何一端独有类型**的机制实现层抽到本 crate。每域两层：
//!
//! ```text
//! 实现层（本 crate）：      纯机制语义——权限门（权限词汇经参数传入）、纵深守卫、
//!                           属主分区、能力路由、真源接入；端口 trait 定义在此
//! 各端 adapter（wasm-core）：把本端宿主服务（SqlitePorts / host_ctx /
//!                           granted_permissions …）接到实现层的端口 trait 上
//! WIT 绑定层（各自）：      component.rs 的 bindgen trait impl → 域函数 → adapter
//! ```
//!
//! ## 边界判据（实现层的准入条件，违者应留在各端）
//!
//! - **禁止**引用 `bedcode_plugin_api` / `bedcode_plugin_api_mobile`（双端 WIT SDK）
//!   与双端 wasm-core 的宿主胶水（`crate::plugin` 等）；
//! - **只允许**：机制词汇（permission 常量经参数传入）、本 crate 定义的端口 trait、
//!   机制级依赖（serde_json / tracing）；
//! - **禁止**宿主平台（tauri）与运行时（tokio）依赖——阻塞 / 驱动异步是各端 adapter
//!   的职责（桌面 `runtime_util::block_on_async`、移动 `block_in_place` + `block_on`）。
//!
//! 上述判据由 `tests/boundary_lock.rs` 强制（票 18 §6）。
//!
//! ## 域进度（每域一步、可回退，迁移顺序见票 18 §3）
//!
//! | 域 | 状态 |
//! | --- | --- |
//! | `storage` | 已抽（批次 1） |
//! | `bus` | 已抽语义（批次 2：topic 形态机制 + 命名空间/订阅面/互调门禁 + 发布门禁链；队列与订阅簿留各端） |
//! | `database` / `events` / `http` / `fs` / `config` / `log` | 后续批次（http / fs 为「骨架 + 端口」形态，不追求逐行同构） |
//!
//! ## 与 `bedcode-host-kit` 的边界
//!
//! host-kit 是「机制锚点」最小面（实例状态 / 能力模块契约 / 注册表，wasmtime
//! **装配期**）；本 crate 是 host_api **域实现层**（原语语义，**运行期**）。同族
//! 不合并：host_api 实现层塞入会模糊 host-kit 的最小面边界（票 18 §4 选项 A 裁决）。

#![deny(missing_docs)]

pub mod bus;
pub mod gate;
pub mod storage;
