//! 宿主能力模块描述符「零产品名词」锁（ADR 0035 D4 的移动扫描点）
//!
//! ## 为什么扫描点在宿主而不是 wasm-core
//!
//! `inventory` 收集按**测试二进制**隔离：`ModuleRegistry::collected()` 只含本
//! 二进制链接到的 `ModuleEntry`。wasm-core 的 lib 测试二进制看不到移动宿主
//! adapter（`src/plugin/*.rs`）自报的模块，那些模块只有本 crate 的测试二进制能
//! 收集到。判据与词表真源在 `bedcode_host_kit::ModuleRegistry::product_noun_violations`
//! （词段匹配实现与契约例随其住在 `packages/bedcode-host-kit/src/registry.rs`），
//! 本文件只是移动侧的全量扫描点。
//!
//! ## 登记例外（移动独有端扩展）
//!
//! `mobile-terminal-stream`（接口 `host-terminal-stream`、权限 `terminal:output`）
//! 是 ADR 0047 / ADR 0052 D4 保留的**端独有窄转发**：插件 → 前端页面的裸字节
//! 通道，桌面无等价物（桌面输出走插件自身 WS 协议）；其名字与权限位里的
//! `terminal` 是端事实而不是共享注册表业务化，故显式登记。新增例外一律在此表
//! 登记并写明理由（判据只对「未登记的命中」报红）。
//!
//! 变异自检：移除例外登记 ⇒ 红并点名 `[("mobile-terminal-stream", "terminal")]`
//! （实测）——证明收集集非空、例外是承载判据的，而不是死登记。
use bedcode_host_kit::ModuleRegistry;

// 强制引用：`inventory` 自报静态住在移动宿主编译单元的 rlib 里，测试二进制若不引用
// 宿主 lib，链接器会按需抽取目标文件而不执行那些静态（桌面侧实测：不引用则收集集为 0）。
use bedcode_mobile_lib as _;

/// 移动侧登记例外 `(模块名, 命中词)`
const EXPECTED_VIOLATIONS: &[(&str, &str)] = &[("mobile-terminal-stream", "terminal")];

/// 移动宿主全部自报模块的描述符都必须干净（词段判据，见 host-kit 单一真源）
#[test]
fn host_module_descriptors_carry_no_product_nouns() {
    let registry = ModuleRegistry::collected();
    // 防锁空转：移动宿主自报模块（mobile-host-core / fs / notify / auth /
    // connection / terminal-stream / desktop-compat）当前 7 个
    assert!(
        registry.len() >= 7,
        "移动宿主能力模块收集面过小（{} 个）——锁可能空转（ModuleEntry 未链接？）",
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
