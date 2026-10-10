# 票 05 · 移动绑定归位（bare 壳 + host_impl + cap-mobile → 移动宿主）

> 输入：`spec.md` §5 T-05。前置：T-03（交集绑定定稿）+ T-04（对称收口，非阻塞依赖）。
> **依赖**：T-03 → **本票**；**本票 ∥ T-04 可并行**（改动集不重叠）；T-06 依赖二者。
> **测试节奏**：针对性自验（移动宿主 `cargo check` + 绑定相关单测 + app 重建 smoke + 锁自检）；移动全量 `cargo test`（含此前 codebuddy 未复跑面）留 T-06 收尾。
> 目的：wasm-core 内移动绑定与装配全部迁出到 `bedcode-mobile/src-tauri/src/plugin/`，`bedcode-wasm-core-mobile` rename 别名删除、直连单形态。

## 1. 迁移对象（bare 侧）

| # | 现位置（wasm-core） | 迁移内容 | 落点（移动宿主） |
| --- | --- | --- | --- |
| M1 | `manager/runtime/bare.rs`（164 行） | 移动运行时壳：Store 装配（无 WASI）、`WasmRuntime::new` 消费 `engine::build_engine_config`（codebuddy 已接入） | `src-tauri/src/plugin/` 既有 `manager.rs` 依赖的 `wasm_runtime` 垫片改写（见 §3） |
| M2 | `manager/runtime/bare/component.rs`（1781 行） | `bindgen!` cap-mobile world + 12 个 Host impl + `LoadedComponentPlugin`（invoke_command / activate / deactivate / on_bus_message / call_lifecycle_event / get_manifest） | `src-tauri/src/plugin/bindings.rs`（既有同名文件，核对承接）+ 移动 `plugin/` 各域文件 |
| M3 | `manager/runtime/bare/host_impl/`（8 域在盘：bus/config/db/event/fs/http/mdns/peer/platform/storage/support/ws 中已删 auth/connection/notify/terminal_stream） | 移动域实现（权限校验 + 同步上下文宿主调用） | `src-tauri/src/plugin/*`（auth/connection/notify/terminal_stream 已落地宿主先例 0047；其余 8 域跟进） |
| M4 | `host_context_registry.rs`（移动装配面 AppHandle/端口注册表） | 移动宿主持有 | `src-tauri/src/plugin/` |
| M5 | `terminal_stream_gateway.rs`（移动独有 interface 流转发闸门） | 移动宿主持有（interface 本身是 cap-mobile 专属，T-03 语义边界内） | `src-tauri/src/plugin/terminal_stream.rs`（既有文件，核对合并） |
| M6 | `host_api/{ports,sql_guard,bare_context}.rs` | 移动 adapter（端口词汇 / sql 护栏 / 移动 WasmHostContext） | `src-tauri/src/plugin/` 既有 `host_ports.rs` / `peer_ports.rs` / `saf_io.rs` 等合并 |
| M7 | `test_support/bare.rs`（594 行）+ `test-support` feature | 移动测试基建（build_host_ctx / build_test_component 等，移动 `src-tauri/tests` 消费） | `src-tauri/tests/` 或 dev-dependencies 承接 |
| M8 | lib.rs 的 bare cfg 门控（`host_context_registry` / `terminal_stream_gateway` / `intercall` 移正） | 删除 | — |

## 2. 关键核对（开工前）

- **codebuddy 坑位**（.codebuddy/memory 已记）：`bare/component.rs` 的 `call_event_export` 闭包已改返回 `wasmtime::Error` 走 `engine::settle_guest_event`；移动 `cargo check --lib` 不报、`cargo test` 才报（test-support feature 多开）——迁移测试面时保留该已知行为。
- 移动 `src-tauri/src/plugin/manager.rs` 的 `use crate::plugin::wasm_runtime::{LoadedComponentPlugin, WasmHostContext, WasmRuntime}`（14 行）与 `host_impl::purge_for_plugin`（832 行）引用整模块垫片——垫片改写为**直连 wasm-core 单形态** + 移动绑定层（见 §3）。
- 3 个 wasm-app（ai-chatbox / file-transfer / terminal-session）的 SDK 引用**不动**；cap-mobile 契约零变化；绑定归位后 app 产物仍指向同一套 WIT，只需重构建验证。

## 3. `bedcode-wasm-core-mobile` 收口

- `bedcode-mobile/src-tauri/Cargo.toml`：`bedcode-wasm-core-mobile = { package = "bedcode-wasm-core", ..., default-features = false, features = ["bare-host"] }` ×2（23/140 行）→ 改直连 `bedcode-wasm-core`（单形态默认 features）或按 T-03 形态调整。
- 全仓替换 `bedcode_wasm_core_mobile::` 为 `bedcode_wasm_core::`（或保留 rename 但指向同一形态——优先删除别名，语义干净）。
- `retired_mobile_wasm_core_fork_lock.rs`（现防 fork crate 回接）语义更新：不再有第二 crate，锁泛化为「禁止任何 fork/rename 织体回归」。

## 4. 门禁

- 移动 `cargo test`（src-tauri 全量，含此前未复跑的 test-support 面）全绿。
- wasm-core `cargo check --lib`（无 feature）0 error——**证明 wasm-core 已不依赖移动绑定**。
- 3 个 wasm-app：`node scripts/build.js --rust-only` 重建成功（wasmHash 注入链路核对）。
- 桌面侧零变化（`cargo check --lib` 回测）。
- `pnpm run check:wit` 通过（cap-mobile 生成物不变）。

## 5. 交付物

- 移动绑定迁移落地（M1-M8 处置表）+ rename 别名删除 + `wasm_runtime` 垫片改写；
- 3 app 重构建产物 + 门禁结果附本票。