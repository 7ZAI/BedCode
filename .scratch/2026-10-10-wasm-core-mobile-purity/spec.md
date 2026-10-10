# wasm-core 形态中立化 + mobile 面纯净性收口（域逻辑进共享 crate，壳留各端）

> Status: **票 01–06 已实施（2026-10-10）**——票 05/06 记录见 `ticket-05-domain-sinking.md` /
> `ticket-06-closure.md`（锁 / 文档 / 统一验证结果）；ADR 0047 实施记录已回填。（票 01–06 见 §4）
> Date: 2026-10-10（同日追加 D6 命名中立化；形态命名已裁决 = `common` / `bare`；票 04 机制形态已裁决 = 路径 X，用户「按推荐执行」）
> 用户指令（原文）：
> ① 「`packages/bedcode-wasm-core/src/manager/runtime/mobile` 这个为什么放入 core 里面 这样不符合 wasm-core 保持纯粹的 wasm-core 机制的原则 引入了额外的能力 不应该」
> ② 「不能和桌面端尽量对齐 使用同一个 crate吗？」
> ③ 「写 spec 文档后再开新会话实施」
> ④ 「同时 wasm-core 中不再有任何 mobile 或者 desktop 的语义 现在存在的 desktop 请改名 避免引起误解」
> ⑤ 「使用common 好了 不要使用wasi」——形态名最终裁决（`common` / `bare`；禁端名与 `wasi` 字样进标识符，见 §2 命名方案）
> ⑥ 「移动端特殊的能力 留在移动端宿主侧 不要放入wasm core 中」——C 类移动独有域**整体（WIT 壳 + 逻辑 + 端口）落移动宿主**，wasm-core 不留移动专属 interface 的 `impl Host`（见 §1.6 核实、§2 D2′/D3、票 04）
> ⑦ 「同时写入 在完成所有ticket的代码修改 后再统一进行 编译验证 单元测试等」——**验证时机后移**：票内不跑编译 / 测试，统一在票 06 一次性执行（见 §5、§2 D5、§6 R11）
> 前置：ADR 0018（移动契约独立）/ 0035（能力域 crate 化 + 端口抽象）/ 0042（mDNS 引擎共享）/ 0045（单一 crate 双形态）/ 0022（宿主/插件边界）；
> `.scratch/2026-10-09-wasm-core-single-crate/`（票 02 批次 03–06 = 桌面侧同名改造先例）、
> `.scratch/2026-10-10-peer-capability-consolidation/`（本会话：peer 引擎 + 移动适配层已下沉）

---

## 0. 一句话目标

1. **命名中立化**：wasm-core 内**不再出现 mobile / desktop 语义载体**（模块、feature、类型、锁名、包描述）——双形态取中立形态名 **`common` / `bare`**（禁端名，亦禁 `wasi` / `wasip3` 等机制名进标识符；机制差只写进模块头注与文档）。
2. **纯净性收口**：移动形态残留在 wasm-core 的逐域 WIT 绑定逻辑（`manager/runtime/mobile/host_impl/*` → 改名后 `runtime/bare/host_impl/*`，15 域 ≈3.3k 行）按桌面已完成的同名改造收口——**域逻辑进共享 crate（WIT-无关函数 + 端口），各端只留壳**。
3. **移动独有能力落移动宿主**（用户指令⑥）：移动专属 interface（`host-notify` / `host-terminal-stream` / `host-fs-mobile` / `host-connection.primary-target` / `host-auth` / 移动专属 `host-events`）的 WIT 壳与逻辑一律在 `bedcode-mobile/src-tauri/src/plugin/*`；wasm-core 不留其 `impl Host`，只保留双端共有机制域 + 装配机制。

**不变量（硬）**：WIT / ABI / SDK / 插件 wire 形状 / 权限位**零变更** ⇒ 插件产物无需重建。

## 1. 现状实测（2026-10-10，证据可复跑）

### 1.1 双形态目录（`packages/bedcode-wasm-core/src/manager/runtime/`）

| 形态（现名 → 新名） | 文件 |
| --- | --- |
| desktop → **`common`** | `desktop.rs`（57.6 KB）+ `desktop/{component.rs, fixture_build.rs, fixture_target.rs, tests}` —— **无 `host_impl/`** |
| mobile → **`bare`** | `mobile.rs`（20.8 KB）+ `mobile/component.rs`（1843）+ `mobile/host_impl.rs`（64）+ `mobile/host_impl/` **15 域 + support**：peer 510 / fs 365 / ws 333 / mdns 301 / notify 291 / db 282 / bus 245 / http 210 / storage 181 / auth 127 / terminal_stream 122 / connection 109 / config 60 / platform 48 / event 29 / support 33 |

