# 票 03 · 核心 WIT 真源 + 双端拼装脚本 + 端清单单点

Status: **✅ done（2026-10-10 落地，实施记录见 §7）**
依赖：票 01（P1 / P2 / P4 已证）、票 02（批 01–05；批 06 切片外迁在本票之后，本票首版只重组不切 interface，不阻塞）
前置：开工前 `git status` 核对在途改动（并行会话与票 02 批 06–09 的未提交改动共存工作区，AGENTS §11 精确 edit 纪律；本票涉及双端 SDK `wit/` 目录，改动前先 `git diff` 复核归属）

## 1. 现状（2026-10-10 实测，勿凭记忆）

| 项 | 事实 |
| --- | --- |
| 桌面端 WIT | `bedcode-desktop/packages/plugin-sdk-desktop/rust/wit/bedcode.wit`（81,217 B）30 interface / **6 world**（`plugin` = 21 import + 5 export；另 `plugin-binary` / `plugin-ws` / `plugin-task` / `plugin-auth-policy` / `plugin-system`） |
| 移动端 WIT | `bedcode-mobile/packages/plugin-sdk-mobile/rust/wit/bedcode.wit`（31,881 B）22 interface / **2 world**（`plugin` = 16 import + 5 export；`plugin-binary`） |
| 能力域 bindgen | pty / http / ws / peer 四 crate 的 `bindgen!` **全指桌面端整份 WIT、`world: "plugin"`**（`plugin_binding.rs:244 / 251 / 1196 / 624`）；discovery-engine 用 `lib.rs` 的 `ln!` —— 契约面仍绑桌面（spec §1.2 第一性问题，票 05 解决） |
| POC 组合等价 | 票 01 P2：同 package 分片 + `include` 组合后主 `plugin` world 成员逐项等价（26/26，diff 空）；**P2 只覆盖了主 world，附加 world 是盲区**（本票 §4.1 必须补证） |
| POC 拼装脚本 | `/tmp/wit-slice-poc/compose.py`（manifest 驱动、幂等、sha256 逐字校验）+ `manifest.json` —— 生产版需升级（多端输出 / 附加 world / CI 接入） |
| 端清单 | **尚不存在**（spec §2：「一份 `<end>-capabilities.json` 同时驱动 ① WIT 拼装 ② 宿主白名单 ③ ABI 计数」未落地） |
| ADR | `docs/adr/0045-wasm-core-single-crate-and-wit-slice-composition.md` 状态 proposed；本票 / 票 04 落地后转 accepted |

## 2. 首版分片归属（「只重组不变更语义」的机械划分）

核心 WIT 真源 = `packages/bedcode-wasm-core/wit/core.wit`（D1，ADR 0036「机制与真源同侧」同款判据）；双端 SDK `wit/` 目录降级为**生成物**（入库 + 漂移锁守护）。

**同名 interface 只能有一份定义** ⇒ 双端要跨文件共享一份 `core.wit`，里面只能放**交集**。首版核心 = spec §1.5 实测的 **11 个双端全等 interface**：

`command(1)` / `lifecycle(4)` / `manifest(1)` / `events-binary(1)` / `host-bus(5)` / `host-config(1)` / `host-log(5)` / `host-storage(3)` / `host-plugin-database(5)` / `host-mdns(5)` / `host-peer(19)` —— 函数体与全部文档注释逐字保留。

**其余 interface 全部归各端 cap 文件**（函数集以 `/tmp/wit_iface_diff.py` 复跑为准，禁止凭记忆写）：

| 端 | 端 cap 文件 | 收录 |
| --- | --- | --- |
| 桌面 | `cap-pty.wit` ← pty-engine | `host-pty` |
| 桌面 | `cap-http.wit` ← server-http | `host-http`（桌面 3 函数全量） |
| 桌面 | `cap-ws.wit` ← server-websocket | `host-websocket`（桌面 15 全量） |
| 桌面 | `cap-desktop.wit` | host-{task,crypto,process,app,timer,api-call,connection,auth,events,fs,platform} 桌面全量 + export `events`/`abi`（桌面版）+ `events-ws`/`events-task`/`auth-policy` + 附加 world `plugin-binary`/`plugin-ws`/`plugin-task`/`plugin-auth-policy`/`plugin-system` |
| 移动 | `cap-mobile.wit` | host-{notify,terminal-stream,connection,auth,events,fs,platform,http,websocket} 移动全量 + export `events`/`abi`（移动版）+ world `plugin-binary`（或逐字保留在移动生成物） |

