//! WASM 宿主能力实现层（host 函数，从 wasm_runtime.rs 拆分）
//!
//! 各功能域与 host_* 函数一一对应；共享辅助在 support.rs。
//! 由 wasm_runtime.rs 的 `register_host_functions` 经 `use host_impl::*` 注册。
//!
//! 注：域内函数为 `pub(super)`（对 host_impl 可见），此处显式 re-export 到
//! wasm_runtime（glob 导入不含 pub(super) 项，须逐一列出）。

pub(super) mod bus;
pub(super) mod config;
pub(super) mod db;
pub(super) mod event;
pub(super) mod filesrv;
pub(super) mod fs;
pub(super) mod http;
pub(super) mod log;
pub(super) mod notify;
pub(super) mod session;
pub(super) mod storage;
pub(super) mod support;
pub(super) mod terminal;

pub(crate) use bus::*;
pub(crate) use config::*;
pub(crate) use db::*;
pub(crate) use event::*;
pub(crate) use filesrv::*;
pub(crate) use fs::*;
pub(crate) use http::*;
pub(crate) use log::*;
pub(crate) use notify::*;
pub(crate) use session::*;
pub(crate) use storage::*;
pub(crate) use terminal::*;