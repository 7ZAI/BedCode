//! pairing 域（票 04）— 配对码 / QR token / JWT 策略 / 密钥访问
//!
//! 语义自认证中心插件整体搬入，模块内容与宿主 `src-tauri/src/utils/auth/{pairing,qr_token,jwt}.rs`
//! 行为等价（对照测试见 `tests_pairing_equivalence`，宿主桥接改指见
//! `utils/auth/auth_center.rs`）。
//!
//! 模块构成：
//! - [`code`]：6 位配对码策略（TTL 60s、`>` 过期语义、序列化剩余时间）
//! - [`qr`]：QR token 策略（128-bit hex、`>=` 过期语义、一次性消费）
//! - [`jwt`]：JWT HS256 签发/校验（RFC 7515 向量 + jsonwebtoken 语义复刻）
//! - [`keys`]：密钥经 host-auth secret-store 获取（首启随机生成 + 持久化）
//!
//! **唯一真源（票 06 contract 完成）**：独立认证中心插件已整体退役，pairing 语义只此
//! 一份；expand/contract 期的双副本窗口关闭。宿主 `src-tauri/src/utils/auth/` 保留为
//! **降级轨**（插件未激活时走迁移前行为，D7 无单点），故两侧语义仍须同步演进，
//! 否则降级轨与插件轨分叉——对照测试 `tests_pairing_equivalence` 是这条约束的门禁。

pub mod code;
pub mod jwt;
pub mod keys;
pub mod qr;
