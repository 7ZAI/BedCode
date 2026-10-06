# 桌面端 wasm_core 整核抽出：`bedcode-wasm-core` 可复用 crate

> Status: ready-for-agent
> Date: 2026-10-06
> 分支：dev（**仅桌面端**；移动端零改动，双端共享是后续独立阶段，见 §8）
> 决策依据：`AGENTS.md` §5.1（宿主侧无业务代码）/ §5.1.4（fail-visible 三形态）/ §6（Rust 规范）；`docs/adr/0022`（边界裁决单一事实源）；`docs/adr/0035`（能力域 crate 化 + 机制内核 bedcode-host-kit，前作成果）；`docs/adr/0036`（**机制与真源同侧**——本票边界的第一原则）
> 同类前例：`.scratch/2026-09-24-wasm-core-decouple/`（内部依赖单向化，地基）、`.scratch/2026-10-04-wasm-core-lib-split/spec.md`（**能力实现**出内核；本票是其自然后继——把**机制整核**也搬出去）、`.scratch/2026-09-30-server-lib-split/spec.md`（bin crate 整面拆 crate 的先例形态）
> 实施票：`issues/01..06`（**串行链**，票面为准；§6 表为对照）

---

## 1. Problem Statement

> **† 量测基准声明**：本 spec 全部行数取自工作区 `dev` 分支 HEAD `132f6846e` + 并发会话在途改动
> （`host_api.rs` / `manager/host.rs` / `scaffold.rs` / `runtime.rs` 四文件 +44/−31，即
> `install_capability_domain_ports` 单入口收口，2026-10-06 实测存在，与本票目标正交、不冲突）。
> 在途改动不影响下列量测（四文件总量变化 <0.3%）。**开工前按票 01 复核。**

### 1.1 现状：插件机制整核仍长在 bin crate 里

ADR 0035（2026-10-04~05）把**能力实现**（mdns / websocket / peer / http 四域，55 条原语）
搬进了 `packages/` 的能力 crate，并建出机制内核 `bedcode-host-kit`（1,110 行）。但**机制本体
没有动**：`bedcode-desktop/src-tauri/src/wasm_core/` 仍是 **54,394 行 / 119 个 rs 文件**（2026-10-06 票 01 复核实测，双口径一致；前作 spec 在 `dbe50d229` 测 57,592 行，lib-split 迁出四域约 3,198 行后为 54,394），
直接编译在 bin crate `bedcode-desktop` 内部，不是库。

**出边（wasm_core → lib 其他模块，生产段逐文件实测）**——远少于直觉：

| 落点 | 使用方（生产段） | 现状 |
| --- | --- | --- |
| `crate::AppError` | config / context / platform / capability / downloader / activation / api_bridge / commands / errors / database / network_auth / host / frontend_channel / approval… | **已是 crate**（`bedcode-server-base::error`，经 `system/error.rs` 再导出） |
| `crate::db::Database` | context / host / network_auth / auth_policy / storage / sqlite / sqlite_ports / approval / auth… | lib 模块（404 行 + `schema.sql`） |
| `crate::system::constants` | watcher / validation / host / downloader / host_api/config / database 护栏… | **已是 shim**（→ `bedcode-server-base::constants`） |
| `crate::system::error::{EventEnvelope, DEFAULT_ERROR_CODE}` | errors / commands | **已是 crate**（bedcode-server-base） |
| `crate::system::error_boundary` | process / pty / host | **已是 shim**（→ bedcode-server-base） |
| `crate::system::config::AppConfig` | host_api/config / pty_process / pty_reader | lib 模块（839 行，引擎级配置） |
| `crate::system::app_context::AppContext::try_global()` | mdns adapter / boot / errors（→ `app_handle` 与 `peer_ctx`） | lib 组合根 |
| `crate::system::opener::reveal_existing_in_dir` | platform（host-platform 原语） | lib 模块（378 行） |
| `crate::system::process::create_command` | pty/wsl | lib 模块（22 行） |
| `crate::pty::{PtyRing, PtySession, …}` | host_api/pty / pty_output / platform | lib 引擎（2,469 行） |
| `crate::enums::PtySessionStatus` | pty 引擎 | lib 模块（134 行） |
| `crate::crypto::registry` | host_api/crypto | **已是 crate**（`bedcode-crypto-engine`，经 `crypto.rs` 再导出） |
| `crate::utils::auth::auth_center::invoke_auth_method` | host_api/auth | lib 模块（216 行，内容 90% 是 wasm_core 互调） |
| `crate::server::peer_net_cmds::peer_ctx(&app)` | mdns / peer adapter / activation | lib 引擎装配点（tauri state 读取） |
| `crate::server::ports_impl::HostBusPort` | ws adapter / register / 测试 | lib 类型（包 `MessageBus`——crate 属物） |
| `tauri::*`（AppHandle / Emitter / Manager / async_runtime / `#[tauri::command]`） | **119 处 / 17 文件** | 依赖 |

