# 04: 「始终允许」档（文件侧）

**What to build:** 用户把某 wasm-app 的文件策略设为「始终允许」后，插件碰到未覆盖的目录不再弹窗、直接放行；管理界面能看到这些目录并标出「未经用户确认」。

**Blocked by:** 03

**Status:** done（2026-09-28）

- [x] 未覆盖的新目标免询问放行，并以 `source='always_allow'` 落账
- [x] 每 (应用, 资源) 记录封顶 500 条，超出丢弃并在 core-monitor 计数
- [x] 从「默认」降到「始终允许」必须**二次确认**
- [x] 管理界面「免询问自动放行」分区带「未经确认」标记，与用户确认的记录视觉区分
- [x] 硬闸门不受档位影响：即使 `always_allow` 且记录命中，路径规范化失败 / 配额超限仍拒
- [x] 语义边界写进 i18n 与文档：切档**不会**一次性授予全盘权限（新目标要等插件实际碰到才产生记录），但持续访问会逐步累积——这是该档位的固有语义

## 测试纪律

- [x] 变异自检：`always_allow` 改成询问，必须杀死 ≥1 项（实测 4 项，见下）

## 实现记录（2026-09-28）

**判定链（`wasm_core/security/fs_auth.rs`）**

- `StrategyStep::AutoAllow` 分支从票 03 的「显性拒绝」换成本票的「就地放行 + 留痕」：
  `land_auto_allow(...)` → `NoDialogDecision::Allowed(FsGrantLayer::AlwaysAllow)`（新层，
  `as_str()` = `always-allow`，排障日志能一眼看出「为什么这次没弹框」）。
- **落账粒度 = 本次请求的规范化路径本身**（与插件直请路径「记住」同一套 `GrantScope::Exact`）：
  记录是**前缀**，逐个目标累积；上溯父目录落账会把 `~/.npmrc` 一类家目录直子项放大成
  整个家目录（`seed_legacy_granted_path` 的历史教训），是该档最该避免的一侧偏差。
- 落账失败 / 容量上限（`DroppedByCap`）**不影响放行方向**：档位语义是「不问」，留痕是义务，
  但留不下痕时拒绝访问是更坏结果（且 §4.2 的硬闸门在更靠前的位置已拦过一遍）。
- 落账发生在 `decide_without_dialog` 内 ⇒ `check` / `check_batch` / `is_granted`（WASI 预打开、
  任务单元等无弹窗面）四处判定**同一口径**、同样留痕（判定单点的既有约定）。
  已核对 `resolve_preopen_dirs` 调用 `is_granted` 时不持 DB 锁 ⇒ 新增的写路径无死锁面。
- `FsAuthChecker::set_monitor`（转交授权真源）——两阶段注入，与 `SecurityFramework` /
  `MessageBus::set_monitor` 同一形态。

**容量上限与计数（`security/auth_policy.rs` + `monitor.rs`）**

- `AUTH_RECORDS_CAP = 500`（spec §8.2）+ `GrantOutcome{Stored, DroppedByCap}`（`grant` 的返回）。
- `grant` 在**新建**行前数该 (应用, 资源) 的记录行数：达上限即丢弃本次落账 +
  `PluginMetrics::record_authz_record_dropped()` + 一条 warn（含 cap / target / source 字段）。
  既有目标的并入（不新增行）与 deny 落账都不受上限影响——**deny 永不允许因容量被丢弃**。
- 计数落在落账点（唯一写面），fs 与将来的 network 共用；`PluginMetrics` 新增
  `authz_records_dropped_total`，快照出 `authz.records_dropped`（与 allow/deny 决策计数分开：
  丢弃是「账本满了」，不是某次判定）。
- 接线：`PluginHost::new`（生产）与 `host/tests/scaffold.rs`（无头）各一行
  `wasm_runtime.fs_auth().set_monitor(wasm_runtime.monitor())`。

**前端（`views/AuthorizationView.vue` + locales）**

