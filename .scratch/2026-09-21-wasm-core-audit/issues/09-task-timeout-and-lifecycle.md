# 09: core-task 单元超时 + 生命周期收尾（P1-3 及三处状态机缺陷）

**What to build:** 并发任务域兑现「单元超时」这条已写进文档与常量的承诺，并把生命周期三处漏口收干净：并发双激活去重、退出时 Degraded 插件也走 `on_shutdown`、purge 与 guest deactivate 的次序不再留泄漏小窗。

**Blocked by:** 无

**Status:** 待裁决（2026-09-22：验收 2 已存在、3/4/5/6 已完成并提交 `28cf5e889` + `a2b169a0b`；**只剩验收 1 单元超时**，需用户在「看门狗抢占」与「退役常量」间裁决，见实施记录 §D）

## 现状

- **单元超时未实现**（已复核）：`PLUGIN_TASK_UNIT_TIMEOUT_MS = 600_000`（`system/constants/plugin.rs:176`）全仓零引用；后果：挂死的 `fs.read` / `http.fetch` 单元永久占用池槽且不被任务墙钟抢占，8 线程池是进程级 static → **跨插件无隔离**，单插件可耗尽全池；
- **shutdown 注释漂移**（来源审查记录，未逐行复核）：`manager/task.rs:18-20` 宣称「shutdown 时 cancel 全部任务等池排空」，无对应函数与调用者；
- **双激活竞态**（已复核）：`host/activation.rs:139-145`，阶段 1 的 `match loaded.state` 中 `Activating` 落入 `_ => proceed` → 并发两次 `activate_plugin` 会各跑一次 guest `activate()`（实例锁只保证串行、不保证去重），现仅靠 SDK `OnceLock` 兜底；
- **`deactivate_all` 漏 Degraded**（已复核）：`activation.rs:26-33` 过滤仅 `Activated` → Degraded 插件退出时收不到 `on_shutdown`（其持久化态按 true 计，`:608-629`）；
- **purge 次序小窗**（来源审查记录）：`deactivate_plugin_inner` 的 `purge_for_plugin`（`:475-488`）先于 guest `deactivate` → 在飞 guest 调用可在 purge 后再登记句柄；
- **purge 后回调 channel 复活**（来源审查记录）：`task.rs:861-868` 在跑单元的终态事件会重建 channel → map 条目与消费任务永久残留。

## 验收

- [ ] 单元超时生效：从常量读取、超时单元按 fail-visible 记为失败/取消，**不占用池槽**；红测 = 一个人为挂死单元的用例在超时后释放槽位且任务终态正确
- [ ] 池按插件维度加**并发在册上限**（沿用「调用方声明 + 宿主上下限 + 越界报错」口径：声明进 plan/参数，宿主以 `PLUGIN_TASK_*` 仲裁），杜绝单插件耗尽进程级池
- [ ] 若决定不做 shutdown 排空：删除 `task.rs:18-20` 的漂移注释；若做：补实现并在本票记录调用点
- [ ] 双激活去重：`Activating` 显式分支（等待在途激活完成或直接幂等返回 Ok），并发用例断言 guest `activate()` 恰执行一次
- [ ] `deactivate_all` 覆盖 `Degraded`（含其持久化态判定），退出用例断言 `on_shutdown` 被调用
- [ ] purge 次序改为「guest deactivate → purge → 注销订阅」或在 purge 后加一道**属主级二次清扫**，并用例断言无残留句柄；purge 后到达的终态事件不得重建回调 channel
- [ ] 重入红线不回归：池线程永不回调进插件（仅经消费派发 + 实例锁）、持锁跨 `.await`（AGENTS §7 A0-3）
- [ ] 门禁：`cargo test`（含 `test_task_*` 矩阵全绿）+ `cargo check --lib --tests`

## 实施记录（2026-09-22，提交 `28cf5e889`）

### A. 已完成三项

| 验收 | 处置 |
| --- | --- |
| 5 `deactivate_all` 覆盖 Degraded | filter 放宽为 `Activated \| Degraded(_)`。持久化态判定 `get_activated_state` 本来就认 Degraded（`:748`），无需改——说明「漏 Degraded」只在退出路径这一处 |
| 4 双激活去重 | 阶段 1 的 `match &loaded.state` 新增 `Activating` 显式分支，幂等返回 Ok。**不引入「等待在途激活完成」**：那需要跨调用同步原语（在途句柄表 + condvar）与额外死锁面；返回值只承诺「已在激活中」，终态仍以状态机为准 |
| 3 shutdown 漂移注释 | 判据：全仓 `task.rs` 无 drain / 排空 / shutdown 实现，生命周期入口只有 `purge_for_plugin`（按插件）与 `cancel`（按任务）⇒ 走票面「不做则删」分支，注释改为如实描述（池是进程级 OS 线程，退出即随进程回收，且退出后已无结果消费方） |