**反向（lib → wasm_core）**：9 个文件（`commands.rs`、`lib.rs`、`server/ports_impl.rs`、
`system/app_context.rs`、`system/lifecycle.rs`、`utils/auth.rs` 及 auth_center / test_tokens、
`utils/session_gateway.rs`）+ 5 个集成测试（`ws_auth_rules` / `http_auth_biometric` /
`pty_session_chain` / `broadcast_shutdown` / `wasm_bridge_bench`，均 `use
bedcode_desktop_lib::wasm_core::…`）+ `cross-end-tests/tests/common/desktop_ctx.rs`。

### 1.2 三个病灶

1. **不是库，无法被复用**。不能独立编译、独立跑测试、独立发布；任何 Tauri 宿主想引入
   插件机制，只能把 54,394 行搬进自己的 bin crate。bin crate 每次全量编译都要带上
   wasmtime/cranelift/wasm-tools 栈（约 94 个 crate，ADR 0036 §背景已实测过这个数字）。
2. **边界从未被画过**。ADR 0035 只画了「能力实现 vs 机制」，机制与「bin 宿主」的边界
   没有人定义过：`db`（真源）在 lib、机制在 bin 内的 wasm_core、引擎（pty / opener /
   auth_center / session_gateway）散在 lib 各处。ADR 0036 的「机制与真源同侧」原则
   要求这组东西要么同 crate，要么承受归属两个答案。
3. **迁移成本被高估**（实测反向）：出边里 **5 类已经是 packages/ 的 crate 或 shim**
   （AppError / constants / error_boundary / identity / crypto），真正要随迁或转端口的
   lib 模块只有 ~4,500 行（db 404 + pty 2,469 + enums 134 + opener 378 + config 839 +
   process 22 + auth_center 216 + session_gateway 254 + test_tokens 130）。「整核抽成
   crate」没有想象中那么贵。

### 1.3 前作资产（不重建）

- `bedcode-host-kit`（仓库根 `packages/`）：`HostModule` / `WasmPluginState` / `HostPorts` /
  注册表——新 crate 直接依赖，**不搬不重写**；
- 四个能力 crate + 自动注册 + 白名单锁（`HOST_MODULES` 在 `manager/runtime/component.rs`，
  随迁）；
- `install_capability_domain_ports` **单入口装配链**（在途改动正在收口）——新 crate 的
  宿主上下文注册表沿用同一「单入口」纪律（§4.4）；
- `crate_boundary_lock.rs` 的 `SPLIT_CRATES` 登记表 + 断言①~⑤（§7.3 需要扩表）。

---

## 2. Solution

### 2.1 目标形态

