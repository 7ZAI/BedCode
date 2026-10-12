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

---

## 6. 状态：进行中（2026-10-12 会话 A：前置解锁 + 别名删除 + M4）

> **本会话产物未提交**（与 T-04 会话产物同处工作区）。会话基线 = HEAD `8c71aebdc`
> + T-04 工作区改动 + 并行线（fs_auth）工作区改动。

### 6.1 前置解锁：并行线 fs_auth 的**移动半**收口（原「T-05 阻塞」项）

并行线把内核 `fs_auth` 槽收敛为**具体 `Arc<FsAuthChecker>`**（用户裁决 2026-10-11
「wasm-core 机制全部以桌面端为准收敛一套：fs 授权统一到桌面判定链，移动自实现已退役」），
但移动宿主仍传自实现类型 ⇒ 移动 `cargo check --lib` **1 error**（HEAD 即红，非本票引入）。
本会话按该裁决补齐移动半：

- `plugin/manager.rs`：`fs_auth` 字段 / 构造 / `preauthorize_plugin` 的 `check_batch`
  改内核实 `security::fs_auth::{FsAuthChecker, FsOps}`（`new(storage, app_handle, Vec::new())`，
  移动无 first-party 免弹窗概念 ⇒ 归属清单空表）；
- `plugin/commands.rs`：`plugin_fs_auth_respond` 保持前端契约 `(requestId, allowed, remember)`
  三参不变，宿主侧映射为 `FsDecision::{AllowRemember|AllowOnce|Deny}`；
  **6 个路径 / 插件白名单管理命令删除**（统一判定链没有这一层；前端 grep 实证零调用点），
  `lib.rs` 的 `invoke_handler` 注册与启动期「种子插件白名单」步骤同批移除；
- `plugin/host_ports.rs`：`impl FsAuthGate for 移动 FsAuthChecker` 删除（判定链收敛后
  无需端口投影；`FsAuthGate` 已降为纯判据签名，由内核 `FsAuthChecker` 自实现）；
- `plugin/fs_auth.rs`（527 行自实现）**整文件删除**；`plugin.rs` 的 `pub mod fs_auth;` 与
  模块头注同步。

**门禁实证**：移动宿主 `cargo check --lib` **0 error**（至此并行线在途红清零）。

### 6.2 §3 别名收口：`bedcode-wasm-core-mobile` rename 别名删除（已完成）

- `Cargo.toml` ×2（依赖 / dev-dependencies）改**直连** `bedcode-wasm-core`
  （`default-features = false, features = ["bare-host"(, "test-support")]`），
  注释同步（指名票 05 收口 + 回接锁落点）；
- 移动 `src` / `tests` 全部 `bedcode_wasm_core_mobile::` → `bedcode_wasm_core::`（67 处）；
- `tests/retired_mobile_wasm_core_fork_lock.rs` 语义泛化（票 §3 要求）：
  ④ 由「别名必须指向根 crate」改为「**直连**根 crate + 禁 rename 别名键回归」；
  ⑤ 新增「移动 `src`/`tests` 源码不得出现 `bedcode_wasm_core_mobile` 标识符」。

### 6.3 M4：`host_context_registry.rs` 死代码删除（已完成）

桌面 mdns adapter 迁出后（票 02 批次 03）该注册表**已无任何调用点**（全仓 grep：
只剩模块声明与两处历史注释），桌面 mdns.rs 头注也自述「一并退役」。按 M4 处置为
**删除**而非迁移（留一份永不写入的注册表 = 假装配点）：`lib.rs` 声明删除 +
`host_api.rs` / `context.rs` 注释同步。

### 6.4 未完成（M1 / M2 / M3 / M5 / M6 / M7 + 测试面 + 门禁）

**判决性事实（本会话查证，直接决定下一段的做法，勿重复试错）**：

1. **移动 core-8 不能直接改用 `core_binding`**（T-03 Option A 那份「双端唯一一份」）：
   - **权限门不同源**：移动 = `WasmHostContext.granted_permissions`（实例级集，含
     `bus` / `auth` / `notify` 等 **mobile-only 词汇**）；桌面 = `PermissionManager`
     （wasm-core `permission.rs` 的**桌面词表**，`grant_permissions` 会静默过滤表外项）。
     ⇒ 直接切 `core_binding`：storage/fs/db 门在移动侧恒拒（`ctx.permission` 为空），
     `bus` 门则相反——桌面 adapter `check_permission` **恒 true**（桌面总线无权限位），
     移动的 `bus` 位会失效（安全回归）。
   - **插件私有库布局不同**：移动 = `app_data_dir/plugins/<id 去点划>.db`（每插件单文件）；
     内核 `get_or_create_plugin_db` = `plugins/<plugin_id>/plugin.db`（目录 + `plugin.db`）。
     切换 = 既有插件私有库（file-transfer 会话真源）失联 ⇒ 必须带数据迁移，不能顺手做。
   - ⇒ 移动 core-8 实现**随本票整体迁移动宿主**（自报 `HostModule`，行为保持）；切
     `core_binding` 前置 = 「双 SDK 权限词汇并集（B5）+ 私有库布局统一」两项先落地。
