# ADR 0045：wasm-core 单一 crate + WIT 分片组合（能力域扫描装配）

- 状态：**proposed**（票 01 POC 未跑；POC 四命题成立即转 accepted）
- 日期：2026-10-09
- spec：`.scratch/2026-10-09-wasm-core-single-crate/spec.md`（票序与门禁的真源）
- 相关：ADR 0035（能力域 crate 化 + 自动装配）、ADR 0036（机制与真源同侧）、ADR 0037（wasm-core 整核抽出）、ADR 0039（pty 域整面迁出并承接 WIT 接线）、ADR 0040（移动 fork 两步走）、ADR 0018（移动契约独立）、ADR 0019（双端锁版）、ADR 0022（边界裁决）

## 背景

1. **双份机制**：`packages/bedcode-wasm-core`（桌面，47,563 行）与 `bedcode-mobile/packages/bedcode-wasm-core`（移动 fork，23,930 行）并存。ADR 0040 D1 选项 C 把它定为「先 fork、再抽共享核」的两步走，第二步（票 18/19）只抽了 host_api 五域，机制面仍是两份 ⇒ 每次机制修复两处同步税。
2. **能力域被端 WIT 绑死（实测根因）**：五个能力域 crate 的 provider 侧 `bindgen!` 全部指向**整份端 WIT**
   （`bedcode-server-http/src/plugin_binding.rs:220`、`bedcode-server-peer-net/src/plugin_binding.rs:617`，pty / ws / mdns 同款）⇒ 能力域在契约面绑死桌面，移动侧无法复用 ⇒ 「差异只来自组合的能力 lib」不成立。
3. **用户指令（2026-10-09）**：「强制 wasm-core 是单一 crate；wasm-core 通过扫描能力域 crate lib 完成 host 扩展；单纯的 wasm-core 双端完全一致，差异只来自组合的能力 lib」。与 2026-10-06 终裁（ADR 0039 背景段「不需要绑定…通过声明 trait 静态扫描连接 host api」）同轨。
4. **扫描装配机制已就绪**：`bedcode-host-kit` 已提供 `HostModuleDesc` / `HostModule::install(linker)` / `submit_module!` / `ModuleRegistry::{collected, verify_whitelist, install_all}`（ADR 0035 D2/D3/D5，pty 域是先例）⇒ 本 ADR 不新造装配机制，只补契约分片。

## 决策

### D1 · 单一 crate + 核心 WIT 真源同侧

`packages/bedcode-wasm-core` 是**唯一**的 wasm-core crate，且同时是**核心机制与核心 WIT**的单一真源（`wit/core.wit`）——ADR 0036「机制与真源同侧」判据的同款应用。移动 fork crate 退役（票 06）。

### D2 · WIT 分片：同 package 拼装

每个能力域 crate 自持 `wit/<domain>.wit`（`package bedcode:plugin;` + 本域 interface + `world cap-<domain>`）；端 SDK 的 `wit/` 目录是**生成物**——脚本把 core 与各 cap 收集到端**单目录**（同目录即同 package），端 world 用 `include core; include cap-…;` 组合。

依据（源码级）：
- WIT 语言层支持 world `include`（同 / 跨 package 均可）——官方 WIT Reference「Including other worlds」。
- 跨 package 才需要 `deps/` 目录（项目锁定的 `wit-parser-0.254.0/src/resolve/fs.rs:56-135` 明示自动探测 `deps/my-package/*.wit`、`.wit`、`.{wasm,wat}`；`0.256.0` 同款）；同 package 多文件须**同目录**。
- **选择同 package 的理由**：import 的 package 名保持 `bedcode:plugin/*` 不变 ⇒ 零 ABI 破坏、插件产物无需因分片本身重建（跨 package 方案会改 package 名 ⇒ 破坏性）。

### D3 · 核心 world = 交集切片（含已知代价）

