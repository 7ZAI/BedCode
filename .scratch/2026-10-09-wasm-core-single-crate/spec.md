# wasm-core 单一 crate + WIT 分片组合（能力域扫描装配）

Status: **planning（票 01 POC **完成**：P1 / P2 / P4 **已证**，P3 **不成立**（编译绿、依赖图 28 处红）⇒ 新增票 02 依赖图脱桌面；记录见 `ticket-01-pty-wit-slice-poc.md` §6）**
Date: 2026-10-09
前置：ADR 0035（能力域 crate 化 + 自动装配）、ADR 0037（wasm-core 整核抽出）、ADR 0038/0039（引擎面外迁 + pty 域整面迁出）、ADR 0040（移动 fork 两步走）、ADR 0018（移动契约独立）、ADR 0019（双端锁版）、ADR 0022（边界裁决）

## 0. 用户指令（原文，本 spec 的目标与裁决来源）

> 「是的我需要只有一份 wasm-core，双端核心机制完全一致」
> 「强制 wasm-core 是单一 crate，wasm-core 通过扫描能力域 crate lib 完成 host 扩展，单纯的 wasm-core 是双端完全一致的，但是组合的能力 lib……调研 WIT 可以组合吗？」

裁决（2026-10-09 三问拍板）：

| # | 裁决点 | 结果 |
| --- | --- | --- |
| A | WIT 分片 package 策略 | **同 package 拼装**：全部分片声明 `package bedcode:plugin`，构建脚本收集到端 `wit/` 单目录，world 用 `include` 组合；import 名不变 |
| B | 核心 world 切片粒度 | **交集切片**：11 个全等 interface + `host-websocket` / `host-fs` / `host-platform` / `host-http` / `host-events` 的交集子集进核心 |
| C | 落地节奏 | **先 POC 后全量**：pty 域端到端 POC 验证「WIT 分片 + 能力域自带 bindgen + 扫描装配 + 移动侧无桌面依赖编译」 |

---

## 1. 现状（2026-10-09 实测，勿凭记忆）

### 1.1 crate 与契约规模

| 项 | 桌面（根 `packages/bedcode-wasm-core`） | 移动（`bedcode-mobile/packages/bedcode-wasm-core`） |
| --- | --- | --- |
| 规模 | 47,563 行 / 134 文件 | 23,930 行 / 68 文件 |
| WIT | 30 interfaces，`world plugin` = **21 import / 5 export** + 4 个附加 world | 22 interfaces，2 个 world |
| ABI | **35** | **19** |
| 共有 interface | 20 个（其中 **11 个函数集完全一致**） | 同左 |

### 1.2 真正根因：能力域 crate 被端 WIT 绑死（实测）

五个能力域 crate 的 provider 侧 `bindgen!` **全部指向整份端 WIT**：

```220:221:packages/bedcode-server-http/src/plugin_binding.rs
    path: "../../bedcode-desktop/packages/plugin-sdk-desktop/rust/wit/bedcode.wit",
    world: "plugin",
```

```617:618:packages/bedcode-server-peer-net/src/plugin_binding.rs
    path: "../../bedcode-desktop/packages/plugin-sdk-desktop/rust/wit/bedcode.wit",
    world: "plugin",
```

（`server-websocket` / `discovery-engine` / `pty-engine` 同款，pty-engine 见 `src/plugin_binding.rs:131`。）

⇒ 能力域 crate 在**契约面**上绑死桌面：移动侧无法复用同一能力域 crate ⇒ 「差异只来自组合的能力 lib」在当前形态下不成立。这是本 spec 要解决的第一性问题。

### 1.3 扫描装配机制已就绪（无需新造）

`packages/bedcode-host-kit/`：

| 机制 | 落点 |
| --- | --- |
| 描述符（接口路径 / 权限位） | `module.rs` `HostModuleDesc { interfaces, permissions }` |
| 能力模块契约 | `module.rs` `HostModule::install(&self, linker: &mut Linker<WasmPluginState>)` |
| 自报 + 静态收集 | `module.rs` `submit_module!`（`inventory::submit!`）、`registry.rs` `ModuleRegistry::collected()` |
| 白名单双向校验 | `registry.rs` `verify_whitelist()`（多出 / 少了一律红） |
| 统一装配 | `registry.rs` `install_all(linker)` |

⇒ 「wasm-core 通过扫描能力域 crate lib 完成 host 扩展」= **既有机制**（ADR 0035 D2/D3/D5 落地，pty 域是先例）。本 spec 只补「契约分片」，让能力域不再绑端。

### 1.4 WIT 组合性（源码级结论，决定方案可行）