用例 2 条（均在 `host/tests/lifecycle_test.rs`）+ 变异自检：过滤改回仅 Activated →
`test_deactivate_all_covers_degraded` 转红；移除 Activating 分支 →
`test_repeat_activation_during_activating_is_idempotent` 转红；其余 6 条保持绿。

**诚实边界**：双激活用例断言的是状态机层面（「Activating 期重复激活不推进状态」），不是「guest
`activate()` 恰执行一次」——后者需要真实 WASM fixture 与 guest 侧计数，无头测试里断言不到真实 guest
调用次数；状态机去重是 guest 只执行一次的充分条件。

### B. 复核结论：验收 2 已实现（只缺用例）

`PLUGIN_TASK_MAX_JOBS_PER_PLUGIN = 4` 在 `register_job`（`:549-566`）已做每插件在册上限仲裁，且带
「惰性 GC 终态任务」防配额误拒 ⇒ 与票面「调用方声明 + 宿主上下限 + 越界报错」口径一致。
**缺一条配额用例**：`task.rs` 的 `mod tests` 只有纯函数用例（无 `WasmHostContext` 构造工具），
补它要先造宿主上下文 fixture —— 留待下轮连同单元超时一起做。

### C. 已完成：purge 次序与回调 channel 复活（验收 6，提交 `a2b169a0b`）

- `enqueue_event` 新增判据 `may_open_callback_channel(owner)`：注册表里已无该属主在册任务时直接
  丢弃、**不新建** channel（`purge_for_plugin` 只 cancel 任务、不中断在跑单元，此前在飞单元跑完
  走到这里会重建 channel + 消费派发任务 → 二者永久残留，且事件派发给已停用的插件）。
  判据不误伤正常路径：`submit` 的 started 事件在 `register_job` 之后投出，任务必在表；
  `execute-batch` 不留痕但也不投事件。
- `deactivate_plugin_inner` 在 guest `on_shutdown` / `deactivate` **之后**再跑一遍
  `task::purge_for_plugin`（幂等二次清扫），补上「原 purge 早于 guest 清理」的次序小窗。
- 用例 `may_open_callback_channel_requires_registered_job` + 变异自检（函数改恒 true → 转红）；
  既有 `task_e2e::test_task_submit_events_dispatched_and_status` 仍绿，作为「正常路径未被误伤」的
  回归证据。

### D. 未完成：单元超时（验收 1）——**需用户裁决后实施**

`PLUGIN_TASK_UNIT_TIMEOUT_MS` 仍零引用。根因：`execute_unit` 是**同步阻塞体**（`fs::fs_read` /
`http::http_fetch` / `process::process_run_sync` 直调），池线程内没有抢占点。

- **选项 A：看门狗 + 弃用线程补偿**（真释放槽位）。新增 in-flight 单元表
  （job_id / index / owner / started_at_ms）+ 一条监督线程按 `PLUGIN_TASK_UNIT_TIMEOUT_MS` 扫描，
  超时单元写 fail-visible 结果并推进并发窗口；被卡住的池线程永不返回 ⇒ 必须为池补 spawn 一条新
  线程，否则长期运行后池被耗尽（比现状更糟）。代价：新增全局状态与一条常驻线程，且要处理
  「监督线程与池线程同时写一个单元结果」的竞态（需给单元加 Pending/Running/Done 状态位）。
- **选项 B：退役常量 + 如实写文档**（与验收 3 同思路）。删除 `PLUGIN_TASK_UNIT_TIMEOUT_MS`，
  文档写明「v20 不提供单元级抢占：任务级墙钟 `PLUGIN_TASK_JOB_TIMEOUT_MS` 是唯一兜底，阻塞单元
  的超时由被调用方自带参数负责（如 `process.run-sync` 的 timeout）」。代价：票面承诺不兑现，
  但消除「常量与实现脱钩」的漂移。

### D. 门禁（实跑）

`cargo test` 全 target 全绿：**lib 1140 passed / 0 failed**，8 个集成 target 与 doctest 全过，
`[skip]` 计数 0。

## Comments

- 2026-09-21 立项：来源 spec §2（`manager/host.rs + host/**`、`manager/task.rs` 两行）与 §5-P1-3。
