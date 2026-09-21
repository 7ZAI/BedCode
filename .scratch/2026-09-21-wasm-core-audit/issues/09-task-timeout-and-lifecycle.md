# 09: core-task 单元超时 + 生命周期收尾（P1-3 及三处状态机缺陷）

**What to build:** 并发任务域兑现「单元超时」这条已写进文档与常量的承诺，并把生命周期三处漏口收干净：并发双激活去重、退出时 Degraded 插件也走 `on_shutdown`、purge 与 guest deactivate 的次序不再留泄漏小窗。

**Blocked by:** 无

**Status:** ready-for-agent

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

## Comments

- 2026-09-21 立项：来源 spec §2（`manager/host.rs + host/**`、`manager/task.rs` 两行）与 §5-P1-3。