核心 = 双端函数集完全一致的 11 个 interface（`command` / `lifecycle` / `manifest` / `events-binary` / `host-bus` / `host-config` / `host-log` / `host-storage` / `host-plugin-database` / `host-mdns` / `host-peer`）+ 五个交集切片（`host-websocket` 客户端 5、`host-fs` 交集 6、`host-platform` 交集 2、`host-http` 出站 1、`host-events` 交集 1、`abi` 的 `version`）。

**已知代价（显式接受）**：WIT 无 interface 级 `include`，一个 interface 只能有一份完整定义 ⇒ 交集切片必然**拆 interface**（桌面新增 `host-websocket-server` / `host-fs-desktop` / `host-platform-desktop` / `host-http-endpoint` / `abi-form`）⇒ 桌面插件产物 import 集变化 ⇒ **双端 ABI bump + 产物全量重建**（走 `stale_artifact_rebuild_hint` fail-visible 形态②）。

> **POC 后复评（票 04 前置）**：若重建面评估后不可接受，退回「核心 = 仅 11 个全等 interface」（`host-websocket` / `host-fs` / `host-platform` 整块归端扩展，核心更小但零拆分）。

交集为 0 的三个（`host-auth` / `events` / `host-connection`）**整块归端扩展**，不进核心。

### D4 · 能力域 crate bindgen 自持分片

`bindgen!` 的 `path` 从整份端 WIT 改为本 crate 的 `wit/<domain>.wit`、`world: "cap-<domain>"` ⇒ 能力域与端解耦，可被任意端组合。

**沿用 ADR 0035 D5 硬约束**：wasm-core 与能力域各自生成的 `bedcode::plugin::host_*::Host` 是**同名但不同类型**的 trait ⇒ 宿主必须同批删除该域的 `impl Host` 与 `add_to_linker` 行，否则装配期 `defined twice`；白名单双向校验继续兜底。

### D5 · 平台形态端口化（wasm-core 双端一致的最后一块）

Store 装配差异（桌面 WASI p3：`wasmtime_wasi::{p2,p3}` + `FsPerms` + `ResourceLimiter`；移动无 WASI）经端口 trait 注入，wasm-core 只调端口。桌面能力域依赖全部 optional + feature 门控（`desktop-host` / `mobile-host` 对称）；`cfg(target_os = "linux")` 的平台依赖（`webkit2gtk` / `dbus`，Android 同 `target_os` ⇒ 会撞）移出 wasm-core 或加 `cfg(not(target_os = "android"))`。

### D6 · 端清单单点

一份 `<end>-capabilities.json` 同时驱动 ① WIT 拼装 ② 宿主白名单 ③ ABI 计数 ⇒ 「这一端组合了什么」只有一个答案（`verify_whitelist` 双向校验直接消费它）。

## 代价与风险

| 项 | 说明 | 处置 |
| --- | --- | --- |
| 拆 interface 的 ABI 破坏 | 桌面插件产物 import 集变化 | 票 04 双端 ABI bump + 产物全量重建 + fail-visible 点名；POC 后复评（D3） |
| 端 `wit/` 变生成物 | 手工改坏 / 离线构建 | 生成物入库 + 分片真源 vs 端目录逐字漂移锁 + CI `--check` |
| 移动侧依赖图被桌面拖入 | `portable-pty` / `actix-web` / `webkit2gtk` 等 | **票 01 P3 已实测暴露**：`cargo check --no-default-features` **绿**（14s，host target），但 `cargo tree --no-default-features` 命中 **28 处**桌面依赖（`bedcode-plugin-api`、`bedcode-{pty-engine,server-http,server-websocket,server-peer-net,discovery-engine}`、`portable-pty`、`actix-web`、`webkit2gtk`、`dbus`）——根因是这些依赖在 `Cargo.toml` 里是**必选**且能力域被强制 `features = ["desktop-host"]` ⇒ 新增**票 02** 专做依赖图脱桌面（判据对齐 `bedcode-headless-host-probe`） |
| 移动 fork 退役 | 23,930 行真源搬迁 | 票 06 走 fail-visible 三形态 + 防回接锁 + 变异自检（AGENTS §5.1.4） |

