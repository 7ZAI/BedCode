//! WASM ABI 版本契约
//!
//! 迁移阶段 C 后插件产物统一为 Component Model 组件（WIT 契约，
//! 见 `wit/bedcode.wit`），名称常量与签名表（core 形态的 (ptr,len) ABI）
//! 已全部删除，此处仅保留组件通过 WIT `abi` 接口声明版本与形态的常量。
//!
//! # 版本演进
//!
//! 版本号语义不变（与历史 core ABI 共用同一序列）：
//! - v1: 初始版本（27 个 host functions + 11 个插件导出）
//! - v2: 新增 4 个参数绑定 SQL host functions（*_params），消灭插件侧手写转义
//! - v3: 新增提交输入行观察扩展点（host function `SESSION_INPUT_REGISTER`
//!   + 可选导出 `ON_INPUT_SUBMITTED`），见 ADR 0001
//! - v4: 新增插件状态上报扩展点（host function `MARK_PLUGIN_ERROR`），
//!   插件自检失败（如 hooks 配置失败）时上报宿主标记错误并通知前端
//! - v5: 新增通用文件服务能力（host functions `FILESRV_*` / `TRANSFER_*`
//!   + 可选导出 `ON_UPLOAD_REQUEST` 上传策略钩子），见内网文件传输插件规格
//! - v6: 新增会话创建与宿主定时器（host functions `SESSION_CREATE` /
//!   `TIMER_REGISTER`），支撑插件定时自动任务，见 ADR 0003
//! - v7: 新增会话关闭（host function `SESSION_CLOSE`），支撑插件在
//!   定时自动任务执行完后关闭其创建的会话
pub const ABI_VERSION: u32 = 7;

/// 组件形态标识：`abi.form() == FORM_COMPONENT`（WIT `abi` 接口的 form() 声明）
///
/// 组件通过 WIT `abi` 接口的 `form()` 声明形态，语义与 ABI_VERSION 解耦：
/// 不 bump ABI 大版本，仅区分加载路径（core module vs component，
/// 后者为迁移后唯一形态；FORM_CORE=0 已在阶段 C 删除）
pub const FORM_COMPONENT: u32 = 1;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_abi_version_is_v7() {
        // 版本号序列与历史 core ABI 共用：v7 = 新增 SESSION_CLOSE（ADR 0003 配套）
        assert_eq!(ABI_VERSION, 7);
    }

    #[test]
    fn test_form_component_constant() {
        // 迁移阶段 C 后唯一形态是组件；FORM_CORE=0 已删除，
        // 锁定 1 防未来误引入 0 值导致宿主加载路径回退
        assert_eq!(FORM_COMPONENT, 1);
        assert_ne!(FORM_COMPONENT, 0);
    }
}
