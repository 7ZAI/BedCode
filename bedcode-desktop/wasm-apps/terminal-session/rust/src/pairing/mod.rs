//! pairing 域（票 04）— 配对码 / QR token / JWT 策略 / 密钥访问
//!
//! 语义自认证中心插件整体搬入；宿主侧已无配对语义实现（宿主 `utils/auth/` 只剩
//! `auth_center.rs` 桥接 + `identity.rs`，`pairing/qr_token/jwt` 模块已随 ADR 0033
//! 退役），本模块是配对语义的**唯一实现**，宿主经 `auth-grant` 零解析转发到本域。
//!
//! 模块构成：
//! - [`code`]：6 位配对码策略（TTL 60s、`>` 过期语义、序列化剩余时间）
//! - [`qr`]：QR token 策略（128-bit hex、`>=` 过期语义、一次性消费）
//! - [`jwt`]：JWT HS256 签发/校验（RFC 7515 向量 + jsonwebtoken 语义复刻）
//! - [`keys`]：密钥经 host-auth secret-store 获取（首启随机生成 + 持久化）
//!
//! **唯一真源（票 06 contract 完成 + ADR 0033）**：pairing 语义只此一份；宿主
//! `src-tauri/src/utils/auth/` 不再保留配对实现（降级轨已随入场密码学下沉退役，
//! 宿主只经 `auth_center.rs::invoke_auth_method` 转发 `auth-grant`，零解析零解释）。
//! 插件未激活时宿主 fail-closed 拒绝（`deny_kind=no_center`），不存在需要同步的
//! 第二份实现。

pub mod code;
pub mod jwt;
pub mod keys;
pub mod qr;
