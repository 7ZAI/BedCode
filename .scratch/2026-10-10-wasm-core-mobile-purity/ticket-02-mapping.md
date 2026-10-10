# 票 02 改名映射表（2026-10-10 开工时重扫生成）

> 判据：wasm-core 内**标识符级零端名**（desktop / mobile）、零机制名（wasi / wasip3 / wasm32）；
> 注释保留「桌面/移动」中文历史说明，但英文标识符 / feature 名 / 活引用一律改新名（避免字面失真）。
> 范围：`packages/bedcode-wasm-core/**` + 消费方同步（`bedcode-mobile/src-tauri`）+ 双端锁 + 文档。
> 端事实**保留**（不在本票）：端 SDK 路径 `plugin-sdk-{desktop,mobile}`、端工程路径 `bedcode-{desktop,mobile}/**`、
> WIT interface 名 `host-{fs,platform,events}-desktop` / `host-fs-mobile`（wire 契约零变更）、
> `bedcode_plugin_api_mobile`（SDK crate 名）、日志配置串 `bedcode_desktop_lib`。

## 1. cargo feature

| 现名 | 新名 | 定义处 | 引用同步 |
| --- | --- | --- | --- |
| `desktop-host`（默认） | `common-host` | `packages/bedcode-wasm-core/Cargo.toml` | `bedcode-mobile/src-tauri/Cargo.toml`（无）、双端锁、文档 |
| `mobile-host` | `bare-host` | 同上 | `bedcode-mobile/src-tauri/Cargo.toml`（`features = ["mobile-host"]` ×2）；`bedcode-mobile/src-tauri/src/{system/error.rs,plugin.rs,plugin/host_ports.rs}` 注释活引用；4 个移动锁 |
| `test-support` | 不变 | — | — |

**wasm-core `Cargo.toml` 保留行（外部 crate feature 引用，登记例外）**：
`bedcode-server-http/desktop-host` / `bedcode-server-websocket/desktop-host` / `bedcode-server-peer-net/desktop-host`
—— 引用的是**能力域 crate 自身**的 feature 名（其公共 API）；级联改名超出本 spec 范围（spec §1.5 范围边界精神），
若需级联另开票（建议目标名 `common-host`）。

## 2. 模块 / 目录

| 现路径 | 新路径 | 引用同步 |
| --- | --- | --- |
| `src/manager/runtime/desktop.rs` | `src/manager/runtime/common.rs` | `runtime.rs` `mod desktop;` → `mod common;`、`pub use desktop::*` → `common::*`、`fixture_build`/`fixture_target` |
| `src/manager/runtime/desktop/` | `src/manager/runtime/common/` | `runtime/desktop/tests/engine_limits.rs` 的 `runtime::desktop::component` 全路径 |
| `src/manager/runtime/mobile.rs` | `src/manager/runtime/bare.rs` | `runtime.rs` `pub mod mobile;` → `pub mod bare;`、`pub use mobile::{...}` |
| `src/manager/runtime/mobile/` | `src/manager/runtime/bare/` | 同上（host_impl 随目录） |
| `src/test_support/desktop.rs` | `src/test_support/common.rs` | `test_support.rs` `mod desktop;`、`l2_gating_test.rs:89` 白名单条目 |
| `src/test_support/mobile.rs` | `src/test_support/bare.rs` | `test_support.rs` `pub mod mobile;`、`test_support/common.rs:585` `pub mod mobile;` |
| `src/test_support/mobile/`（mock_plugin_ws.rs） | `src/test_support/bare/` | 同上 |
| `src/host_api/mobile_context.rs` | `src/host_api/bare_context.rs`（模块 `bare_context`） | `host_api.rs` `pub mod mobile_context;` + `pub use`；`host_context_registry.rs`；`runtime/mobile.rs`（→bare.rs） |

## 3. 类型 / 函数 / 变量

| 现名 | 新名 | 落点 | 备注 |
| --- | --- | --- | --- |
| `DesktopPluginInfo` | `HostPluginInfo` | `manager/types.rs`、`manager/host.rs`、`manager/host/api_bridge.rs` | 双端编译共用（避免与 SDK `PluginInfo` 混淆）；同步测试函数名 `test_*_desktop_info_*` → `test_*_host_info_*` |
| `MobileDiscoveryPorts` | `StateDiscoveryPorts` | `runtime/bare/host_impl/mdns.rs` | 遵循 crate 内既有惯例 `StateBusPorts` / `StateStoragePorts` |
| `MobileWsClientPorts` | `StateWsClientPorts` | `runtime/bare/host_impl/ws.rs` | 同上（避让引擎 trait `WsClientPorts`） |
| `MobileReconnectPolicy` | `StateReconnectPolicy` | 同上 | 同上 |
| `mobile_ports`（局部变量） | `state_ports` | mdns.rs:214 | — |
| `desktop_root()`（测试辅助） | `sdk_root()` | `permission.rs` | join 路径 `../../bedcode-desktop` 保留（端事实） |
| `bus_subscribe_gates_now_enforced_on_mobile` | `bus_subscribe_gates_now_enforced` | `runtime/bare/host_impl/bus.rs` | 测试名去掉端名 |
| `system_space_guard_now_enforced_on_mobile` | `system_space_guard_now_enforced` | `runtime/bare/host_impl/storage.rs` | 同上 |

## 4. 锁 / 断言名

| 现名 | 新名 | 落点 |
| --- | --- | --- |
| `desktop_sliced_interfaces_must_not_return_to_wasm_core` | `sliced_interfaces_must_not_return_to_wasm_core` | `src/lib.rs`（锁体 needle `host_fs_desktop` 等 WIT interface 名保留） |

## 5. 包元数据 / 文档

| 项 | 处置 |
| --- | --- |
| `Cargo.toml` `description`（"桌面插件机制整核…"） | 改中立（"插件机制整核（单一 crate 双形态：common / bare）"） |
| `Cargo.toml` 头注 | 活引用同步（feature 名），历史叙述保留 |
| `AGENTS.md` §5.4 | `desktop-host`/`mobile-host` → 新名 |
| 双端 `docs/code-map.md` | 形态段 + feature 名 + 路径同步 |
| 新 ADR（0047） | 登记形态命名（common/bare）+ 改名映射（历史 ADR 不改写） |
| CHANGELOG 双语 | 票 06 收口写（本票不写） |

## 6. 复核判据（票 02 静态自检）

1. `grep -rn 'desktop-host\|mobile-host' packages/bedcode-wasm-core/src` → 0（注释同判）
2. `grep -rn 'desktop-host\|mobile-host' packages/bedcode-wasm-core/Cargo.toml` → 仅 3 行外部引用（登记例外）
3. `grep -rn 'mod desktop\|mod mobile\|runtime::desktop\|runtime::mobile\|test_support::desktop\|test_support::mobile\|mobile_context' packages/bedcode-wasm-core/src` → 0
4. 标识符 `DesktopPluginInfo|MobileWsClientPorts|MobileDiscoveryPorts|MobileReconnectPolicy` → 0
5. `wasi` / `wasip3` / `wasm32` 字样（标识符级）→ 仅注释/文档/外部 crate 名允许（`wasm32-wasip3` 是插件目标三元组，属机制事实——检查是否在标识符中）
6. 双端锁内路径/特性名同步后自洽（`test_support/desktop.rs` → `test_support/common.rs` 等）