2. **导出面与 import 注册解耦**：`core_binding::exports()` / `abi_version()` 只是对
   `Instance` 的导出视图，**不依赖 `core_binding::add_to_linker`** ⇒ 移动壳可只取导出面
   （command / lifecycle / manifest 走单份 core 导出、`abi.version` 走原生探测），
   import 侧全部由移动宿主自报模块供。
3. **`events`（移动词汇，导出，带 `result` 无法原生探测）** 的派发：按 T-03 §9.8 同款
   「机制留内核、契约面上移」，扩展 `GuestEventDispatcher`（复用 `on_message` 承载
   `on-bus-message`；新增 `on_auth_success` / `on_disconnect` / `on_session_created` /
   `on_session_stopped` 四个**带 fail-visible 默认实现**的方法，桌面不覆写）+
   移动宿主 `plugin/guest_events.rs` 实现（用其 `bindings::Plugin`）。
4. **测试面**：`bare/component.rs` 的 fixture e2e（`test_component_roundtrip_and_host_impl` /
   `ws_client_domain_full_loop_with_real_component` /
   `terminal_session_domain_full_loop_with_real_component`，合计约 950 行）与各域文件内
   测试**随域迁移动宿主**（T-04 同款纪律）；`test_support/bare.rs` 的转发面经
   `test-support` feature 供移动宿主 dev-deps 消费（`#[cfg(test)]` 项对宿主不可达）。
5. **M5 `terminal_stream_gateway`**：仍是活代码（移动命令面 + `terminal_stream` adapter +
   `host_impl::purge_for_plugin` 三处消费）——迁移需同时改这三处；M7 `test_support/bare.rs`
   与 M1/M2 的壳迁移同批评估（`WasmRuntime` / `LoadedComponentPlugin` 若整体迁宿主，
   需内核开放 `engine::{new_store, instantiate}` 等 `pub(crate)` 面——T-04 桌面半**未**做
   此步（桌面壳仍在 wasm-core），本票对称处理建议保持「壳留内核、绑定与域上移」）。

**门禁（本会话已跑 / 待跑）**：
- ✅ 移动宿主 `cargo check --lib` 0 error（6.1 收口后 + M4 删除后各复跑一次）；
- ✅ 锁 `retired_mobile_wasm_core_fork_lock` 5 用例实跑全绿；**变异自检**：
  ⑤ 注入 `bedcode_wasm_core_mobile` 到 `src/plugin.rs` → 红（已 `cp` 还原 + sha256 一致）；
  ④ 注入别名键 → **构建级即红**（`cargo` 直接拒绝：直连 + 别名共存 =「depends on crate
  multiple times with different names」；替换式回归则源码全红）——即在可编译状态下无法构造
  别名回归，锁 ④ 是第二道（源码 + Cargo 键同时回退时）才轮到它；
- ⏳ 移动 `cargo test`（含 test-support 面）——留 T-06 全量；
- ⏳ wasm-core `cargo check --lib`（三态）——本会话改动（M4 删除 + 注释）预期零影响，
  未单独复跑（移动 check 已覆盖 `bare-host` 形态编译）；
- ⏳ 3 个 wasm-app 重建（`node scripts/build.js --rust-only`）——**契约零变化，重构建留 T-06**；
- ⏳ `pnpm run check:wit`——WIT 零变化。

---

## 7. 状态：M1/M2/M3/M5/M6/M7 已完成（2026-10-12/13 会话 B）——本票收口

> 会话基线 = HEAD `8c71aebdc` + 会话 A 工作区改动。**产物仍未提交**（同一工作区）。

### 7.1 M2 + M3：12 个 core 域整体归位移动宿主（含各自 `Host` impl）

- 新增 `bedcode-mobile/src-tauri/src/plugin/host_domains/`（14 文件 / 2518 行）：
  `mod.rs`（`host_ctx`/`current_handle` 转发 + `mobile-host-core` 模块描述符 +
  12 interface 注册 + `purge_for_plugin` 聚合）+ 每域「域逻辑 + `impl Host` 同处」：
  `log` / `storage` / `db` / `event` / `http` / `fs` / `config` / `bus` / `peer` /
  `mdns` / `platform` / `ws` / `support`。域逻辑逐字搬自内核
  `manager/runtime/bare/host_impl/*`（只改导入路径；**行为保持**：权限门仍读实例级
  `granted_permissions`，插件私有库仍是 `plugins/<id 去点划>.db`）。
