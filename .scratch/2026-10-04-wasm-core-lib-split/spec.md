# 桌面端 wasm_core 库化拆分：一个机制内核 + 组合式能力 crate

> Status: draft（规格已定，待开工）
> Date: 2026-10-04
> 分支：dev（**仅桌面端**；移动端零改动，双端共享是后续独立阶段，见 §11）
> 决策依据：`AGENTS.md` §5.1（宿主侧无业务代码）/ §5.1.4（落地顺序硬约束 + fail-visible 三形态）/ §6（Rust 规范）；`docs/adr/0022-plugin-host-interface-primitive-boundary.md`（边界裁决单一事实源）；`docs/adr/0017`（互调）；`docs/adr/0018`（移动端契约独立）/ `0019`（wasmtime 双端锁版）
> 同类前例：`.scratch/2026-09-19-pty-base-service/spec.md`（基础能力服务模式）、`.scratch/2026-09-21-wasm-core-audit/spec.md`（内核全面审查，票 06 的 ISP 化 / 能力装配框架是本票的地基）
> 实施票：`issues/01..09`（**串行链**，票面为准；§8 表为对照）

---

## 1. Problem Statement

> **† 量测基准声明**：本 spec 全部行数取自 `dev` 分支 **HEAD `dbe50d229`**（`bedcode-desktop/src-tauri/src/wasm_core/` = 57,592 行、`host_api/` = 15,014 行）。曾短暂存在的在途改动（`host_api/ws.rs` / `security/network_auth.rs` 被并发会话目录化拆分，见 §13）已于 2026-10-04 16:56 被该会话自行回滚，两文件恢复 HEAD 形状（1,960 / 1,830 行），故本 spec 的量测与行数标注**恢复成立**。R6 保留为「开工票 04 前的一次自检项」。

### 1.1 「耦合度高」的实测形态

`wasm_core` 共 **57,592 行**（`host_api` 15,014 / `manager` 30,378 / `security` 8,839 / 其余 3,400）。第一直觉是「跨模块耦合高」，实测**不成立**：

- **出边很干净**：`wasm_core` → lib 其他模块只有 5 个落点，每文件 1~2 处 —— `crate::db::Database`（~12）、`crate::system::*`（~20）、`crate::pty::*`+`enums`（5）、`crate::crypto`（1）、`crate::server`（~25，其中 `host_api/peer.rs` 占 20）
- **反向更干净**：lib 内只有 8 个文件引用 `wasm_core`

真正的问题是**接线耦合**：加一个 host 原语要同时改 7 处，且 22 个接口的接线全部硬编码在单文件里。

| 位置 | 内容 | 规模 |
| --- | --- | --- |
| `wasm_core/manager/runtime/component.rs` | 22 个 `impl …Host for WasmPluginState` + **22 行硬编码 `add_to_linker::<WasmPluginState, HasSelf<..>>`** + p2/p3 | **2,466 行 / 158 fn** |
| `wasm_core/host_api/context.rs` | `WasmHostContext` 15 字段上帝对象 + 13 个 `impl XxxScope for WasmHostContext` | 800 行 |
| `wasm_core/host_api/*.rs` | 每域一文件（`ws.rs` 1,960† / `database.rs` 1,518 / `http.rs` 1,161 / `mdns.rs` 795 / `peer.rs` 727 / `fs.rs` 734） | 15,014 行† |
| `packages/plugin-sdk-desktop/rust/wit/bedcode.wit` | 31 interface；`world plugin` **硬列 22 个 import / 120 条原语** | 契约单一事实源 |
| `packages/plugin-sdk-desktop/rust/src/host/*.rs` | 21 个**手写** `Host*` trait + `wasm_host.rs` 逐接口实现 | 与 WIT 无生成关系 = 第三份真源 |
| `wasm_core/manager/capability.rs` | `ROUTABLE_CAPABILITIES` / `PROBE_CAPABILITIES` 闭表（`ROUTABLE` 生产仅 `host-storage` 一项） | 455 行 |

### 1.2 三个具体病灶

1. **无法按依赖定制内核**。要让「引入一个 lib 就自动扩展 host api」，当前结构做不到：`add_to_linker` 是 22 行硬编码，且没有「实现方 crate → 自动注册」的机制。
2. **能力实现的位置是历史偶然，不是边界**。`packages/` 下已有 6 个 `bedcode-server-*` + `crypto-engine`（合计 ~17,700 行），但 `host_api/{ws,http,peer,mdns,database}.rs` 的实现仍长在 `wasm_core` 里，而 `host-fs` / `host-pty` / `host-log` 等 POSIX 原生面与 `host-database` / `host-websocket` 等协议面**混在同一目录、同一装配函数**。
3. **方向倒置**。`server/ports_impl.rs:337` 反向调用 `crate::wasm_core::host_api::mdns::shared_daemon()` —— 宿主 server 的端口层依赖 wasm_core 的**插件绑定模块**。这是 ADR 0022 裁剪线要消除的方向。

