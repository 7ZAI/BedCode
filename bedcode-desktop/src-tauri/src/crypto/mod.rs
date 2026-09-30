//! Host Crypto Engine —— 宿主加密引擎（server-lib-split：真源已随
//! `bedcode-crypto-engine` crate 拆出）
//!
//! 本文件保留 `crate::crypto::*` 模块路径与全部公开名字，宿主其余代码
//! （server 面 / wasm_core host_api/crypto.rs）经 `crate::crypto::provider` /
//! `crate::crypto::registry` 引用不受影响。
//!
//! 裁剪线约束（ADR 0022 / AGENTS §8）：本引擎只暴露**无业务语义的中性算法原语**，
//! 绝不提供「认证编排 / 协商流程 / 密钥策略」等产品规则；密钥材料一律由调用方提供，
//! 引擎不触碰宿主身份密钥（Kd / JWT keystore，仅宿主 filter 链内部使用）。

pub use bedcode_crypto_engine::{provider, registry};

pub use bedcode_crypto_engine::registry::{resolve_aead, resolve_kdf, resolve_key_agreement, PROVIDERS};
