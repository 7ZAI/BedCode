# 票 05 · 元数据常量与锁同步（与票 03 / 票 04 并行）

**来源**：`.scratch/2026-10-13-wit-package-semver/spec.md` 事实基线 5-6、R4 对策、§5 D1 项 6-7、§7 T5。

**Blocked by**：票 02（版本值定即可开工；**不依赖票 03/票 04**——纯机械同步面，可与两者并行）。

**Status**：**✅ done（2026-10-13）**——49 处元数据常量全部带 `bedcode:plugin@0.1.0/` 前缀；随行修正
**基线元数据漂移 4 处**（桌面 `host-connection`→`host-connection-registry` / `host-auth`→`host-auth-center`，
移动 `host-connection`→`host-connection-target` / `host-auth`→`host-auth-client`——常量与 WIT 生成物对齐，
运行期绑定本就已走新名，纯元数据修正）；桌面 `bindings.rs` 与移动 `mobile_wit_composition_lock.rs` 的原
`abi.version` 双查改为**版本一致性检查**（真源 = 仓根 `contracts/bedcode-plugin-package.rs`，票 03 实施期由
JSON 改 Rust 常量以支持 const 上下文；`abi.version` 留待票 04 退役，本票不读它）；两把改写锁变异自检各
1/1（注入版本漂移 → 红 → 还原 → sha256 一致 → 绿）；其余锁逐一核对（purity 判据 7 的 `cap-*` needles、
`binding_surface_must_not_return_to_wasm_core` 的 Rust 模块路径 needles、`wit_package_version_lock`、
`bindings.rs` 的 `rsplit('/')` 版本免疫）均不受版本化影响。`pnpm run check:wit` 9/9 绿；桌面宿主 lib
155/0、移动组合锁 3/3 + purity 7/7、能力 crate（pty 100 / http 85 / ws 51 / peer 56 / discovery 34）全绿。
⚠️ 运行期面 2 项红属**并行线票 03/04 在途**（非本票）：wasm-core `a03_p2_fuel_async_semantics`（燃料计量）、
移动 `ws_client_e2e` 编译（引用已删 `bedcode_plugin_api_mobile::abi`）——待并行线合流后归绿。

**Status**：ready-for-agent

## What to build

版本化后的机械同步面：元数据常量（不断裂，仅机械同步）+ 锁与断言跟改：

1. **49 处元数据常量** `bedcode:plugin/...` → `bedcode:plugin@0.1.0/...` 机械替换：`HostModuleDesc.interfaces`（host-kit）+ 各能力 crate `MODULE_INTERFACES`（pty / peer / http / ws / mdns 5 处）。注意运行期匹配**不走**它（`add_to_linker` 由 wit-bindgen 自动生成版本名；`ModuleRegistry::verify_whitelist` 只比对 `desc().name`）——替换后无需逻辑改动，但消费侧字符串须与生成物一致。
2. **桌面 `bindings.rs` 覆盖测试**：按 `/` 取尾段（`rsplit('/')`）天然免疫版本后缀——核对即可；原 `abi.version == 38` 断言（`bindings.rs:93` 处）**改为版本一致性检查**（compose package 版本 == 源文件 package 行，随票 02 新锁口径）。
3. **~6 把锁 needles 跟改**：
   - `mobile_wit_composition_lock.rs`：`abi.version` 双查（`mobile_wit_composition_lock.rs:40`）→ 版本一致性检查；worlds 存在性核对
   - `mobile_wasm_core_purity_lock.rs` 判据 7（CAP_SLICE_WORDS 五词 needles）：核对版本化后词汇面（接口名带 `@0.1.0` 是否影响判据；若判据基于无版本前缀匹配，确认仍命中）
   - `binding_surface_must_not_return_to_wasm_core`（wasm-core lib.rs）
   - 桌面 `bindings.rs` 断言（如上）
   - 其他既有锁：逐把核对 needles 在新接口名形态下仍生效
4. **每把改写的锁变异自检**：注入违背形态 → 红 → 还原 → 核 sha256 → 绿，逐锁记录（惯例同 mobile_wasm_core_purity_lock 变异表）。

## 验收标准

- [x] 49 处元数据常量全部带版本，与生成物一致（桌面 29 / 移动 11 / 能力 crate 7 / wasm-core 测试 2；
      随行修正 4 处基线漂移使常量与 WIT 生成物对齐）
- [x] 原两处 `abi.version` 双查（桌面 `bindings.rs` / 移动 `mobile_wit_composition_lock.rs`）改为版本一致性检查
      （compose package 版本 == 源文件 package 行；真源 `contracts/bedcode-plugin-package.rs`）
- [x] ~6 把锁 needles 核对/跟改完成（purity 判据 7 / binding_surface / wit_package_version_lock / bindings rsplit
      免版本免疫 / host 模块声明测试），两把改写锁变异自检各 1/1 记录
- [x] 双端相关 crate 测试绿（桌面 lib 155/0、移动组合锁 3/3 + purity 7/7、能力 crate 全绿）；
      wasm-core 574/1 与移动全量 2 红为并行线票 03/04 在途（运行期面，非本票领域）

## 门禁

- 双端 `cargo test`（宿主 + SDK）
- purity / composition 锁绿 + 变异自检记录
- 本票不碰运行期匹配逻辑（那是票 03）、不碰 WIT 契约面（那是票 02/票 04）

## 注意

- 纯机械同步：不顺手改语义；发现行为不一致处停下问用户（spec §8）。
- 涉及在途文件（bindings.rs、purity 锁）先认领来源，写前重读。
- 命令字眼以 `docs/commands.md` 为准。