### 1.3 地基已经打好（不重建）

- 13 个 scope trait（ISP，票 04/05/06）—— `host_api/context.rs:622-706`
- `CapabilityProvider` / `CapabilityTarget` 运行期能力路由 + `CapabilityRegistry`
- `packages/plugin-system-test`（插件导出同形能力接口的形态验证）
- 权限词汇双生成物（CLI JSON + 前端 TS）+ `manifest-gen.js` 加载即抛自检
- `LoadedWasmPlugin::stale_artifact_rebuild_hint` 实例化期点名（v29 / v32 / v33 已有 4 处在用）
- `inventory = "0.3"` 已是宿主依赖（`src-tauri/Cargo.toml:127`）+ `submit_plugin!` / `collect!` 宏（SDK `traits.rs:75` / `:86`）+ `manager/host.rs:398` 收集点

---

## 2. Solution

三根支柱，缺一不可：

1. **机制内核 crate**（`packages/bedcode-host-kit`，仓库根）：`HostModule` trait + `collect!` 提交类型 + `WasmPluginState` + `HostPorts` 聚合 trait + 注册表 + 锁。这是「机制」的锚点，**双端将来共用同一份**。
2. **能力实现 crate 化**：非 POSIX 原生的 55 条原语实现搬进 `packages/`，经组合定制不同核心。
3. **auto-registry**：能力 crate 用 `inventory::submit!` 自报，宿主 `inventory::collect!` 扫描 + 白名单锁，`add_to_linker` 从 22 行硬编码变成一次循环。

### 2.1 目标形态

```
                    ┌──────────────── 机制内核（双端共享的锚点）─────────────────┐
                    │  packages/bedcode-host-kit                                │
                    │    HostModule trait · collect!(ModuleEntry)              │
                    │    WasmPluginState · HostPorts · ModuleRegistry · 锁     │
                    └───────┬───────────────────────────────────┬──────────────┘
                            │                                   │
        bedcode-plugin-api ─┤                                   ├─ wasmtime / wasmtime-wasi(p2,p3)
        （WIT 绑定已对外）   │                                   │
                            ▼                                   ▼
   ┌──────────────────────── 能力 crate（按需组合） ─────────────────────────┐
   │ bedcode-server-websocket    ws 15 原语（+ plugin_binding 子模块）      │
   │ bedcode-server-peer-net     peer 19 原语（+ plugin_binding）            │
   │ bedcode-server-http         http 入站 2 + 出站 fetch/SSE 1 原语          │
   │ bedcode-sqlite-engine    🆕  db 10 + storage 3 原语                     │
   │ bedcode-discovery-engine 🆕  mdns 5 原语 + shared_daemon                │
   └────────────────────────────────────────────────────────────────────────┘
                            │
                            ▼
   src-tauri（宿主）· wasm_core 全部保留 · host-fs/pty/log/crypto/... 留在 core
                    use bedcode_cap_* as _;  ← 强制链接（唯一一行接触）
                    host_modules![sqlite, discovery, egress, ...]  ← 白名单锁
```

### 2.2 与既有 ADR 的关系

- **不推翻 ADR 0022**：能力 crate 化后，宿主侧的「四类允许薄壳」判定不变（引擎实现 / 安全闸门 / 通用注册表 / 零解析窄转发）。模块注册表本身就是 §5.1.3 明列的「通用注册表与寻址」。
- **不推翻 ADR 0018**：本期不碰移动端，不引入「共享超集 world」。ADR 0018 否决的是「共享超集」，本方案连能力面都不共享（见 §11）。
- **不触发 ABI bump**：`world plugin` 的 22 个 import 一个不动（§7 D3）。

---

## 3. 边界判定：按「是否 POSIX 原生」二分

**判据基线（取证）**：WASI p3（wasmtime 48，`component.rs:1025` 已注册）只提供 `cli / clocks / filesystem / random / sockets`。这就是 POSIX 原生面。`world plugin` 的 22 个 import / **120 条原语**据此二分：

### 3.1 拆出 core（55 原语 / 7 接口 = 46%）

| 接口 | 原语 | 判据 |
| --- | --- | --- |
| `host-peer` | 19 | 对等网络编排引擎，非 OS 面 |
| `host-websocket` | 15 | WebSocket 是应用层帧协议；WASI 只给裸 TCP/UDP |
| `host-database` + `host-plugin-database` | 10 | SQLite 方言与文件格式 |
| `host-mdns` | 5 | 组播 DNS 协议 |
| `host-storage` | 3 | kv，真源即 db |
| `host-http` | 3 | HTTP 协议（出站 1 + 入站 2） |

### 3.2 留 core（65 原语 / 15 接口）