> **host-mdns / host-peer 的摆法（组合要点）**：它们是 11 全等（交集）⇒ 定义进 `core.wit`；但宿主实现面在能力域 crate（discovery-engine / peer-net）。分片归属只决定**定义文件在哪**，不影响宿主实现归属 —— 能力域 crate 为**自己 bindgen** 的 `wit/mdns.wit` / `wit/peer.wit` 是另一个 package 实例（不同目录 = 不同 package），可以自持同名 interface 定义，二者不冲突（票 05 细账）。组合层唯一硬约束：**合成 package 里每个 import 名有且仅有一份定义**（core 与 cap 文件不得重复定义同一 interface）。
>
> **世界 import/export 的交集**：`world core` 的 import/export 列表必须等于双端交集（即 11 全等 + 两端都有的 5 个 export 里交集的部分），否则移动端 `include core` 会把桌面独有 import 带进移动组合（移动 package 无对应定义 ⇒ resolve 报错）。首版建议：`world core` 只含 11 全等 import + `command`/`lifecycle`/`manifest` export；`events`/`abi` 的 export 经各端 cap 的 world（`cap-desktop world { export events; export abi; }`）并入 —— 组合前后 export 集合等价是 §5 门禁的兜底。

**为什么首版就砍到 11 全等**：非等接口（host-websocket 桌面 15 ≠ 移动 5 等）**无法**进共享 `core.wit`；核心越小，票 04 复评面越小。若票 04 复评否决拆 interface，本票形态即终态（spec §3 D3 回退方案「核心 = 11 全等」）。该复评点见票 04 §0。

## 3. 端清单（单一真源，本票硬交付）

新增两文件（路径可调，schema 建议与 POC `manifest.json` 兼容扩展）：

```jsonc
// packages/plugin-sdk-desktop/compose.json（桌面示例；移动端同理换 caps 集合与 worlds）
{
  "package": "package bedcode:plugin;",
  "world": "plugin",
  "core": "packages/bedcode-wasm-core/wit/core.wit",
  "caps": {
    "pty":    "packages/bedcode-pty-engine/wit/pty.wit",
    "http":   "packages/bedcode-server-http/wit/http.wit",
    "ws":     "packages/bedcode-server-websocket/wit/ws.wit",
    "desktop": "packages/plugin-sdk-desktop/rust/wit-src/cap-desktop.wit"
  },
  "worlds": ["plugin", "plugin-binary", "plugin-ws", "plugin-task", "plugin-auth-policy", "plugin-system"],
  "abi": { "version": 35 }
}
```

（peer / mdns 若按 §2 进 core.wit，桌面清单不必列它们；移动清单 caps = { "mobile": … }，worlds = ["plugin", "plugin-binary"]，abi version 19。）

同一清单是票 04（ABI 计数驱动）、票 06（移动组合面）、票 07（漂移锁）的输入 ——「组合了什么」只有一个答案。宿主白名单（票 02 的 `expect_host_module!` 面）与清单的对照关系在 §4.4 落地，作为「② 宿主白名单」的驱动连点。

## 4. 步骤

### 4.1 生产版拼装脚本 `scripts/compose-wit.mjs`（在 POC compose.py 之上）

1. 输入：端清单 JSON + 分片真源；输出：端 `wit/` 目录（`core.wit` + `cap-*.wit` + `bedcode.wit`）
2. 端 `bedcode.wit` = package 声明 + `world plugin { include core; include cap-…; }` + **附加 world 显式重建或原样保留**。POC 只验过主 world —— 附加 world 是 P2 盲区，**本票必须补证** `plugin-binary` / `plugin-ws` / `plugin-task` / `plugin-auth-policy` / `plugin-system` 的组合等价（`include` 只带被引用 world 的 import/export，附加 world 需要显式声明；两级方案：附加 world 定义留 `cap-desktop.wit` 逐字复制，仅主 world 走 include）
3. `--check <end>` 模式：读生成物 sha256 清单与入库生成物逐字比对（漂移锁原型，票 07 收口 CI）
4. 幂等：连跑两次输出目录 `diff -r` 空
5. **注释保全（利刃）**：WIT 文件带大段 `///` 文档注释与 `//!` 头注释（桌面 bedcode.wit 开头 36 行迁移约定、每 interface 的版本演变注释）——按「interface 边界整段复制」，任何脚本切分都必须跑「抽取段 vs 原文逐字 diff」自检，diff 非空即停人工裁决