1. **语言层：可组合**。`use` 跨 interface 复用类型（同 package）；world 可 `include` 另一个 world（同 / 跨 package 均可）；world 可 `import/export` 其他 package 的 interface。（component-model 官方 WIT Reference「Including other worlds」「Interfaces from other packages」）
2. **工具链层**：跨 package 依赖靠 `deps/` 目录。**项目实际锁定的 `wit-parser-0.254.0`**（桌面 `src-tauri/Cargo.lock`）在 `src/resolve/fs.rs:56-135` 明示自动探测 `deps/my-package/*.wit`、`my-package.wit`、`my-package.{wasm,wat}` 三种形态（`0.256.0` 同款，`src/resolve/fs.rs:119`）；**同 package 多文件必须同目录**（不同目录 = 不同 package）。
3. **分片自包含性已证**：`host-pty` interface 定义无跨 interface `use`（`sed -n '692,760p'` + `grep use` 无命中）⇒ 抽出为独立 `.wit` 分片后，能力域 crate 单文件即可 bindgen，**无需 deps**。
4. **装配层**：`component::Linker` 可被多个 crate 各自 `add_to_linker` 填充（本项目五域已在跑）。

### 1.5 双端共有 interface 的函数级差异（`/tmp/wit_iface_diff.py` 可复跑）

- **完全一致（11）**：`command`(1)、`lifecycle`(4)、`manifest`(1)、`events-binary`(1)、`host-bus`(5)、`host-config`(1)、`host-log`(5)、`host-storage`(3)、`host-plugin-database`(5)、`host-mdns`(5)、`host-peer`(19)
- **真子集 / 可切片**：
  - `host-websocket`：移动 **5 ⊂ 桌面 15**（桌面多服务端 10）
  - `host-fs`：交集 6（移动独有 `save-to-document` / `write-media-downloads`；桌面独有 `canonicalize` / `read-dir` / `stat`）
  - `host-platform`：交集 2（桌面多 `local-ipv4-addresses` / `pick-folders` / `reveal-in-dir` / `wsl-distros`）
  - `host-http`：交集 1（桌面多 `register-endpoint` / `unregister-endpoint`）
  - `host-events`：交集 1（桌面 `notify`；移动已收编进 `host-notify`）
  - `abi`：交集 1（桌面多 `form`）
- **交集为 0（只能整块归端扩展）**：`host-auth`（移动 5 配对/生物 vs 桌面 10 secret+认证中心）、`events`（移动 5 vs 桌面 2）、`host-connection`（移动 `primary-target` vs 桌面 `connections-list`）
- **端独有**：移动 `host-notify` / `host-terminal-stream`；桌面 `host-{pty,task,crypto,process,app,timer,api-call}` + `auth-policy` / `events-ws` / `events-task` 三附加 world

---

## 2. 目标形态

```text
packages/bedcode-wasm-core（单一 crate，双端完全一致）
  ├── wit/core.wit           ← 核心 WIT 的单一真源（机制与契约同侧，ADR 0036 口径）
  ├── 核心 world 的 bindgen!（唯一一份，双端同一份代码）
  ├── 扫描装配：ModuleRegistry::collected() → verify_whitelist → install_all(linker)
  └── 平台形态端口化：Store 装配（WASI p3 vs 无 WASI）经 HostPorts/StorePorts 注入

packages/bedcode-*（能力域 crate，差异的唯一来源）
  ├── wit/<domain>.wit       ← 自持分片：package bedcode:plugin + 本域 interface + world cap-<domain>
  ├── bindgen! 指向自己的分片（不再指向端 WIT）
  └── submit_module! 自报 → 被 wasm-core 扫描装配

packages/plugin-sdk-{desktop,mobile}/rust/wit/   ← 生成物（脚本拼装 + 漂移锁）
  └── core.wit（来自 wasm-core）+ cap-*.wit（来自能力域）+ bedcode.wit（端 world：include core; include cap-…）
```

**端清单（单一真源）**：一份 `<end>-capabilities.json` 同时驱动 ① WIT 拼装 ② 宿主白名单 ③ ABI 计数 ⇒ 「组合了什么」只有一个答案。

---

## 3. 决策

### D1 · 核心 WIT 真源 = `packages/bedcode-wasm-core/wit/core.wit`

单一 crate 同时是**机制与核心契约**的单一真源（ADR 0036「机制与真源同侧」同款判据）。两端 SDK 的 `wit/` 目录降级为**生成物**。

### D2 · 同 package 拼装（裁决 A）

全部分片声明 `package bedcode:plugin;`（**不含版本号差异**），由脚本收集到端 `wit/` 单目录（同目录即同 package），端 world 用 `include` 组合：

```wit
// packages/plugin-sdk-desktop/rust/wit/bedcode.wit（生成物）
world plugin {
    include core;          // 来自 wasm-core/wit/core.wit
    include cap-pty;       // 来自 pty-engine/wit/pty.wit
    include cap-task;
    // …
}
```

