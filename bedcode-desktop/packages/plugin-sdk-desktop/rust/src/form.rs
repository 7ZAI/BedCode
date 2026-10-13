//! 产物形态常量（WIT `abi-form` 接口；`form()` 导出值）
//!
//! **只管形态，不管版本**（ADR 0051 D2）：整数 `ABI_VERSION` 与 WIT `interface abi`
//! 已退役——契约版本搬上 wire（`package bedcode:plugin@<version>`，真源见仓根
//! `contracts/bedcode-plugin-package.rs`），宿主与 guest 版本不等由 wasmtime linker
//! 按含版本的完整 import 名精确匹配直接拒绝，诊断自带包名 + 版本。
//!
//! 形态与版本**正交**：`abi-form.form` 走独立接口（v36 自 `abi` 拆出），语义是
//! 「component 形态 vs core 形态」的**加载路径判别**——迁移阶段 C 后组件是唯一形态，
//! `FORM_CORE = 0` 已随该阶段删除；保留常量 1 是为了挡住未来误引入 0 值导致加载路径回退。
//!
//! 移动端无此导出（移动 WIT 不含 `abi-form`），故移动 SDK 无对应模块。

/// 组件形态标识：`abi-form.form() == FORM_COMPONENT`
pub const FORM_COMPONENT: u32 = 1;

#[cfg(test)]
mod tests {
    use super::*;

    /// 锁定 1 防未来误引入 0 值（core 形态已在迁移阶段 C 删除）
    #[test]
    fn form_component_is_pinned_to_one() {
        assert_eq!(FORM_COMPONENT, 1);
        assert_ne!(
            FORM_COMPONENT, 0,
            "0 = core module 形态，已随迁移阶段 C 删除；出现即加载路径回退"
        );
    }
}