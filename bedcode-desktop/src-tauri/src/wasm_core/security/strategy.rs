//! 授权策略求值（授权策略增强 · 票 03）
//!
//! spec §6.1 的判定管线里「与资源无关」的那一层：遇到授权记录**未覆盖**的目标时，
//! 要不要问用户。只有这一处实现——文件与网络各自只提供两件资源相关的小事：
//! **目标归一化**与**记录匹配**（fs：`fs_auth::canonicalize_path` + 路径前缀 / 操作集
//! 覆盖；network：`network_auth::normalize_target` + `record_covers`）。
//!
//! ## 判定顺序（spec §6.1，资源侧按此顺序调用本模块）
//!
//! ```text
//! 0. manifest 声明门（权限位）      —— 资源侧 / 框架
//! 1. 硬拒绝记录命中                 —— 资源侧（deny 优先于一切放行路径）
//! 2. 第一方免询问目录（fs 专属）     —— 资源侧（优先级高于策略档位，spec §7）
//! 3. 策略层                        ← 本模块
//! 4. 授权记录命中                   —— 资源侧
//! 5. 询问用户                       —— 资源侧
//! ```
//!
//! **顺序为什么必须共用**：两处各写一遍必然漂移，而漂移的形态是安全语义级的——
//! 「总是询问在文件侧跳过记录、在网络侧却仍读记录」这种差异从代码上看两边都自洽，
//! 只有把顺序与档位映射收在一处才防得住。
//!
//! ## 归属（ADR 0022 §5.1.3）
//!
//! 策略是**安全闸门**（「未覆盖目标要不要问」），不是业务默认值：默认档 = fail-safe
//! 方向，档位不得由 manifest 声明（声明即加载期拒绝，见 `manager::validation`）。

use super::auth_policy::{AuthPolicyStore, AuthResource, AuthStrategy};

/// 策略层给出的下一步（spec §4.1 三档 → 动作）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrategyStep {
    /// 总是询问：**跳过全部 allow 记录**（含旧版回退），每次判定都问
    ///
    /// 跳过的**只是** allow 记录：deny 记录在更靠前的第 1 步就被拦下了（spec §6.1）
    /// ——「总是询问」不等于「忽略用户已经说过的拒绝」，否则同一条硬拒绝会在每次
    /// 访问时重新弹窗，等于把 deny 记录作废。
    Ask,
    /// 默认：读 allow 记录，命中即放行，未命中才问
    ConsultRecords,
    /// 始终允许：免询问直接放行，并以 `source='always_allow'` 落账
    ///
    /// 落账是档位的义务而非可选（spec §4.3）：不留痕的管理界面在最高风险档上
    /// 会出现不可见空洞。fs 侧接线见 `fs_auth::land_auto_allow`。
    AutoAllow,
}

impl StrategyStep {
    /// 档位 → 动作（**唯一**映射点）
    pub const fn of(strategy: AuthStrategy) -> Self {
        match strategy {
            AuthStrategy::AlwaysAsk => Self::Ask,
            AuthStrategy::Default => Self::ConsultRecords,
            AuthStrategy::AlwaysAllow => Self::AutoAllow,
        }
    }

    /// 该步是否「用授权记录」
    ///
    /// 一个判断管两件互为镜像的事：判定时读不读 allow 记录、询问层给不给「记住」。
    /// 两者必须同向——「总是询问」档既然跳过记录，就不能再让弹窗落一条以后不会被
    /// 读到的记录（spec §6.3：那正是该档不出现「记住」按钮的理由）。
    pub const fn uses_records(self) -> bool {
        matches!(self, Self::ConsultRecords)
    }

    /// 日志用的层名（排障要能一眼看出这次判定走的是哪一支）
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ask => "always-ask",
            Self::ConsultRecords => "default",
            Self::AutoAllow => "always-allow",
        }
    }
}

/// 读某应用在某资源上的档位并求值（**判定时实时读取**，不缓存、不做激活期快照）
///
/// 缓存会让「已改成总是询问」却仍有缓存判定在放行 / 跳过记录，策略语义就是骗人的
/// （spec §8.1）。`resource` 即分派轴：fs 传 [`AuthResource::Fs`]、network 传
/// [`AuthResource::Network`]，两者共用同一张策略表与同一套动作映射。
pub async fn evaluate(store: &AuthPolicyStore, plugin_id: &str, resource: AuthResource) -> crate::Result<StrategyStep> {
    Ok(StrategyStep::of(store.strategy(plugin_id, resource).await?))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 三档映射逐条锁死：`always_ask` 必须**不**用记录（跳过），其余两档才碰记录。
    ///
    /// 变异判据：把 `AlwaysAsk` 映射到 `ConsultRecords`（= 退回「仍然读记录」），
    /// 本条与 fs_auth 侧的行为用例一起转红。
    #[test]
    fn strategy_step_maps_every_tier() {
        assert_eq!(StrategyStep::of(AuthStrategy::AlwaysAsk), StrategyStep::Ask);
        assert_eq!(StrategyStep::of(AuthStrategy::Default), StrategyStep::ConsultRecords);
        assert_eq!(StrategyStep::of(AuthStrategy::AlwaysAllow), StrategyStep::AutoAllow);

        assert!(!StrategyStep::Ask.uses_records(), "总是询问：不读任何 allow 记录");
        assert!(StrategyStep::ConsultRecords.uses_records());
        assert!(
            !StrategyStep::AutoAllow.uses_records(),
            "始终允许：不读记录（未覆盖目标直接放行并落账）"
        );
    }

    /// 层名是排障口径，三支必须互不相同（同一名字无法回答「走的哪一支」）
    #[test]
    fn strategy_step_names_are_distinct() {
        let names = [
            StrategyStep::Ask.as_str(),
            StrategyStep::ConsultRecords.as_str(),
            StrategyStep::AutoAllow.as_str(),
        ];
        for (i, name) in names.iter().enumerate() {
            assert!(!name.is_empty());
            assert!(
                !names[i + 1..].contains(name),
                "层名不得重复（否则日志分不清档位）: {name}"
            );
        }
    }
}