```
        ┌─────────────────── 宿主 bin crate（bedcode-desktop，剩组合根 + 宿主胶水） ───────────────────┐
        │  lib.rs（垫片）: pub use bedcode_wasm_core as wasm_core;  pub use bedcode_wasm_core::db;     │
        │  system/app_context.rs（组合根）· server/*（composition / ports_impl / peer_net_cmds）         │
        │  commands.rs · system/{lifecycle,logging,info,power,power_wake,error,app_context}            │
        │  mdns/advertiser.rs · utils/{session_gateway 的 lib 侧消费方经垫片取用}                        │
        └───────┬──────────────────────────────────────────────────────────────────────────────────────┘
                │ 依赖（lib → crate，垫片保路径）
                ▼
   ┌───────────────────────── bedcode-wasm-core（bedcode-desktop/packages/） ─────────────────────────┐
   │  插件核心机制整核：manager/ · security/ · host_api/ · bus/ · config/ · monitor/ · permission/      │
   │  runtime_util/ · intercall/ · storage/                                                        │
   │  + 引擎面（机制与真源同侧，ADR 0036）：db/（schema.sql 真源）· pty/ · enums/                    │
   │  + 宿主胶水迁入：system/{config(AppConfig),opener,process} · auth_center · session_gateway ·     │
   │    HostBusPort · peer_ctx 的 crate 侧调用点                                                    │
   │  依赖：tauri（AppHandle/command/Emitter）· wasmtime 48 · bedcode-host-kit（根）· bedcode-server-  │
   │  base · bedcode-crypto-engine · 四能力 crate · bedcode-plugin-api（WIT 绑定）· bedcode-server-core│
   └──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

### 2.2 两条边界原则（本票裁决的单一事实依据）

1. **机制与真源同侧（ADR 0036 D1）**：`host-api/database.rs` / `storage.rs`（机制面）
   与 `src/db/`（真源 `plugin_*` 四表 + `settings`，schema 单一事实源）**必须同 crate**。
   整核移出后二者都在新 crate，归属只有一个答案——这是对 ADR 0036 的**延续**（内核整体
   迁出），不是推翻。
2. **引擎归内核（AGENTS §5）**：host-pty 的引擎面（`src/pty/`）、host-platform 的引擎面
   （`system/opener.rs`）、认证中心桥接（`utils/auth/auth_center.rs`）、会话窄转发
   （`utils/session_gateway.rs`）都是**应用无关引擎**，随机制走。lib 对它们的消费经垫片
   保持零改动。

### 2.3 端口面：只有一个端口

**`peer_ctx` provider**（`fn(&AppHandle) -> Arc<PeerCtx>`）是唯一必须由 lib 提供的东西
（它读 tauri managed state + server 端口装配）。经 `PluginHost::new` 的新参数注入，
`PluginHost::new` 是**既有单装配入口**（lib.rs 与全部测试夹具都走它）。

其余「lib 胶水」全部被 crate 内部机制替代，不需要端口：

| wasm_core 现状 | 替代 |
| --- | --- |
| `AppContext::try_global()?.app_handle()`（boot / errors 的 emit 路径） | `self.wasm_host_ctx().app_handle()`（AppHandleScope 已存在，context.rs:707；无头 None 语义不变） |
| `AppContext::try_global()` → `plugin_host().wasm_host_ctx()`（mdns adapter 的 check_permission / 路由） | crate 内宿主上下文注册表（§4.4，单入口装 `Weak<WasmHostContext>`） |
| `AppContext::try_global()` → `plugin_host().message_bus()`（mdns publish） | 同上注册表 → `host_ctx.message_bus()`（或 adapter 持总线，同 ws 的 `HostWsPorts::from_bus` 先例） |
| `crate::server::peer_net_cmds::peer_ctx(app)`（mdns / peer / activation 三处） | crate 内 `peer_ctx_for(app)` 壳 → **peer_ctx provider 端口**（lib 实现） |

> **为什么端口这么少**：a) `app_handle` 早已是 `PluginHost::new` 的第四参数（lib.rs:517
> 传 `Some(…)`、测试传 `None`），且已存进 `WasmHostContext`；b) 其余 lib 模块要么是
> packages/ 的 crate（直接依赖）、要么随迁（§3.1）。c) `db` / `pty` 若做成端口，端口
> trait 就得复制 `Database` / `PtyRing` 的整套 API——那是 ADR 0036 明确拒绝过的形状
> （「端口不再是架构边界，只是可测性缝」）。

---

## 3. 边界判定（移 / 留 / 端口三张表）

### 3.1 移入 crate（11 项，≈59,000 行）

| # | 源（`bedcode-desktop/src-tauri/src/`） | 行数 | 依据 |
| --- | --- | --- | --- |
| M1 | `wasm_core/`（整目录，119 文件） | 54,394 | 机制本体 |
| M2 | `db/`（database.rs / models.rs / operations.rs / schema.sql） | 387 + schema(73) | ADR 0036「机制与真源同侧」 |
| M3 | `pty/`（6 文件） | 2,469 | host-pty 引擎面 |
| M4 | `enums/`（pty_status / special_key / plugin） | 33 | pty 引擎词汇（`PtySessionStatus` 等） |
| M5 | `system/process.rs` | 22 | pty/wsl 依赖的 `create_command` |
| M6 | `system/opener.rs` | 378 | host-platform 引擎面 |
| M7 | `system/config.rs`（AppConfig 全量） | 839 | 引擎级配置（见 D6 评审点） |
| M8 | `utils/auth/auth_center.rs` | 216 | 认证中心桥接（ADR 0022「宿主只剩…认证中心桥接」属机制） |
| M9 | `utils/session_gateway.rs` | 254 | 会话窄转发（纯 wasm_core 互调，零 lib 依赖） |
| M10 | `utils/auth/test_tokens.rs` | 130 | 测试夹具（lib 的 `utils/auth.rs` 保留 `#[cfg(test)]` 再导出） |
| M11 | `server/ports_impl.rs` 的 `HostBusPort`（ports_impl.rs:81-130 抽出） | ~53 | 包 `MessageBus`（crate 属物），base `BusPort` 实现 |

