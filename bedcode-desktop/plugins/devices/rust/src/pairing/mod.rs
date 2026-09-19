//! pairing 模块（票 07）— 配对码 / QR token / JWT 签发校验策略
//!
//! 语义层从宿主 `src-tauri/src/utils/auth/{pairing,qr_token,jwt}.rs` 平移，
//! 宿主保留密码学引擎与密钥托管（spec §3）；行为等价约束见各子模块
//! 头部注释与对照测试（同一输入同输出）。
//!
//! 模块构成：
//! - [`code`]：6 位配对码策略（TTL 60s、`>` 过期语义、序列化剩余时间）
//! - [`qr`]：QR token 策略（128-bit hex、`>=` 过期语义、一次性消费）
//! - [`jwt`]：JWT HS256 签发/校验（RFC 7515 向量 + jsonwebtoken 语义复刻）
//! - [`keys`]：密钥经 host-auth secret-store 获取（首启随机生成 + 持久化）

pub mod code;
pub mod jwt;
pub mod keys;
pub mod qr;