### 4.2 首版面（机械重组，双端各自保持与 HEAD 等价）

1. `wasm-core/wit/core.wit`：`package bedcode:plugin;` + 11 全等 interface（逐字）+ `world core`（交集 import/export）
2. 五能力域 cap 源文件：`package bedcode:plugin;` + 各自 interface 原段 + `world cap-<domain> { import <interface>; }`（pty/http/ws 桌面 version；peer/mdns 定义进 core.wit 时不重复摆，能力域侧自持份留给票 05）
3. `cap-desktop.wit` / `cap-mobile.wit`：各端独有 + 非等接口的端全量 + 附加 world（移动端 `plugin-binary` 若在两端一字不差可进 core.wit，差一字就不行——执行期核对）
4. 端组合与等价比对：**每端**用 `wit_iface_diff.py` 口径导出组合版 vs HEAD 的 world import/export 集合 + 每个 interface 定义逐字 diff（含附加 world、export 集合）
5. 生成物入库（覆盖双端 SDK `wit/` 目录）

### 4.3 移动端

移动 WIT 独立契约（ADR 0018），本票同样做「移动组合版 vs 移动 HEAD 逐字等价」验证；**不碰**移动 fork `bedcode-mobile/packages/bedcode-wasm-core`（那是票 06 的退役对象）。移动 `cap-mobile.wit` 的源建议放 `bedcode-mobile/packages/plugin-sdk-mobile/rust/wit-src/`（端清单同目录）。

### 4.4 端清单 ↔ 宿主白名单对照（首步静态版）

票 02 后宿主白名单 = 内核 `IN_CRATE_HOST_MODULES` ∪ 宿主 `expect_host_module!` 自报。本票加一条**静态锁**：桌面清单 `caps` 的域名集合 == 白名单里「能力域模块」集合（域名一致即认为对齐；语义核对不用过深——票 04/07 加深）。锁落宿主测试（`src-tauri/tests/`），首版以注释 + 单测断言形式。

## 5. 门禁

| 项 | 要求 |
| --- | --- |
| 组合等价 | 双端生成物按 `wit_iface_diff.py` 口径与各端 HEAD `bedcode.wit` **逐项等价**：world import/export 集合 diff 空 + interface 定义逐字 diff 空（**含附加 world**） |
| 生成物紧盯分片 | `core.wit` 与 wasm-core 真源逐字一致；`cap-*` 与能力域 / 端源逐字一致（P4 判据） |
| 幂等 | 连续两次 `--check` 的 sha256 清单 diff 空 |
| 漂移锁 | 变异自检 1/1：手改生成物一字节 → 锁红（点名文件）→ 还原 → 绿 |
| 回归 | 双端 SDK `cargo check`（生成物语义等价 ⇒ 绑定层输出应零变化）；内核 `cargo check --tests` 0 error；桌面宿主 `cargo check --lib` |
| 端清单 | 双端 `compose.json` 字段完整、可被脚本消费、`worlds` 列表与实际 world 一一对应 |

## 6. 风险与回退

| 风险 | 吸收 / 回退 |
| --- | --- |
| 附加 world 组合不等价（P2 盲区实证不通过） | 附加 world 不进 include，留各端 cap 文件逐字复制，仅主 world 走 include（组合层退一级，语义不变） |
| 注释 / 空行漂移在逐字 diff 暴露 | 按 interface 边界整段复制；diff 非空即停人工裁决，**首版目标零改动**，任何一行文本差异都必须记录理由 |
| 与票 02 批 06（切片外迁）撞期 | 批 06 若先落地：cap-desktop.wit 内容随新 interface 名更新，其余不变；若本票先落地：批 06 基于本票文件改 |
| 端清单与白名单不一致（多/少组合） | §4.4 静态锁兜底；漂移锁只盯文件字节，组合语义靠本锁 |
| 回退 | 脚本与清单是新增文件可直接删；生成物 `git checkout` 还原；`/tmp/wit-slice-poc` 保留 POC 对照 |

