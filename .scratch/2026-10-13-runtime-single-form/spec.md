# wasm-core 运行时单形态（common/bare 壳统一）

- 状态：进行中
- 日期：2026-10-13
- 用户裁决（原文摘录）：
  - 「ABI 版本 为什么和wasm core 有关 wasm-core 提供统一的基础ABI 宿主组装多个sdk 的 wit 后才是真正的ABI」
  - 「装配Linker 应该统一 wasm-core 不一致的有宿主组装实现差异 而不是中wasm core 中」
  - 「其他剩下的一样 统一按照桌面端的来 移动端进行改造」
- 关联：ADR 0049 遗留项 #1（形态 feature 与双 SDK 词汇收敛）、ADR 0045/0047/0018（契约端差异不变）

## 裁决落地方式

| 裁决 | 落地 |
| --- | --- |
| wasm-core 零端 ABI 知识 | `verify_abi` 的版本/形态上限检查参数化：`AbiPolicy { version, form: Option<u32> }` 由宿主装配时注入（桌面 `bedcode_plugin_api` 37 + form=1；移动 `bedcode_plugin_api_mobile` 20 + form=None）。`bedcode-plugin-api` / `bedcode-plugin-api-mobile` 从 wasm-core 依赖摘除 |
| Linker 装配统一 | 只留一份（桌面形态 = `core_binding` 8 域 + 宿主自报 `HostModule` + WASI），无条件编译。端差异 = 各端宿主自报模块集（移动 host_domains 12 域已住宿主） |
| 其余按桌面形态、移动改造 | `WasmRuntime` / `LoadedWasmPlugin` / 实例化 / 导出调用面全部用桌面版；移动宿主改调用（for_plugin 派生 context 传权限、`on_bus_message`→统一 `on_message`、`call_lifecycle_event` 无条件保留） |

## 文件级改动清单

### wasm-core（packages/bedcode-wasm-core）
1. `Cargo.toml`：删 `common-host` / `bare-host` feature；摘除 `bedcode-plugin-api` / `bedcode-plugin-api-mobile` / `bedcode-discovery-engine` / `bedcode-ws-client-engine`；`bedcode-server-websocket` / `bedcode-server-http` 无条件；桌面专属装配面（PluginHost/intercall/auth_center/密码学/server-core/crypto-engine）收进 `desktop-assembly` feature（默认开）
2. `src/lib.rs`：删 `compile_error!` 互斥守卫；feature 门控清理（`intercall` / `host_harness` / `test_support` / facade 再导出）；SHELL_FILES 锁与 `binding_surface_must_not_return_to_wasm_core` 更新
3. `src/manager/runtime.rs`：删 bare 分支；common 壳提升为唯一形态（无条件）
4. `runtime/common.rs` → 并入 `runtime.rs`（WasmRuntime / EngineSetup / AbiPolicy / panic_payload_to_string + tests mod 声明）
5. `runtime/common/component.rs` → `runtime/component.rs`（唯一组件壳；verify_abi 参数化；`call_lifecycle_event` 无条件保留）
6. `runtime/common/tests/` → `runtime/tests/`；删 `bare.rs` + `bare/` + `common.rs` + `common/` 目录
7. `src/manager/host.rs`：`PluginHost::new` 增加 `abi: AbiPolicy` 参数（桌面宿主注入；wasm-core 内部测试用 unconstrained）
8. `src/test_support`：common/bare 合并（统一 setup_wasm_runtime 带 AbiPolicy）
9. 各处 `#[cfg(feature = "common-host")]` 门控清理

### 桌面宿主（bedcode-desktop）
- `src-tauri/src/lib.rs` 与 `tests/support/mod.rs`：`PluginHost::new` 传桌面 AbiPolicy（`bedcode_plugin_api::abi`）
- 其余零改动（统一形态即桌面形态）

### 移动宿主（bedcode-mobile）——改造适配
- `src-tauri/src/plugin/manager.rs` / `loader.rs`：统一 `WasmRuntime` 构造（storage/app_handle/first_party_dirs + AbiPolicy 20/None）+ `instantiate_component(&component, pid, ctx.for_plugin(perms), &[], None)` + `LoadedComponentPlugin` → `LoadedWasmPlugin`
- `manager.rs`：`on_bus_message` 调用点改统一 `on_message`；`call_lifecycle_event` 沿用（统一壳保留）
- `host_domains/**`：白名单声明（`expect_host_module!`）核对；`tests/` 构造点改
- 锁：`mobile_wasm_core_purity_lock` 七判据改单形态；`retired_mobile_wasm_core_fork_lock` 核对
- `Cargo.toml`：依赖面调整（SDK 仍宿主自持）

### 文档
- AGENTS.md §5.4、双端 code-map、ADR 0049 修订、CHANGELOG 双语

## 行为变更（标注）
- 移动 `on_message_binary` 探测从平名 `#` 改 ItemName 点号（ADR 0049 遗留项 3 自动修复：二进制消息开始真实投递）
- 移动实例化从 Sync 改 Async（同步组件在 async 路径等价，引擎已有实证）
- 移动实例获得桌面增强面：monitor 指标、可选导出探测（events-ws/task 探测 None 无害）、stale_artifact_rebuild_hint

## 验证
- wasm-core `cargo test --lib`（桌面形态）
- 桌面 `cargo test`
- 移动 `cargo check` + `cargo test`
- 锁组全绿
