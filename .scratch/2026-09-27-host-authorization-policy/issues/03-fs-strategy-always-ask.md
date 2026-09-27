# 03: 「总是询问」档（文件侧）

**What to build:** 用户把某 wasm-app 的文件策略设为「总是询问」后，即使该目录此前已被授权，插件再碰它也每次弹窗询问；用户点「以后都拒绝」后不再弹窗且直接拒绝。

**Blocked by:** 02

**Status:** done（2026-09-28）

- [x] 策略求值抽成按 `ResourceKind` 分派的共用函数（spec §6.1 顺序）；fs 与 network 各自只提供「目标归一化 + 记录匹配」两个小实现
- [x] `always_ask` 下已授权目录**仍询问**（不读 allow 记录）
- [x] `always_ask` 下已有 deny 记录**不再询问**——跳过的只是 allow 记录，不是用户已说过的拒绝
- [x] deny 记录优先于第一方免弹窗目录
- [x] 弹窗按钮 = 允许本次 / 拒绝 / 以后都拒绝；**不出现**「记住」（与「跳过记录」语义矛盾，且会造成两处口径）
- [x] 「以后都拒绝」落 `effect='deny'`；30s 超时按拒绝且不落任何记录
- [x] 策略判定时**实时读取**，不缓存、不做激活期快照
- [x] 已在等待应答的弹窗按弹出时的旧策略走完，不半途改口径
- [x] 策略持久化于 `plugin_auth_policies`，重启后仍生效
- [x] 设置页出三档策略控件（本票先只让「总是询问」与「默认」可选）
- [x] manifest 声明策略档位时**加载期显性报错**（宿主不得替插件决定业务策略）
- [x] **不**在本票把 fs 从手工内联链接到 `framework.rs` 的 `ResourceAuthorizer`（独立重构票，不夹带）

## 测试纪律

- [x] 变异自检：`always_ask` 改回读记录、deny 移到第一方层之后——两个变异各杀死 ≥1 项（实测 5 项 / 1 项，见下）

## 实现记录（2026-09-28）

**共用策略求值（`wasm_core/security/strategy.rs`，新）**

- `StrategyStep`（`Ask` / `ConsultRecords` / `AutoAllow`）+ 档位→动作映射 `StrategyStep::of`（**唯一**映射点）、
  `uses_records()`（一个判断管两件互为镜像的事：判定读不读 allow 记录、弹窗给不给「记住」）、
  `as_str()`（日志层名，排障要能一眼看出走的哪一支）。
- `evaluate(store, plugin_id, resource)`：按资源分派读档位并求值，**判定时实时读取**（spec §8.1，不缓存、
  不做激活期快照）。fs 传 `AuthResource::Fs`、network 传 `AuthResource::Network`，两资源共用同一张策略表
  与同一套动作映射。模块文档写明 spec §6.1 的完整顺序，并点明「顺序为什么必须共用」（两处各写一遍必然
  漂移，漂移形态是安全语义级的）。
- **未**把 fs 改接 `framework.rs::ResourceAuthorizer`（票 03 明确不夹带）；`framework.rs` 本票零改动。

**判定管线（`security/fs_auth.rs`）**

顺序：deny 记录 → 第一方目录 → **策略档位** → allow 记录（新表，操作集覆盖）→ 旧记录回退 → 询问。

- `NoDialogDecision::Ask` 改为 `Ask(AuthStrategy)`：进入询问时把**弹出时档位**一起带出去（渲然决定集 + 应答落账口径）。
- `StrategyStep::Ask`（always_ask）⇒ 直接 `Ask(AlwaysAsk)`，**跳过新表 allow 与旧记录两条路径**；
  `StrategyStep::AutoAllow` ⇒ 本票**显性拒绝**（fail-visible；票 04 换成就地放行 + 落账。静默当默认档会让
  用户配的档位与实际行为不符）；档位读失败 ⇒ 按 `Ask(Default)` 处理并 error 日志（fail-safe 方向，
  与 `AuthStrategy::parse` 的未知值口径一致）。