**成因**：ADR 0045 D6 迁 fork 时按当时桌面形态搬入；桌面随后走「纯净性收口票 02 批次 03–06」把逐域 WIT impl 迁到宿主 adapter
（`bedcode-desktop/src-tauri/src/plugin/{fs,platform,events,pty,peer,mdns,http,ws,crypto,auth,task,process,app,timer,connection,api_call}.rs`：
自带 `bindgen!` + `expect_host_module!` + `install` 装配自报 + 强制引用行 + `plugin/bindings.rs` 白名单双向校验），**移动侧无对应票** ⇒ 结构不对称。

### 1.2 移动装配机制（与桌面不同，决定实施形态）

- 移动 `mobile/component.rs:423–457` 的 `build_component_linker` 是**硬编码 `add_to_linker` 清单**（`428–448`，17 interface：log/storage/plugin_database/events/notify/http/fs/fs_mobile/config/bus/peer/mdns/platform/websocket/terminal_stream/connection/auth）——**不走 host-kit 的 `ModuleRegistry` / `install_all` / `verify_whitelist` / inventory 自报**，且**无注册回调 / 扩展钩子 / cfg 分支**（宿主无法在不改 wasm-core 的前提下注入自己的 `impl Host`）。
- **17 个 `impl Host for WasmPluginState` 全在 `mobile/component.rs` 内**（log 39–61 / storage 63–75 / plugin_database 77–97 / events 102–107 / notify 110–130 / http 132–136 / fs 138–162 / fs_mobile 166–185 / config 187–191 / bus 193–215 / peer 217–303 / mdns 307–327 / platform 329–337 / websocket 345–365 / terminal_stream 373–377 / connection 384–388 / auth 395–415），体为薄映射到 `host_impl/*`。
- `mobile/host_impl.rs` 把各域函数 `pub(crate) use` 到 host_impl 层，`component.rs` 的 `impl Host` 直接调用。

### 1.3 「同一个 crate 复用」可行性（三证据）

| 证据 | 事实 |
| --- | --- |
| 域函数已 WIT-无关 | `packages/bedcode-server-peer-net/src/plugin_binding.rs:148–524` = **19 个 `pub fn peer_*(ports: &Arc<dyn PeerPorts>, plugin_id, …) -> Result<_, String>`，无 cfg 门控**；`524` 行起才是 `#[cfg(feature = "desktop-host")]` 的绑定/装配面。端口 trait 与 `plugin_binding::ports::{install_ports, ports, HEADLESS_UNAVAILABLE}` 在 `plugin_binding/ports.rs`（无门控） |
| 接口双端全等 | `packages/bedcode-wasm-core/wit/core.wit` 的 `host-peer` = **19 函数**（dial-peer / close / send-files / respond-consent / respond-transfer / set-receive-policy / pause|resume-transfer / set|list-shared-roots / browse-directory / pull-files / set-download-dir / start|stop-node / list|revoke-trusted / active-transfers / collect-outgoing），与桌面逐字一致（票 04 已证核心 17 全等；**移动 code-map 旧文「13 原语」为错，已修**） |
| 无 Cargo 环 | 能力域 crate 只依赖 `bedcode-host-kit`（`desktop-host` 面）与 `bedcode-server-base`，不依赖 wasm-core ⇒ 移动可引；反向（crate 命名移动 state）不可行 ⇒ **壳必须留能命名本端 state 的地方** |

### 1.4 域分类（**2026-10-10 逐域核验后修订**；明细见 `ticket-01-domains.md`）

