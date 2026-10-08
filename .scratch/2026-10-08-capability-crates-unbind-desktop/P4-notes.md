# P4 票完成记录：server-peer-net / pty-engine 脱绑

> Date: 2026-10-08
> Status: **P4 完成（验证全绿 + 已知基线）**。P5（server-base 常量下沉 + wasm-core/桌面接线收口）可继续。

## 本票改动

### bedcode-server-peer-net（锁 + 段落 cfg）

1. `src/wire.rs`（新）：`PERMISSION_PEER` 自持副本 + 模块文档记录**引擎面保留的 SDK 类型引用**
   （`lib.rs:1401` 的 `impl bedcode_server_base::ports::BusMessageHandler` 用
   `&bedcode_plugin_api::BusMessage`——base 的 trait 签名是 P5 改动面，base 脱绑前必须保留
   plugin-api 依赖以写这个 impl，仅此一个类型）。`wire/drift_lock.rs` 行锁比 `permission.rs`。
2. `plugin_binding.rs`：机制函数（peer_dial 等 19 条，默认可用）+ 绑层段落（DESC / PeerNetModule /
   impl HostModule / HasSelf / MODULE / inventory::submit! / ports_for / bindgen! / impl Host）
   `#[cfg(feature = "desktop-host")]`；`DOMAIN` / `install` / `install_ports` 默认可用（P1 决策同款）。
   `PERMISSION_PEER` import 换 `crate::wire`（机制函数 + 测试经 super::* 可见）。
3. `Cargo.toml`：`desktop-host` feature + host-kit/wasmtime/wit-bindgen/inventory optional；
   **plugin-api 保留非 optional**（BusMessage holdover，注释说明 P5 清）；description 更新。
4. `lib.rs`：`pub mod wire;`。

### bedcode-pty-engine（最复杂域：KeyCombo 大类型自持 + DESC 拆字符串常量）

1. `src/wire.rs`（新）：`TOPIC_NS_SEP` / `owned_topic`（逐字，end marker「互调请求道前缀」+
   「解析 topic 的属主」）、`PTY_EXIT` / `PTY_OUTPUT`（逐字，end marker「生成属主私有事件
   topic」）、`pty_event_topic`（**本地实现**：调 `owned_topic`——语义与 SDK 逐字等价由
   owned_topic 锁 + 形状测试保证，不逐字复制 SDK 的 `super::bus::owned_topic` 调用行）、
   `PERMISSION_PTY_SPAWN` / `PERMISSION_PTY_IO`。
2. `src/wire/key.rs`（新）：**KeyCombo 生产段整段复制**（SDK `wire/key.rs` 1-476 行，含
   文档头 / MOD 常量 / KeyCode / KeyCombo / Serialize/Deserialize impl；测试段不复制）。
   锁用 end marker「// ==================== 单元测试 ====================」。**逐字段豁免
   rustfmt/expect 警告**（SDK 原样）。
3. `wire/drift_lock.rs`：5 个锁（TOPIC_NS_SEP / owned_topic / PTY 事件名 / 权限位行 / KeyCombo
   块）+ `pty_event_topic_shape_matches_sdk_semantics` 行为锁。
4. `plugin_binding.rs`：**DESC 拆纯字符串常量**——`MODULE_NAME` / `MODULE_INTERFACES` /
   `MODULE_PERMISSIONS` / `MODULE_ABI_MIN` 默认可用（pub），`DESC`（cfg）组装引用它们。
   原因：wasm-core `host_api/pty.rs` 测试引 `DESC.{name,interfaces,permissions}`，DESC 是
   host-kit 类型（无 feature 时不存在），拆字符串常量后**无头态也可引用描述符形状**。
   段落 cfg 同前几域。
5. `plugin_binding/{registry,primitives,output}.rs`：imports 换 `crate::wire`。
6. `pty_process.rs`：`KeyCombo::parse` 路径换 `crate::wire::key`。
7. `plugin_binding/tests.rs`：PERMISSION + `use bedcode_plugin_api::host as sdk` → `crate::wire as sdk`
   （语义从「与 SDK 比对」变「与自持 wire 比对」——漂移锁保证 wire == SDK）+ output.rs 测试段同改。
