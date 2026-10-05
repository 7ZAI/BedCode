//! 能力模块自动注册表：收集 → 排序 → 白名单双向校验 → linker 装配
//!
//! ## 为什么「自动」还要「锁」
//!
//! `inventory` 的固有风险是**能力集随链接到的 crate 漂移**：某个 crate 被误删依赖，
//! 它提供的 interface 就静默从插件 import 集里消失，而 guest 侧毫无察觉——插件
//! 编译期照常 import，运行期才在实例化时炸。
//!
//! 治理手段是**白名单锁**（不是弃用自动发现）：宿主维护一个树内白名单常量，与收集
//! 结果**双向**比对——
//!
//! - 收集到但白名单没有 ⇒ 红（新增能力强制过 review，不会悄悄进产品）
//! - 白名单有但没收集到 ⇒ 红（依赖被删/没链上时立刻定位，不等到插件实例化）
//!
//! 「强制引用行」（`use bedcode_cap_x as _;`）与白名单常量**放在同一处**，两者不漂移。
//!
//! ## 装配顺序
//!
//! 按模块名**字典序**装配（而非注册顺序）：linker 的 interface 命名空间彼此独立，
//! 顺序本无语义；固定顺序只为让重复注册的报错信息可复现。

use wasmtime::component::Linker;

use crate::module::HostModuleDesc;
use crate::state::WasmPluginState;
use crate::{HostKitError, Result};

/// 宿主能力模块注册表
pub struct ModuleRegistry {
    modules: Vec<&'static dyn crate::module::HostModule>,
}

impl ModuleRegistry {
    /// 收集全局已自报的能力模块（未过滤、未校验）
    pub fn collected() -> Self {
        let mut modules: Vec<&'static dyn crate::module::HostModule> =
            inventory::iter::<crate::module::ModuleEntry>
                .into_iter()
                .map(|entry| entry.module)
                .collect();
        modules.sort_by_key(|m| m.desc().name);
        Self { modules }
    }

    /// 已装配模块的描述符（装配顺序 = 字典序）
    pub fn descs(&self) -> Vec<HostModuleDesc> {
        self.modules.iter().map(|m| m.desc()).collect()
    }

    /// 已装配模块数
    pub fn len(&self) -> usize {
        self.modules.len()
    }

    /// 是否一个模块都没有（宿主刚起来、能力 crate 全没链上的诊断用）
    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }

    /// 白名单双向校验：收集结果必须与白名单**完全相等**
    ///
    /// 差异即 [`HostKitError::WhitelistMismatch`]——**显性失败**，不静默忽略：
    /// 「多出来的」意味着有 crate 被加进来却没过 review；「少了的」意味着依赖丢失。
    pub fn verify_whitelist(&self, whitelist: &[&str]) -> Result<()> {
        let mut listed: Vec<&str> = self.modules.iter().map(|m| m.desc().name).collect();
        listed.sort_unstable();
        let mut expected: Vec<&str> = whitelist.to_vec();
        expected.sort_unstable();

        if listed == expected {
            return Ok(());
        }
        let unlisted: Vec<String> = listed
            .iter()
            .filter(|n| !expected.contains(n))
            .map(|n| (*n).to_string())
            .collect();
        let missing: Vec<String> = expected
            .iter()
            .filter(|n| !listed.contains(n))
            .map(|n| (*n).to_string())
            .collect();
        Err(HostKitError::WhitelistMismatch { unlisted, missing })
    }

    /// 逐模块装配进插件 linker（顺序 = 字典序）
    ///
    /// 任一模块装配失败即整体失败并点名该模块（fail-visible：不允许「装了一半」的
    /// linker 流到实例化，那会让 guest 在更晚、更难定位的地方炸）。
    pub fn install_all(&self, linker: &mut Linker<WasmPluginState>) -> Result<()> {
        for module in &self.modules {
            let desc = module.desc();
            module
                .register(linker)
                .map_err(|source| HostKitError::Register {
                    module: desc.name,
                    source,
                })?;
        }
        Ok(())
    }
}
