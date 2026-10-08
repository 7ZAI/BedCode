# 能力域脱绑桌面端：根 packages 各能力域独立可被任何宿主引用（实施前置条件）

> Date: 2026-10-08
> Status: **spec（未实施）**——用户指令「先写 spec 先不用实现」。
> 用户方向指令：「package 下各个能力域的脱离绑定桌面端，应该独立可以被任何宿主引用才对」——
> 本 spec 是 `.scratch/2026-10-08-dual-end-shared-libs/spec.md`（双端共享 mDNS/对等网络）的**实施前置条件**：
> 先让能力域脱绑，下游的 mDNS 解绑（M1/M2）与移动端 host-peer 解绑（D6）才有地基。
> 全部事实取自 2026-10-08 工作区实测（行号 / 依赖 / 消费者可复现）。
> 前置文档：ADR 0018（移动契约独立）/ 0022（宿主插件边界）/ 0035（能力域 crate 化）/ 0037（wasm-core 整核抽出）；
> `.scratch/2026-10-07-capability-crates-to-root-packages/`（8 能力域迁根记录）。

---

## 0. 一句话目标

根 `packages/` 下**带桌面 WIT 绑定的 5 个能力域**（`bedcode-discovery-engine` / `bedcode-server-http` /
`bedcode-server-websocket` / `bedcode-server-peer-net` / `bedcode-pty-engine`）**从桌面端解绑**：
能力域 crate 默认形态 = 纯引擎机制 + 端口抽象（零 WIT 依赖），**任何宿主**（桌面 / 移动端 / 无头测试宿主）
可直接引用；桌面 WIT 绑定层（`bindgen!` / `HostModule` / `inventory` / `impl Host`）收进 `desktop-host`
feature，桌面宿主开 feature 后语义逐字不变。

---

## 1. 现状盘点（2026-10-08 实测）

### 1.1 WIT 绑定矩阵（Cargo.toml 依赖 + 源码命中双重核验）

| 根 crate | wasmtime/wit-bindgen/plugin-api/inventory/host-kit 依赖 | bindgen!/HostModule/impl Host 命中文件 | 绑定层行数 |
| --- | --- | --- | --- |
| `bedcode-discovery-engine` | 全有 | `src/lib.rs`（bindgen! + HostModule + inventory + 5 Host impl）、`src/ports.rs`（注释提及）| lib.rs 132 |
| `bedcode-server-http` | 全有 | `src/plugin_binding.rs` + `plugin_binding/ports.rs` | 221 |
| `bedcode-server-websocket` | 全有 | `src/plugin_binding.rs` + `plugin_binding/ports.rs` | **1,216** |
| `bedcode-server-peer-net` | 全有 | `src/plugin_binding.rs` + `plugin_binding/ports.rs` + `dependency_direction_lock.rs` | 692 |
| `bedcode-pty-engine` | 全有 | `src/plugin_binding.rs` + `plugin_binding/ports.rs` + `src/lib.rs` | 191 |
| `bedcode-server-base` | 仅 `bedcode-plugin-api`（**无** wasmtime/wit-bindgen）| 0 | `constants.rs:107` `pub use bedcode_plugin_api::constants::{…}` + `ports.rs:60` 引 `BusMessage` 类型 |
| `bedcode-server-core` | 无 | 0 | 已脱绑 |
| `bedcode-crypto-engine` / `peer-net` / `link-crypto` | 无 | 0 | 已脱绑（peer-net 引 mdns-sd 但非 WIT）|
| `bedcode-host-kit` / `bedcode-wasm-core` | 机制本体（wasmtime 类型 / bindgen）| module.rs、lib.rs 等 | **机制层，不在本 spec 范围**（§8）|

### 1.2 统一结构模式（脱绑可行性底座）

5 个能力域结构同构：`引擎模块 + plugin_binding（bindgen! + HostModule + inventory::submit! + impl Host for WasmPluginState）`；
`plugin_binding/ports.rs` 端口 trait **纯 Rust**（`BoxedTask` / `BoxedBlocked` / `PtyPorts` / `DiscoveryPorts` 等，零 wasmtime 类型依赖——
命中仅为注释提及）。即「引擎 + 端口」与「WIT 绑定层」物理可分。

### 1.3 桌面 WIT 依赖的两个层次

1. **类型/宏层**：`wasmtime`（`Linker`/`HasSelf`）、`wit-bindgen`（`bindgen!`）、`inventory`（`submit!`）、
   `bedcode-host-kit`（`HostModule`/`HostModuleDesc`/`ModuleEntry`/`WasmPluginState`）——绑定层专属；
2. **常量层**：`bedcode-plugin-api`（桌面 SDK）的纯字符串/类型常量被能力域引用——如 discovery-engine 的
   `owned_topic`/`MDNS_FOUND`/`MDNS_LOST`、server-websocket 的 `WS_CLIENT_CONNECT` 等、server-base 的
   `constants` 集。**纯 wire 契约，不该挂在桌面 SDK 上**（C4：桌面 SDK 另有消费方，不能删，须自持副本 + 漂移锁）。