## 7. 实施记录

**状态：✅ 已落地（2026-10-10，票 02 批 06 之后）**。首版重组完成：核心 WIT 真源 + 双端拼装脚本 + 端清单单点全部就位，双端 SDK / 内核 / 宿主回归绿。

### 7.1 产物清单

| 类别 | 文件 | 说明 |
| --- | --- | --- |
| 核心真源 | `packages/bedcode-wasm-core/wit/core.wit`（303 行） | 14 全等 interface（spec §1.5 的 11 + abi + host-events + host-platform——后三者批 06 拆分后已全等）+ `world core`（9 import / 3 export，= 双端交集） |
| 能力域 cap | `packages/bedcode-pty-engine/wit/pty.wit`（82 行）/ `server-http/wit/http.wit`（38）/ `server-websocket/wit/ws.wit`（109） | host-pty / host-http / host-websocket 桌面形态，`world cap-* { import …; }` |
| 端 cap 真源 | `plugin-sdk-desktop/rust/wit-src/cap-desktop.wit`（666 行）/ `plugin-sdk-mobile/rust/wit-src/cap-mobile.wit`（281） | 桌面 17 interface + 6 world；移动 8 interface + 2 world；abi 版本演进历史注释在各自头部保全 |
| 拼装脚本 | `scripts/compose-wit.mjs` | `--all` 双端拼装 / `--check` 只读漂移锁 / 幂等 |
| 端清单 | `plugin-sdk-desktop/compose.json` + `plugin-sdk-mobile/compose.json` | caps / worlds / abi.version（桌面 36、移动 19） |
| 生成物 | 双端 `rust/wit/`（core.wit + cap-*.wit + bedcode.wit） | 入库；bedcode.wit = package + `world plugin { include core; include cap-…; }` |

### 7.2 关键裁决（先写，实测中确认/修正）

1. **全等 interface 实为 14 个**（票面 §2 写的 11 是按批 06 前口径）：批 06 拆分后 abi（1=1）、host-events（1=1）、host-platform（2=2）均已全等 ⇒ 进 core。host-fs（桌面 6 ≠ 移动 8）等仍非等 ⇒ 各端 cap。
2. **注释定稿以桌面为基底 + 中性化端内版本号**：host-bus / host-peer / events-binary / abi 的 v11/v31/v30/v10 等端内 ABI 历史引用去除（共享核心不携带端内版本——各端 SDK abi.rs 是权威记账）；abi 版本演进长注释迁各端 cap 文件头部保全；**host-mdns 的桌面函数注释 `mdns:found.<plugin-id>` 修正为 `<plugin-id>::mdns:found`**（与桌面实现 discovery-engine `owned_topic` 一致——原注释文档漂移）；lifecycle / host-log 吸收移动端更有信息量的注释行。差异全记录（verify 逐行 diff 见 §7.4）。
3. **同 package 多文件的 package 注释唯一性（实测踩坑）**：wit-parser 把 package 声明前的**任意注释**（含普通 `//`）都当 package doc，多文件各带即报 `found doc comments on multiple 'package' items` ⇒ **只有 core.wit 在 package 前带注释（共享核心文档），cap 文件头注释整体移到 package 声明之后**（普通文件注释，内容保全）。
4. **bindgen path 目录化（票面未预见的必要配套）**：原 bedcode.wit 是单文件自包含，合成后 bedcode.wit 的 `include core/cap-*` 依赖同目录文件——wit-bindgen 对**文件** path 用 push_file（不加载同目录），单文件解析即报 `world core does not exist` ⇒ 全部 15 处 bindgen `path` 从 `…/wit/bedcode.wit` 改为 `…/wit` 目录（push_dir 加载合成 package）。消费方：桌面 SDK 5（wasm / wasm_ws / wasm_task / wasm_binary / wasm_auth_policy）+ 移动 SDK 2（wasm / wasm_binary）+ 内核 component.rs（桌面 + 移动 fork 各 1）+ 宿主 `src/plugin/bindings.rs` + 能力域 5（pty-engine / server-http / server-websocket / server-peer-net / discovery-engine）。**目录模式解析的合成 package 与 HEAD 等价（§7.4 实证），绑定层输出零变化**。
5. **cap-desktop.wit 的 world 顺序与 interface 顺序**：按桌面 WIT 原顺序排列（host-events-desktop → … → auth-policy）；world cap-desktop 只 import 12 个路径 B 域 + export events/abi/abi-form（host-pty/http/websocket 经各自 cap world 并入）。