| 接口 | 原语 | 判据 |
| --- | --- | --- |
| `host-auth` | 10 | **编排桥接面，非实现**：实测 10 条 = `secret-get/set/delete/keys` + `auth-setting-set` + `link-identity-parts` + `auth-center-register/unregister` + `auth-methods-list/invoke`；真实认证执行在认证中心插件（ADR 0031/0033/0034） |
| `host-fs` | 9 | 文件（WASI `filesystem` 同面）；本期明确保留 |
| `host-platform` + `host-crypto` | 13 | OS/arch/env；`getrandom` + 算法引擎，非协议 |
| `host-pty` | 6 | 真 POSIX 原语（`posix_openpt`/`forkpty`）；ADR 0022:122 已论证「离开宿主就物理上无法实现」（该处论据是「WASI 0.2 无 PTY 接口 + wasmtime 默认 deny 设备 + `portable-pty` 宿主独占依赖」，非引用具体 syscall） |
| `host-bus` + `host-events` + `host-api-call` | 8 | ADR 0022 四通道，通信机制非能力 |
| `host-log` + `host-task` + `host-process` + `host-timer` | 12 | sink / 执行引擎 / `wasi:cli` / clocks |
| `host-app` + `host-config` + `host-connection` | 5 | 应用无关的应用面与在册连接事实 |

---

## 4. crate 布局

| crate | 位置 | 处置 | 内容 |
| --- | --- | --- | --- |
| `bedcode-host-kit` | **仓库根 `packages/`** 🆕 | 新建 | 机制内核（§5）。**放根而非 `bedcode-desktop/packages/`** —— 根 `packages/` 是仓库既定的双端共享位置（先例：移动端 `Cargo.toml:107-109` 依赖根 `packages/peer-net` 9,919 行 + `link-crypto` 744 行） |
| `bedcode-server-websocket` | `bedcode-desktop/packages/` | 复用 + 增 `plugin_binding` | ws 15 原语（引擎面已存在，宿主已引用 69 处） |
| `bedcode-server-peer-net` | `bedcode-desktop/packages/` | 复用 + 增 `plugin_binding` | peer 19 原语（+ 根 `peer-net` 9,919 行）；`PeerCtx` 获取改注入 |
| `bedcode-server-http` | `bedcode-desktop/packages/` | 复用 + 增 `plugin_binding` | http **入站 2 + 出站 1** 原语。入站（`registry`）已在（宿主引用 22 处）；出站（reqwest 客户端 + SSE）本期并入（D10） |
| ~~`bedcode-sqlite-engine`~~ | —（**票 07/08 已由 ADR 0036 撤销，2026-10-05**：crate 整体删除） | 不新建 | 引擎面回到 `src-tauri/src/db/`；db 10 + storage 3 原语回到 `wasm_core/host_api/{database,storage}.rs`（机制与真源同侧） |
| `bedcode-discovery-engine` | 仓库根 `packages/` 🆕 | 新建 | mdns 5 原语 + `shared_daemon`（终结 §1.2 病灶 3 的方向倒置） |

**不建 `bedcode-egress-engine`（决策 D10）**：http 出站只有 1 条 WIT 原语，独立 crate 过薄；并入 `bedcode-server-http` 的 `plugin_binding/egress.rs`（同 crate 不同模块）。实测依据见 §7.1。

**http 出站为何并入而非留在 core**：`host_api/http.rs` **本就是入站 + 出站两个方向混在一个文件**（生产出站段行 1~310 对 `bedcode_server*` 零引用；入站段行 317~349 依赖 `bedcode_server_http::registry`；另 8 处在 `#[cfg(test)]`）。既然入站已属该 crate，出站跟随即可，无需第三个归属决策。

命名对齐既有 `bedcode-crypto-engine` / `bedcode-server-*` / `bedcode-peer-net` 风格。

> `bedcode-sqlite-engine` / `bedcode-discovery-engine` 放根 `packages/` 而非 `bedcode-desktop/packages/`：两者都是**平台无关的引擎**，将来移动端大概率复用（移动端已有 `host_impl/{db,mdns}.rs`）。放根的成本为 0，收益是将来省一次搬迁。http 出站因并入 `bedcode-server-http`（D10）不单独放根。
>
> **修正（票 07 结案，2026-10-05）**：上段的「放根」只对 `bedcode-discovery-engine` 的
> **engine 部分**成立，而两个 crate 的**终态都在桌面端**：① 能力域绑定层必须自带 provider
> 侧 `bindgen!`（票 03 已实测证伪 D8 的前提），绑定桌面 WIT 真源；② 绑定层取
> `bedcode-server-base` 的共享错误类型（票 04/05/06 已定先例），而 `bedcode-server-base`
> 在 `bedcode-desktop/packages/`。故两个 crate 实落于 `bedcode-desktop/packages/`。
> 「根 `packages/` 是双端共享位」——一个必然依赖桌面基础层的 crate 放那里是**陷阱**
> （移动端永远拉不动，且读者会误以为可直接复用）。详见各票 Comments。

---

## 5. 机制内核契约（`packages/bedcode-host-kit`）

