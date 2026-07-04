//! mDNS Service Advertisement
//!
//! 桌面端 mDNS 广播模块 - 将桌面端服务注册到局域网，供移动端发现
//! 桌面端只作为被发现者，不需要 discovery 功能

pub mod advertiser;
pub mod types;