- 内核侧删除：`manager/runtime/bare/host_impl/`（12 域 + `host_impl.rs`）整目录 +
  `runtime.rs` 的 `pub use bare::host_impl`。
- 移动 `Cargo.toml` 新增三个生产依赖：`bedcode-host-api-core` / `bedcode-server-http`
  （原为 dev-dep）/ `bedcode-ws-client-engine`。
- **内核 API 面微调（2 处）**：`WasmHostContext` 的 `plugin_dbs` / `storage` /
  `app_handle` 三字段 `pub(crate)` → `pub`（与既有 `ports` / `granted_permissions` /
  `status_reporter` 同侧先例：移动宿主 adapter 直读）；`test_support::bare` 增
  `SYSTEM_PLUGIN_ID` 转发（域测试断言系统空间纵深守卫）。
- **判决性事实（供 T-06 锁/文档引用）**：移动 core-8 仍**不能**切内核
  `core_binding`（权限门词表 + 私有库布局两处不同源，见 §6.4-1）；`host-terminal-stream`
  的窄转发表（M5）同批迁入宿主。

### 7.2 M1：移动壳去绑定（内核零 WIT 绑定）

`manager/runtime/bare/component.rs` 重写（1830 → 437 行）：
- 删 `bindgen!` + 12 域 `Host` impl + 5 个 `cfg(test)` 替身 + `host_domain_standins`；
- linker 装配 = **宿主自报能力模块**（`ModuleRegistry::collected()` → 白名单双向校验
  → `install_all`）+ WASI（`wasi::add_to_linker`）；内核里 `#[cfg(test)]` 分支消失
  （与桌面壳同形）；
- 导出调用骨架改走 `core_binding`（`command` / `lifecycle` / `manifest`，
  双端同一份绑定）+ 原生探测（`abi.version`）+ 宿主注入的 `GuestEventDispatcher`
  （`events.*`）；
- `bare.rs` 增 `guest_events`（缺省 `UnwiredGuestEvents`，fail-visible）+ `set_guest_events`
  （生产在 `PluginManager::init_wasm_runtime_with` 注入）；`get_manifest` 提 `pub`。

### 7.3 事件派发契约面扩展（§6.4-3 落地）

- 内核 `guest_events.rs`：新增 `on_auth_success` / `on_disconnect` /
  `on_session_created` / `on_session_stopped` 四个**带 fail-visible 默认实现**的方法
  （桌面不覆写；误接即显性报错）；`on_message` 复用承载移动 `on-bus-message`
  （移动 WIT 无 sender 参数，实现方不消费）。
- 移动宿主新增 `plugin/guest_events.rs`（`MobileGuestEvents`，用宿主自有绑定实现
  5 个移动导出；`on_process_done` 显性报「移动 world 无此导出」）。

### 7.4 M5：终端输出窄转发表迁移动宿主

`packages/bedcode-wasm-core/src/terminal_stream_gateway.rs`（155 行）合并进
`bedcode-mobile/src-tauri/src/plugin/terminal_stream.rs`（网关 + WIT layer 同文件，
78 → 232 行）；消费点改 `crate::plugin::terminal_stream::terminal_stream_gateway()`
（`commands.rs` ×2 / `host_domains/mod.rs`）；内核 `lib.rs` 声明删除、`plugin.rs` 垫片移除。

### 7.5 M6：`host_api/{ports,sql_guard}` 处置

- `ports.rs`（`HostEnginePorts`）**留内核**：它是统一上下文 `WasmHostContext.ports` 的
  端口词汇（双端常编译），移动宿主只实现之——迁出会让内核类型无法指名自己的端口。
- `sql_guard.rs`（11 行垫片）**退役删除**（非迁移）：主库收归后只剩一条
  `column_to_json` re-export，**两侧均无消费者**（域实现直连共享核）；移动 `plugin.rs`
  的 `wasm_host` 垫片同批删除（零调用点）。

### 7.6 M7：测试基建转发面（内核 → 宿主测试）

`test_support/bare.rs` 增 4 个转发：`plugin_abi_version` / `plugin_raw_store` /
`build_component_linker` / `probe_wasi_p2_reregistration`（宿主 e2e 断言 ABI 协商、
燃料、linker 装配契约用；生产 API 零放宽）。

### 7.7 测试面迁移（12 用例全部实跑绿）

- `tests/component_e2e.rs`（10）：roundtrip（含 host-storage 直测 + ABI + 燃料）/
  业务方法全链 / 高 ABI 拒绝 / 燃料 trap / limiter / SDK 宏产物 / AOT 缓存 /
  linker 重复注册 / WASI 默认注册 / WASI 可关；