### 5.1 为什么这部分必须出 crate（两条硬约束，均已实测）

**约束一 · 被链接性。** `inventory::submit!` 展开为 linker-section 静态；未被引用的 rlib 不会进最终二进制，静态不执行 ⇒ 注册丢失。spike（`/tmp/inv-spike`，三 crate）：

```
A) app 完全不引用 cap 的任何 item   → collect() == []            ← 未链接，submit 未执行
B) app 加一行 use cap as _;          → collect() == ["cap-ws"]   ✅
```

**约束二 · Cargo 环路。** 能力 crate 必须命名两样东西：① `collect!` 里声明的类型（`inventory::collect!` 必须在**定义该类型的 crate** 里 —— spike 漏了它直接编译不过）；② `WasmPluginState`（wit-bindgen 的 `add_to_linker::<S, D>` 是**单态**的）。若这两样住在 bin crate 内的 `wasm_core`，能力 crate 就得 `depends on bedcode-desktop`，而宿主又必须 `depends on 能力 crate`（约束一）⇒ 环。spike（`/tmp/cyc`）实测 Cargo 硬拒：

```
error: cyclic package dependency: package `cap` depends on itself. Cycle:
package `cap` … path dependency `cap` of package `host` … path dependency `host` of package `cap`
（exit=101；即使 host 是 crate-type = ["cdylib","rlib"] 同样拒）
```

⇒ 一个最小锚点 crate 是**结构必需**，不是风格选择。

### 5.2 内容清单（≈250 行，其中 210 行新增、40 行搬迁）

| 项 | 行数 | 来源 |
| --- | --- | --- |
| `HostModule` trait（`desc()` + `register(linker)`） | ~30 | 新增 |
| `ModuleEntry` + `inventory::collect!` | ~20 | 新增 |
| `WasmPluginState` 及其字段类型 | ~120 | 从 `manager/runtime.rs` 搬迁 |
| `HostPorts: DbScope + PermissionScope + …`（聚合 supertrait，纯声明） | ~60 | 新增（13 个 scope trait **原地不动**） |
| `ModuleRegistry`（collect + 排序 + 锁校验 + linker 装配） | ~40 | 新增 |

### 5.3 留在 `wasm_core` 的（`context.rs` 一行不改）

`WasmHostContext` 全部 15 字段 + 13 个 scope trait **原地不动**，只新增一行 `impl HostPorts for WasmHostContext {}`。`PluginMetrics` / `StoreLimits` 通过两个窄端口 trait（`MetricsPort` / `LimitsPort`）访问，实现留在 `wasm_core`，避免 kit 依赖业务侧类型。

### 5.4 `HostModule` 描述符

```rust
pub struct HostModuleDesc {
    pub interface: &'static [&'static str],  // 如 ["bedcode:plugin/host-database"]
    pub permissions: &'static [&'static str],
    pub abi_min: u32,
}
```

**约束（D4）**：descriptor 只描述**机制**（接口路径 / 权限位 / ABI 下界），**禁带任何产品名词** —— 否则命中 §5.1 B1/B5，模块注册表从「通用注册表」变成业务容器，越红线。

---

## 6. auto-registry 机制

```rust
// host-kit
pub trait HostModule: Send + Sync + 'static {
    fn desc() -> HostModuleDesc;
    fn register(linker: &mut Linker<WasmPluginState>) -> crate::Result<()>;
}
pub struct ModuleEntry { pub module: &'static dyn HostModule }
inventory::collect!(ModuleEntry);
```

- 能力 crate：`inventory::submit! { host_kit::ModuleEntry { module: &sqlite::MODULE } }`
- 宿主：`inventory::iter::<ModuleEntry>.into_iter()` → 排序 → 锁校验 → 逐个 `register(linker)`

### 6.1 治理：自动 + 可审兼得（D5）

inventory 的固有风险是「能力集随链接到的 crate 漂移」。用**锁测试**治，而不是弃用机制：

```
collect 结果排序  ==  树内白名单常量 host_modules![sqlite, discovery, egress, ...]
```

- 漏进白名单即红（新增能力强制过 review）
- 白名单有而未实现即红（防删 crate 留残声明）
- 强制链接那一行（`use bedcode_cap_sqlite_engine as _;`）与白名单放**同一处**，不漂移

> 修正一处不准确说法（自查）：`inventory` 在本仓库是「机制在、生产未跑过」—— `submit_plugin!` 宏与 `collect!` 声明都在（SDK `traits.rs:75` / `:86`），但生产代码无任何 crate 提交 `BedcodePluginEntry`（仅 `manager/host/tests/scaffold.rs` 提交测试项），因为业务早已全搬去 WASM 应用。本期须补**跨 crate 实证测试**（spike B 即其最小形态）。

---

## 7. 关键决策