8. `Cargo.toml`：**plugin-api 直接移除**（KeyCombo 自持后零引用——比 peer-net 的 BusMessage
   holdover 更彻底）；features/optional/description。
9. `lib.rs`：`pub mod wire;`。

### wasm-core 接线（P4）

- Cargo.toml：peer-net/pty-engine 依赖加 `features = ["desktop-host"]`；features 转发补两项。
- component.rs：peer / pty 两个强制引用 gate 加 `#[cfg(feature = "desktop-host")]`。
- host_api/pty.rs 测试：`DESC.{name,interfaces,permissions}` → `MODULE_*`（配合 DESC 拆字符串）。
- host_api/tests/pty_wiring.rs：`DESC.permissions` → `MODULE_PERMISSIONS`（**该文件是未接
  mod 声明的孤儿文件，不编译**——修改为防御性一致，实测不在测试列表）。

### 并行会话接线缺口修复（P3 遗留，wasm-core 编译 blocker）

并行会话完成 ws 域脱绑后，wasm-core 两处没跟上（编译红，阻塞一切验证含 CI）：
- `register.rs:123`：`use bedcode_plugin_api::EndpointAuth` → `use bedcode_server_websocket::wire::EndpointAuth`
  （传 ws 域 register 签名）。
- `contributions_test.rs:174/195/199`：断言 ws 端点表的 auth 为 wire 版。

机械类型同步，修复方式唯一；汇报中已点名。

## 验证证据（全实跑）

| 项 | 结果 |
| --- | --- |
| peer-net 无 feature 编译 + tree（无 host-kit/wasmtime/inventory 直接依赖） | ✅ |
| peer-net `cargo test` / `--features desktop-host`（49） | ✅ 全绿 |
| pty-engine 无 feature 编译 + tree | ✅ plugin-api 直接依赖归零 |
| pty-engine `cargo test` / `--features desktop-host`（100，含 5 漂移锁 + 1 行为锁） | ✅ 全绿 |
| wasm-core `cargo check`（默认 desktop-host） | ✅ 3 既有 warning |
| wasm-core `cargo check --no-default-features`（无头态） | ✅（DESC 拆字符串后 pty 面无头可编译） |
| wasm-core `cargo test --lib`（678） | ✅ 677 绿 + 1 既有 perf 红（scratchpad 记录） |
| wasm-core 关键测试：host_module_name_matches_capability_domain_desc / registered_manifest_declared_ws_endpoints / capability_registry_matches_whitelist | ✅ |
| 桌面 src-tauri `cargo check` | ✅ 4 既有 warning |

## 已知项 / 决策记录

1. **peer-net 的 plugin-api 保留**（仅 `BusMessage` 类型，base trait 契约）——P5「server-base
   常量下沉」时随 base 脱下（base 的 `BusMessageHandler` 签名换 base 自持类型后，peer-net 的
   impl 同步换名）。spec §5 门禁 3 对 peer-net 的 plugin-api 项为**记录豁免**（有基座原因）。
2. **pty 的 KeyCombo 逐字段豁免 rustfmt/expect**：复制自 SDK 原样（漂移锁要求逐字），不按
   仓库 rustfmt/clippy 规则改写。
3. **pty_event_topic 不逐字复制**（SDK 实现调 `super::bus::owned_topic`，本地调 `owned_topic`）——
   语义等价由 owned_topic 锁 + 形状测试双重保证；锁只钉常量块。
4. `pty_wiring.rs` 孤儿文件（无 mod 声明、不编译）——既有状态；修改为防御性一致。
5. 并行会话在 P3（ws 域）——本票与其不重叠（peer/pty），wasm-core 为共享文件，编辑基于实时
   重读，两行共存未覆盖；其接线遗留（register.rs 等）由本票修复并点名。