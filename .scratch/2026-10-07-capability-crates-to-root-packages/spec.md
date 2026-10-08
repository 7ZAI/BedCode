# 能力域 lib 迁到仓库根 `packages/`（2026-10-07）

## 用户指令

> `bedcode-desktop/packages` 下的 bedcode 能力域 lib 迁移到 `packages` 下

## 范围裁决（用户指令的字面口径 = 本仓库既有的「能力域 crate」口径）

搬：**8 个能力域 / 传输面 crate**（`capability_crates_no_product_ids.rs::SCANNED_CRATES` 同一集合）
`bedcode-server-base` · `bedcode-crypto-engine` · `bedcode-server-core` · `bedcode-server-http` ·
`bedcode-server-websocket` · `bedcode-server-peer-net` · `bedcode-discovery-engine` · `bedcode-pty-engine`

不搬：~~`bedcode-wasm-core`（**插件机制整核**，ADR 0037，不是能力域）~~ + `plugin-sdk-*` / `plugin-*-test`
（契约与夹具 crate，按设计非 `bedcode-` 语义类）。

> **2026-10-08 补充（本 spec 的后续票）**：`bedcode-wasm-core` 已按用户指令
> 「wasm-core 也迁出来」在本票之后**一并迁根**至仓库根 `packages/`，理由与做法见
> `.scratch/2026-10-08-wasm-core-to-root-packages/spec.md`。其 `test_support.rs` /
> `fixture_target.rs` / `fixture_build.rs` 的 `CARGO_MANIFEST_DIR` 相对路径与夹具落点
> （原 `../target/fixtures` = 遗留分裂落点 `packages/target/fixtures`）已随迁根重做并归一。
> 本票正文写「8 个 crate 一起搬」的验证数字不含 wasm-core（其验证在后续票记录）。

## 为什么 8 个要一起搬（不能只搬 5 个「纯」能力域）

`bedcode-server-base` 是共享错误类型真源，5 个能力域 crate 全都依赖它（`crypto-engine` 也不例外）。
只搬 5 个会让仓库根 `packages/` 反向依赖 `bedcode-desktop/packages/` —— 正是 ADR 0035 D6 当初
反对的另一面（根目录 crate 反向拉桌面基础层）。**连带把 base/core/crypto 一起搬，两个 packages/
之间的依赖边归零**，D6 的反对理由随之消失。

搬完之后：
- 仓库根 `packages/`：`bedcode-host-kit`（机制内核）+ `link-crypto` + `peer-net` + 8 个能力域 crate
  = **全部引擎 / lib crate**
- `bedcode-desktop/packages/`：只剩 `plugin-sdk-desktop` / `plugin-sdk-fixtures` / `plugin-*-test`
  = **契约与夹具 crate**，命名约定（`bedcode-` 引擎 vs `plugin-` 契约）自洽

## ADR 冲突与处置

ADR 0035 D6「能力域 crate 落 `bedcode-desktop/packages/`」被本指令推翻 ⇒ 需在 ADR 0035 记
**部分撤销**（与 ADR 0036 撤销 sqlite 域同一手法），并改写 D6 段：D6 当初的两条理由
（① 必须绑死桌面 WIT 与 `bedcode-server-base` ② 必然依赖桌面基础层的 crate 放根目录是陷阱）
在本布局下同时消解——桌面基础层自己也在根 `packages/`。ABI / WIT / world 一个字节不动。

## 落点副作用清单（动手前核实过）

1. **path 依赖**：src-tauri（`../packages/*` → `../../packages/*`）、wasm-core（`../bedcode-*` →
   `../../../packages/bedcode-*`，它自己的深度没变，指向根 packages 的三处
   `../../../packages/{host-kit,peer-net,link-crypto}` 也不用改）、terminal-session wasm 应用
   （`../../../packages/*` → `../../../../packages/*`）、cross-end-tests（`../bedcode-desktop/packages/*`
   → `../packages/*`）、8 个 crate 之间的 `../bedcode-*` 边（同样不动）。
2. **target 落点**：`bedcode-desktop/packages/*/.cargo/config.toml` 写死 `../../../target/server-libs`
   与 `../../../target/host-kits`（相对 crate 根解析 = 仓库根）。搬后须改 `../../target/*`，否则
   静默落到仓库根的**上级**目录。每处都有 `cargo metadata` 自检要求，必须逐个核验。
3. **两条治理锁按目录约定推导覆盖面**（手写名单是零成本后门）：
   - `capability_crates_no_product_ids.rs`：`packages_dir()` 单根 → 双根（仓库根 + 桌面），
     根目录多出 `bedcode-host-kit` 须进 `PENDING_SCAN_CRATES`（它此前在锁的宇宙外）。
   - `capability_crates_unit_tests_only.rs`：治理面同理；`bedcode-host-kit` 进 `PENDING_GOVERNANCE`
     （`tests/forced_link*.rs` 是按设计的跨 crate 集成测试，原注释已说明「明确不在本锁宇宙内」）。
   - 两条锁都改成「双根枚举 + 按 crate 名解析根」，缺目录即 panic（fail-visible，不静默空转）。
4. **CI** `.github/workflows/test.yml` 的 server-libs 循环 `cd bedcode-desktop/packages/$lib` → `cd packages/$lib`。
5. **文档**：AGENTS.md 路径基准、桌面 code-map、ADR 0035 / 0037 / 0038 / 0039、`docs/commands.md`、
   `docs/knowledge/build-process.md`、cross-end-tests README、CHANGELOG 双语。
6. **不动**：`bedcode-wasm-core` 的夹具 target 解析（`../target/fixtures`）、`wasm-apps` / 夹具
   两处 `.cargo/config.toml`。

## 验证（AGENTS §10）

- 每个搬动 crate：`cargo metadata` 核 `target_directory` 仍是仓库根 `target/server-libs` / `target/host-kits`
- 每个搬动 crate：`cargo test`（8 个 crate 根逐个 cd 进去跑，§3）
- `bedcode-desktop/src-tauri`：`cargo test` 全量（含两条治理锁 + crate_boundary_lock + 全部结构锁）
- `cross-end-tests`：`cargo test`（跨端 wire 契约门禁）
- wasm 应用 `terminal-session`：`cargo test`（依赖 server-base/core/http/ws）
- `cargo fmt --check` / `cargo clippy` 自查；根 `pnpm exec eslint .` 0 error