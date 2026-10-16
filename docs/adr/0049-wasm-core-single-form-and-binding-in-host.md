# ADR 0049：wasm-core 单形态（机制零差异）+ 绑定层归位宿主 + 契约差异经 SDK compose 组合

- 状态：**accepted**（2026-10-13；票 T-01–T-06 全落地，票 06 收口完成——实施记录见文末）
- 日期：2026-10-13（裁决发生于 2026-10-11 / 10-12，收口于 10-13）
- spec：`.scratch/2026-10-11-wasm-core-single-form/spec.md`（票序与门禁真源；逐票记录同目录 `ticket-0*.md`）
- 相关：ADR 0045（单一 crate + WIT 分片组合，本 ADR 修订其 D6 与「双形态」表述）、ADR 0047（形态命名中立化 + 移动独有域落宿主，本 ADR 修订其 D1 的「形态」含义与门禁第 7 项）、ADR 0018（移动契约独立——**维持不撤销**）、ADR 0019（wasmtime 双端锁版）、ADR 0022（宿主/插件边界，事件派发契约面上移的依据）、ADR 0034 L3.b（worker wasm 双端同源 ⇒ WASI 面住核心的依据）、ADR 0035（能力域 crate 化 + 自动装配）、ADR 0037（整核抽出）
- 用户指令（原文摘录）：
  - 「从功能 / 机制的角度出发，wasm-core 就应该是统一的，不应该有差异代码；差异的部分全部迁移或者移除。」
  - 「WIT 通过在宿主或者 SDK 中组合来实现整体对移动端或者桌面端，而不是在 wasm-core 中实现差异统一。」
  - 「不；双端都可以开启，默认开启；wasip2 p3 就放在核心——为了以后 worker wasm 使用。或者通过 config 让外部去配置开启或者关闭，默认全部开启。」

## 背景

1. **单 crate 但仍是双形态**（ADR 0045 + 0047 的现状）：`packages/bedcode-wasm-core` 已是一份，但内部分 `common-host`（桌面）/ `bare-host`（移动）两套装配树，各挂一套全 world `bindgen!`（桌面绑 `bedcode-desktop/packages/plugin-sdk-desktop/rust/wit`，移动绑 `bedcode-mobile/.../rust/wit`）与各 12 个 `impl … Host`。机制面虽已大幅收敛（`engine.rs` 统一、`WasmPluginState` 统一、上下文合一），**但绑定与端事实仍在内核**——内核因此知道「有桌面这回事、有移动这回事」，`lib.rs` 需要 `compile_error!` 互斥守卫防两套 bindgen 同名 `bedcode` 模块冲突。
2. **绑定归位在桌面已有先例、移动没有**（实测不对称）：桌面自「纯净性收口票 02 批次 02–06」起把 pty / mdns / peer / ws / http / auth / crypto / task / process / app / timer / connection / fs / platform / events / api_call 逐域迁到宿主 `src-tauri/src/plugin/*.rs`（自带 bindgen + `HostModule` 自报）；移动侧当时仍把 16 域 `host_impl/` 与 cap-mobile 绑定整面留在内核。两端在同一 crate 内载荷严重不均。
3. **WASI 曾是形态差异轴**：桌面装 wasip3（preopen + fs_auth 过滤），移动不装（`wasm32-unknown-unknown`）。这是「内核知道有端之分」的最后一块硬事实。
4. **原 spec 方向（union world）被否决**：初版 spec 打算把 cap-desktop + cap-mobile 并进同一 package、做 union world 并合并 ABI。这会让移动跟演桌面的 ABI 破坏性变更，与 ADR 0018 直接冲突。

## 决策

### D1 · 单形态：机制零分叉

wasm-core 内**只有一份机制**，双端零机制分叉代码：Engine 构建与资源限制、Store 装配、组件加载 / AOT / 实例化 / 导出调用、事件循环机制、安全授权、存储、db 机制、消息总线、任务引擎、监控。`engine.rs` 是「组件引擎机制唯一一份」的落点。

**判定口径**：任何一段代码若「桌面这样写、移动那样写」，它就不属于机制，应整体迁出（到共享 crate 或端宿主），而不是加 `cfg`。

### D2 · 绑定层归位宿主

**wasm-core 内唯一合法的 WIT 绑定是 `manager/runtime/core_binding.rs`**——它绑契约真源 `packages/bedcode-wasm-core/wit/core.wit` 的 `world core`（双端全等的 17 个 interface；**2026-10-13 计数口径随 ADR 0052 更新为 12 个机制 interface = 8 import + 4 export**，业务能力交集改住能力域 crate 分片）。它在双端都编译、双端都是同一份，这是「交集 ≠ 差异」。

其余全部归位：

