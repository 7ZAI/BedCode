//! Mobile Plugin System
//!
//! 插件系统入口 — WASM 动态加载 + 前端插件管理

pub mod android_plugins;
pub mod approval;
pub mod commands;
pub mod downloader;
pub mod fs_auth;
pub mod loader;
pub mod manager;
pub mod message_bus;
pub mod registry;
pub mod saf_io;
pub mod saf_path;
pub mod storage;
pub mod transfer;
pub mod types;
pub mod validation;
pub mod wasm_host;
pub mod wasm_runtime;

pub use android_plugins::{asset_extractor_plugin, foreground_service_plugin};
pub use registry::builtin_manifests;
