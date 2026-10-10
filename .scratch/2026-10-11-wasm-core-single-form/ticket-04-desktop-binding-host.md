# 票 04 · 桌面绑定归位（common 壳 + WASI + 桌面扩展域 → 桌面宿主）

> 输入：`spec.md` §5 T-04 + §2 路径 B 先例。前置：T-03（交集绑定定稿）。
> **依赖**：T-03 → **本票**；**本票 ∥ T-05 可并行**（桌面宿主与移动宿主改动集不重叠；并行前确认两票在 wasm-core 的文件切面无交集——由 T-03 交付物切开）。
> **测试节奏**：针对性自验（桌面宿主 `cargo check` + 绑定相关单测 + 锁自检）；桌面全量 `cargo test` 留 T-06 收尾。
> 目的：wasm-core 内的桌面侧绑定与装配全部迁出到 `bedcode-desktop/src-tauri/src/plugin/`，wasm-core 不再引用桌面生成物路径。

## 1. 迁移对象（common 侧）

| # | 现位置（wasm-core） | 迁移内容 | 落点（桌面宿主） |
| --- | --- | --- | --- |
| D1 | `manager/runtime/common.rs`（约 545 行，T-01 后） | 运行时壳：Engine/Linker/Store 生命周期装配 + WASI p2/p3 preopen + fs_auth 授权过滤 + `expand_preopen_declarations` / `resolve_preopen_dirs` + `LoadedWasmPlugin` 使用面 | `src-tauri/src/plugin/` 相应模块（既有 `bindings.rs` 承接绑定） |
| D2 | `manager/runtime/common/component.rs` 中**桌面扩展** Host impl | `host_events_desktop` / `host_fs_desktop` / `host_platform_desktop`（lib.rs 防回接锁已注明迁宿主，本票闭环） | `src-tauri/src/plugin/{events,fs,platform}.rs`（已有同名文件，核对是否已有 impl——避免 defined twice，见下） |
| D3 | `common/component.rs` 中**交集 9 域** 的桌面绑定形态 | 若 T-03 选 Option C：连同交集绑定全上移；Option B：交集留在 core，仅把桌面**装配接线**移出 | 按 T-03 裁决 |
| D4 | WASI 装配 + `wasmtime-wasi` 依赖 | wasip3 preopen、fs_auth 过滤链 | `src-tauri` Cargo.toml 依赖（wasm-core 删除该依赖） |
| D5 | `abi-form` 消费（桌面独有必选导出） | verify_abi 对 abi-form 的检查 | 宿主 verify_abi 流程 |
| D6 | common 侧测试 + fixture（`fixture_build` / `fixture_target` / tests 15 文件） | 装配面测试 | `src-tauri/tests/`（迁移后改造，不整套搬） |
| D7 | common 侧 `host_api` 14 域模块中**纯装配词汇** | 随绑定迁出；机制核 / sqlite_ports / auth_center / unit_executor 等**留 core**（lib.rs 防回接锁合法残余清单为凭） | 核对 lib.rs 各锁注释的「合法残余」列表，逐一确认去留 |

## 2. 关键核对（开工前）

- **defined twice 风险**：桌面宿主 `plugin/events.rs`、`plugin/fs.rs`、`plugin/platform.rs` 是否已含这些 interface 的 `Host` impl？若已有，wasm-core 侧必须**删**而非迁（防回接锁注释确认「WIT impl 落宿主」——即宿主**已持有**，wasm-core 的 common 侧 impl 可能是测试替身或同步残留，逐行确认）。
- `host_module_whitelist()`（common/component.rs:328）与宿主 `pty_wiring.rs` 双向比对的合并结果，随迁移改为宿主单点持有。
- 桌面 `server/crate_boundary_lock.rs` 的分裂产物登记表（wasm-core 与宿主单向引用）——若绑定层迁宿主后产生新 crate 拆分或路径变化，同步登记表。

## 3. 依赖面

- wasm-core `Cargo.toml`：删除 `wasmtime-wasi`（若 move）、桌面 SDK `bedcode-plugin-api` 的绑定引用（机制面已在 T-02 剥离）；`bedcode-server-{http,websocket,peer-net}/common-host` 联动 feature 引用确认（这些是能力域 crate 自身 feature，**保留**——T-02 已登记例外）。
- `bedcode-desktop/src-tauri/Cargo.toml`：按需补 `wasmtime-wasi` 等。

## 4. 门禁

- 桌面 `cargo test`（src-tauri 全量）绿；桌面宿主 `cargo check --lib` 0 error。
- wasm-core（无桌面 feature）`cargo check --lib` + `cargo test --lib` 全绿——**证明 wasm-core 已不依赖桌面绑定**。
- `pnpm run check:wit` 通过。
- 锁：`sliced_interfaces_must_not_return_to_wasm_core` 等防回接锁更新为「wasm-core 禁一切桌面扩展词汇」最终形态；变异自检。

## 5. 交付物

- 桌面绑定迁移落地（D1-D7 处置表结果）+ 宿主绑定层单点持有；
- 防回接锁最终化 + 门禁结果附本票。