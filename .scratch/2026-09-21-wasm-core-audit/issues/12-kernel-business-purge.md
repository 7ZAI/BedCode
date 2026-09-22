# 12: 内核业务语义清零（platform-kernel 清单收敛）

**What to build:** 内核不再携带任何产品身份与产品生命周期：`FILE_TRANSFER_PLUGIN_ID` 及其驱动的 peer-net 启停从 `manager/host/activation.rs` 移出，改为「插件按声明请求引擎服务、内核按声明装配」；两个一次性迁移模块登记退役条件并摘出内核主目录。修完后阶段 4「内核 ABI 冻结」才有可冻结的面。

**Blocked by:** 11（收敛后才有单一改造点）

**Status:** blocked（2026-09-23 开工复核：**两个接线点被并发线占住，其中一个是语义级冲突而非文本级**，
详见文末「开工复核」。本票 6 项验收里 **2 项已落**（验收 4 文案清理、验收 5 依赖登记，见「本轮已落」），
其余四项（peer-net 改造、migration 摘出、grep 防回归锁、门禁）等并发线 v24 落地后再做）

## 现状（2026-09-21 立项；**2026-09-23 重新核对，行号与条目数都已漂**）

| 残留 | 位置（2026-09-23 实测） | 性质 |
| --- | --- | --- |
| `FILE_TRANSFER_PLUGIN_ID` 常量 | `peer_net.rs:641`（定义 + 唯一带产品名的消费点在 `:666`）、`manager/host.rs:27`（同名常量再定义一处） | 内核硬编码产品身份。**两处同名定义**（票 11 收敛后仍各留一份） |
| 激活/停用成功路径按该 id 开关 peer-net 节点 | `manager/host/activation.rs:85`（激活）、`:589`（停用）<br>立项时记的是 `:69` / `:448`，已漂 | 内核携带产品生命周期 |
| 状态对账的第三个入口 | `lib.rs:482` 启动装配调 `peer_net::sync_node_with_plugin_state`（内部走 `plugin_transfer_activated()` → 硬编码 id） | **立项时未列**：节点生命周期其实有三个入口（激活外壳两处 + boot 对账一处），改的时候要一起换，漏掉 boot 对账会出现「外壳已去产品名、启动仍按 id 判」的半改造态。注释记着这条对账存在的原因：boot 装配期 AppContext 全局未注册，activate 外壳里的节点启动会被静默跳过（2026-09-06 实机实证） |
| preauth 语义注释绑定 file-transfer | `manager/host/preauth.rs:13` | 文案残留 → **本轮已清**（见「本轮已落的两项」） |
| `WasmHostContext` 持 `SessionManager` / `SessionConfigManager` | `wasm_runtime.rs:428`（结构体）/ `:436` + `:438`（字段）/ `:923` + `:924`（ctor 参数）<br>立项时记的是 `:917-927`、`:246`，已漂 | 阶段 3 存量（roadmap 明标终端本体留内核），非新违规但属冻结前必清 → **本轮已登记**，且登记时核出两条性质不同（`SessionManager` 活的 / `SessionConfigManager` **零读取点的死字段**） |
| 一次性迁移模块每次启动执行 | **四个**（立项时两个）：`plugin.rs:18` `auth_records_migration`、`:24` `quick_actions_migration`、`:28` `session_db_migration`、`:30` `task_data_migration`；挂钩点 `lib.rs:459 / 464 / 468 / 475` | 活的过渡态。**scope 已翻倍**：`session_db_migration`（插件 id 改名带来的私有库迁移，2026-09-22）与 `auth_records_migration`（认证记录下沉，**并发线 2026-09-23 00:11 新建并已接进启动链**）都晚于本票立项。quick_actions 反向依赖 `utils/auth/auth_center` 且硬编码业务 api 名 `com.bedcode.session.quick-actions-import` |

> **给下一轮的提醒**：验收 3 的措辞「两个 migration 摘出 `plugin/`」按现状应改成**四个**，
> 且其中两个（`session_db_migration` / `auth_records_migration`）此刻正在并发线的改动面上。
> 摘出动作的范围与顺序要重开一次判定，别照 2026-09-21 的两项清单做。

## 需裁决项