## 验证门禁

- **票 01 POC（零生产改动，全部在 `/tmp/wit-slice-poc/`）**：
  - P1 分片 bindgen 与整份 world bindgen 的 `Host` trait 对等（双 impl 编译探针 + 双 `add_to_linker`）
  - P2 组合后 `world plugin` 的 import/export 集合与 HEAD **逐项等价**（`wasm-tools component wit --json` 或 `wit-parser 0.256` 临时 crate）
  - P3 `cargo check --no-default-features` 通过 + `cargo tree` 桌面依赖归零（Android target 实证为可选项，跑不了须写明原因）
  - P4 拼装脚本幂等 + sha256 逐字一致
- 后续票：双端 `cargo test` 全量 + 两端 `pnpm run test:run` + 根 `pnpm exec eslint .` 0 error；ABI 变更票加 `cross-end-tests` + 产物全量重建（wasm32 真门禁）。

## Out of scope

- 桌面端业务行为改动（本专项只做机制与契约的重组，行为逐字不变是硬要求）。
- 双端 WIT 契约**合并**（ADR 0018 契约独立不变；本 ADR 是「分片 + 组合」，不是「统一成一份 world」）。
- wasmtime / wit-bindgen 版本升级（ADR 0019 双端锁版）。
- 能力域 crate 发布为 crates.io 包（仓库内 path 依赖形态不变）。

## Comments

- 2026-10-09：用户三问拍板 = 同 package 拼装 / 交集切片 / POC 先行；本 ADR 随 spec 落档，状态 proposed。
- 事实底座（2026-10-09 实测）：双端共有 20 interface、其中 11 个函数集完全一致；`host-pty` interface 自包含（无跨 interface `use`）⇒ 分片无需 `deps/`；`wit-parser-0.254.0`（项目锁定版本）支持 `deps/` 自动探测。差异量化脚本 `.scratch/2026-10-09-wasm-core-single-crate/_tools/wit_iface_diff.py` 可复跑。
- 2026-10-09 校正：初稿把 `deps/` 支持的证据写成 `wit-parser-0.256.0`，实测项目锁定的是 **0.254.0**（`0.256.0` 仅被 `wit-component =0.256.0` 夹具链使用），已按 AGENTS §0「文档字面 ≠ 事实」修正版本与路径引用。
- **票 01 POC 进度（2026-10-09）**：
  - **P2 已证**：组合版 `world plugin`（`include core` + `include cap-pty`）与 HEAD 扁平 world 的 import/export **逐项等价**（26 vs 26，diff 空）⇒ 同 package 分片 + `include` 成立，且分片本身零 ABI 破坏。
  - **P4 已证**：拼装脚本原型幂等（两次 sha256 清单 diff 空）+ 与分片真源逐字一致。
  - **P3 编译绿但依赖图红**：`cargo check --no-default-features` 通过（14s），`cargo tree` 命中 **28 处**桌面依赖 ⇒ 「无桌面依赖」命题**不成立**，新增**票 02** 专做依赖图脱桌面（详见 spec 票表）。
  - **P1 已证**：分片版 `host_pty::Host` 的 6 个方法签名与「整份 world 绑定」版**逐项一致**（基准签名取自生产 impl `packages/bedcode-pty-engine/src/plugin_binding.rs:185-226`，对分片版 trait 写同签名 impl 编译通过，无 E0046/E0053）；`add_to_linker::<ProbeHost, HasSelf<ProbeHost>>` 首次 `Ok`、重复注册报 `map entry 'spawn' defined twice` ⇒ 分片装配面成立，ADR 0035 D5 的 `defined twice` 风险实测复现。
  - **POC 定论**：D1 / D2 / D4 成立；**D5 的前置（无桌面依赖）不成立** ⇒ 票 02 成为硬前置，其完成前本 ADR 维持 `proposed`。