**垫片**（lib 侧保持路径，逐项 `pub use`，遵循 `enums.rs` 既有「反双份锁」纪律，D3）：
`src/wasm_core.rs` 删除 → `lib.rs` 加 `pub use bedcode_wasm_core as wasm_core;`；
`src/db.rs` 删除 → `pub use bedcode_wasm_core::db;`；`system/process.rs`、`system/opener.rs`、
`system/config.rs`、`utils/auth.rs`（auth_center / identity / test_tokens）、`utils.rs`
（session_gateway）、`server/ports_impl.rs`（HostBusPort 再导出）改为 `pub use` 垫片。

> **垫片能成立的先决条件**：crate 的公开面 ≥ lib 消费面且**名字逐字一致**。已有先例
> （`system/constants.rs` / `error_boundary.rs` / `crypto.rs` / `enums.rs` 都是纯 `pub use`
> 垫片，含反双份锁）。唯一可见性修正：`runtime_util` 由 `pub(crate)` 改 `pub`
> （lib 的 `server/ports_impl.rs` 经 `crate::wasm_core::runtime_util::*` 消费）。

### 3.2 留 lib（组合根 + 宿主胶水）

`system/app_context.rs`（组合根，含 `mdns_advertiser` 等 lib 侧服务）、`system/error.rs`
（bedcode-server-base 再导出，保留）、`system/{lifecycle,logging,info,power,power_wake}.rs`、
`server/*`（composition / ports_impl 其余 / peer_net_cmds——**peer_ctx 本体留 lib**，
仅作为端口实现）、`commands.rs`、`mdns/advertiser.rs`、`utils/session_gateway` 的 lib 消费侧
（经垫片）。

### 3.3 端口（唯一）

| 端口 | 形状 | lib 实现 | 注入点 |
| --- | --- | --- | --- |
| `PeerCtxProvider` | `Option<Arc<dyn Fn(&AppHandle) -> Arc<PeerCtx> + Send + Sync>>` | `peer_net_cmds::peer_ctx` | `PluginHost::new` 新参数（lib.rs 传 `Some(…)`，测试/无头传 `None` → `HEADLESS_UNAVAILABLE` 语义不变） |

---

## 4. crate 布局与契约（`bedcode-desktop/packages/bedcode-wasm-core`）

### 4.1 位置与依赖

- **位置**：`bedcode-desktop/packages/`，**不放仓库根 `packages/`**。理由沿用 ADR 0035 D6
  的修正结论：本 crate 必然依赖桌面基础层（`bedcode-server-base` 的错误/常量、
  `bedcode-plugin-api` 桌面 WIT、四个桌面能力 crate）——放根 `packages/` 是陷阱（移动端
  永远拉不动，读者误以为可直接复用）。**双端共享锚点仍是 `bedcode-host-kit`**（§8）。
- **依赖**（桌面包）：`bedcode-host-kit`（根）、`bedcode-server-base`、`bedcode-crypto-engine`、
  `bedcode-server-core`（`ServerSupervisor`，host_config 用）、四个能力 crate
  （`bedcode-discovery-engine` / `-server-websocket` / `-server-peer-net` / `-server-http`）、
  `bedcode-plugin-api`（WIT 绑定 + `abi` / `WasiPreopenDir` / `BedcodePluginEntry`）。