1. peer-net 节点启停的正确归属：(A) `host-peer` 增引擎级「按需启动节点」原语，由 file-transfer 插件激活时自行调用（推荐——同 `host-pty`/`host-mdns` 的「引擎留内核、语义归插件」形态）；(B) 插件声明 `dependencies` 由内核装配（但 peer 引擎是宿主服务而非 WASM 能力，需先定义「宿主服务也可被依赖声明」这一新形态）；
2. 两个 migration 的退役触发条件与时间窗（契约退役即删），以及删除前是否保留只读兜底；
3. `host-session` / `host-terminal` 两域与 platform-kernel「进程原语 vs 会话业务」清单的最终边界（属 roadmap 阶段 3 终端部分，本票只登记不实施）。

## 验收

- [ ] 按裁决项 1 改造后，`manager/**` 内 grep 无产品名（`file-transfer` / `session` 业务前缀 / `quick_actions` / `auto-task` / `pairing`），把这条 grep 固化为测试或 CI 断言（防回归）
- [ ] file-transfer 插件行为等价：激活/停用后 peer 节点状态与现在一致，界面维持；移动端不受影响（peer 面本就桌面独有）
- [ ] 两个 migration 摘出 `plugin/`（归 `manager/` 安装迁移面，或独立 `migrations` 面并登记退役条件），其硬编码业务 api 名改为常量清单并注明退役时机；`plugin.rs` facade 的 `pub mod` 文档注释随之更新
- [x] `preauth.rs:13` 等文案残留清理 → **已落**（2026-09-23，见「本轮已落的两项」；顺带厘清它与票 07
  「特权表项逐条注释归属」规则的边界）
- [x] `WasmHostContext` 的 `SessionManager` 依赖登记进 roadmap 阶段 3 未完成清单（不在本票实施，但必须显式留痕，禁止「视而不见」）
  → **已落**（roadmap 新增「阶段 3 —— 冻结前未完成清单」节）。登记时核出两条依赖性质不同：
  `SessionManager` 是活的装配依赖、`SessionConfigManager` 是零读取点的死字段
- [ ] 门禁：`cargo test` 全绿 + `cargo check --lib --tests`

## Comments

- 2026-09-21 立项：来源 spec §6 表。与 `microkernel-gap.md` 的「阶段 4 冻结前置清单」同源。

### 裁决（2026-09-22 用户裁决，开工前已定）

1. **peer-net 归属 = 选项 A**：`host-peer` 增引擎级「按需启动节点」原语，由 `com.bedcode.file-transfer`
   激活时自行调用；内核不再认得任何产品身份（与 `host-pty` / `host-mdns` 的「引擎留内核、语义归插件」同形）。
   按 AGENTS §7 现口径，既有 interface 的函数级追加在同一批次内不 bump ABI。
   选项 B（依赖声明装配宿主服务）本轮不做。
2. **两个一次迁秹模块**：登记「**契约退役即删**」并摘出内核主目录（归 `manager/` 的安装迁移面或独立 `migrations` 面）。
   **不设日历时限**（本仓库没有可挂的版本发布节奏），改为在模块头与本票写明触发条件 + 加一条检查用例，
   被迁移契约一旦退役即由该用例转红提醒删除。
3. **`WasmHostContext` 的 `SessionManager` 依赖**：按票面只登记进 roadmap 阶段 3 未完成清单留痕，不在本票实施。

## 本轮已落的两项（2026-09-23，与接线点无关）

- **验收 4「preauth.rs:13 文案残留」已做**：`PREAUTH_PATHS_STORAGE_KEY` 的注释原写
  「file-transfer mount-local 时追加写入」——把消费方产品名钉在内核常量上。改为按**角色**描述
  （写入方是插件自己的挂载配置面，经 `host-storage` 用同名 key 追加/去重；宿主不认得是谁写的）。
  可追溯性不丢：key 名 `preauth_paths` 本身就是锚点，实测唯一写入方是
  `plugins/file-transfer/rust/src/peer.rs::mount_local`。**注释归属规则（票 07 对
  `FIRST_PARTY_TRUSTED_DIRS` 那条「逐条注释归属」）针对的是「特权表项要说得出消费它的函数」——
  一个 storage key 的文档注释不属于那一类，为它保留产品名是本票要清的残留而不是要保的归属。**
