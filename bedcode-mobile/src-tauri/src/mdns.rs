//! mDNS Service Discovery
//!
//! 局域网内 BedCode 设备的自动发现（browse 宿主薄壳；广播面已随 2026-10-10
//! 能力域集中化退役——引擎广播走共享守护 `bedcode-discovery-engine::engine`，
//! 宿主零自建广播实例）
//! 基于 mdns-sd crate 的纯 Rust 实现

pub mod discovery;
pub mod types;