- 第一方集成目录仍排在策略**之前**（spec §7 裁定：档位管不着这批内置免询问项），deny 仍排在它们之前（spec §6.1 第 1 步）。
- `PendingRequest` 增 `strategy` 字段（弹出时快照），`offers_remember()` 由它派生。
- `respond(request_id, decision: FsDecision)` 替代 `(allowed, remember)` 双布尔（`allow_once` / `allow_remember` /
  `deny` / `deny_always`）：`deny_always` 落 `effect='deny'`（`source='user_deny'`，粒度与 allow 同一套
  `grant_scope` 规则）；`allow_remember` 在该弹窗未提供「记住」时**降级为一次性放行**并 warn（弹窗没给的
  按钮不能落一条此后不会被读到的记录）。
- `FsAuthChecker` 的 `app_handle` 抽成 `PromptEmitter` 注入点（与 `network_auth` 同一形态）：测试线程建不出
  `AppHandle`，而「弹窗 payload 带档位 / 应答落账 / 超时不落账」必须在**真实弹窗链路**上验（spec §12.2 的教训）。
  `prompt_timeout` 生产固定 30s、`#[cfg(test)]` 才可调小（fail-safe 的时限不是可配置项）。
- 落账拆成 `land_allow_records` / `land_deny_records` 两个私有方法（`respond` 保持短小、两个方向对称）。

**真源写入面（`security/auth_policy.rs`）**

- `AuthStrategy::parse_wire`（未知值 `None`，**写面显性报错**不猜档位）+ `parse` 改为它的
  `unwrap_or(Default)` 兜底——档位词汇表收成一处，两处各拼一套必然漂移。
- `set_strategy`：upsert（每 (应用, 资源) 至多一行，`ON CONFLICT ... DO UPDATE`，与 `storage.rs` 同形态），
  设置页策略控件唯一写入口。

**命令面（`manager/host/api_bridge.rs` + `lib.rs`）**

- `plugin_fs_auth_respond(request_id, decision)`：未知决定值 `InvalidInput`（**不兜底成放行**）。
- `plugin_auth_set_strategy(plugin_id, resource, strategy)`：宿主面凭证（`require_host_surface` 复用）；
  资源 / 档位未知值均 `InvalidInput`。`lib.rs` invoke_handler 登记。

**manifest 闸门（`manager/validation.rs`）**

- `parse_manifest_json` 改为先解析 `serde_json::Value` → `reject_policy_declaration` → 再反序列化：
  宿主 manifest 结构里**根本没有**策略字段，反序列化之后未知键已被 serde 丢掉，届时再查就查不到了
  （这正是不加这道检查时「声明了也静默无效」的机制）。
- 命中即拒的键名（`auth*` / `authorization*` / `fs*` / `network*` 的 strategy/policy 系列）+ 通用键名
  （`strategy` / `policy` 一类，**仅在取值命中档位词汇表时拒**，避免误伤别的用途）；递归扫描（含 `contributes`
  嵌套与数组元素，只查顶层等于留一条绕过路径）。档位词汇表 `TIER_VALUES` 直接取自 `AuthStrategy::as_str()`。
- 已核对仓库内 17 个真实 `plugin.json`：无任何策略味儿键名（既有插件零影响）。

**前端**

- `FsAuthDialog.vue`：三态按钮（允许本次 / 拒绝 / 以后都拒绝）；「记住」只在 `strategy !== 'always_ask'` 时出现。
  决定集**由宿主下发的 `strategy` 决定**（不在前端按当前档位二次判断——两处各判一次必然漂移）。
  allow 按钮的 `text-white` 顺改为 `--color-primary-contrast`（frontend-styles 反模式表的 token-bound 要求）。
- `views/AuthorizationView.vue`：展开面板内每资源一组三档控件（`STRATEGY_TIERS`，保守 → 宽松），
  「始终允许」禁用并 title 说明（票 04 开放）；点击写 `plugin_auth_set_strategy` → 成功 toast + 重拉读模型
  （不乐观更新，档位徽标跟真源走）；越界点击与「已是当前档位」都不发命令。