- **第三方**：`tauri`（`#[tauri::command]`、`AppHandle`、`Emitter`、`Manager`、`async_runtime`）、
  `wasmtime` 48 + `wasmtime-wasi` p3、`tokio`、`serde`、`tracing`、`dirs`、`inventory` 等
  （与 `src-tauri/Cargo.toml` 同版本锁步；wasmtime 双端 48 不因本票变化）。

### 4.2 WIT 绑定路径

`manager/runtime/component.rs` 的 `bindgen!` 现用相对路径
`"../packages/plugin-sdk-desktop/rust/wit/bedcode.wit"`（相对 src-tauri）。迁入 crate 后改
`"../plugin-sdk-desktop/rust/wit/bedcode.wit"`——**先例**：`bedcode-discovery-engine/src/lib.rs:82`
同款相对路径。**WIT 一个字节不改**（`world plugin` 的 22 个 import 不动，L1 零 ABI 变更）。

### 4.3 lib.rs 垫片契约（D3）

- `lib.rs`：`pub use bedcode_wasm_core as wasm_core;`——全部既有 `crate::wasm_core::*`
  引用（lib 9 文件 + 5 集成测试 + cross-end-tests）**零改动**编译通过。
- `pub use bedcode_wasm_core::db;`——`crate::db::*` 引用（commands / composition /
  app_context / cross-end-tests 的 `bedcode_desktop_lib::db::Database`）零改动。
- **反双份锁**（复用 `enums.rs:45-83` 的既有纪律，独立成锁测试）：垫片文件只允许
  `pub use`；`src-tauri/src/wasm_core/` 目录**不得存在实现文件**（迁移完成后的结构锁）。
- `runtime_util` 可见性 `pub(crate)` → `pub`（§3.1 注）。

### 4.4 宿主上下文注册表（crate 内机制，单入口）

mdns adapter 的零大小类型需要按调用取 `WasmHostContext`（现在经 lib `AppContext::try_global()`）。
迁入 crate 后改为 **crate 内全局注册表**（`OnceLock<Weak<WasmHostContext>>`）：

- 装配：在 `install_capability_domain_ports`（现 runtime.rs:594，被 `PluginHost::new` 调用）
  里一并写入——**单入口纪律**（2026-10-05 实测教训：装配链曾三份拷贝导致顺序依赖假绿；
  本注册表禁止出现第二个装配点）；
- 语义：`None`（未装配 / 弱引用已失效）= 无头，与今天的 `AppContext::try_global() → None`
  逐字一致（fail-safe 拒绝 / 跳过 / `HEADLESS_UNAVAILABLE` 文案不变）；
- 幂等：`OnceLock::set`，重复装配忽略而非替换（与在途的 `install_capability_domain_ports`
  同款语义）。

### 4.5 测试 harness（`#[cfg(test)]`，≈15 行）

`ws_e2e.rs` 有 4+ 处 `crate::server::composition::start_http_server(port, &config)`——那是
lib 组合根。迁入 crate 后该助手无法再引用 lib（循环）。实测 `composition.rs:56-61` 只是
`bedcode_server_core::app::serve(port, config, transport_faces())` 的薄壳，而
`transport_faces()`（:42-47）只是两个 face 结构体。crate 测试内给一个 `#[cfg(test)]`
`host_harness::start_http_server`（dev-deps 已有 server crates）即可，**顺带成为「第三方
宿主如何装配本 crate」的可执行范例**（测试缝即复用缝）。

---

## 5. 关键决策