| # | 决策 | 理由 |
| --- | --- | --- |
| **D1** | 判据 = 「是否 POSIX 原生」，基线取 WASI p3 的 `cli/clocks/filesystem/random/sockets` | 有客观外部基线，不靠主观分类 |
| **D2** | `host-auth` **留 core**（初稿曾误划入拆出组） | 实测 10 条全是编排桥接 + secret-store，真实执行在认证中心插件；ADR 0031 的中心注册面属机制 |
| **D3** | 本期 `world plugin` 的 22 个 import **一个不动** | 「实现搬出 crate」与「interface 出 core」是两个层次。L1 零 ABI 变更即可拿到依赖驱动扩展；L2 是独立 ABI 决策 |
| **D4** | descriptor 禁带产品名词 | §5.1 B1/B5 红线 |
| **D5** | auto-discovery + 白名单锁，不用「不用 inventory」 | 自动与可审可兼得；锁测试即 §5.1.4 防回接锁同一手法 |
| **D6** | `host-kit` 落**仓库根** `packages/` | 根 `packages/` 是既定的双端共享位置（先例 `peer-net` / `link-crypto`）；放桌面端目录下将来要还 |
| **D7** | 本期不做平台后端 trait（`FsBackend` 等） | 最小改动；双端共享阶段的独立立项（§11） |
| **D8** | 能力 crate 一律**不自带 `wit_bindgen::generate!`** | SDK 已 `pub use wasm::bedcode`（`lib.rs:65`）⇒ `bedcode_plugin_api::bedcode::plugin::host_*::add_to_linker` 对任何依赖 SDK 的 crate 可达。沿用现有 interface 路径时，搬迁 = 纯 Rust 代码移动，WIT 零改动 |
| **D9** | 不引入 `dylib` 运行期热插拔 | 宿主进程内任意代码 = 绕过 WASM 沙箱；权限门退化为自证；`capabilities_lock.rs` 与 target 治理都会拦 |
| **D10** | **http 出站并入 `bedcode-server-http`，不建 `bedcode-egress-engine`** | 1 条 WIT 原语独立 crate 过薄；`reqwest 0.12` 已是宿主依赖（`src-tauri/Cargo.toml:98/172`）⇒ 并入**零新增编译成本**；该 crate description 本就写「HTTP **传输面**」，一名双扣；`host_api/http.rs` 本就是入站+出站混装 |

### 7.1 D10 的实测依据（顶住「出站是基于 http 构建的」这一直觉）

**前提纠正**：两者**零依赖**，不是「基于」关系，只是同名。

| | `bedcode-server-http` | `host_api/http.rs`（出站段） |
| --- | --- | --- |
| 技术栈 | `actix-web` 4（**服务端**），无 reqwest | `reqwest` 0.12（**客户端**），4 个 `LazyLock<Client>` |
| 对 `bedcode_server*` 的引用 | — | **生产出站段 0 处**（行 1~310）；入站段 4 处 `registry`；另 8 处在 `#[cfg(test)]` |
| 特殊职责 | 端点注册表 / 协议网关 / 中间件 / DTO 形状锚点 | SSRF 跳转加固（`redirect_decision` 拦 302→内网/云元数据）、超时/体积上限、SSE 流 |

**为何合**：① 同 crate 不同模块（`plugin_binding/egress.rs`）零结构成本；② `crate_boundary_lock.rs` 的 `SERVER_LIB_CRATES` 锁只禁六个传输面 crate 之间的**横向依赖**与**反向依赖宿主**，新增外部依赖 reqwest 不触锁；③ 拆第三个 crate 只为装 1 条原语，不值当一个 target 指纹 + 一份 `Cargo.toml` 治理。

**为何不留在 core**：入站已经在 `bedcode-server-http`，让出站与入站分居两处会给「http 原语归谁」留下第三个答案。

### 7.2 被否决的方案

| 方案 | 否决理由 |
| --- | --- |
| 运行期动态扩展 host interface | Component Model 下 guest import 段编译期固定；宿主注册更多接口无害，但无法让未按该 WIT 编译的 guest 去 import |
| 扩 `ROUTABLE_CAPABILITIES` 作为主要路线 | 它只能**替换已声明接口**的实现（运行期路由），不能新增 guest 可 import 的接口。是 A 的补充，不是替代 |
| `inventory` 真·零表无锁 | 能力集随链接到的 crate 漂移，与 fail-visible / 确定性 / review 可审三条硬要求冲突 |
| 每个能力 crate 自带 WIT 生成 | D8 已说明不需要；自带会引入 wit-bindgen 多 `generate!` / `export!` 宏重名风险（SDK `wasm_ws.rs` 注释已记录该坑） |

---

## 8. 分期落地

**实施以 `issues/01..09` 为单一事实源**（串行链，每票只依赖前票；`Blocked by` 行写在该票文件里）。下表只作对照，票面为准。