| 类 | 域 | 处置 |
| --- | --- | --- |
| **A 复用共享 crate 域逻辑** | `peer`（域逻辑最重：进程级 `PEER_HANDLES` 句柄表 + `with_auto_redial` 自动重拨）、`mdns`、`websocket`(客户端) | mdns/ws 已是「5 原语 + 端口适配器」薄层（引擎在 `bedcode-discovery-engine` / `bedcode-ws-client-engine`，票 05 收窄的是适配器装配面）；**peer 是必做项**（510 行 vs crate 现成 19 函数） |
| **A′ 含域逻辑（不能只收窄）** | `fs`（`fs_auth` 双检链 + Android/MediaStore/SAF 分支 + 下载目录守卫，365 行）、`db`（`plugin_dbs` 连接表缓存与生命周期，282 行）、`http`（流式 streamId/streamEvent 编排 + egress 前置，210 行） | 逻辑先下沉 `bedcode-host-api-core` 共享实现，或随域迁宿主（票 05 逐域定）；不是「一行转发」能收口的域 |
| **B 机制真源在 crate `bedcode-host-api-core`** | storage / db / bus / event(s) / http / fs / platform … | 注意：wasm-core 内 `host_api/{storage,database,bus,events,config,fs,platform}.rs` 是 **`cfg(desktop-host)` 桌面 adapter**（`host_api.rs:58–80`），移动**不共享**它们 ⇒ 「纯净性」= 移动 adapter 只留转发，缺共享实现的需先抽到 `host-api-core` |
| **C 移动独有接口（桌面 crate 无对应物）** | `host-notify`(5) / `host-terminal-stream`(1) / `host-fs-mobile`(2，在 `fs.rs:230–365`，**无独立文件**) / `host-connection.primary-target`(1) / `host-auth`(5) / `events`(5，**export 面**：`component.rs:788–896` + 移动 SDK `wasm.rs:240–320` + 组件测试插件三处) | **整体落移动宿主**（用户指令⑥）：宿主 adapter = 自带 bindgen + `impl Host` + 域逻辑 + 宿主端口；wasm-core **不留壳**（见 §1.6、票 04） |
| **例外 / 纠正** | `log`（无 `host_impl/log.rs`：`component.rs:39–61` 直调 `bedcode-host-api-core::log`，非移动独有）；`config`（移动键集独有 + Android 桥，**无共享核**，B 类反例）；`connection` 归 C 不归 B（桌面同名 15 函数域，移动只有 `primary-target` 1 函数）；`support.rs`（`guarded_host_call`，10 域共用的 panic 守卫，属迁移原子件） | 见 `ticket-01-domains.md` §纠正 |
| **C 移动独有接口（桌面 crate 无对应物）** | `host-notify` / `host-terminal-stream` / `host-fs-mobile`（save-to-document / write-media-downloads）/ `host-connection.primary-target` / `events`(5) / `host-auth`(5) | **整体落移动宿主**（用户指令⑥）：宿主 adapter = 自带 bindgen + `impl Host` + 域逻辑 + 宿主端口；wasm-core **不留壳**（见 §1.6、票 04） |

### 1.5 命名普查（desktop / mobile 语义载体）

wasm-core 内粗筛：**68 个源文件**含字样；标识符级命中 `desktop` 108 / `mobile` 64（含 cfg 特性串与模块路径）+
`DesktopPluginInfo` 10 / `desktop_root` 5 / `MobileWsClientPorts` 4 / `MobileDiscoveryPorts` 3 / `MobileReconnectPolicy` 2 / `mobile_ports` 1 / `mobile_context` 1 / 锁名 `desktop_sliced_interfaces_must_not_return_to_wasm_core` 1。

| 载体类别 | 内容 | 处置 |
| --- | --- | --- |
| 模块/目录 | `manager/runtime/{desktop.rs, desktop/, mobile.rs, mobile/}` | 改名（票 02） |
| cargo feature | `desktop-host`（默认）、`mobile-host` | 改名（票 02）；消费方 `bedcode-mobile/src-tauri/Cargo.toml` 同步 |
| 类型/常量/函数 | `DesktopPluginInfo` / `MobileWsClientPorts` / `MobileDiscoveryPorts` / `MobileReconnectPolicy` / `mobile_context` / `mobile_ports` / `desktop_root` … | 逐项定名（票 02 映射表） |
| 锁/断言名 | `desktop_sliced_interfaces_must_not_return_to_wasm_core` 等 | 改名 + 锁内路径同步 |
| 包元数据 | `Cargo.toml` 的 `description`（"桌面插件机制整核…"） | 改中立描述 |
| **注释** | 大量「桌面/移动」历史说明 | **保留**（解释 why：迁入来源、形态差异理由），但不得作为命名/判据载体；新注释优先用新名 |

**范围边界（明确不在本 spec）**：端 SDK 目录与构件名（`packages/plugin-sdk-{mobile,desktop}`）、端工程（`bedcode-{desktop,mobile}/`）、构建脚本端参数（`compose-wit.mjs <end>`）与端 `compose.json`——端名是**端事实**，不属 wasm-core 语义；历史 ADR 正文不改写，改名映射登记到**新 ADR**。

### 1.6 「移动独有域落移动宿主」可行性核实（2026-10-10，为指令⑥）