- `tests/ws_client_e2e.rs`（1）/ `tests/terminal_session_e2e.rs`（1）：两个域全链路闭环；
- 内核侧不再留任何夹具实例化用例（原 `bare/component.rs::tests` 908 行整批迁出）。

### 7.8 锁（重写 + 路径同步 + 变异自检 4/4）

- `mobile_wasm_core_purity_lock.rs` **按新终态重写（6 判据）**：① 移动独有域不回流
  ② core 12 域绑定/实现不回流（唯一合法落点 `core_binding.rs`；移动壳禁 `bindgen!` 与
  `bedcode::plugin::`）+ 正向钉住宿主绑定层 ③ 共享实现核在场且宿主 adapter 真转发
  ④ 移动壳行数上限（单文件 600 / 合计 800，当前 437+195=632）⑤ 移动独有机制与退役垫片
  不回流（网关住宿主 / `sql_guard` 不复活）⑥ 退役 http 引擎不复活。
  变异自检 **4/4**（core 域 impl 回流 / 移动壳 `bindgen!` / 移动独有域 impl 回流 /
  网关文件回归——逐条注入→红→`cp` 还原→绿）。
- 三把引用旧路径的锁同步（`mobile_host_websocket_client_domain_lock` 的 ws 实现路径、
  `retired_mobile_session_control_face_lock` / `retired_mobile_terminal_link_lock` 的
  保留面路径）——其中 ws 锁的路径在会话前**已是陈旧值**（扫描目标不存在即红，属在途基线）。

### 7.9 门禁实证（实跑）

- 移动宿主 `cargo check --lib` **0 error**；移动 `cargo test`（全量含 test-support 面）
  **全绿**（239 lib + 全部集成目标；新增 component_e2e 10 / ws 1 / terminal_session 1）；
- wasm-core `cargo test --lib`（common-host）**570/0**；三态 `cargo check --lib`
  （default / `--no-default-features` / bare-host）**0 error**；
- 桌面 `cargo check --lib` **0 error**（本票内核改动对桌面零破坏）；
- 3 个移动 wasm-app 重建：`pnpm run plugins:build` **3/3 成功**（产物 10-13 01:04~01:05
  落 `src-tauri/resources/plugins/mobile/`；`plugin.json` 的 `wasm` 指向新产物）；
- 文档：移动 `docs/code-map.md` 事实同步（终端链路 / ws 适配器 / bare 面三层 /
  宿主 `host_domains`+`guest_events` / 绑定落点 / 典型任务入口 / peer 域 / 锁索引）。

### 7.10 遗留（不计入本票门禁；建议 T-06 或专项收）

1. **feature / 依赖面收敛（未做）**：`common-host` / `bare-host` 退役、`compile_error!`
   守卫删除、双 SDK 类型词汇合一。**证据**：① bare 形态 `cargo check --lib` 从迁移前
   9-10 warning 升到 ~102（桌面向 `host_api` 机制在移动形态已零消费者 ⇒ dead_code）；
   ② 内核源码对 `bedcode-discovery-engine` / `bedcode-ws-client-engine` **零引用**
   （`rg` 实证），两者仍留在 `bare-host` feature 列表（已在 Cargo.toml 注明为待摘过渡项）；
   ③ `bedcode-plugin-api-mobile` 仍被 `bare/component.rs` 消费（`BusMessage` 形参 +
   `abi::ABI_VERSION` 比较）——桌面壳同样引桌面 SDK，属同一收敛项。
2. **bare 形态 `cargo test --lib` 不可编译**（HEAD 即如此）：`host_api::{tests,
   sqlite_scaffold,grant_permissions}` 等是 common 专属 cfg(test) 项 ⇒ 移动形态验证面
   仍是「双形态 `cargo check` + 移动端 `cargo test`」。内核侧新增的 WASI linker 断言
   因此落在移动宿主 `component_e2e`（经 `test_support::build_component_linker` 转发）。
3. **`on_message_binary` 平名探测缺陷（迁移中发现，未修）**：`bare/component.rs::verify_abi`
   沿用迁入前的 `"bedcode:plugin/events-binary#on-message-binary"` 平名串，组件接口导出是
   嵌套实例形态 ⇒ 该探测恒 `None`（桌面侧早已改 ItemName 点号并实证）。修它 = 行为变更
   （二进制消息开始真的投进插件），按「迁移保持行为」留给专项；已在代码注释点名。
4. **桌面 app 重建 + `session_e2e` 12 红**（产物过期，票 04 §6.6-4）与 T-06 全量门禁
   （两端 `pnpm run test:run`、根 eslint、`check:wit`、`cross-end-tests`）——T-06 收口。