# 票 19 · 防回接与漂移锁（SDK 对照锁 Part A 可与票 17 批次 2 并行）

Status: **未实施；Part A（SDK 对照锁）已定稿可与票 17 批次 2 并行；Part B（双端结构锁）挂 17b 之后**
专项: `.scratch/2026-10-07-mobile-wasm-core-refactor`（阶段 4 第三票）
依据: spec §5 票 19 + ADR 0022（双端偏离 / wire 形状单真源）+ ADR 0040（fork 面收缩路线）+ 票 02（五同步点口径）。
依赖: Part A 零依赖（只碰 `packages/plugin-sdk-mobile/rust/` + fork crate `tests/` + 文档）；Part B 依赖票 17 批次 2 结构定稿。

---

## 0. 一句话目标

把「双端机制双份 = 每次机制修复 / ABI 演进两处同步」的漂移税，用**源码扫描锁**钉死：
- **Part A（本票先行）**：移动 SDK ↔ WIT 真源 ↔ fork crate 消费面的**全接口逐函数对照锁**（票 02 点名的 `mobile_parallel_copy_shape_lock` 扩展）；
- **Part B（17b 后）**：wasm-core 双端对称结构锁 + `crate_boundary_lock` 移动端登记 + 防回接锁索引更新。

## 1. 现状实测（2026-10-08 工作区）

### 1.1 `mobile_parallel_copy_shape_lock` 不存在（文档字面 ≠ 事实）

- ADR 0022（§318 行）与 CHANGELOG（双语）声明「移动端保留平行副本并由 `mobile_parallel_copy_shape_lock` 逐变体钉住」——**全仓 grep 零命中该锁文件**（桌面/移动/SDK 均无）。
- 处理：**本票 Part A 落地该锁**（兑现文档承诺），并在落地时修正 CHANGELOG/ADR 的声明口径（由「已有」改为「票 19 落地」或直接保留兑现后状态）。

### 1.2 已有对照锁先例（Part A 的形状参照）

- fork crate `packages/bedcode-wasm-core/src/permission.rs` 内嵌单测 `fork_visible_permissions_match_sdk_source`：读 SDK `permission.rs` 源文件的 `VALID_PERMISSIONS` 静态表，与 fork crate re-export 可见词汇集逐字断言一致——**词汇漂移锁的现成范式**，Part A 将其扩展为独立 lock 文件并补五同步点全链。
- fork crate `tests/fork_boundary_lock.rs`（3 例 + 变异 3/3）：钉住「不绑桌面契约 / 不依赖桌面能力域 / 不复活桌面域文件 + 机制核在场」——Part B 的对称结构锁在其上扩展。
- 宿主退役锁 12 把（`src-tauri/tests/retired_mobile_*.rs`）：源码扫描锁的标准形状（非注释行 + needle 点名 + 变异自检）。

### 1.3 WIT v17 真源（对照锁的数据底座，`packages/plugin-sdk-mobile/rust/wit/bedcode.wit`）

- **16 import 接口**：`host-storage` / `host-database` / `host-plugin-database` / `host-events` / `host-http` / `host-fs` / `host-config` / `host-log` / `host-bus` / `host-peer` / `host-mdns` / `host-platform` / `host-websocket` / `host-terminal-stream` / `host-connection` / `host-auth`
- **5 必选 export + 1 可选**：`command` / `lifecycle` / `events` / `manifest` / `abi` + `events-binary`（宿主动态探测，不进 world）
- **2 world**：`plugin-binary` / `plugin`
- ABI 真源：SDK `src/abi.rs` `pub const ABI_VERSION: u32 = 17`
- 权限词汇真源：SDK `src/permission.rs` `VALID_PERMISSIONS`（已 `pub`，fork crate 经 re-export 消费）

### 1.4 五同步点移动侧实测（票 02 口径对照）

| 同步点 | 移动侧现状 | Part A 锁落点 |
| --- | --- | --- |
| ① SDK 常量 | `permission.rs` `VALID_PERMISSIONS`（真源） | 数据底座 |
| ② 打包 CLI / manifest 校验 | 无独立白名单（`scripts/plugin-build.js` 零权限词 grep） | 锁应断言「无第二份白名单」存在即红 |
| ③ 前端合法集合 | 零命中（前端无权限词校验面） | 同上（前端直用 manifest，无集合面则不锁） |
| ④ 宿主能力清单 | fork crate `permission.rs` re-export（已有单测） | 扩展为 lock 文件 |
| ⑤ 权限门 | 宿主 `permission.rs` 仲裁面（re-export 同源） | 词汇锁覆盖 |

## 2. 批次划分

- **Part A（可并行，本票先行）**：只碰 SDK（`packages/plugin-sdk-mobile/rust/`）、fork crate（`packages/bedcode-wasm-core/tests/` 新增文件）、文档。**不碰移动宿主 `plugin/`**（与 17b 零文件重叠）。
- **Part B（17b 后）**：fork crate 结构定稿后，扩展对称结构锁 + `crate_boundary_lock` 登记 + code-map 索引。

## 3. Part A 设计（4 把锁，`packages/bedcode-wasm-core/tests/sdk_wit_contract_locks.rs`）

> 锁的数据底座 = **WIT 文件本身**（读源文件解析接口/函数名，不手抄清单——手抄即第二真源）。参照 `fork_boundary_lock.rs` 的目录遍历 + 非注释行形状。

### 锁 A1 · WIT 接口清单锁（ABI 漂移早发现）
读 `bedcode.wit`，断言：16 import 接口名集合 == 锁内声明表（逐名点名）；5 必选 export + `events-binary` 可选 + 2 world 在场；`abi.version()` 常量与 SDK `ABI_VERSION`（当前 17）一致。**增删/改名接口即红**——ABI bump 必须显式走 ADR 0019 流程先改锁。