| # | 事实（带证据） | 含义 |
| --- | --- | --- |
| 1 | 移动 17 个 `impl Host` 全在 `mobile/component.rs`（行号见 §1.2）；linker 为硬编码清单、无钩子 | 宿主要在「不改装配链」的前提下注入 `impl Host` **不可能** ⇒ wasm-core 必须提供注册点（或移动改走注册面） |
| 2 | 移动 state `WasmPluginState`（`mobile.rs:121`）：`plugin_id` / `host_ctx: Arc<WasmHostContext>` / `runtime_handle` / `granted_permissions` / `on_message_binary`——**字段全私有、类型定义在 wasm-core** | 宿主 adapter 读不到状态；且 trait + 类型皆外部 ⇒ **孤儿规则 E0117**（不能直接 `impl Host for WasmPluginState`） |
| 3 | 桌面绕法（先例）：宿主**自带 bindgen**（`bedcode-desktop/src-tauri/src/plugin/bindings.rs:22`，path 指端 WIT + `world: "plugin"`）⇒ trait 变本地；impl 挂 **host-kit 的** `WasmPluginState`（字段 pub，`host: Arc<dyn HostPorts>`，域内 `downcast_host::<WasmHostContext>` 还原上下文）；四件同处：`expect_host_module!` + `HostModule` + `inventory::submit!` + `impl Host`（样板 `plugin/fs.rs:21 / 46–54 / 63–65 / 69–81`）；装配 `ModuleRegistry::verify_whitelist` + `install_all`（`desktop/component.rs:287–412`） | 移动要照搬⇒**必须三件齐备**：宿主 bindgen + 可命名可访问的 state + 装配注册点 |
| 4 | host-kit 注册面签名绑死自身 state：`HostModule::register(&self, &mut Linker<WasmPluginState>)`、`ModuleRegistry::install_all(&mut Linker<WasmPluginState>)`（`module.rs:40–52`、`registry.rs:78–115`）；`Linker<State>` 单态 | 移动沿用 wasm-core 自持 state ⇒ **复用不了** host-kit 注册表；要么统一 state（路径 X），要么在 wasm-core 再写一套注册机制（路径 Y，机制重复） |
| 5 | host-kit 已为移动预留：`state.rs:115–130` 的 `#[cfg(not(feature = "wasi-store"))]` 构造注明「无 WASI 形态（移动宿主 / 无头第三方宿主）」；`lib.rs:7–19` 说明独立成 crate 正是为让能力域能命名 state | 「统一 state」路线在 host-kit 侧**零新增设计** |
| 6 | 移动宿主现状：`bedcode-mobile/src-tauri/src/plugin/` 无 adapter、无 inventory 自报，只有 `host_ports.rs:135` 的 `HostEnginePorts` 端口投影（装配点 `manager.rs:271–279`）；`Cargo.toml` **未依赖** `bedcode-host-kit` | 需新增 adapter 面 + 依赖；端口范式（`HostEnginePorts` ↔ host-kit `HostPorts`）需衔接 |
| 7 | 移动目标 interface 与桌面「路径 B」域**并非同名同形**（移动 `host-events` 为交集 `emit`；通知走 `host-notify`；`host-fs-mobile` 移动独有；`primary-target` 在 `host-connection`） | **不能复用桌面 adapter 文件**，需移动专属 adapter；共享的是域逻辑 crate，不是 WIT 壳 |
| 8 | 现存锁已把 `host_fs_mobile` / `host_terminal_stream` 等列为「不得回流 wasm-core」needle（wasm-core `lib.rs` 的 `desktop_sliced_interfaces_must_not_return_to_wasm_core`，票 02 改名） | 迁移需同步改锁 + 新增「移动域回接 wasm-core」反向锁（移植后可测红） |

**两条候选路径（2026-10-10 已裁决 = 路径 X，用户「按推荐执行」）**：

| 路径 | 内容 | 代价 / 风险 |
| --- | --- | --- |
| **X（对齐桌面，已裁决）** | 移动改接 **host-kit state**（`host: Arc<dyn HostPorts>` + `downcast_host::<…>`）+ 移动 `HostPorts` 适配 + `ModuleRegistry` 注册；域壳随迁移动 adapter | 大：移动各域状态访问面重写（`state.host_ctx` → `downcast_host`）+ `host_ports.rs` 范式转换；但**消除两套 state**、装配面与桌面同形 |
| **Y（备选，退役）** | wasm-core 保留移动自持 state，仅**开公有访问面**（`host_ctx()` / `has_permission()` 等）并在移动面加**宿主模块注册点**（`Linker<WasmPluginState>` 模块 trait + inventory，或显式注入参数） | 已否决：wasm-core 会出现**第二套注册机制**（与 host-kit `HostModule` 重复 ⇒ 漂移风险），两套 state 长期并存 |

