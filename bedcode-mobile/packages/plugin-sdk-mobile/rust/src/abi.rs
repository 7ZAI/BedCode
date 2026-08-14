//! WASM ABI 契约 — 组件模型为单一事实来源（迁移 ticket 04 / 09 清理完成）
//!
//! 组件形态下，宿主导入 / 插件导出契约全部定义在 `wit/bedcode.wit`，
//! 由 wit-bindgen 编译期校验 —— 插件侧不再需要名称常量与签名表。
//! 本模块仅保留 [`ABI_VERSION`]：插件经 WIT `abi.version()` 导出向宿主
//! 协商的版本号。core 形态遗留（自研导出/导入名常量、签名表、内存搬运常量）
//! 已随 09 清理与宿主 core 路径一并删除。

/// 当前 ABI 版本
///
/// - v1–v5：自研 ABI 演进史（组件迁移后仅作版本号序列保留）
/// - v6：组件形态（Component Model）首个版本；语义与旧 v6 一致（批量传输
///   批准协议后定稿），插件 `abi.version()` 与宿主加载校验均以本常量对齐
pub const ABI_VERSION: u32 = 6;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_abi_version_is_contract() {
        // 宿主加载时与组件 abi.version() 导出比对，漂移导致拒绝加载（高 ABI 拒绝测试依赖）
        assert_eq!(ABI_VERSION, 6);
    }
}