### 7.3 门禁实跑

- **等价比对（`/tmp/verify_ticket03.py`，口径 = 票面 §5）**：
  - 桌面：合成 45 块 vs 基准 40 块；**6 world 成员集合全一致**（plugin 30 / plugin-binary 1 / plugin-ws 1 / plugin-task 1 / plugin-auth-policy 1 / plugin-system 7，含 include 展开）；**5 附加 world 逐字一致**；38 个 interface 逐字一致；7 处 DIFF 全为注释定稿（abi / events-binary / host-bus / host-events / host-mdns / host-peer / lifecycle），函数集全部一致。
  - 移动：合成 26 块 vs 基准 24 块；**2 world 成员集合一致**（plugin 21 / plugin-binary 1）；plugin-binary 逐字一致；其余 interface DIFF 全为注释对齐桌面（含 host-mdns Android MulticastLock 注记移入 cap-mobile 头部保全）。
  - 结论：双端**通过**（零 FAIL）。
- **幂等**：双端 `--check` 连跑两次 sha256 清单 diff 空。
- **漂移锁变异自检**：手改生成物 core.wit 一字节 → `compose-wit.mjs desktop --check` 红（exit=1，点名 core.wit 漂移）→ 还原 → 绿。1/1。
- **回归**：桌面 SDK `cargo check --features wasm` 绿（bindgen 真展开——默认 features 不含 wasm，首版 check 是假阳性，已用 wasm feature 验证）；移动 SDK `cargo check --features wasm` 绿；内核 `cargo check --tests` 0 error（32 告警为存量 database.rs 票 18 在途等）；桌面宿主 `cargo check --lib` 绿（`check --tests` 回基线：ws_e2e / ws_output_perf 的 EndpointAuth E0308 为 ticket-02 记录的既有基线，零新增）；移动宿主 `cargo check --lib` 绿（40s，移动 fork component.rs path 目录化为必要配套）。
- **§4.4 静态锁**：`src/plugin/bindings.rs` `#[cfg(test)] mod tests` 两条（compose 清单字段完整 + cap-desktop import 面 == 宿主路径 B 域 12 接口）——`cargo test --lib plugin::bindings` 2/2 绿。注：首版放 `tests/` 集成测试因 wasmtime 全量链接超时，移入 lib cfg(test)（只编 lib，0.00s）。

### 7.4 注释定稿 diff 记录（等价比对允许的差异，理由已述 §7.2.2）

桌面侧 7 处：abi（版本历史迁 cap-desktop 保全，core 中性化）、events-binary（去 v11 + 桌面详细版）、host-bus（去 v11×2 + 载荷格式段）、host-events（v36 切片说明 → 双端交集描述）、host-mdns（`<plugin-id>::mdns:found` 修正）、host-peer（去 v30/v31×6）、lifecycle（吸收移动行尾注释）。
移动侧：同 core 基底差异 + command/host-config/host-storage/host-log/host-plugin-database/manifest 的桌面头注释行（对齐桌面，信息量增）。

### 7.5 欠账（如实记）

- 桌面宿主 `--lib` 全量测试与 `cargo test --test compose_manifest_lock`（集成版）未跑：集成测试二进制 wasmtime 链接 10G+，磁盘/时间不可行（ticket-02 同口径欠账）；静态锁已移 lib 级实跑。
- 双端 CHANGELOG 条目 + 桌面/移动 code-map 登记（核心 WIT / 能力域分片 / 端清单 / bindgen path 目录化）+ ADR 0045 转 accepted：随票 04 收口统一补（ticket-02 接缝提示同口径）。
- 能力域 bindgen 改自持分片（票 05）：本票只把 5 个能力域 bindgen path 目录化指向端生成物，自持分片（peer/mdns 自持同名 interface 定义等）留票 05。