**裁决（2026-10-10）**：走 **X**——移动改接 host-kit `WasmPluginState` + `HostPorts` + `ModuleRegistry`，装配面与桌面同形；Y 不再评估。票 04 按 X 展开（见 §4）。

---

## 2. 决策

| # | 决策 | 依据 |
| --- | --- | --- |
| D1 | **域逻辑进共享 crate（WIT-无关函数 + 端口 trait），各端只留壳** | 桌面先例 + 用户原则①；本会话已验证 peer 可行 |
| D2 | **双端共有机制域**的形态壳留 wasm-core 双形态面（`common` 面已达标；`bare` 面收窄为纯转发） | 机制实现与真源同侧；`bare` 壳留 crate 是因本端 state 在 crate 内（Cargo 环约束） |
| **D2′** | **移动独有域（C 类）整体落移动宿主**（用户指令⑥）：宿主 adapter = 自带 bindgen（解孤儿规则）+ `impl Host` + 域逻辑 + 宿主端口；wasm-core 不留其壳与 `add_to_linker` 项 | 用户指令⑥ + 桌面「路径 B」先例；移动专属能力属端事实 |
| **D2c** | 移动装配链必须获得**宿主侧装配点**（硬编码裸清单 → 注册面）；机制形态 = **路径 X**（host-kit state + `HostPorts` + `ModuleRegistry`） | §1.6 事实 1 / 4；用户「按推荐执行」 |
| D2b | **已裁决 = 路径 X**：两端装配面外形逐字对齐（同一 host-kit 注册表 + 每域宿主 adapter） | 用户 2026-10-10「按推荐执行」 |
| D3 | C 类域**移动自持**：宿主 adapter（壳 + 逻辑 + 端口 + android 桥），不进共享 crate、**不回 wasm-core** | 桌面 crate 无对应接口；用户指令⑥ |
| D4 | **WIT / ABI / SDK / wire 零变更** | 纯 Rust 侧落点搬迁与改名；不触发插件产物重建 |
| D5 | 逐域/逐票推进、可独立回退；**票内只做代码改动 + 静态自检，编译验证 / 单元测试 / 集成测试统一在所有票改动完成后一次性执行**（用户 2026-10-10 指令） | 用户指令：避免逐票长编译与磁盘反复占满；回退仍逐票逆向编辑（AGENTS §11） |
| **D6** | **wasm-core 内标识符与装配语义中立化**：模块 / feature / 类型 / 锁名 / 包描述零端名、零机制名（不出现 `wasi` / `wasip3` 字样）；注释保留历史说明但不作命名载体 | 用户指令④⑤；命名取中立词 `common` / `bare`（见 §命名方案） |

### 命名方案（**已裁决 2026-10-10**：形态名不用 `wasi`，取 **`common` / `bare`**）

| 载荷 | 定名 | 对应现名 |
| --- | --- | --- |
| cargo feature（默认形态） | **`common-host`** | `desktop-host` |
| cargo feature（非默认形态） | **`bare-host`** | `mobile-host` |
| 模块 / 目录 | **`runtime/{common.rs, common/}`** | `runtime/{desktop.rs, desktop/}` |
| 模块 / 目录 | **`runtime/{bare.rs, bare/}`** | `runtime/{mobile.rs, mobile/}` |

- **形态机制差（只写注释与文档，不进标识符）**：`common` 形态装配 WASI p3 Store（插件目标 `wasm32-wasip3`）；`bare` 形态不装配 WASI（插件目标 `wasm32-unknown-unknown`）。
- **命名纪律（票 02 判据）**：形态名只取中立词——不出现端名（desktop / mobile），不出现机制名（`wasi` / `wasip3` / `wasm32`）。

- `test-support` feature 名不变（无端名）。
- 派生子项在票 02 内逐项定名并登记映射表（例：`DesktopPluginInfo` → `PluginInfo`（形态模块内）或 `HostPluginInfo`；`mobile_context` → 形态前缀/中性名；与既有 trait 冲突者避让）。
- **默认 feature**：`default = ["desktop-host"]` → `default = ["common-host"]`；移动消费方显式 `default-features = false, features = ["bare-host"]` 同步改（`bedcode-mobile/src-tauri/Cargo.toml`）。

## 3. 目标形态

