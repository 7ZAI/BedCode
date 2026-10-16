//! 对等网络（peer-net）宿主模块：装配 / 端口 / 平台缝 / 委托入口 / 迁移。
//!
//! 引擎控制面真源在共享能力域 `packages/bedcode-server-peer-net`（纯引擎形态，
//! 双端同引，ADR 0048）；本模块只保留宿主侧适配面：
//!
//! - [`net`]：装配（`peer_ctx` 是宿主唯一认识 `AppHandle` 的引擎装配点）+
//!   节点生命周期 + 引擎入口委托；
//! - [`ports`]：宿主端口实现（插件总线 / 前端事件 / 路径 / 系统信息 / mDNS）
//!   + 平台资源预热缓存；
//! - [`seam`]：平台缝（Android SAF 共享访问 / MediaStore 落点提升 / 内置共享条目）；
//! - [`transfer`] / [`receive`] / [`remote`]：发送 / 接收 / 远端浏览拉取的委托入口；
//! - [`migration`]：旧引擎数据 → 插件存储键一次性幂等迁移（随引擎停写收尾删除）。
//!
//! 边界：宿主不持传输任务 / 设置 / 历史真源（在 file-transfer 插件共享核）；
//! 适配模块零 Tauri 命令面（防回接锁
//! `tests/retired_mobile_peer_transfer_command_face_lock.rs`），引擎控制面不得
//! 回流宿主（防回接锁 `tests/peer_engine_consolidation_lock.rs`）。

pub mod migration;
pub mod net;
pub mod ports;
pub mod receive;
pub mod remote;
pub mod seam;
pub mod transfer;