| 载荷 | 原落点 | 终态落点 |
| --- | --- | --- |
| 桌面全 world `bindgen!` + 12 域 `Host` impl + `LoadedWasmPlugin` | `manager/runtime/common/component.rs`（1412 行） | 桌面宿主 `src-tauri/src/plugin/{bindings.rs, <domain>.rs}` |
| 桌面 4 个 `#[cfg(test)]` 替身 + `TestGuestEvents` + `fixture_build` 转出 | 内核 | 桌面宿主 `src-tauri/tests/` 与 `plugin/guest_events.rs` |
| 移动全 world `bindgen!` + 12 域 `Host` impl + `LoadedComponentPlugin` | `manager/runtime/bare/component.rs`（1781 行） | 移动宿主 `src-tauri/src/plugin/{bindings.rs, host_domains/**}` |
| 移动 16 域 `host_impl/` | 内核 | 移动宿主 `plugin/host_domains/`（域逻辑与 `Host` impl 同处） |
| 移动独有域（notify / terminal_stream / fs_mobile / connection.primary-target / auth / 移动 events）+ 终端窄转发表网关 | 内核 `host_api/` + `terminal_stream_gateway.rs` | 移动宿主 `plugin/*`（ADR 0047 D5） |

**形态壳保留什么**：Engine / Linker / Store 生命周期 + 资源限制接线 + 导出调用骨架（core 交集导出走 `core_binding`、`abi.version` 走原生 `TypedFunc` 探测、带 `result` 的导出由宿主注入的 `GuestEventDispatcher` 派发）。**壳里零 `bindgen!`、零 `impl … Host`**。

### D3 · 事件派发：机制留内核、契约面上移

事件循环（订阅、路由、owner 定向）是机制；`events` 导出的**调用契约**两端词汇完全不同，且带 `result` 返回的导出无法用原生 `TypedFunc` 探测 ⇒ `manager/runtime/guest_events.rs` 定义 `GuestEventDispatcher` trait，由各端宿主实现并在装配期注入；缺省 `UnwiredGuestEvents` 派发即 fail-visible。符合 ADR 0022（宿主只暴露原语，不解释产品事件）。

### D4 · WASI 装配面住内核，配置驱动（用户裁决 3）

`manager/runtime/wasi.rs`（p2/p3 linker 注册 + preopen / fs_auth 过滤链）**双端同源一份、双端默认全开**，开 / 关是**运行期配置** `CoreConfig::wasi`（`enabled` / `p2` / `p3`）。

**理由**：将来 worker 类别插件（ADR 0034 L3.b）双端都要跑 wasm，装接线必须已经双端同源，不能再挂在桌面形态上。WASI 因此**从形态差异轴降级为共享机制**——本 ADR 据此作废 spec §2 事实表的「WASI 有 / 无」行，也作废原锁语义里的「禁 WASI 词汇」。

### D5 · 不做 union world、不合并 ABI（否决原 spec 方向）

契约端差异 = 各端 SDK `compose.json` 各自组合 cap 分片 + 宿主侧各自注册实现。cap-desktop / cap-mobile 永不同时进 wasm-core 的绑定层；同名 interface 已按角色改名的现状保留（不冲突即不改）。

**ABI 各端独立演进**：桌面 37 / 移动 20 维持。**ADR 0018 维持不撤销。**

### D6 · 上下文合一 + 命名中立

`context::WasmHostContext` 与 `bare_context::WasmHostContext` 合成一个中立类型；`host_api/sql_guard.rs`（零消费者）与 `host_context_registry.rs`（无调用点）退役删除而非留垫片。`ports.rs`（`HostEnginePorts`）**留内核**——它是统一上下文的端口词汇，双端常编译，移动宿主只实现之。

### D7 · 移动 core-8 不切 `core_binding`（显式记录的不合并项）

移动的 12 个 core 域**整体迁移动宿主**，行为保持，**不**改用内核那份 `core_binding`。两处不同源是硬理由：

1. **权限门不同源**：移动读实例级 `granted_permissions`（含 `bus` / `auth` / `notify` 等 mobile-only 词汇），桌面用 `PermissionManager`（桌面词表，表外静默过滤，且桌面 bus adapter `check_permission` 恒 true）。直接切会让移动侧 storage/fs/db 恒拒、而移动的 `bus` 权限位失效（安全回归）。
2. **插件私有库布局不同**：移动 `app_data_dir/plugins/<id 去点划>.db`（每插件单文件）vs 内核 `plugins/<plugin_id>/plugin.db`（目录 + 文件）。切换 = 既有插件私有库（file-transfer 会话真源）失联，须带数据迁移。

切 `core_binding` 的前置 = 「双 SDK 权限词汇并集」+「私有库布局统一」两项先落地。

### D8 · 契约与锁的收口口径

