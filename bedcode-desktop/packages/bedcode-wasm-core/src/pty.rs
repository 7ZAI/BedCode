//! PTY 引擎面垫片（wasm-core 纯净性收口票 02）
//!
//! PTY 引擎本体（`pty_process` / `pty_reader` / `pty_ring` / `output_sink` /
//! `lifecycle` + `PtySessionStatus` 词汇）已迁出至独立 crate `bedcode-pty-engine`
//! （零 wasm 依赖、零宿主依赖的可复用引擎，任何宿主可直接 path 依赖）。
//!
//! **WSL 发行版列举（`wsl-distros`）留在本 crate**（E3 判 ①）——它是
//! `host-platform` 原语的引擎面（平台事实，非 pty 能力；被 `host_api/platform.rs`
//! 消费），与 PTY 引擎正交，随引擎迁出会让 platform 域反向依赖引擎 crate
//! （wasm-core → pty-engine → ? 环）。故 `wsl` 归属 `crate::system::wsl`，
//! 这里只做兼容重导出保持 `crate::pty::{list_distributions, WslDistro}` 路径。
//!
//! 本文件是纯 `pub use` 垫片（反双份锁：内容零实现，只转发）。

/// PTY 引擎（真源 bedcode-pty-engine，经 lib `lib.rs` 垫片路径不变）
pub use bedcode_pty_engine::{lifecycle, output_sink, pty_process, pty_reader, pty_ring};

pub use bedcode_pty_engine::{
    PtyEngineConfig, PtyOutputSink, PtyRing, PtyRingFetch, PtyRingSink, PtySession,
    PtySessionStatus, PtyTerminated, PtyTerminationGate,
};

// WSL 发行版列举（真源 crate::system::wsl，E3 留宿主侧 platform 引擎面）
pub use crate::system::wsl::{list_distributions, WslDistro};