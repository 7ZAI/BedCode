# 票 04 · ABI 退役（契约 + 运行期 + 断言 + SHARED_ABI_VERSION）

**来源**：`.scratch/2026-10-13-wit-package-semver/spec.md` §2.1（退役评估，用户已裁）、§5 D1 项 5、R1/R4/R7 对策、§7 T4。

**Blocked by**：票 03（bindgen 联动——`core.wit` 删 `interface abi` 后 `core_binding.rs` 的 `abi_version` 探测函数消失，且 `AbiPolicy` 删 `version` 会打破未收口的 `verify_abi` 引用；契约面与运行期面不可拆票并行，中间态编译红）。

**Status**：**✅ done（2026-10-13）**——契约面（`interface abi` / 双端 `export abi` /
`compose.json` 的 `abi.version`）· 运行期（`AbiPolicy`→`FormPolicy` 只管形态、删
`verify_abi` version 分支与 `abi_version` 探测、退役 `plugin_abi_version` 转发面）· SDK
（双端 `abi.rs` 删除、`FORM_COMPONENT` 迁 `form.rs`、`wasm_entry!` 去导出、
`wasm_entry_with_abi!` 退役）· 共享产物（`SHARED_ABI_VERSION` 退役、`wiring_lock.rs`
判据 6 改写为双端版本对齐 + 退役面不得复活）全量落地；`abi-form` 检查原样保留。门禁：
wasm-core 574/575 · file-transfer-core 76/76 · 版本锁 5/5 · 双端 SDK/宿主 check 绿 ·
check:wit 9/9 · 变异自检 1/1。**遗留**：`a03_p2_fuel_async_semantics`（实验证明与本票无
因果，根因为 wasip3 fixture 重建后产物差异 + fuel 计量敏感）→ 票 06。

## What to build

一次性退役整数 ABI（用户裁决：不派生、不分两阶段）。退役面可枚举、双端同步，漏一端即编译红或运行期缺导出：

1. **契约面**：`core.wit` 删 `interface abi`；桌面 cap world 删 `export abi`（2 处）；移动 cap world 删 `export abi`（1 处）；双端 `compose.json` 删 `abi.version`。
2. **运行期面**：`AbiPolicy` 删 `version` 字段（退化为「只管形态」，更贴合「wasm-core 零端 ABI 知识」）；删 `verify_abi` 的 version 比较分支与 `core_binding::abi_version` 探测。**`abi-form.form` 检查保持不动**（与 version 正交，仅桌面导出）。
3. **SDK 面**：双端 SDK `wasm_entry!` 宏去掉 `abi.version` 导出。
4. **共享产物协商下界**：`SHARED_ABI_VERSION` 常量与其锁（file-transfer-core `wiring_lock.rs`）**整体退役**，改由「共享产物按哪端 world 构建就要求双端版本对齐」承担（R7：替换现「`SHARED_ABI_VERSION ≤ 双端 ABI_VERSION`」判据为「双端 compose package 版本对齐」断言；file-transfer 双端单产物场景）。非共享场景双端各自演进、不要求全局同版本。
5. **断言基线**：全部 e2e / fixture 的「产物 `abi.version == ABI_VERSION`」断言删除（含 test_support 的 e2e 基准等；四方不变量收敛为「compose 版本 == 源文件 package 行」两方，由票 02 新锁承担）。

## 验收标准

- [x] `core.wit` 与双端 cap world 无 `interface abi` / `export abi`；双端 compose.json 无 `abi.version`
- [x] `AbiPolicy` 只剩 form 语义（`FormPolicy` + 别名）；`verify_abi` 无 version 分支；`core_binding::abi_version` 探测删除
- [x] 双端 SDK `wasm_entry!` 不再导出 `abi.version`；e2e / fixture 无「`abi.version == ABI_VERSION`」断言
- [x] `SHARED_ABI_VERSION` 及 `wiring_lock.rs` 旧判据退役，替换为双端版本对齐断言（共享产物场景）
- [x] wasm-core / 双端 SDK / file-transfer-core `cargo test` 绿（wasm-core 574/575，唯一红为 `a03_p2_fuel_async_semantics`，非本票因果 → 票 06）；锁变异自检 1/1
- [x] form 检查（`abi-form.form = 1`，仅桌面导出）原样保留、测试绿（`form.rs` 单测锁定 1）

## 门禁

- wasm-core / 双端 SDK / file-transfer-core 各自 crate 根 `cargo test`
- 锁变异自检（注入 `abi.version` 复活 → 红 → 还原 → 绿）
- 台账前置：票 01 已完成双端 `abi.rs` 台账迁移（先迁后删——本票删文件前核对迁移落文在场）

## 注意（并行线前置，写前重读）

- ⚠️ `wiring_lock.rs` 正被并行线（file-transfer single-wasm）活跃修改——此票的锁退役步骤**在其合流提交后**；写前重读、写后复查。
- `form` 检查不在退役范围（走独立 `abi-form`，与 version 解耦）。
- 退役后无任何运行期版本协商：版本一致性由 linker 的包名匹配保证，失败诊断直接来自 wasmtime（命中 `stale_artifact_rebuild_hint` 重建指引，`component.rs` 判据 `has not been defined` / `not found in the linker` / `component does not export`——三条形态仍有效，做迁移期语义验证）。
- 命令字眼以 `docs/commands.md` 为准。