⇒ import 名字保持 `bedcode:plugin/host-pty` 形态，**跨 package 方案才会改 package 名**（那是被否决的 B 项）。

### D3 · 交集切片（裁决 B）与它的必然代价

核心 world = 11 个全等 interface + 5 个交集切片。但 **WIT 没有 interface 级 include**——一个 interface 只能有一份完整定义。故「交集切片」意味着**拆 interface**：

| 原 interface | 核心（双端一致） | 端扩展（新增 interface 名） |
| --- | --- | --- |
| `host-websocket` | 客户端 5 函数 | 桌面新增 `host-websocket-server`（10 函数） |
| `host-fs` | 交集 6 函数 | 桌面新增 `host-fs-desktop`（3）/ 移动新增 `host-fs-mobile`（2） |
| `host-platform` | 交集 2 函数 | 桌面新增 `host-platform-desktop`（4） |
| `host-http` | 出站 1 函数 | 桌面新增 `host-http-endpoint`（2） |
| `abi` | `version` | 桌面新增 `abi-form`（`form`；移动不组合） |
| `host-events` | 交集 1 | 桌面 `notify` 归 `host-events-desktop`（移动收编在 `host-notify`） |
| `host-auth` / `events` / `host-connection` | **交集 0 ⇒ 整块归端扩展** | — |

**代价（必须显式接受）**：拆 interface 会改变桌面插件产物的 import 集合 ⇒ **双端 ABI bump + 插件产物全量重建**（走既有 `stale_artifact_rebuild_hint` fail-visible 形态②：旧产物实例化期点名缺失 interface 与重建版本）。

> **POC 后复评点（票 04 前置）**：若拆分带来的重建面评估后不可接受，退回「核心 = 仅 11 个全等 interface」方案（`host-websocket` / `host-fs` / `host-platform` 整块归端扩展，核心更小但零拆分）。该复评在票 01 POC 绿之后、票 04 开工之前做。

### D4 · 能力域 crate 改为 bindgen 自持分片

`path` 从「整份端 WIT」改为「本 crate 的 `wit/<domain>.wit`，`world: "cap-<domain>"`」。收益：能力域与端解耦 ⇒ 同一能力域 crate 可被任意端组合（移动侧组合 `cap-ws-client` 之类成为可能）。

**硬约束（ADR 0035 D5 已知风险）**：同一 interface 在 wasm-core 与能力域各自 bindgen 生成的是**同名不同类型的 `Host` trait** ⇒ 宿主必须同批删除自己的该域 `impl Host` 与 `add_to_linker` 行，否则装配期 `defined twice`。分片化后该风险不变，白名单双向校验继续兜底。

### D5 · 平台形态端口化（wasm-core 双端一致的最后一块）

| 差异 | 处置 |
| --- | --- |
| Store 装配：桌面 WASI p3（`wasmtime_wasi::{p2,p3}` + `FsPerms` + `ResourceLimiter`）vs 移动无 WASI | 抽 `StorePorts`（或扩 `HostPorts`）端口 trait，wasm-core 只调端口；两端各自实现 |
| 平台依赖 `webkit2gtk` / `dbus`（`cfg(target_os = "linux")`，Android 同 `target_os` ⇒ 会撞） | 全部移入能力域 / 宿主 crate，或加 `cfg(not(target_os = "android"))` |
| 桌面能力域依赖（`portable-pty` / `actix-web` / `sysinfo` …） | 全部 optional + feature 门控；移动侧 `default-features = false` |

### D6 · 移动 fork crate 的归位

`bedcode-mobile/packages/bedcode-wasm-core`（fork，23,930 行）在票 06 **退役**：移动 `src-tauri` 改为依赖根 `packages/bedcode-wasm-core`（`--no-default-features` + `mobile-host` feature）。fork 面里「移动 WIT 绑定 + 移动 host_impl 16 域 + 移动 ports」迁到根 crate 的 `mobile-host` feature 门控下（与桌面 `desktop-host` 对称），其余机制面与桌面合一。

> 退役走 fail-visible 三形态 + 防回接锁（AGENTS §5.1.4），与票 15 / 票 16 同款。

---

## 4. 票划分