- **验收 5「`WasmHostContext` 会话依赖登记」已做**，登记在
  `.scratch/2026-09-10-plugin-kernel-roadmap/spec.md` 新增的「阶段 3 —— 冻结前未完成清单」节。
  登记过程中核出一条**与票面假设不同**的事实，两条依赖不是一条：
  | 依赖 | 票面假设 | 实测 |
  | --- | --- | --- |
  | `SessionManager`（`wasm_runtime.rs:436`） | 阶段 3 存量，冻结前必清 | **活的**——`host_impl/lifecycle.rs:29,51` 经 `session_manager_arc()` 注册会话生命周期与输入监听，是 host-session 原语的装配依赖，删不掉；冻结前要判的是「换成窄接口还是原样定形」 |
  | `SessionConfigManager`（`:438`） | 同上 | **死的**——全 `src/` 零读取点（构造时塞进 `:946` 就没人取过），与票 11 退役的 `granted_permissions` 同类。正解是删字段 + 删 ctor 参数，不是登记成待清存量 |
  删除动作未随本轮落：`WasmHostContext::new` 有 6 处调用点，其中 `host_impl/mod.rs` 在并发线在途面上，
  且改的是 pub ctor 签名——单独做、单独验，别塞进一个文案 commit。

## 开工复核（2026-09-23）：两个接线点被并发线占住，本票暂缓

裁决已收齐（选项 A），但**此刻实施会在两个位置上与另一条活跃线做语义对撞**，不是文本冲突：

### 1. peer-net 原语 = WIT 函数追加，而 ABI v24 已被占用

裁决 1 的形态（`host-peer` 增「按需启动节点」原语 + 插件激活时自行调用）必然要动
`packages/plugin-sdk-desktop/rust/wit/bedcode.wit` 与 `rust/src/abi.rs::ABI_VERSION`。
并发线此刻两份文件都在途未提交，且**他们已经在把 `ABI_VERSION: 23 → 24`**，内容是
host-auth 记录面七函数退役 + host-session 配置读取面删除（破坏性收缩，旧产物须按 v24 SDK 重建）。

- 我现在追加就得声明 v25，压在一个**尚未定形的 v24** 上：他们 v24 的函数增删若再变，
  v25 的语义基准跟着漂。AGENTS §7 的词汇/计数漂移锁正是防这个。
- 而且 v24 本身要求全插件产物按新 SDK 重建；同窗口再叠一次 v25 重建，两批产物谁对谁错无法判读。

### 2. 迁移模块摘出 `plugin/`，而并发线正在往同一处**加**迁移模块

验收 3 要把 `plugin/quick_actions_migration.rs` / `plugin/task_data_migration.rs` 摘出内核主目录，
涉及 `plugin.rs` facade 与 `lib.rs:474,483` 挂钩点。并发线 2026-09-23 00:09 刚写过 `plugin.rs` 与
`lib.rs`，并且**新建了 `src/plugin/auth_records_migration.rs`**（认证记录下沉带来的一次性迁移）——
即他们正在把票 12 想搬走的那个目录**扩大**。此方向上两边动作正相反，先落的一方会被后落的一方搬回原地。

### 3. 记录一条**已被否掉的替代形态**（免得下轮当成捷径再走一遍）

选项 A 要动 WIT，所以自然会想「能不能不改 WIT，让内核自己按权限推导节点该不该起」：
把 `peer_net::plugin_transfer_activated()` 的硬编码 id 换成「有任一已激活插件声明 `peer` 权限 ⇒ 起节点」。
**这条不成立，实测否掉**：`com.bedcode.terminal-session` **同样声明了 `peer`**
（`plugins/terminal-session/plugin.json` permissions 含 `peer`；`mdns` + `peer` 两个都有的是 file-transfer）。
按权限推导的结果是「只要会话插件在跑，peer-net 节点就常开」，直接违反本票验收 2
「file-transfer 激活/停用后 peer 节点状态与现在一致」——现在的语义是「文件传输插件在跑才广播」，
停用 file-transfer 后本机不再在 `_bedcode-peer` 上广播（`ensure_node_started` 上方注释记着这条改判的
理由：旧实现无条件自启，对端照样发现且监听未关，受信对端可直连数据面）。
若改走「插件声明一个专用 manifest 字段 ⇒ 内核按声明装配」，那就是裁决 1 里明确**本轮不做**的选项 B。

**结论：本票的两项核心（peer-net 归属改造、迁移模块摘出）都要等并发线的 v24 批次与
`plugin/` 迁移面落定后再做**，做的时候按裁决 A 的形态走，并把 ABI 号取在他们之后的下一个。
本轮只落与接线点无关的验收 4 / 验收 5。