---

## 2. 目标形态

```text
packages/bedcode-<capability>/            （能力域，默认 = 纯引擎）
├── engine 模块（无 WIT：句柄表 / 事件循环 / 端口调用）
├── ports.rs（端口 trait，纯 Rust——任何宿主实现即用）
├── 常量自持（topic / 事件名 / wire 形状，不再依赖 bedcode-plugin-api）
└── plugin_binding.rs  ←── 包进 #[cfg(feature = "desktop-host")]
      bindgen! / HostModule / inventory::submit! / impl Host
Cargo.toml: wasmtime / wit-bindgen / inventory / host-kit / plugin-api 全部 optional，
            desktop-host = ["dep:wasmtime", "dep:wit-bindgen", "dep:inventory", "dep:bedcode-host-kit", "dep:bedcode-plugin-api", …]
```

- **任何宿主**：默认（不开 feature）引能力域 → 纯引擎可用（移动端、无头测试、第三方宿主）；
- **桌面宿主**：`features = ["desktop-host"]` → 绑定层装配，行为逐字不变（ABI/WIT/权限位/事件形状零变化）。

---

## 3. 方案设计

### 3.1 统一脱绑契约（P1 定案，全部能力域同一模式）

1. **feature 命名统一**：`desktop-host`（语义 = 桌面宿主 WIT 绑定层；移动端/无头宿主永不开启）。
2. **绑定层 cfg 包裹**：`#[cfg(feature = "desktop-host")] mod plugin_binding { … }`（含 `bindgen!`、`HostModule` impl、
   `inventory::submit!`、`impl bedcode::plugin::host_X::Host`）；`lib.rs` 的 `pub mod plugin_binding;` 同步 cfg。
3. **optional 依赖**：`wasmtime` / `wit-bindgen` / `inventory` / `bedcode-host-kit` / `bedcode-plugin-api` 全部
   `optional = true`；`desktop-host = ["dep:…"]`（依赖按各域实际面收窄，不硬性全带）。
4. **常量下沉**：能力域引用的 plugin-api 常量（topic / 事件名 / wire 形状）改为**能力域自持副本**；双真源
   （SDK 原常量 vs 能力域副本）落**结构锁**（逐字一致比对，漂移即红）。server-base 的 `pub use …constants` 同法。
5. **crate 描述更新**：Cargo.toml `description` 加「引擎默认 / `desktop-host` feature 装配 WIT 绑定层」。
6. **强制引用 gate**：桌面侧「`use bedcode_xx as _;` 保证 inventory 静态注册」行同步 `#[cfg(feature = "desktop-host")]`
   （无头/移动端宿主不开 feature 时**不应**注册——它没有插件宿主机制）。

### 3.2 逐个能力域清单（P2–P4，每票一个域 + 全量回归）

| 票 | 能力域 | 特殊点 |
| --- | --- | --- |
| P2 | `bedcode-discovery-engine` | topic 常量自持（`<owner>::mdns:found|lost` 规则）；**下游 dual-end-shared-libs M1/M2 的直接前置** |
| P3 | `bedcode-server-http` / `bedcode-server-websocket` | ws 域常量（`WS_CLIENT_CONNECT` / `WS_CLIENT_DISCONNECT` / `WS_CLOSE` 等）自持；server-websocket 绑定层最大（1,216 行） |
| P4 | `bedcode-server-peer-net` / `bedcode-pty-engine` | **下游 dual-end-shared-libs D6（移动端 host-peer 解绑共享）与 pty 能力域共享的前置**；server-peer-net 的 `dependency_direction_lock.rs` 核对依赖方向不变 |
| P5 | `bedcode-server-base` + wasm-core/桌面宿主接线 | server-base 常量下沉；wasm-core 的 `use … as _;` 强制引用 gate；桌面 Cargo.toml 各能力域加 `features = ["desktop-host"]` |

### 3.3 验证矩阵（P6）

1. **桌面**：`cargo test` 全量（开 `desktop-host`，wasm-core 零回归——语义逐字不变）；
2. **移动端**：任一能力域默认引编译通过（`cargo tree -e features` 核对零 WIT 依赖）；
3. **无头测试宿主**：新开最小测试 crate（或借用 `plugin-system-test` 形态）默认引能力域跑引擎单测；
4. **锁**：`packages/.cargo/config.toml` SPLIT_CRATES / `dependency_direction_lock` 核对（脱绑不改 crate 名与依赖方向，预期零动；若有漂移同步修）。

---

## 4. 票划分（渐进：每票一件事、可验证、可回退）