- **契约零变更**：双端 `compose.json` 保持现状；`pnpm run check:wit` 通过（生成物逐字一致）；移动 3 个 wasm-app 的 SDK 引用不动、契约面零变化（只需重构建产物）。
- **锁**：移动侧两把锁按新终态重写（`retired_mobile_wasm_core_fork_lock` 泛化为「禁任何第二份 wasm-core 织体」+ 第 5 判据「同名新目录」；`mobile_wasm_core_purity_lock` 重写为七判据，新增判据 7「端词汇全集」）；wasm-core 内域锁**保留但核对**——`desktop_binding_surface_must_not_return_to_wasm_core` 判据从「桌面壳」扩到「双端四壳」并**去端名化**为 `binding_surface_must_not_return_to_wasm_core`（票 04 的遗留措辞违反 ADR 0047 D2）。
- **登记表**：`crate_boundary_lock.rs` 的 `SPLIT_CRATES` **无需新增**——绑定上移没有产生新 crate（core 交集留内核，移动域进移动宿主已有的 `src-tauri`，没有「交集绑定独立 crate」这一步）。

## 代价与风险

| 项 | 说明 | 处置 |
| --- | --- | --- |
| 内核失去「一键可用」性 | 第三方宿主想复用 wasm-core，必须自己写 bindgen + `Host` impl + linker 装配 | 有意接受：绑定层本就携带端契约事实，不是机制。机制复用（Engine/Store/安全/存储）零成本 |
| 两端各养一份绑定层 | 同名 interface 在两个宿主各实现一份（不同 crate、不同类型） | host-kit `ModuleRegistry` + `inventory` + `verify_whitelist` 双向校验兜底；重复注册装配期 `defined twice` 即红 |
| 移动 core-8 与桌面 core-8 行为分叉 | 两份实现，机制修复要两处同步 | D7 已记录为显式不合并项（权限词表 + 私有库布局两处不同源）；同步税在机制面收敛、绑定面各自演化 |
| 移动插件私有库布局特殊 | 无头测试读私有库必败；切 `core_binding` = 数据迁移 | T-05 已按「保持行为」处置；迁移随 D7 前置一并做，不顺手 |
| `bare-host` 形态 `cargo test --lib` 不可编译 | `host_api::{tests, sqlite_scaffold, grant_permissions}` 是 `common-host` 专属 `cfg(test)` 项（HEAD 即如此） | 移动形态验证面 = 三态 `cargo check --lib` + 移动端 `cargo test`；内核 WASI linker 断言经 `test_support` 转发面落在移动宿主集成测试 |
| 形态 feature 尚未退役 | `common-host` / `bare-host` 与 `compile_error!` 守卫仍在，双 SDK 类型词汇未合一 | **已声明遗留**（见「遗留项」），本 ADR 只锁定不修；`compile_error!` 的判据注释已改写为「形态分叉并存」而非「两套 bindgen 冲突」 |

## 验证门禁（票 06 统一执行）

1. **契约零漂移**：`pnpm run check:wit` 通过（双端 `core.wit` + 各 cap + `bedcode.wit` 生成物逐字一致）。
2. **锁变异自检**：新增 / 改写的锁逐把注入违背终态的形态 → 红 → `cp` 还原 + 核 sha256 → 绿。
3. **全量回归**：wasm-core `cargo test --lib`、桌面 `cargo test`、移动 `cargo test`、两端 `pnpm run test:run`、根 `pnpm exec eslint .`（0 error）、`cargo fmt` / `cargo clippy` 自查。
4. **wasm32 真门禁**：3 个移动 wasm-app `node scripts/build.js --rust-only` 重建（native 绿 ≠ 可交付）。
5. `cross-end-tests`（协议面零变化，按用户指示）。

## 遗留项（本 ADR 显式不修，明列不隐藏）

1. ~~**形态 feature 与双 SDK 词汇收敛未做**~~ **已收口（2026-10-13 runtime-single-form，`.scratch/2026-10-13-runtime-single-form/spec.md`）**：`common-host` / `bare-host` 退役（`desktop-assembly` 收桌面专属装配面）、`compile_error!` 守卫删除、移动 SDK 词汇退出 wasm-core（ABI 规格经 `AbiPolicy` 宿主注入、`BusMessage` 由移动宿主拆分走统一 `on_message`）。桌面 `bedcode-plugin-api` 仍在 wasm-core（`desktop-assembly`：Rust 插件体系 / intercall 词汇）——它是端专属装配面依赖而非形态依赖。
2. ~~**移动 core-8 不切 `core_binding`**（D7）~~ **已收口（2026-10-13 runtime-single-form）**：移动 8 个双端全等域（storage / log / config / plugin-database / events / fs / bus / platform）删除移动实现、改走内核 `core_binding` 单份——权限门收敛统一 `PermissionManager`（激活授予进统一词表）、插件私有库布局统一内核（研发起步期无用户数据，按 AGENTS §0 无迁移直接切）；移动 host_domains 只留端差异 4 域（http / peer / mdns / ws）。
3. **`on_message_binary` 平名探测缺陷**：`bare/component.rs::verify_abi` 沿用 `"bedcode:plugin/events-binary#on-message-binary"` 平名串，组件接口导出是嵌套实例形态 ⇒ 探测恒 `None`。修它 = 行为变更（二进制消息开始真的投进插件），留给专项；已在代码注释点名。
4. **桌面 app 产物过期**（`host-connection` → `host-connection-registry`）致 `session_e2e` 红，票 06 全量门禁时重建覆盖。

