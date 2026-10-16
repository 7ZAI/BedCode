//! 宿主能力模块描述符「零产品名词」锁（ADR 0035 D4 的桌面扫描点）
//!
//! ## 为什么扫描点在宿主而不是 wasm-core
//!
//! `inventory` 收集按**测试二进制**隔离：`ModuleRegistry::collected()` 只含本
//! 二进制链接到的 `ModuleEntry`。wasm-core 的 lib 测试二进制经 dev-deps 只看到
//! 三个 server 能力域的自报；桌面宿主 adapter（`src/plugin/*.rs`）自报的十几个
//! 模块只有本 crate 的测试二进制能收集到。判据与词表真源在
//! `bedcode_host_kit::ModuleRegistry::product_noun_violations`（词段匹配实现与契约例
//! 随其住在 `packages/bedcode-host-kit/src/registry.rs`），本文件只是桌面侧的全量
//! 扫描点——**不要把判据复制回宿主**，否则两端各自演化出第二套词表。
//!
//! ## 判据
//!
//! 模块名 / 接口路径 / 权限位三类机制属性不得含产品名词词段（AGENTS §5.1
//! B1/B5——描述符一旦带产品语义，通用注册表就退化成业务容器）。桌面宿主当前
//! 无登记例外；新增例外必须进 [`EXPECTED_VIOLATIONS`] 并在交付说明里写明理由。
//!
//! 变异自检：不引用宿主 lib（无 `use bedcode_desktop_lib as _;`）⇒ 收集集 0 ⇒
//! 防空转断言红（实测，原位带独立测试二进制的链接语义）；判据自身的词段契约例
//! （复合名词命中 / 单词内子串不命中）随真源在 host-kit `registry.rs` 单测。
use bedcode_host_kit::ModuleRegistry;

// 强制引用：`inventory` 自报静态住在宿主编译单元的 rlib 里，测试二进制若不引用
// 宿主 lib，链接器会按需抽取目标文件而不执行那些静态（实测：不引用则收集集为 0）。
// pty_wiring.rs 靠 `support::setup_wasm_runtime` 实引；本文件只需引用 lib 本身。
use bedcode_desktop_lib as _;

/// 桌面侧登记例外 `(模块名, 命中词)`；当前为空
const EXPECTED_VIOLATIONS: &[(&str, &str)] = &[];

/// 桌面宿主全部自报模块的描述符都必须干净（词段判据，见 host-kit 单一真源）
#[test]
fn host_module_descriptors_carry_no_product_nouns() {
    let registry = ModuleRegistry::collected();
    // 防锁空转：桌面宿主 adapter 自报十多个模块，收集面过小说明自报/链接面漏了
    assert!(
        registry.len() >= 10,
        "桌面宿主能力模块收集面过小（{} 个）——锁可能空转（ModuleEntry 未链接？）",
        registry.len()
    );
    let hits: Vec<(&str, &str)> = registry
        .product_noun_violations()
        .iter()
        .map(|violation| (violation.module, violation.noun))
        .collect();
    assert_eq!(
        hits.as_slice(),
        EXPECTED_VIOLATIONS,
        "宿主描述符含未登记的产品名词（AGENTS §5.1 B1/B5）：{hits:?}"
    );
}