| 票 | 内容 | 门禁 |
| --- | --- | --- |
| **P1 · 脱绑契约定案 + 样板域** | 选最小绑定面域（`bedcode-server-http`，221 行）做脱绑样板：feature/cfg/optional/常量自持/强制引用 gate 全套；契约写进本 spec §3.1 定稿 | 桌面 cargo test（开 feature）+ 无 feature 编译双态验证；`cargo tree` 依赖面核对 |
| **P2 · discovery-engine 脱绑** | §3.2 表 | 桌面 mdns 域测试 + 移动端默认引编译 + **topic 常量漂移锁** |
| **P3 · server-http / server-websocket 脱绑** | §3.2 表 | 桌面 http/ws 域测试全量 |
| **P4 · server-peer-net / pty-engine 脱绑** | §3.2 表 | 桌面对等网络 / pty 测试全量 |
| **P5 · server-base 常量下沉 + wasm-core/桌面接线** | §3.2 表 | 桌面全量 cargo test；`rg "bedcode_plugin_api" packages/bedcode-*` 归零（引擎面）|
| **P6 · 验证矩阵 + 文档/锁收口** | §3.3 全项；AGENTS.md 路径基准、双端 code-map、ADR 0035 修订补「能力域脱绑」条目、CHANGELOG 双语 | 四态验证全绿；文档与事实一致 |

---

## 5. 验证门禁（每票通用）

1. **双态编译**：同一能力域「无 feature（纯引擎）」与「`desktop-host`（绑定层）」都能独立编译；
2. **语义零变化**：桌面开 feature 后 ABI / WIT / 权限位 / 事件形状 / inventory 注册逐字不变（wasm-core 全量回归）；
3. **依赖面**：无 feature 时 `cargo tree` 无 wasmtime / wit-bindgen / inventory / bedcode-plugin-api / host-kit 实编译依赖
   （Cargo.lock 可记录 optional，但不进编译图）；
4. **常量锁**：能力域自持副本与桌面 SDK 原常量逐字一致（结构锁，漂移即红）；
5. **宿主独立性**：至少一个非桌面宿主形态（移动端默认引 or 无头测试 crate）编译通过。

---

## 6. 约束与风险（不可自行放松，冲突按 AGENTS §0 优先级上报）

| # | 约束/风险 | 影响 |
| --- | --- | --- |
| C1 | 移动端不能拉桌面 WIT（ADR 0018/0037 C2）| 脱绑后移动端默认引能力域必须零 WIT——feature gate 的 optional 依赖必须收干净 |
| C2 | **bindgen! / inventory 在 cfg(feature) 下的行为需实测** | 宏展开通常跟随条件编译，但 `inventory::submit!` 依赖「宿主二进制 use 该 crate」的 link-section 静态——P1 样板票首验；若 cfg 包裹导致展开异常/注册丢失 → **退拆壳方案**（绑定层拆出独立 `*-binding` crate，引擎保持裸 crate），票内记录原因 |
| C3 | 桌面 SDK `bedcode-plugin-api` 常量另有消费方（wasm-core `host_api/bus.rs`、`bedcode-server-websocket/channel/plugin.rs` 等）| 常量下沉 = 自持副本 + 漂移锁，**不删除** SDK 原常量（§3.1 ④）|
| C4 | **并行会话在途（票 13/15，session 01a1191e 活跃）** | 本 spec 桌面面（P1–P6 桌面侧）不受在途影响；移动端「默认引编译」验证若被在途中间态阻塞（egress 6 失败基线、session/terminal 在途），只验证编译面并写明原因 |
| C5 | 桌面 `Cargo.lock` / 移动端 `Cargo.lock` 变更 | optional 依赖不进编译图但可能进 lock——锁文件只经包管理器变更（AGENTS §9）|
| C6 | `dependency_direction_lock.rs`（server-peer-net）| 脱绑不改 crate 名与依赖方向（host-kit/plugin-api 变 optional 不改变方向），预期零动；若有漂移 P4 同步修 |

---

## 7. 与下游 spec 的关系（前置 → 实施）

```text
本 spec（能力域脱绑，P1–P6）
  ├── P2 完成后 → dual-end-shared-libs M1/M2（mDNS 解绑 + 双端共享）可启动
  ├── P4 完成后 → dual-end-shared-libs D6（移动端 host-peer 接入层解绑共享）可启动
  └── P5 完成后 → 移动端插件机制复用 wasm-core（2026-10-07-mobile-wasm-core-refactor 票 17–19）的前置更充分
```

- `dual-end-shared-libs` spec 的 M1 票描述更新：依赖本 spec P2，不再自建脱绑。
- 本 spec 不触碰 mDNS topic 统一（D2）与移动端接线（M3）——那是下游 spec 的票。

---

## 8. Out of scope

- `bedcode-host-kit` / `bedcode-wasm-core`（插件**机制本体**：wasmtime 单态 `WasmPluginState`、bindgen 装配、manager/runtime）——
  它们是宿主机制不是能力域；host-kit 的 wasmtime 依赖随移动端复用 wasm-core（票 17）一并评估，不在此脱绑。
- 桌面 SDK `bedcode-plugin-api` 本体（插件契约，双端 SDK 各自演进，ADR 0018）。
- 能力域业务语义（B1–B6 判据面）——本 spec 纯机制面，零产品名词新增。
- 对等网络 / mDNS 的共享落地细节（见下游 `dual-end-shared-libs` spec）。