> ⚠️ **票 07 / 08 已撤销（2026-10-05，ADR 0036）**：SQLite 能力域 crate
> `bedcode-sqlite-engine` 整体删除，`host-database` / `host-plugin-database` /
> `host-storage` 三 interface 与引擎面都留在 `wasm_core`——它们的真源（`plugin_auth_*` /
> `plugin_secrets` / `plugin_storage` 表）与授权判定本来就在宿主，机制面出内核会让归属出现
> 两个答案。票 01~06 / 09（机制内核 + mdns / ws / peer / http 四域 + 契约锁）不受影响。

| 票 | 内容 | 行为变更 | ABI |
| --- | --- | --- | --- |
| **01** | 修正三处文档失真 | 无（纯注释） | 无 |
| **02** | 认领 `wasm_core` 在途改动，定开工顺序（**gate**） | 无（不碰代码） | 无 |
| **03** | 🟢 **机制内核 crate + mdns 域全链路（tracer bullet）** | 无 | 无 |
| **04** | ws 域（15 原语）→ `bedcode-server-websocket` | 无 | 无 |
| **05** | peer 域（19 原语）→ `bedcode-server-peer-net`，解 20 处反向耦合 | 无 | 无 |
| **06** | http 域（入站 2 + 出站 1）→ `bedcode-server-http`（D10） | 无 | 无 |
| **07** | ~~建 `bedcode-sqlite-engine` + `db/` 整体搬迁（expand，留转发层）~~ **已撤销**（ADR 0036） | 无 | 无 |
| **08** | db + kv 绑定层迁入 + 删转发层（**contract**，20 引用点改指） | 无 | 无 |
| **09** | 契约收口：白名单锁 + 跨 crate 实证 + 路由表扩展 + ADR/CHANGELOG/code-map | **有**（新增可路由能力） | 无 |

**实施结果（2026-10-05）**：票 01-09 全部完成，ADR `docs/adr/0035-*.md` 落档。票 09 结案时
**新增立项票 10**（`issues/10-capability-forward-caller-identity.md`）：运行期路由链路不传
调用方身份，而每个能力域的真源都按 `plugin_id` 分区 ⇒ **运行期替换机制当前对全部能力域都
不可用**；票 09 因此只接通机制、不开放入口（`world plugin-system` 未加 `export host-mdns`，
路由在构造上不可达）。

### 8.1 与本 spec 早期草案的三处修正（record）

1. **取消「`component.rs` 按接口拆分」作为独立票**。它是水平切片。各域迁出时其接线与实现一起搬走，该文件**自然收缩**；先花一票专拆是为不可见的收益付费。收缩是 03~08 的副产品。
2. **「机制内核先建、域后迁」改为单票（03）**。内核建成但注册表为空、22 个接口仍硬编码 = 无 bullet 可言，也验证不了「加依赖即注册」成立。03 把内核与首个能力域（mdns）捆成一个可演示的完整路径。
3. **bullet 押 mdns 而非 storage/db**。mdns 约 795 行自包含（只依赖组播库 + 事件总线 + 权限窄端口）、不拖数据库层，且**独立修掉一个真实缺陷**（端口层反向依赖）；storage 看似更小（153 行）但真源建在 db 上，会把整个数据库搬迁拽成第一步。

**L2（独立立项，不在本期）**：ABI v35，capability import 移出 `world plugin` + 全产物重建 + rebuild hint 点名。触发 §5.1.4 落地顺序硬约束（ABI bump + 双端 WIT 副本同步 + 移动端影响评估 + CHANGELOG 双语条目）。

---

## 9. 红线与门禁自检

### 9.1 §5.1 B1-B6 零命中自检

| 判据 | 自检 |
| --- | --- |
| B1 产品类型/字段 | 能力 crate 只搬现有原语实现，不新增任何业务名词类型 |
| B2 业务编排 | crate 化不新增编排；编排留在插件侧 |
| B3 业务真源 | 真源位置不变（sqlite 仍是宿主主库 + 插件私有库） |
| B4 业务投影 | 不新增 DTO |
| B5 业务默认值/策略 | descriptor 只带接口路径 / 权限位 / ABI 下界（D4） |
| B6 业务生命周期挂钩 | 不新增对产品事件的解释或回调 |

**§5.1.2 三问**：① 离宿主能否实现 —— 能力实现本就是宿主引擎，拆 crate 不改变归属；② 携带产品语义 —— 否；③ 都不命中 ⇒ 允许进宿主，但须满足 WIT 纯增量（本期 WIT 零改动 ✓）/ 权限位有门禁落点（`check_permission` 随实现同迁 ✓）/ 停用可回收（`purge_for_plugin` 不变 ✓）。

**提交前自检 3 问**：① `HostPorts` 聚合 trait 在宿主有第二个消费者吗 —— 有（22 个 `impl Host` + 13 个 scope 实现）；② 删掉它，第三方能否用同一形状的既有原语自建 —— 能（`ModuleEntry` + `HostModule` 就是给第三方的形状）；③ 是否新增宿主对产品事件的解释 —— 否。

### 9.2 §5.1.4 fail-visible 三形态平移