## Out of scope

- **契约合并 / union world / 单 SDK 化**（否决方向，D5）。
- **ADR 0018 撤销**（维持不撤销）。
- **wasmtime / wit-bindgen 版本升级**（ADR 0019 双端锁版）。
- **feature 面收敛与双 SDK 词汇合一**（遗留项 1）。

## 实施记录（2026-10-11 → 2026-10-13，票 T-01–T-06）

| 票 | 内容 |
| --- | --- |
| T-01 | 基线收口（认领在途提交、工作区归零、双端 `cargo check` 确证基线绿） |
| T-02 | 机制侧去分叉（M1–M13：Engine 统一、上下文合一、`fs_auth` 槽收敛具体 `FsAuthChecker`、`sql_guard` 退役） |
| T-03 | **交集绑定层定稿 = Option A**：core 绑定单份（`core_binding.rs` 绑 `wit/core.wit` 的 `world core`）+ 导出调用面单份 + guest 事件派发契约面上移 + `abi.version` 走原生 `TypedFunc`（`ItemName` 点号 `bedcode:plugin/abi.version`） |
| T-04 | 桌面绑定归位：全 world `bindgen!` + 4 个 `cfg(test)` 替身 + `TestGuestEvents` + fixture e2e（10 用例迁宿主 `src-tauri/tests/`）+ 锁 `binding_surface_must_not_return_to_wasm_core` |
| T-05 | 移动绑定归位：12 个 core 域整体迁 `plugin/host_domains/`（14 文件）+ 移动壳去绑定（1830 → 437 行）+ 事件派发契约面扩 5 个方法 + 终端网关 + `test_support` 转发面 + 12 个夹具 e2e 迁宿主 + rename 别名删除 + 6 个白名单命令退役 |
| T-06 | 契约 / 文档 / 锁 / ADR 收口（本文件 + 锁重写 + 双端 code-map + AGENTS §4/§5.4 + CHANGELOG 双语 + 全量门禁） |

**踩坑与判决性事实**（后续维护必读）：

1. **`bindgen!` 的 `path` 基准 = `CARGO_MANIFEST_DIR`**（不是工作区根）。
2. **导出视图与 import 注册可解耦**：`core_binding::{exports, abi_version}` 只建 `Instance` 视图，不依赖 `add_to_linker` ⇒ 形态壳可只取导出面。
3. **返回 `result` 的导出无法用原生 `TypedFunc`**（`Result` 不实现 `ComponentNamedList`）⇒ 只能经派发器 trait。
4. **一份契约可多次 bindgen**：同模块内两次才冲突，不同模块各生成各的 `bedcode` 模块不冲突。
5. **`wasm_core` 内核测试二进制与宿主集成测试链的是不同的 lib**（后者不带 `cfg(test)`）⇒ 内核 `#[cfg(test)]` 项对宿主不可达，需常编译转发面（`test_support`）。
6. **形态专属 crate 内单测易腐**：`bare-host` 下 `cargo test --lib` HEAD 即不可编译（见「代价与风险」）。

## 收口补录（2026-10-14）

- **端事实零内核分支**：`host-platform.platform` 与 `host-config.network.port` 的
  `desktop-assembly` 编译期二选一（含 `"desktop"`/`"mobile"` 字面量与
  `ServerSupervisor` 直读）收口为 `HostFactsPort` 端口（`WasmHostContext::set_host_facts`
  注入）：桌面实现 `bedcode-desktop/src-tauri/src/plugin/host_facts.rs`（`PluginHost::new`
  末参，先于任何 `activate()`），移动实现 `bedcode-mobile/src-tauri/src/plugin/host_ports.rs`。
  wasm-core 生产段不再依赖 `bedcode-server-core`（降 dev-dependency）；桌面
  `crate_boundary_lock` 对应生产必填边摘除。D1「机制零分叉」至此覆盖到
  WIT 可见端值面。
- **测试面口径**：`src/test_support` **保留**桌面 / 移动两个消费面（运行时壳与
  core 绑定早为单份；合并两侧夹具会造超集）；`TEST_PLUGIN_ID` / `plugin_raw_store`
  上提父模块单点定义。`.scratch/2026-10-13-runtime-single-form/spec.md` 第 8 条已按实修正。