### 锁 A2 · 权限词汇五同步锁
- 读 SDK `permission.rs` `VALID_PERMISSIONS` 静态表（逐行收集 `PERMISSION_*` 常量值）；
- 断言 fork crate `permission` re-export 面可见词汇集与之一致（现内嵌单测扩展为锁文件）；
- 断言移动侧**无第二份白名单**（扫 `scripts/`、`src/`、`packages/plugin-sdk-mobile/dev-shell/src/` 的权限词校验面，出现独立集合即红——五同步点②③的口径是「不另立」而非「同步维护」）。

### 锁 A3 · WIT ↔ host_api 域接线对照（全接口逐函数，**17b 后启用**）
- 数据：WIT 16 import 接口函数名清单 ↔ fork crate `host_api/` 16 域 impl（每 interface 有且仅有一个域实现）；
- **17b 批次 2 前不启用**（fork crate `host_api/` 现仅 `context/http_engine/ports/sql_guard` 4 文件，域未迁入）；锁文件先行落骨架（接口清单表 + 域映射表），宿主切换后补断言。
- 附 `events-binary` / `terminal-stream` 等移动独有面：钉住「桌面 events-ws/events-task/auth-policy 不回流」。

### 锁 A4 · wire 形状逐变体对照（`mobile_parallel_copy_shape_lock` 兑现）
- 移动宿主 `src-tauri/src/enums/{auth,control,plugin,session,sumary}.rs`（平行副本）与 SDK `wire`（桌面侧 `packages/bedcode-plugin-api` wire 或移动侧对应真源）逐变体断言一致；
- 落地时以实测真源为准：若移动 wire 形状真源已是 SDK / fork crate 侧，则锁方向为「宿主 enums 垫片面 ↔ crate 真源逐字段」；若仍是宿主自持，锁钉「宿主副本不得漂移」并注明收口方向（票 21 文档联动）。
- 变异自检 3 例：① WIT 增接口 → A1 红；② SDK 加权限词未同步 → A2 红；③ 宿主 enums 字段改名 → A4 红。全部还原后 `git diff` 复核。

## 4. Part B 设计（17b 后）

1. **双端对称结构锁扩展**（fork `fork_boundary_lock.rs` 上）：`wasm-core`（桌面）与 `bedcode-wasm-core-mobile`（fork）机制核模块路径对称在场断言——一侧以「裁剪」为名删机制即红。
2. **`crate_boundary_lock` 移动端登记**：fork crate 不得回接桌面能力域/桌面 SDK（锁 1 已覆盖主体，登记防回接锁索引）。
3. **防回接锁索引更新**：`bedcode-mobile/docs/code-map.md` 文末锁索引补 `fork_boundary_lock`（3 例）+ Part A 4 锁 + 票 20 egress 锁。

## 5. 并行约束（Part A 与票 17 批次 2）

| 文件 | 属主 | 说明 |
| --- | --- | --- |
| `packages/plugin-sdk-mobile/rust/**` | **本票 Part A** | 锁读源文件（只读）+ 可能补 `pub`（`VALID_PERMISSIONS` 已 pub，零 SDK 改动预期）；SDK WIT 零改动（v17 冻结） |
| `packages/bedcode-wasm-core/tests/**` | **本票 Part A** | 新增 `sdk_wit_contract_locks.rs`；不动 `fork_boundary_lock.rs` 本体 |
| `bedcode-mobile/src-tauri/src/**`（宿主 plugin/） | **17b 所有** | Part A 禁碰；锁 A3 骨架先落数据表，断言补在 17b 后 |
| `bedcode-mobile/src-tauri/src/enums/` | **本票只读** | 锁 A4 扫源文件；宿主 enums 形状在 17b 不动（垫片面批次 2 评估） |

**注意**：锁 A4 若发现「宿主 enums 与 SDK wire 已漂移」——先记账不改代码，收口口径留给 17b（宿主切换时 enums 归属一并定）；Part A 只落锁与文档。

## 6. 门禁

- Part A：fork crate `cargo test` 全绿（新锁 4 例 + 既有 233 例）+ 变异自检 3/3 + 移动宿主 `cargo check` 零变化（Part A 不动宿主）+ 桌面 `packages/bedcode-wasm-core` 零改动
- 根 `pnpm exec eslint .`（若动了前端测试则端目录 vitest；预期零前端改动）
- 文档联动：CHANGELOG 双语补 `mobile_parallel_copy_shape_lock` 兑现条目（修正 §1.1 声明）；ADR 0022 相关行加票 19 指针；code-map 锁索引
- Part B（17b 后）：fork crate + 宿主全量 + 12 把退役锁 + fork_boundary_lock 回归

## 7. 风险

| 风险 | 吸收 |
| --- | --- |
| 锁读 WIT 源文件解析脆弱（排版漂移） | 锁只断言「接口名集合 + 函数名集合」，不解析语法；WIT 语法变更先改锁 |
| A3 骨架先落、17b 后才启用 → 空锁嫌疑 | 骨架锁仍断言接口清单与 SDK ABI_VERSION（A1 已覆盖）不悬空；A3 的域映射断言 17b 后补，文档明示 |
| 锁误伤合法 ABI bump | 锁是点名式清单：ABI 演进 = 先改锁再改代码（ADP 0019 双端同步口径不变） |
| Part A 与 17b 同时改 fork crate `tests/` | 文件所有权分配（§5）——Part A 只新增 `sdk_wit_contract_locks.rs`，17b 的 `fork_boundary_lock` 不动 |