```text
packages/bedcode-*（能力域 crate：域逻辑 + 端口 trait；WIT 绑定面随各端）
  ├── bedcode-server-peer-net   plugin_binding::peer_*（19 个 WIT-无关函数）+ PeerPorts + install_ports   ← 票 03 主战场
  └── bedcode-discovery-engine / bedcode-ws-client-engine / …（引擎面，现状已共享）
packages/bedcode-wasm-core（机制 + 双形态装配，零端名、零端专属域）
  ├── common 面（`runtime/common*`）：已达标（无 host_impl；域壳在宿主 adapter）
  ├── bare 面（`runtime/bare*`）：共有机制域壳收窄为**纯转发**（无域逻辑、无状态机、无载荷构造）
  └── 移动独有域（notify / terminal_stream / fs_mobile / primary-target / auth / 移动 events）**整面移除**
bedcode-mobile/src-tauri（移动宿主）
  ├── 移动独有域 adapter：`plugin/{notify,terminal_stream,fs_mobile,connection,events,auth}.rs`（自带 bindgen + impl Host + 域逻辑 + 端口）   ← 票 04 新增落点
  ├── 装配注册面 = host-kit `ModuleRegistry` + `HostPorts`（路径 X）
  └── peer 端口实现（PeerPorts）+ 既有适配层（peer_*.rs 已 peer_engine 化）        ← 票 03 新增落点
```

## 4. 票划分（顺序即实施序）

| 票 | 内容 | 交付与静态自检（**编译 / 测试统一在票 06 后一次性跑**，见 §5） |
| --- | --- | --- |
| **01** | **核验清单（零改动）**：① 15 域逐域分类（纯转发 / 含域逻辑 / 移动独有 / 有无共享 crate 对应物；`wc -l` + 头注 + `grep "fn "` 出具）；② 命名普查落表（§1.5 四类载体 → 逐处新名映射表；**在票 02 开工时即时重扫生成**——抗在途改动导致的行号漂移）；③ **C 类域迁移面清单**：逐 interface 列函数集 / 状态与端口依赖（`host_ctx` 字段用法、`granted_permissions` 用法）/ 现有锁命中 / 与移动宿主 `host_ports.rs` 的衔接点；④ ADR 编号核对（`docs/adr/` 现状），产出并入本 spec | ①③ 已完成（2026-10-10 → `ticket-01-domains.md`，16 域表 + WIT 函数清单 + 6 条分类纠正）；② 映射表在票 02 开工时重扫落表；④ 待核 |
| **02** | **形态命名中立化（先做：机械、零行为变化）**：按 §命名方案 定名一次改净——feature `desktop-host`/`mobile-host` → `common-host`/`bare-host`；模块 `runtime/{desktop,mobile}*` → `runtime/{common,bare}*`；类型/常量/函数 + 锁名/断言名 + 包描述走票 01 映射表；同步：`bedcode-mobile/src-tauri/Cargo.toml`（feature 名）、双端锁内路径/特性名、双端 code-map 与 AGENTS §5.4 引用、**新 ADR 登记改名映射**（历史 ADR 不改写） | 改名映射全落 + 全仓 `grep` 复核（wasm-core 内标识符级零端名、零 `wasi` 字样，注释除外）+ 无 ABI/WIT 变更（编译 / 测试归收尾） |
| **03** | **P1 · peer 同 crate 复用**（原票 02）：① 移动宿主新增 `plugin/peer_ports.rs` 实现 crate `plugin_binding::ports::PeerPorts`（`check_permission` 走移动既有唯一权限判定 / `peer_ctx` 走 `crate::peer_net::peer_ctx` / `block_on_any` 走移动 `block_on_async`）+ 开机 `install_ports`；② wasm-core `<bare 形态>/host_impl/peer.rs` 重写为 **19 条薄转发**（`plugin_binding::peer_*(&ports(), plugin_id, …)`），删句柄表/自动重拨/载荷解析（510 → 约百行）；③ `component.rs` 接线不变；④ 逐条对表 19 原语的入参形状与 wire 文案（`HEADLESS_UNAVAILABLE` 由 crate 单点持有） | 行为对表 19/19 + 新增锁（票 06-①）；移动全量 / 桌面 check 归收尾统一跑 |
| **04** | **移动独有域宿主化（指令⑥主线，按已裁决路径 X）**：① 移动改接 host-kit `WasmPluginState` + `HostPorts` + `ModuleRegistry`（按需开 `wasi-store` 之外的无 WASI 形态，`host-kit/state.rs:115–130` 已预留）；② 移动宿主新增 `plugin/bindings.rs`（绑移动 WIT，解孤儿规则）；③ 装配注册面落地（X = 移动接 host-kit `HostPorts` + `ModuleRegistry`；Y = wasm-core 移动面提供宿主模块注册点）；④ 逐域搬迁 `impl Host` + `host_impl/*` 到 `plugin/<domain>.rs`（notify / terminal_stream / fs_mobile（含 SAF 与媒体落盘）/ connection.primary-target / auth / 移动 events），wasm-core 删对应 impl 与 `add_to_linker` 项；⑤ 端口衔接（`HostEnginePorts` → 新范式）+ 依赖补齐（`bedcode-host-kit`） | 逐域迁移面对表（notify 5 / terminal_stream 1 / fs_mobile 2 / primary-target 1 / auth 5 / events 5）+ 反向锁（移动域不得回接 wasm-core）；移动全量 / 桌面 check / 真机冒烟归收尾统一跑 |
| **05** | **收窄 / 下沉含逻辑域**（票 01 已定清单）：含域逻辑 = `peer`（票 03 覆盖）/ `fs` / `db` / `http`——逻辑先下沉 `bedcode-host-api-core` 共享实现或随域迁宿主；纯转发 = mdns / ws / storage / bus / config / platform / event / auth / connection / terminal_stream（只收窄适配面）；**共有机制域不搬壳位置**（D2） | **已完成（2026-10-10）→ `ticket-05-domain-sinking.md`**：`http` 下沉能力域 crate（含 `jwtAuth` 端口 + 行为对表 + 覆盖对表）/ `fs` 上移 `bedcode-host-api-core::fs`（8 例新单测）/ `db` 判定已在目标形态（零改动）；纯转发 10 域只收窄适配面（票 04 已覆盖）。符号面锁归票 06-①（测试归收尾统一跑） |
| **06** | **收口**：① 防回接锁（wasm-core 形态面不得含域逻辑：行数上限 + 禁现 `struct .*HandleTable` / `fn .*redial` / 载荷构造符号；同风格参照桌面 `capability_crates_no_product_ids`）；② 双端 code-map（形态段改写 + 锁索引）；③ ADR（域逻辑在 crate / 壳在各端 / 引擎在能力域 / 移动独有域落移动宿主 / 形态命名）；④ CHANGELOG 双语；⑤ AGENTS §5.4 口径（命名与形态） | **统一验证（所有票改动完成后一次性执行，见 §5）**：两端 `cargo check` + `cargo test` 全量、两端 `pnpm run test:run`、根 `pnpm exec eslint .` 0 error、`cross-end-tests` 全量、锁变异自检（注入 → 红 → 还原核 sha256）、全仓引用核对 |