| # | 决策 | 理由 |
| --- | --- | --- |
| **D1** | 整核 crate 命名 `bedcode-wasm-core`，落 `bedcode-desktop/packages/` | 桌面宿主库；依赖桌面基础层（§4.1）；命名对齐 `bedcode-host-kit` / `bedcode-server-*` |
| **D2** | crate **保留 tauri 依赖**，不做零-tauri 反转 | 119 处引用含 `#[tauri::command]`（api_bridge 命令桥架构）、`Emitter`、`Manager`；反转 = 独立立项（§8）。「复用」= 任何 Tauri 宿主可直接依赖；移动端复用是后续阶段（ADR 0018 契约独立） |
| **D3** | lib 垫片 `pub use` + 反双份锁，**既有引用零改动** | 已有 5 个垫片先例 + enums 反双份锁纪律；集成测试与 cross-end-tests 免改 |
| **D4** | `db` 随迁（机制与真源同侧，ADR 0036 延续） | 三 interface 机制面与 `plugin_*` 表真源同 crate，归属唯一；`schema.sql` + 幂等迁移测试同行 |
| **D5** | 唯一端口 = `PeerCtxProvider`；**不建 DbPort / PtyPort** | `Database`/`PtyRing` API 作端口 = 复制整套接口（ADR 0036 拒绝的形状）；AppHandle 已随 `PluginHost::new` 注入 |
| **D6** | `AppConfig`（839 行）随迁（**评审点**） | 引擎级配置（config.rs 头注释明示「宿主只存引擎级配置」），pty 引擎 + host_config 只有 3 个读点；lib（server / commands）经垫片零改动。**备选**：3-getter `ConfigPort`（成本：pty 引擎内部穿端口）——见 §9 Q1 |
| **D7** | `auth_center` / `session_gateway` / `test_tokens` 随迁 | 内容 90% 是 wasm_core 互调 + 零 lib 依赖（§3.1 M8-M10）；`test_tokens` 供 wasm_core 测试用，留 lib 会造成 crate 测试 dev-dep 环 |
| **D8** | `HostBusPort` 从 `ports_impl.rs` 抽出迁入 crate | 包 `MessageBus`（crate 属物）；lib 的 server 组合根经垫片再导出，`assemble()` 本体留 lib |
| **D9** | `peer_ctx` 本体留 lib（`peer_net_cmds.rs`），作端口实现注入 | 它读 tauri managed state + lib `ports_impl::assemble()` 兜底（组合根唯一性，crate_boundary_lock 断言⑤）；随迁会把它与 `assemble()` 拆散 |
| **D10** | 锁更新清单（§7.3）随迁移同步落地，**不许滞后** | 滞后 = 锁空转或误红；hot_path_logging_lock 的路径字符串是「迁移完成」的可执行判据之一 |
| **D11** | 分期：先机械搬迁（票 02/03，tracer bullet）再胶水剥离（票 04），最后契约收口（票 05/06） | 每票后 crate 与 lib 各自可编译可测；不搞「一次大爆炸」 |

---

## 6. 分期落地

**实施以 `issues/01..06` 为单一事实源**（串行链，每票只依赖前票；`Blocked by` 写在该票文件里）。下表只作对照。

| 票 | 内容 | 行为变更 | 验收 |
| --- | --- | --- | --- |
| **01** | 量测复核 + 认领在途改动（**gate**） | 无 | spec 全部行数/路径按票面复核；`git status` 认领四文件在途改动的归属 |
| **02** | 🟢 **骨架 crate + 机制本体机械搬迁（tracer bullet）**：`wasm_core/` 全目录入 crate + lib.rs 垫片 + `runtime_util` pub 化 + `bindgen!` 路径 + Cargo.toml | 无 | src-tauri 全量编译绿（垫片生效）；crate 根 `cargo check` 绿（**首次可达在票 04 的 M1-M11 编译闭包完成后**，票 01 裁定：wasm_core 生产段引用 db/pty/system/server/utils 等 lib 模块，纯 wasm_core 入 crate 无法单独编译） |
| **03** | 引擎面随迁：`db` + `pty` + `enums` + `system/{process,opener}`（M2-M6） | 无 | `schema.sql` 真源迁移 + 幂等迁移测试同行；`retired_tables_are_not_created` 等 db 锁随迁后仍绿；每步后 src-tauri 全量编译绿 |
| **04** | 宿主胶水剥离：`AppConfig`（M7，D6 定案后执行）+ `auth_center`/`session_gateway`/`test_tokens`（M8-M10）+ `HostBusPort`（M11）+ AppContext→宿主上下文注册表（boot/errors/mdns 三处改造）+ `PeerCtxProvider` 端口 + `PluginHost::new` 签名扩展 + 测试 harness（§4.5） | **有**（`PluginHost::new` 增参，lib.rs 与全部夹具同步） | 无头测试语义逐字不变（`None` → 拒绝/跳过文案不变）；crate 根 `cargo check` 绿（闭包首次可达）；crate 内 `cargo test` 全量绿（含 ws_e2e 经 harness）；lib `cargo test` 全量绿 |
| **05** | 契约收口：锁更新（§7.3）+ 集成测试指向核对 + `CHANGELOG` 双语 + ADR 0037 落档 | 无 | 全部锁绿；`rg "src/wasm_core"` 在 lib 侧仅剩锁测试与文档引用 |
| **06** | **contract**：独立验收（crate 根 `cargo test` 全量 + src-tauri `cargo test` 全量 + `cross-end-tests` 全量） | 无 | 与基线对照零回归；`lens_diagnostics mode=all` 无 blocker |