- `utils/authPolicy.ts` 增 `STRATEGY_TIERS`；`plugin/commands.ts` 增 `FsAuthDecision` 类型 +
  `pluginFsAuthRespond(requestId, decision)` + `pluginAuthSetStrategy(...)`。
- i18n：`desktop.ts`（`fsAuthAllowOnce` / `fsAuthDenyAlways`）+ `settings.ts`（`strategyControl.*`）zh / en 同步。

**偏离与判断（如实记录）**

1. 「以后都拒绝」在**「默认」档也保留**：spec §6.3 的按钮清单是全局定义，括号里只写了「总是询问档下不出现
   『记住』」；两档共用同一决定集，避免两套弹窗分支（默认档的「允许」受「记住」勾选影响：勾 = `allow_remember`）。
2. 「默认」档的允许按钮文案仍为「允许」（未改成「允许本次」）：是否落账由紧邻的「记住」勾选表达，
   改文案会让两档出现两套口径。
3. 前端样式判断：本票只给既有弹窗加一个按钮、给既有页面加一组内联控件，全部复用既有 token 与按钮样式，
   按 frontend-styles 的豁免条款（既有组件的微调：纯 token 级、按既有蓝图）处理，**未**新引入色板 / 字体 / 布局模式。
4. 交付义务中 CONTEXT.md 词条 / ADR 0022 补段 / WIT「不 bump 的语义变更」登记 / CHANGELOG / code-map
   归**票 09**（其清单明确列出），本票未动。
5. 「重启后仍生效」由「写的是主库 `plugin_auth_policies`、判定侧每次实时查表」这一事实保证；用例断言的是
   写后读回、单行性与**跨资源不串台**（内存库无法真的重开，不假装重开过一次）。

**验证**

- 变异自检（票 03 指定两条 + 前端补两条）：
  - 变异 ①：`StrategyStep::of(AlwaysAsk)` → `ConsultRecords`（= always_ask 改回读记录）⇒ **杀死 5 项**
    （`always_ask_skips_new_table_records_and_asks_again` / `always_ask_skips_legacy_fallback_too` /
    `prompt_payload_carries_the_frozen_strategy` / `pending_prompt_keeps_the_strategy_it_was_raised_with` /
    `strategy_step_maps_every_tier`）。
  - 变异 ②：deny 判定移到第一方层之后 ⇒ **杀死 1 项**（`revoke_records_deny_that_wins_over_first_party_dir`）。
  - 变异 ③（前端）：`offersRemember` 固定 `true` ⇒ 杀死 2 项（「不出现记住」与「allow_once」）。
  - 变异 ④（前端）：放开「始终允许」（去掉 `:disabled` 与越界兜底）⇒ 杀死 2 项（禁用断言 + 不发命令断言）。
- Rust 定向：`wasm_core::security::` **123 passed / 0 failed**（`fs_auth` 39 项，含本票新增 10 项；
  `strategy` 2 项；`auth_policy` 新增 2 项）；`wasm_core::manager::validation` 7 passed。
- Rust 全量（宿主 lib）：**1022 passed / 0 failed**（64s）。
- 前端：`FsAuthDialog` 14 / `AuthorizationView` 14 / `channelIdentity` 5 / `locales` 28 全绿；
  **全量 110 files / 1342 tests 全绿**（36s）。
- `cargo check --all-targets` 0 error；`eslint bedcode-desktop/src` **0 error**（19 warning 全在他人文件）。
- 收尾确认无残留进程（cargo / rustc / vitest / gradle）。

**未做 / 留给后续票**

- `always_allow` 的免询问放行 + `source='always_allow'` 落账 + 500 条封顶 + 降档二次确认（票 04）。
- 网络侧接线 `strategy::evaluate(AuthResource::Network)`（票 06；`network_auth::read_wired_strategy` 目前对
  非默认档显性报错，本票抽出的共用求值函数就是它接入时的落点）。
- 应用详情页四分区读模型（票 07）、第一方免询问项可撤销（票 08）、文档与 CHANGELOG（票 09）。
- 本票改动**未提交**（共用 worktree 多线并行，提交由用户决定）。
