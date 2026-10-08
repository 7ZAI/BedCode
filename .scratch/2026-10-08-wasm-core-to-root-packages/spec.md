# wasm-core 整核迁根至仓库根 `packages/`（2026-10-08）

## 用户指令

> `bedcode-desktop/packages/bedcode-wasm-core` 为什么还留在桌面端的 package 里没有迁移出来，请迁移

## 范围裁决

搬：**插件机制整核 crate `bedcode-wasm-core`**（ADR 0037；2026-10-07 能力域迁根 spec 明确
「不搬」，本票即其点名的后续票）。搬后：
- 仓库根 `packages/` = `bedcode-host-kit` + `link-crypto` + `peer-net` + 8 能力域/传输面
  crate + **整核本体** = 全部引擎 / lib crate
- `bedcode-desktop/packages/` = 只剩插件契约 / 夹具 crate（`plugin-*`）

不搬：`plugin-sdk-*` / `plugin-*-test`（契约与夹具）。移动端契约独立（ADR 0018），
fork 对齐走 ADR 0040 的 `bedcode-mobile/packages/bedcode-wasm-core`。

## 落点副作用清单（动手前核实过）

1. **path 依赖**（wasm-core 自身 Cargo.toml）：11 个根 `packages/` 兄弟
   `../../../packages/<crate>` → `../<crate>`；桌面 SDK 契约 `bedcode-plugin-api`
   `../plugin-sdk-desktop/rust` → `../../bedcode-desktop/packages/plugin-sdk-desktop/rust`。
   src-tauri 的 `bedcode-wasm-core = ../packages/...` → `../../packages/...`。
2. **target 落点**：wasm-core `.cargo/config.toml` `../../../target/host-kits` →
   `../../target/host-kits`（同一目录；多一个 `..` 静默落到仓库根上级，必须 cargo metadata 核验）。
3. **`CARGO_MANIFEST_DIR` 相对外部路径**（crate 根变浅一层，全部统一改
   `../../bedcode-desktop/…`）：
   - `test_support.rs`：`session_apis`（`../../wasm-apps/...`）、`sdk_fixture_artifact_bytes` /
     `system_test_artifact_bytes`（`../target/fixtures/...`）
   - `manager/runtime/fixture_target.rs` `dir()`（`../target/fixtures`）与 fixture_build.rs
     两处 `packages_dir`（`..` → `../../bedcode-desktop/packages`）
   - `manager/runtime.rs` build_wasi_test_component 的 `packages_dir`
   - `permission.rs` `desktop_root()`（`../..` → `../../bedcode-desktop`）
   - `host_api.rs` 两份生成物路径（SDK bin / 前端 permission-vocabulary）
   - `manager/host/api_bridge.rs` 退役面扫描根（src-tauri/src 与 plugins）
   - `manager/runtime/component.rs` 与 `tests/p3_async_host_import.rs` 两处 `bindgen!` WIT 路径
   - `manager/host/tests/l2_gating_test.rs`（auth_center 源码面）、`host_api/tests/pty_wiring.rs`
     （include_str lifecycle.rs）
4. **夹具落点归一（顺手修的既有分裂）**：整核抽出时 fixture_target.rs 基准从
   `<desktop>/src-tauri` 悄然变成 crate 根，`../target/fixtures` 实际指向不存在的
   `bedcode-desktop/packages/target/fixtures`——与 `packages/.cargo/config.toml` / bench /
   工具链脚本指向的 `bedcode-desktop/target/fixtures` 分裂（check-target-size.js
   legacyTargetLive 有据）。本次统一为 `../../bedcode-desktop/target/fixtures`；
   legacyTargetLive 的 `packages/target` 登记删除。
5. **治理锁**：
   - `bedcode-wasm-core/src/crate_boundary_lock.rs`：`SPLIT_CRATES` 基准从 `<desktop>/packages`
     变为仓库根 `packages/`，全部 `../../packages/<crate>` 条目回归裸 crate 名
   - `src-tauri/src/server/crate_boundary_lock.rs`：`packages_dir()` 上移一层
     （`..` → `../..`），文档同步
   - `capability_crates_no_product_ids.rs` / `capability_crates_unit_tests_only.rs`：
     双根枚举语义不变（桌面根已无 `bedcode-*`，保留枚举防回接），仅文档
   - `wasm_core_whole_crate_lock.rs` 消息字面量；`wasm_bridge_bench/support.rs` 注释
6. **CI**：wasm-core 非 CI 循环成员（随 src-tauri `cargo test` 作 path 依赖跑其单测），无改动。
7. **文档**：AGENTS.md 路径基准、桌面 code-map、ADR 0035 / 0037 / 0038 / 0039 / 0040、
   `docs/commands.md`、`docs/knowledge/{build-process,wasip3-toolchain}.md`、
   `packages/.cargo/config.toml`（fixture_target 真源路径注释）、CHANGELOG 双语、本票 spec。

## 刻意不动

- `api_bridge.rs` 插件前端扫描根保持 `<desktop>/plugins` 路径等价（2026-09-25 改名 wasm-apps
  后该面已空转；wasm-apps 前端含同名命令属插件自有命令面，恢复扫描须先厘清判据——另立票据）。
- `packages/.cargo/config.toml` 的 `../target/fixtures`（基准 = `bedcode-desktop/packages/`，
  与归一后的 fixture_target.rs 指向同一目录）。

## 验证（AGENTS §10）

- [x] `cargo metadata` 核 wasm-core `target_directory` = 仓库根 `target/host-kits`
      （实测 `/packages/bedcode-wasm-core/../../target/host-kits`，归一后 = 仓库根）
- [x] wasm-core crate 根 `cargo test` 全量：**677 通过 / 1 失败**（唯一失败 =
      既有 perf 基线 `perf_p2_guest_ring_fetch_batch_curve`，CHANGELOG 已记载；pty_e2e
      两例在满负载全量跑时红、隔离复跑全绿，属负载敏感）
- [x] 夹具落点归一生效：本次测试把 p3/sdk 夹具写进 `bedcode-desktop/target/fixtures`，
      `bedcode-desktop/packages/target` 未再生成
- [x] `bedcode-desktop/src-tauri` `cargo check --tests` 通过（9m26s，零 error）——path 依赖 /
      全部锁文件 / test_support 导入的编译级验证
- [ ] `bedcode-desktop/src-tauri` `cargo test` 全量——**磁盘阻塞**（需 ~15-20G 新 target，
      本次清 sccache 后仍只 ~4G 余量；10-07 波迁移同样记录未跑）
- [ ] `cross-end-tests` `cargo test`——**磁盘阻塞**（依赖两端 lib，首次构建峰值十几 G）
- [x] `cargo fmt --check`：我的改动全部干净（diff 均落在并行会话在途代码：bus.rs 重构 /
      first_party_dirs / ports_impl，未碰）

### 磁盘阻塞记录（2026-10-08）

本机 12G 空闲；src-tauri 全量测试需要新 target（其 debug 缓存 10-07 后被清，
~1000 crate 重编 / 15-20G），cross-end-tests target 不存在（首次构建峰值十几 G）。
wasm-core 自身测试走既有的仓库根 `target/host-kits`（20G 温缓存），已跑通。
本次清掉 sccache 冷缓存（4.5G，dev profile 下本就无效）后 src-tauri `cargo check --tests`
通过；src-tauri / cross-end 的重编需继续释放磁盘，交付时点名。