## 5. 验证时机与门禁

**时机（用户 2026-10-10 指令，强制）**：**所有票的代码改动全部完成之后，再统一进行编译验证、单元测试等**——即票 01–05 票内**不跑**门禁式 `cargo check` / `cargo test` / 前端测试 / `cross-end-tests`，只做代码改动 + 静态自检（`grep` 复核、映射表对表、接口函数清单对表、文件在场断言）；编译与测试**统一在票 06（收口）一次性执行**。

- **理由（用户）**：避免逐票长编译与磁盘反复占满（本仓 desktop lib 全量重链 ~20 分钟级，磁盘长期紧张）。
- **排障例外**：统一验证若一次性爆出大量编译错误，允许按票 / 按域逐个 `cargo check -p <crate>`（或 `--no-default-features --features bare-host` 形态）**作为定位手段**——这是排障，不是票级门禁，不得据此宣布票据通过。
- **风险自负声明**：验证后移 ⇒ 票 02 改名 + 票 04 装配面改造的错误会累积到最后一次性暴露；票内静态自检（尤其改名映射 `grep` 复核、接口对表）必须做足。

**统一验证清单（票 06 一次性执行，全实跑并贴结果）**：

1. 内核双形态：`packages/bedcode-wasm-core` `cargo check`（默认 `common-host`）+ `cargo check --no-default-features --features bare-host` + `cargo test --lib`。
2. 消费方与宿主：`bedcode-mobile/src-tauri` `cargo test` 全量（当前基线 **314/0**）+ `cargo check`；`bedcode-desktop/src-tauri` `cargo check`（零回归）+ 受影响集成测试。
3. 受影响 `packages/bedcode-*` crate `cargo test`；`cross-end-tests` 全量。
4. 两端 `pnpm run test:run` + 根 `pnpm exec eslint .` 0 error；`cargo fmt --check` / `clippy` 自查。
5. **锁**：新增/改动锁必须**变异自检**（注入 → 红 → 还原核 sha256）并登记到双端 code-map 锁索引（AGENTS §5.1.4）。**改名票特别注意**：锁内路径 / 特性名同步（本会话已踩过陈旧路径导致「边界锁空转」的坑）。
6. 未纳入自动化的手工项（真机 peer 流程冒烟；票 04 后另加：移动独有域真机冒烟 = 通知 / 终端流 / SAF 落盘 / 配对）逐项写「跑了 / 没跑 + 原因」。
7. 磁盘：统一验证前 `df -h`，预留 desktop lib 全量重链预算；收尾清残留进程 / 端口。