---

## 7. 红线与门禁自检

### 7.1 §5.1 B1-B6 零命中自检

| 判据 | 自检 |
| --- | --- |
| B1 产品类型/字段 | 只搬现有实现，不新增任何业务名词类型 |
| B2 业务编排 | 不新增编排；编排仍在插件侧 |
| B3 业务真源 | 真源（`plugin_*` 四表 + `settings`）随 crate 整体迁移，位置唯一 |
| B4 业务投影 | 不新增 DTO |
| B5 业务默认值/策略 | 无新增默认值；AppConfig 是既有引擎配置（D6） |
| B6 业务生命周期挂钩 | 不新增对产品事件的解释或回调 |

**§5.1.2 三问**：① 离宿主能否实现——机制本来就是宿主引擎，crate 化不改变归属；② 携带
产品语义——否；③ 都不命中 ⇒ 进宿主，且满足 WIT 纯增量（本期 WIT 零改动 ✓）/ 权限位有
门禁落点（`check_permission` 随迁 ✓）/ 停用可回收（`purge_for_plugin` 随迁 ✓）。

**提交前自检 3 问**：① 垫片在宿主有第二个消费者吗——有（lib 9 文件 + 5 集成测试 +
cross-end-tests）；② 删掉它，第三方能否自建——能（crate 本身就是第三方的形状）；
③ 是否新增宿主对产品事件的解释——否。

### 7.2 §5.1.4 fail-visible 三形态平移

| 形态 | 落点 |
| --- | --- |
| ① 宿主侧回查显性失败 | 垫片是**编译期**证据（引用断=编译错）；结构锁断言 `src-tauri/src/wasm_core/` 无实现文件；`PeerCtxProvider` 缺失 = `HEADLESS_UNAVAILABLE`（显性文案，无静默） |
| ② 旧产物实例化期点名 | ABI 未动（WIT 零改动），`stale_artifact_rebuild_hint` 行为不变 |
| ③ 退役词汇加载即抛 | 不涉及词汇表；权限位随迁不变 |

### 7.3 锁更新清单（票 05，与迁移同步）

| 锁 | 更新 |
| --- | --- |
| `server/crate_boundary_lock.rs` `SPLIT_CRATES` | 登记 `bedcode-wasm-core`（带路径 `bedcode-wasm-core`） |
| 新增结构锁 | `src-tauri/src/wasm_core/` 目录不存在实现文件；`lib.rs` 的 wasm_core/db 是 `pub use` 垫片 |
| `tests/hot_path_logging_lock.rs` | `LOCKED_SITES` 路径 `src/wasm_core/bus.rs` → `../packages/bedcode-wasm-core/src/bus.rs`（fs_auth 同理）；锁头注释已声明「路径可随 crate 化离开宿主」 |
| `tests/capabilities_lock.rs` / `wasm_flow_test.rs` 等 | 迁移后逐锁核对（不预期变化，跑绿即可） |
| `enums.rs` 反双份锁（既有） | 原样随迁；新垫片（db.rs / system/{process,opener,config}.rs / utils/auth.rs）套用同款锁 |

### 7.4 既有承诺不得回归

- **不推翻 ADR 0022**：四类薄壳判定不变；`session_gateway` 迁入 crate 是**位置**变化，
  角色（零解析窄转发）不变。
- **不推翻 ADR 0035/0036**：机制内核 `bedcode-host-kit`、四个能力 crate、`host-database`
  三域留内核——本票把「内核」整体搬出 bin crate，归属唯一性反而更强。
