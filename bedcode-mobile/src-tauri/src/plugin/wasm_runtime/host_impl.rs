//! WASM 宿主能力实现层（host 函数，从 wasm_runtime.rs 拆分）
//!
//! 各功能域与 WIT import 接口一一对应；共享辅助在 support.rs。
//! 由 wasm_runtime/component.rs 的 Host trait impl 直接调用（值传递逻辑层）。
//! core 形态的 func_wrap 胶水（Caller + (ptr,len) 内存搬运）已随 09 清理删除。

pub(super) mod bus;
pub(super) mod config;
pub(super) mod db;
pub(super) mod event;
pub(super) mod filesrv;
pub(super) mod fs;
pub(super) mod http;
pub(super) mod notify;
pub(super) mod storage;
pub(super) mod support;
pub(super) mod terminal;

// 域内函数为 pub(crate)，显式 re-export 到 host_impl 层（component.rs 经
// `super::host_impl::xxx` 调用，不带子模块路径）
pub(crate) use bus::*;
pub(crate) use config::*;
pub(crate) use db::*;
pub(crate) use event::*;
pub(crate) use filesrv::*;
pub(crate) use fs::*;
pub(crate) use http::*;
pub(crate) use notify::*;
pub(crate) use storage::*;
pub(crate) use terminal::*;