| 形态 | 本方案的落点 |
| --- | --- |
| ① 宿主侧回查显性失败 | 能力 crate 未链接 ⇒ `collect` 结果缺项 ⇒ 锁测试红 + 实例化期点名「缺 `X` 模块，请检查依赖」 |
| ② 旧产物实例化期点名 | 复用 `stale_artifact_rebuild_hint` 通道（v29/v32/v33 已有 4 处先例） |
| ③ 退役词汇加载即抛 | 权限位不在词汇表 ⇒ `manifest-gen.js` 现有自检直接套用（crate 不改变该链路） |

### 9.3 既有锁不得回归

- `capabilities_lock.rs`：新 crate 不得触碰 ACL / CSP / 撤权限↔补命令配对
- `wasm_flow_test.rs` 四个退役域锁：拆分后仍全绿
- `stale_artifact_rebuild_hint`：ABI 未动 ⇒ 行为不变
- `retired_tables_are_not_created`：`src/db/` 搬迁后 schema 仍只有 5 张表（`settings` / `plugin_storage` / `plugin_secrets` / `plugin_auth_policies` / `plugin_auth_records`）

---

## 10. 顺带发现的文档失真（§0「文档字面 ≠ 事实」，先修文档）

| # | 位置 | 失真 |
| --- | --- | --- |
| **F1** | `packages/plugin-sdk-desktop/rust/wit/bedcode.wit` `host-auth` 注释 | 仍写「真源是内核表 `pairings` / `connection_history`」「v18 记录面 `trusted-devices-*`」，而 `schema.sql` 现存表只有 5 张，前两张已随 ABI v31/v32/v34 退役 |
| **F2** | `packages/peer-net/src/discovery.rs:7` | 写「两套发现互不感知、可同进程共存（mdns-sd 多 ServiceDaemon 实例合法）」，但同文件 `:720` 打的是 `"peer mDNS discovery daemon started (shared daemon)"`，且守护经 `Ports::shared_daemon()` 注入（`bedcode-server-base/src/ports.rs:184`）—— **注释是错的** |
| **F3** | `packages/plugin-sdk-desktop/rust/src/traits.rs:143` | ~~疑似编译破口~~ **已查实：不是**（票 01 结案）。inventory 0.3.24 的 `iter::<T>` 是 `Deref<Target = fn() -> Iter<T>>` 的静态，故 `inventory::iter::<T>()` 与 `inventory::iter::<T>.into_iter()` 两种写法都合法且等价；SDK 与宿主解析同版本，`cargo test inventory` 绿 |

---

## 11. Out of scope（本期明确不做）

1. **移动端任何改动**。ADR 0018 明文「移动端插件契约独立于桌面端维护」，且**否决过共享超集方案**：「会把移动端宿主永远 `unreachable!` 的接口泄漏给插件 SDK。这一不对称是产品决策，不是遗漏」。
2. **平台后端 trait**（`FsBackend` / 密钥后端 / 路径解析）。取证：桌面 `security/fs_auth.rs` 3,209 行（弹窗授权）vs 移动 SAF（`saf_io.rs` 856 + `saf_path.rs` 212 + `fs_auth.rs` 532 = 1,600 行）。这是双端共享阶段的独立立项（D7）。
3. **L2（interface 出 core / ABI v35）**。
4. **`WasmHostContext` 15 字段的扁平化**。本期只加聚合 trait，不动字段（最小改动）。

### 11.1 双端共享阶段的前置（记录，本期不执行）

- **可共享量级**：桌面 `wasm_core` 57,592 行 vs 移动 `plugin/wasm_runtime` 4,127 行（**1:14**）。桌面多出的 14 个 interface 移动没有（`host-pty` / `host-auth` / `host-websocket` / `host-task` / `host-plugin-database` …）。`component.rs` 函数集合双端重叠 **36%（57/158）**。
- **真正的共享对象是「机制」**（约 1,000~1,500 行）：`compile/load → linker 装配 → instantiate → verify_abi → rebuild hint → 生命周期与导出调用 → 燃料/限额`。这恰是本期建出的 `bedcode-host-kit` 的主体内容。
- **ADR 0019 当前处于显式偏离**：双端锁 wasmtime 48，但 2026-09-18 桌面升 48.0.2 时移动仍在 47，管控手段含「不改 SDK 绑定与组件编码工具链」。共享机制内核前该偏离须先归零。
- **结论口径**：目标表述为「**一个机制内核 + 两层能力面 + 3 个平台后端 trait**」，与 ADR 0018 的「能力面有意不对称」不冲突，无需推翻任何现行 ADR。

---

## 12. 风险与开放问题

