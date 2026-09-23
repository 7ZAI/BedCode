//! Host Crypto Engine —— 宿主加密引擎
//!
//! 聚合全局加密方法大全：算法实现集中在 [`crate::utils::crypto`]，本模块提供
//! **名称寻址的统一入口**（注册表 + 白名单 + 抽象 trait），供内部过滤器与宿主原语
//! （host-crypto，票 03/04）按算法名调度，插件无需感知具体实现。
//!
//! 裁剪线约束（ADR 0022 / AGENTS §8）：本引擎只暴露**无业务语义的中性算法原语**，
//! 绝不提供「认证编排 / 协商流程 / 密钥策略」等产品规则；密钥材料一律由调用方提供，
//! 引擎不触碰宿主身份密钥（Kd / JWT keystore，仅宿主 filter 链内部使用）。

pub mod provider;
pub mod registry;

pub use registry::{resolve_aead, resolve_kdf, resolve_key_agreement, PROVIDERS};
