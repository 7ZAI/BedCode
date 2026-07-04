//! mDNS Service Discovery & Advertisement
//!
//! 局域网内 BedCode 设备的自动发现和广播
//! 基于 mdns-sd crate 的纯 Rust 实现

pub mod types;
pub mod discovery;
pub mod advertiser;