- 三档全部可选：去掉「始终允许」的 `:disabled` 与 `locked` title（title 改为各档 hint）。
- `requestStrategy()`：同档静默返回；目标档为「始终允许」→ 挂起 `confirmTarget`（弹 `Modal`
  二次确认）；其余直接 `applyStrategy()`。
- 确认弹窗（复用既有 `Modal` + `Button` 蓝图，无新设计决策）：标题 + 正文含 spec §4.3 的
  语义边界（「不会一次性授予全部权限」/「访问会持续累积」/「可随时逐条取消授权」），
  `confirmOk` 按钮用 `danger` 变体（三档里唯一放宽询问的一档）。
  确认 → 写库 + toast + 重拉读模型；取消 / 点遮罩 / 点 X → 不写库不刷新。
- 记录行：`source='always_allow'` 加琥珀色「未经确认」标记（与详情页 `AuthRecordRow`
  同一口径、同一个 i18n key），与用户确认记录视觉区分。
- i18n zh / en 同步：删 `strategyControl.locked`，新增 `confirmTitle` / `confirmBody` / `confirmOk`，
  更新 `hint.always_allow`。

**偏离与判断（如实记录）**

1. 二次确认对**任何档位 → `always_allow`** 都生效（票面只点名「从默认」）：从「总是询问」
   切过来同样是放宽，不设例外。
2. 容量计数统计该 (应用, 资源) 的**全部**记录行（allow + deny）；deny 不经 `grant`，
   因此不会被上限丢弃（安全事实优先于容量）。
3. 前端样式按 `frontend-styles` 的豁免条款（既有组件微调 + 按既有蓝图复用 `Modal` / `Button` /
   既有琥珀色 badge）处理，未新做设计检索。
4. 文档义务（CONTEXT 词条 / ADR 0022 补段 / WIT「不 bump 的语义变更」登记 / CHANGELOG /
   code-map）仍归**票 09**；本票把语义边界写进了 i18n 与代码注释（`strategy.rs` /
   `fs_auth.rs` / `auth_policy.rs` 模块文档）。
5. 网络侧接线（票 05/06 的 `network_auth.rs`）未动：`AuthPolicyStore::set_monitor` 是共用注入点，
   网络侧接入时在生产接线处补一行即可（本票不碰并行会话在飞的文件）。

**验证**

- 变异自检（实测，逐条还原）：
  - ① `StrategyStep::of(AlwaysAllow) → Ask`（= 始终允许改回询问）⇒ **杀死 4 项**
    （`always_allow_allows_new_target_and_lands_always_allow_record` /
    `always_allow_dedupes_per_target_and_merges_ops` /
    `always_allow_allows_undeclared_target_only_after_declaration` /
    `strategy_step_maps_every_tier`）。
  - ② 去掉 `grant` 的容量分支 ⇒ 杀死 1 项（`grant_drops_new_targets_at_cap_and_counts_into_monitor`）。
  - ③ `land_auto_allow` 直接 return（免询问但不落账）⇒ 杀死 3 项（同 ① 的前三项）。
  - ④ 前端去掉二次确认门（直接写库）⇒ 杀死 2 项（确认流与取消流各一）。
- Rust 定向：`wasm_core::security` **128 passed / 0 failed**；`wasm_core::monitor` 13 passed。
- Rust 全量（宿主 lib）：**1028 passed / 0 failed**（63.9s，票前 1022 ⇒ +6 = 本票新增用例）。
- `cargo check --all-targets`：**0 error**。
- 前端定向：`AuthorizationView.test.ts` 17 passed（本票 +3）；相关 4 文件 69 passed 全绿。
- 前端全量：**110 files / 1345 tests 全绿**（票前 1342 ⇒ +3）。
- ESLint（改动文件）0 error；`vue-tsc --noEmit` 只有 3 条**既有**错误（`src/dev/terminal-mock/TerminalMock.vue`
  缺 3 个 utils 模块，与本票无关、未动）。
- 收尾确认无残留进程 / 端口（8765 / 1420 / 5173）。
- 本票改动**未提交**（共用 worktree 多线并行，提交由用户决定）。
