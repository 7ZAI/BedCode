//! PTY 引擎面（bedcode-pty-engine）
//!
//! 跨平台伪终端引擎：进程生命周期（创建 / 启动 / 终止 / 回收）、输出读取线程、
//! 输出环形缓冲（`PtyRing`，游标拉取）、终态汇聚门、WSL 发行版列举。
//!
//! **wasm-core 纯净性收口（票 02，.scratch/2026-10-06-wasm-core-purity/spec.md）**：
//! 本 crate 从 `bedcode-wasm-core` 的 `pty/` 迁移而来，是「引擎面出核心」的第一步。
//! 依赖方向：**只向下**依赖 `bedcode-server-base`（错误 / 配置快照）与第三方
//! （portable-pty / tokio），**零 wasm 依赖、零宿主依赖**——任何 Tauri 宿主、任何
//! 需要终端能力的程序都可直接 path 依赖本 crate。
//!
//! ## 零业务语义（2026-09-23 PTY 解耦票，迁移时保留）
//!
//! 引擎只接受调用方**算好的 argv**（`portable_pty::CommandBuilder`）——不做 shell
//! 包装 / WSL 路径转换 / 业务环境注入。shell 包装与业务环境注入在消费侧
//! （业务会话 = 插件 `com.bedcode.terminal-session` 的 `launch.rs`；插件私有 PTY =
//! 插件自己）。业务输出汇实现亦不在本 crate（调用方自备 `PtyOutputSink`）。
//!
//! ## 引擎级配置（E1/D5：AppConfig 读点参数化）
//!
//! 原实现读 `wasm_core::system::config::AppConfig::global()` 两处（lifecycle 广播
//! 容量 / 读缓冲大小）。迁移后引擎**不感知宿主配置**——这两值改由调用方在
//! [`PtySession::with_command`] / [`PtySession::with_private_command`] 时经
//! [`PtyEngineConfig`] 传入，宿主侧从自己的配置快照取值。

pub mod lifecycle;
pub mod output_sink;
pub mod pty_process;
pub mod pty_reader;
pub mod pty_ring;
pub mod pty_status;
mod runtime;

pub use lifecycle::{PtyTerminated, PtyTerminationGate};
pub use output_sink::PtyOutputSink;
pub use pty_process::PtySession;
pub use pty_ring::{PtyRing, PtyRingFetch, PtyRingSink};
pub use pty_status::PtySessionStatus;

/// PTY 引擎消费的宿主配置快照（E1/D5 参数化：原 `AppConfig::global()` 的两处读点）
///
/// 引擎不感知宿主配置来源（文件 / 环境 / 注入皆可），只收这份与 PTY 行为相关的
/// 快照。`Default` 提供与旧 `AppConfig` 默认值一致的数值（16 / 4096），以便
/// 无配置宿主与测试零成本使用。
#[derive(Debug, Clone, Copy)]
pub struct PtyEngineConfig {
    /// 生命周期事件广播容量（原 `channels.lifecycle_capacity`，默认 16）
    pub lifecycle_capacity: usize,
    /// PTY 输出读缓冲大小（原 `terminal.read_buffer_size`，默认 4096）
    pub read_buffer_size: usize,
}

impl Default for PtyEngineConfig {
    fn default() -> Self {
        Self {
            lifecycle_capacity: 16,
            read_buffer_size: 4096,
        }
    }
}