| # | 风险 | 缓解 |
| --- | --- | --- |
| R1 | Step 1 涉及 `WasmPluginState` 的全局 import 重写，机械面大 | 纯搬迁不改语义；分步提交，每步跑全量 `cargo test` |
| R2 | `src/db/` 搬迁牵动 20 个引用 `crate::db` 的文件 | 与 `schema.sql` + 幂等迁移测试同行；`retired_tables_are_not_created` 锁把关 |
| R3 | auto-registry 引入全局注册表，可能掩盖「模块没链上」 | 锁测试 + 实例化期点名（§9.2 形态①） |
| R4 | `host-kit` 未来要给移动端用，但移动端当前 ABI 11 / WIT 19 interface | 本期不动移动；kit 内的能力面装配须按「只声明机制、不假设 interface 存在」设计 |
| R5 | `peer-net` 在**仓库根** `packages/` 而 `bedcode-server-peer-net` 在 `bedcode-desktop/packages/`，peer 能力 crate 归属需定 | 倾向：binding 放 `bedcode-server-peer-net`，引擎留根 `peer-net`（维持现状，本次不搬引擎） |
| R6 | ~~工作区存在他人未提交的在途改动~~ **已消解**（票 02）：并发会话的测试目录化拆分在 16:56 自行回滚，两文件回到 HEAD 形状，票 04 无阻塞 | 降级为**开工自检项**：票 04 动手前 `git status --short …/host_api/ws.rs` 确认干净；若并发会话重启扫描，改走「先提交拆分、再在新形状上搬迁」，不得同文件双写 |

### 开放问题

- ~~**Q1** `bedcode-egress-engine` 独立还是并入 net crate？~~ → **已定（D10）：并入 `bedcode-server-http` 的 `plugin_binding/egress.rs`**
- ~~**Q2** Step 2 的 `plugin_binding` 子模块放 crates 内部还是各 crate 各自的 `plugin_binding/` 目录？~~ → **已定（票 03-08 实施）**：各 crate 内**独立文件** `src/plugin_binding.rs`（`plugin_binding/` 目录只在接口数超过一组时开，如 http 的 `plugin_binding/egress.rs`）
- **新增（票 09 结案）**：**Q3** 运行期能力路由的调用方身份怎么传（WIT 显式参数 / 每能力 provider 池 / 键名空间前缀）——三个候选与判据见票 10，**必须先定案再动手**（其中两个要动 WIT 或 ABI）

---

## 13. Comments

- 2026-10-04：规格成稿。关键实测数据全部取自当前工作区（`git rev-parse --abbrev-ref HEAD` = `dev`），未凭记忆书写。
- 2026-10-04：初稿曾把 `host-auth` 划入拆出组，经核 `bedcode.wit` 的 10 条原语全为编排桥接 + secret-store 后修正为留 core（D2）。
- 2026-10-04：spike 位于 `/tmp/inv-spike`（三 crate，验证强制链接）与 `/tmp/cyc`（验证 Cargo 环路硬拒），**均在仓库外**，用完即弃；实施时把 spike B 的形态固化成跨 crate 自动化测试。
- 2026-10-04（补）：用户质疑 `bedcode-egress-engine` 是否必要 ⇒ 复核后**取消该 crate**（D10）。实测推翻「出站基于 http 构建」的前提（两者零依赖），但认同「不必第三个 crate」的结论。依据写进 §7.1。
- 2026-10-04（补）：核对 spec 引用时发现**工作区有他人未提交的在途改动**（`host_api/ws.rs` -847、`security/network_auth.rs` -1,004，另有 `ws/tests/`、`network_auth/tests/` 未跟踪目录，mtime 15:40 持续变动）。会话开始时的 `git status` 里这两个文件尚未被修改。**已按 §11「非本任务的改动一律不碰」处理**：本 spec 全部量测改用 HEAD 基准并加 † 声明，风险登记为 R6。**需用户确认那批在途改动的归属**，再决定 Step 0/2 何时开工。
- 2026-10-04（补，票 02 结案）：那批在途改动已查实为**另一个并发会话的「内联 Rust 测试目录化拆分」**（未跟踪脚本 `scripts/audit-rust-tests.mjs` + `scripts/split-rust-tests.mjs` 驱动），与本 spec 的实现归属拆分**目标正交、不可合并**。该批次在 16:53~16:54 落盘后、于 16:55~16:56 被并发会话**自行回滚**（两个 `tests/` 未跟踪目录已删、`ws.rs` 回到 1,960 行、`network_auth.rs` 回到 1,830 行）。HEAD 仍为 `dbe50d229`。故：票 04 **无需等待**即可开工，R6 降级为开工自检项，§1 的 † 声明撤销。详见 `issues/02-claim-inflight-changes.md`。
- 2026-10-04（票 01 结案）：§10 的 F1 / F2 已改（WIT `host-auth` 头注释改写为 10 条原语的真源归属；peer-net 发现模块头注释改写为「服务类型隔离但共用宿主共享守护」）。F3 经查**不是编译破口**（inventory 0.3.24 的 `iter::<T>` 有 `Deref` 到 fn 指针，两种写法等价，`cargo test inventory` 绿），§10 该行已更正。