| 票 | 内容 | 依赖 | 门禁 |
| --- | --- | --- | --- |
| **01** | **POC：pty 域 WIT 分片 + 拼装 + 等价实证**（详见 `ticket-01-pty-wit-slice-poc.md`） | — | 分片 bindgen 编译绿 + 组合前后 import 集逐项等价 + 漂移锁 + 移动侧 `cargo check --no-default-features` |
| **02** | **wasm-core 依赖图脱桌面**（POC P3 实测发现的硬前置）：能力域 / 桌面 SDK / 平台依赖全部 optional + feature 门控 + `cfg(not(target_os="android"))`。**批次 01–02 已落地**（host-kit 生命周期钩子 + 装配自报面；pty 域整面迁宿主：adapter/强制引用/白名单/测试/dep 摘除） | 01 | `cargo check --no-default-features` 绿 **且** `cargo tree --no-default-features` 桌面命中**归零**（当前实测 28 处，见 §5）+ Android target 实证 |
| 03 | 核心 WIT 真源落 `wasm-core/wit/core.wit` + 双端拼装脚本 + 端清单单点 | 01/02 | 双端 `bedcode.wit` 由脚本重新生成且**与 HEAD 逐字等价**（第一步只重组不变更语义） |
| 04 | 接口切片（ws / fs / platform / http / abi）+ 双端 ABI bump + 产物重建 | 03 + POC 后复评 | 双端 `cargo test` 全量 + 插件产物全量重建 + `stale_artifact_rebuild_hint` 判据扩展 |
| 05 | 能力域 crate 改 bindgen 自持分片（pty / task / crypto / process / app / timer / api-call / http / ws / peer / mdns 逐个） | 03（04 若先合并则含切片） | 每域：`HostModule` 白名单双向绿 + 域 crate 测试全绿 + 无 `defined twice` |
| 06 | wasm-core 单一化：移动 fork 退役 + 平台形态端口化 + 移动侧依赖切换 | 02/03/04/05 | 移动 `src-tauri` 编译 + 全量测试绿；桌面零回归 |
| 07 | 防回接 / 漂移锁 + 文档收口（ADR 0045、双端 code-map、CHANGELOG 双语、AGENTS §5.4） | 06 | 锁变异自检；根 `pnpm exec eslint .` 0 error |

> 票 01 只做**验证**，不落生产改动（POC 产物放 `.scratch/` 或临时分支，验证后决定合并方式）。

---

## 5. 风险与回退

| 风险 | 吸收 / 回退 |
| --- | --- |
| 拆 interface 触发双端 ABI bump 与产物全量重建（D3 代价） | 票 04 前置复评；不可接受则退「核心 = 11 全等」方案 |
| 能力域分片 bindgen 与 wasm-core 核心 bindgen 生成同名不同类型 trait ⇒ `defined twice` | ADR 0035 D5 既有纪律：同批删除宿主侧该域 `impl Host` + `add_to_linker`；白名单双向校验兜底 |
| 端 `wit/` 成为生成物后，离线构建 / 手工改坏 | 生成物入库 + 漂移锁（分片真源 vs 端目录逐字比对）+ CI `--check`；手工改即红 |
| 单一 crate 后移动侧依赖图被桌面能力域拖入（POC 的实证点） | 全部 optional + feature；`cargo tree` 门禁（对齐 `bedcode-headless-host-probe` 既有零 wasmtime/零 SDK 判据） |
| `cfg(target_os = "linux")` 平台依赖在 Android 上撞（webkit2gtk / dbus） | 移出 wasm-core 或加 `cfg(not(target_os = "android"))`；票 02 实证 |
| 磁盘 / 编译时长（桌面 wasm-core 全量约 10G+） | 票 01 POC 只编 pty-engine（小 crate）；全量前 `df -h` + `CARGO_INCREMENTAL=0` |

---

## 6. 门禁（通用，AGENTS §10 两段式）

- 开发中：只跑针对性单测自验（Rust 过滤按 crate 根、vitest 端目录执行）；红了立即修。
- 收尾：双端 `cargo test` 全量 + 两端 `pnpm run test:run` + 根 `pnpm exec eslint .` 0 error + `cargo fmt` / `clippy` 自查。
- 跨端协议 / ABI 变更（票 04/06）：`cross-end-tests` 全量 + 插件产物全量重建（wasm32 真门禁，`native 绿 ≠ 可交付`）。
- 真源搬迁（票 06 fork 退役）：fail-visible 三形态 + 防回接锁 + 变异自检。
- 未纳入自动化门禁的手工项（逐项写跑了/没跑 + 原因）：真机 / 浏览器核验、Android target 编译实证。

## 7. 文档联动

- `docs/adr/0045-*`（本 spec 的决策落档，POC 后转 accepted）
- 双端 `docs/code-map.md`：核心 world / 能力域分片 / 端清单三处登记 + 防回接锁索引
- `CHANGELOG.md` + `CHANGELOG_zh.md` 双语条目（票 04/06 各一条）
- AGENTS.md §5.4「双端差异」改写（单一 wasm-core + 差异只来自能力域组合）
- `docs/knowledge/plugin-development-checklist.md`：WIT 分片与拼装的插件侧影响（产物重建口径）