## 6. 风险与回退

| # | 风险 | 吸收 |
| --- | --- | --- |
| R1 | 改名面广（68 文件 / 170+ 标识符命中）漂移或漏改 | 票 01 先出映射表；改完 `grep` 复核（标识符级零端名）+ 双端编译；逐类提交（模块/feature/类型/锁）便于定位 |
| R2 | feature 改名波及消费方与锁 | 票 02 同批改 `bedcode-mobile/src-tauri/Cargo.toml` + 双端锁 + 文档；`cargo tree` 双形态核对 |
| R3 | peer wire 文案/入参形状漂移（移动旧实现 vs crate） | 票 03-④ 逐条对表；`HEADLESS_UNAVAILABLE` 等常量 crate 单点；移动全量 + 真机冒烟 |
| R4 | `block_on_any` 重入/自锁语义差异 | 复用移动既有 `block_on_async`（不新写）；差异记入票 03 |
| R5 | 权限门语义（移动本地判定 vs 插件管理器） | `check_permission` 走移动既有唯一判定点，不新造 |
| R6 | 覆盖漂移（删逻辑带走测试） | 删前逐测试对照：能迁的迁 crate（本会话 peer 先例：3 例补进 crate）；平台专属测试留移动适配层 |
| R7 | **孤儿规则 E0117**：宿主直接 `impl Host for wasm-core state` 编译不过 | 移动宿主自带 bindgen（桌面先例 `plugin/bindings.rs`）——票 04-② 必做项；缺此步整票不成立 |
| R8 | 状态访问面：移动 `WasmPluginState` 字段私有（`mobile.rs:121`） | 路径 X（统一 host-kit state，`downcast_host` 还原上下文）或路径 Y（开公有访问器）——§7 裁决；两者都先要票 01-③ 的状态依赖清单 |
| R9 | 装配注册面重复：`Linker<State>` 单态 ⇒ host-kit `HostModule` / `install_all` 绑自身 state | 路径 X 复用同一注册表（首选）；路径 Y 会在 wasm-core 增第二套注册机制 ⇒ 登记漂移欠账 + 锁 |
| R10 | 移动 adapter 不能复用桌面 adapter（interface 集不同） | 移动专属 adapter 文件；跨端共享的是**域逻辑 crate**（如 peer 19 函数），不是 WIT 壳 |
| R11 | **验证后移**（用户指令）：票 02 改名 + 票 04 装配面改造的错误累积到收尾一次性暴露 | 票内静态自检做足（改名映射 `grep` 复核 / 接口函数清单对表 / 文件在场断言）；爆错时按票逐个 `cargo check -p` 定位（排障手段，非门禁） |
| 回退 | 任一票测红不可修 | 只改文件内落点，按票据逆向编辑回滚（**禁 `git checkout -- <file>`**，AGENTS §11）；票 02 回退 = 反向改名（同映射表） |

## 7. 开放裁决点（实施中确认，勿自行放松）

- ~~**命名方案 A / B**~~ **已裁决（2026-10-10）**：形态名 `common` / `bare`（用户明示「不要使用 wasi」）——票 02 直接按 §命名方案 执行，不再讨论。
- **注释保留口径**（§1.5 末行）：标识符与判据零端名，注释保留历史说明——按现状执行（用户本轮未反对；若要求注释一并清除需另行裁决——会丢失迁入来源与形态差异理由，不建议）。
- ~~**票 04 路径 X / Y**~~ **已裁决（2026-10-10，用户「按推荐执行」）**：走 **X**——移动改接 host-kit `WasmPluginState` + `HostPorts` + `ModuleRegistry`，装配面与桌面同形；Y 退役。前置仍为票 01-③ 的 C 类域状态依赖清单。
- **票 04 迁移范围**：按用户指令⑥取**全迁**——移动专属 interface 全部（notify / terminal_stream / fs_mobile / connection.primary-target / auth / 移动 events）出 wasm-core；逐个由票 01-③ 清单核对后执行，不保留「部分留存」例外。
- **ADR 编号**：0047 起顺延（实施前核 `docs/adr/` 现状）。