- **不触发 ABI bump**：`world plugin` 22 个 import 一个不动。

---

## 8. Out of scope（本期明确不做）

1. **移动端任何改动**。ADR 0018（移动契约独立，否决共享超集）；共享对象仍是
   `bedcode-host-kit` 的机制（前作 §11.1 已论证桌面 54,394 vs 移动 1,818 行，≈30:1）。
2. **tauri 反转为零依赖**（把 119 处 tauri 引用变成端口）。独立立项；D2 已说明理由。
3. **L2（interface 出 core / ABI bump）**。
4. **`WasmHostContext` 15 字段扁平化**、能力路由调用方身份（`.scratch/2026-10-04-wasm-core-lib-split/issues/10` 挂起中）。
5. **`bedcode-wasm-core` 发布为 crates.io 包**（仓库内 path 依赖形态；发布是产品决策，另行立项）。

---

## 9. 风险与开放问题

| # | 风险 | 缓解 |
| --- | --- | --- |
| R1 | 机械搬迁面大（119 文件），`use crate::` → 相对/外部路径重写易错 | 票 02 分步提交；每步 `cargo check` 两目标全绿；`crate::AppError` → `bedcode_server_base::error::AppError` 等替换用 ast-grep 批量 + 编译兜底 |
| R2 | `PluginHost::new` 签名扩展（peer_ctx 参数）牵动 lib.rs + 全部夹具 + cross-end-tests | 参数收进一个小结构体（`HostServicesInit`），`None` 默认；票 04 单独一票改 |
| R3 | 测试与 lib 的循环依赖（ws_e2e 用 lib composition） | §4.5 harness（≈15 行，实测 composition 只是薄壳）；顺带成为第三方装配范例 |
| R4 | 垫片与 crate 公开面漂移（lib 引用某名字而 crate 未导出） | 编译期即失败（fail-visible）；票 05 结构锁钉住「垫片=纯 pub use」 |
| R5 | 并发会话在途改动（`install_capability_domain_ports` 四文件）与票 02 冲突 | 票 01 认领；若并发会话仍在改 wasm_core，先等其落盘/提交，不得同文件双写（前作 R6 同款处置） |
| R6 | 磁盘（此前 99%）不足以支撑双目标全量编译 | 票 02 先 `cargo clean` 相关 target（需确认工作树无他人产物）；分 crate 编译天然减负（bin 不再编 wasmtime 栈？不——lib 还依赖 crate；但 crate 自身 target 独立） |

### 开放问题

- **Q1（D6）**：`AppConfig` 随迁（839 行进 crate）还是 3-getter `ConfigPort`？—— 随迁的
  代价是「引擎级配置语义」进内核 crate；端口的代价是 pty 引擎内部穿端口。**开工前定案**
  （票 04 依赖）。
- **Q2**：`PluginHost::new` 新参数形状——独立 `Option<Arc<dyn Fn(&AppHandle) -> Arc<PeerCtx>>>`
  还是聚合进现有参数？倾向独立（最小 diff）。
- **Q3**：`cross-end-tests` 是否保持依赖 `bedcode_desktop_lib`（垫片）——倾向保持（零改动）；
  备选是改依赖 `bedcode-wasm-core`（更直接，但会重复编 wasmtime 栈）。

---

## 10. Comments

- 2026-10-06：规格成稿。全部量测取自工作区实测（`git rev-parse --abbrev-ref HEAD` = `dev`），
  未凭记忆书写。行数含 in-flight 改动（§1 † 声明）。
- 2026-10-06：初稿曾设想 4+ 个端口（DbPort / PtyPort / AppHandlePort / ConfigPort），实测
  后砍到 **1 个**：`app_handle` 早已注入 `PluginHost::new` 并存进 `WasmHostContext`
  （AppHandleScope 已存在），`db` / `pty` 随迁后无需端口，mdns adapter 的全局取用改由
  crate 内注册表承担。端口数与「复用成本」直接相关，这是本票最值得审的点。
- 2026-10-06：`system/constants.rs` / `error_boundary.rs` / `crypto.rs` / `identity.rs` 已是
  纯 `pub use` 垫片（指向 bedcode-server-base / bedcode-crypto-engine）——本票的垫片方案
  不是发明新形态，是复用既有纪律。
