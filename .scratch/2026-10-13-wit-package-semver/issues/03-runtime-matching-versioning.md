# 票 03 · 运行期按名匹配点版本化改造（主要成本项）

**来源**：`.scratch/2026-10-13-wit-package-semver/spec.md` 事实基线 6、R2 对策、§5 D1 项 4、§7 T3。

**Blocked by**：票 02（版本化接口名存在后方可运行期匹配与测试）。

**Status**：**✅ done（2026-10-13）**——先用临时探针（跑完即删）实测 wasmtime 48 + wit-parser
0.256 定死匹配形态，再按「单点派生 + 版本注入一次」改造：新增
`packages/bedcode-wasm-core/src/contract.rs`（`export_item` / `interface_path`，`OnceLock`
池保 `&'static` 形态）；内核 30 余处探测点全部改为只传接口名 + 函数名；真源由 JSON 改 Rust
常量 `contracts/bedcode-plugin-package.rs`（能力域 `inventory` 静态自报要求编译期常量，
JSON 只能运行期解析）；`routing.rs` const 清单加版本并由版本锁新判据钉住，内核对偶锁改为
双形态抽取 + 值层比对。验证：wasm-core 575/575、discovery-engine 35/35、版本锁 5/5、双端
宿主 check 绿；变异自检 2/2。

## What to build

版本化后，wasmtime linker 按**含版本的完整名**精确匹配 import/export ⇒ 运行期所有硬编码无版本接口名必须带版本。约 32 处按名匹配点全部改造，**但禁止「32 处各写后缀匹配」**——那只是把红线风险从「硬编码版本」换成散落字符串手术：

1. **导出名单点派生**：改造为**单点派生**（由 world / 接口元数据生成 `ItemName`，版本作为构造参数注入一次），保留既有对偶结构与对偶单测（`owner.rs` 的 EXPORT_* 常量与能力域自报侧词汇真源的对偶，值相等由 capability 单测钉住）。改造范围：
   - `core_binding.rs` 的 abi 探测 2 处（`abi.version` / `abi-form.form`）
   - `component.rs` 可选导出探测 4 处
   - `owner.rs` 导出名 12 处（经 `get_typed_func` 真调用）
   - `capability.rs` 常量 9 处（进 `STORAGE_EXPORTS` / `PROBE_ONLY_CAPABILITIES` 探测表）
   - discovery-engine `routing.rs` 5 处（mdns 导出名）
2. **`capability.rs` 前缀匹配修复**：对 `"bedcode:plugin/host-mdns.` 字面量的 `match_indices` 前缀匹配（测试面）版本化后必断，随本票同改。
3. **单测**：覆盖版本名 / 旧名双形态（注入旧名 → 探测失败路径红）。
4. **变异自检**：注入旧名 / 版本漂移 → 红 → 还原 → 绿。

## 验收标准

- [x] 32 处运行期按名匹配点全部经由单点派生 + 版本构造参数注入（无散落硬编码版本后缀；实测需求 34 处：core_binding 2 · component 4 · owner 16 · capability 8 + 域侧 const 5）
- [x] wasm-core 与能力 crate `cargo test` 全绿（575/575、35/35）
- [x] 双形态单测（版本名 / 旧名）存在且有效（旧名注入 → 红）：`contract::tests` 5 例，覆盖末尾版本形态 / 旧名不命中 / 平名非法 / 池稳定性 / 真源自洽
- [x] 变异自检记录（注入旧名 → 红 → 还原 → 绿）2/2，另加「能力域字面量去版本 → 版本锁 + 对偶锁双红」

## 门禁

- wasm-core（`packages/bedcode-wasm-core` crate 根 `cargo test`）与能力 crate 各自 `cargo test` 绿
- 防回接锁 `binding_surface_must_not_return_to_wasm_core`（wasm-core lib.rs）保持绿——本票不得引入内核硬编码版本常量
- 变异自检

## 注意

- **零端 ABI 知识红线**：版本是构造参数，禁止内核硬编码版本常量（AGENTS.md §5.4 + spec R2）。
- 本票只改运行期匹配机制，不改 WIT 接口语义；`verify_abi` 的 version 分支删除属票 04（bindgen 联动面），本票不提前删。
- 涉及 `component.rs` / `core_binding.rs` 的在途改动先认领来源（并行线活跃面），写前重读。
- 命令字眼以 `docs/commands.md` 为准。
