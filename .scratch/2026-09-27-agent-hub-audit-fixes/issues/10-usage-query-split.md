# 10: 会话列表查询域拆分与筛选回显

**What to build:** 拆 `useUsage` 的共享/私有列表状态，消除「使用统计」（追加 load-more）与「会话日志」（按页 replace）共用一份 `sessions` / `page` / `loadedOffset` 导致的两条错误路径（A 重复行 / B 页码与行数不符）；并让 `SessionLogsTab` 的两个日期选择器在 tab 切回时正确回显已应用的时间范围（现状：显示空、列表仍被过滤，点「查询」会静默清掉日期条件）。顺带把 `StatsTab.vue:135` 的 `syncedTag` 从硬编码 `adapters.claude + adapters.pi` 改成遍历 `Object.values(state.adapters)`。

> **基线提醒**：agent-hub 工作区有票 04 的未提交在途改动（+32/-23，含 `useUsage.ts`），本票引用的行号以工作区为准（`useUsage.ts` :38 / :121 / :152-154）；若那些改动先落地，:78 之后行号会 +3 漂移。

**Blocked by:** None

**Status:** resolved

- [x] 路径 A 复现步骤下明细列表**无重复行**（可写单测断言：先 load-more 到 3 页、日志 tab `goPage(2)`、回统计 tab load-more 后 `sessions` 的 id 序列无重复且连续）
- [x] 路径 B 复现步骤下表格行数 == 当前页 `PAGE_SIZE`，分页器 `第 N/共 M 页` 与行数自洽
- [x] 设了日期范围 → 切到统计 tab → 切回来，两个日期框显示原值；点「重置」后清空
- [x] `syncedTag` 在 mock 里加第三个 adapter 后数值随之变化（有单测）
- [x] `pnpm exec vitest run wasm-apps/agent-hub` / `tsc --noEmit` / eslint 全绿

## Answer（2026-09-27 实施完毕）

**改法**（`useUsage.ts` 拆两层，共享条件与两份列表彻底分离）：

```
共享：listFilter / searchText / rangeFrom / rangeTo
统计私有（追加语义）：statSessions / statTotal / statLoaded / statLoading
日志私有（按页语义）：logSessions / logTotal / logPage / logTotalPages / logLoading
```

- `fetchPage(offset)` 只做取数；`loadStatPage(replace)` 按 `statLoaded` 追加，
  `loadLogPage(page)` 按页覆盖 —— 两个游标物理隔离，**不可能再串味**。
- `reloadSessions()`（扫描回流 / 查询 / 重置 / 共享条件变更）让两份列表同时回第 1 页。
- 失败分支各自独立 `try/finally`，`loading` 复位，**保留已加载内容不清空**。

**顺带修掉的 P2-4**：`StatsTab` 的 `syncedTag` 从硬编码 `claude + pi` 改为遍历
`Object.values(state.adapters)` 求和，新增适配器自动纳入（有单测钉住第三家）。

**日期回显**：`SessionLogsTab` 的 `fromInput/toInput` 改为
`ref(usage.rangeFrom ? new Date(...) : null)`，与已正确回填的 `filterAgent`/`keyword` 对齐。
组件随 tab 切换 unmount/remount（`AgentHubView` 的 `v-if/v-else` + `Transition mode="out-in"`），
故 setup 期读共享域即可，无需额外 watch。

**验证**：`src/__tests__/useUsage.test.ts` 20 用例（U1–U9），含两条复现路径的完整序列断言。

| 用例 | 断言 |
| --- | --- |
| 路径 A | 统计 load-more 到 45 行 → `goPage(2)` → 回统计 load-more：id 序列**无重复且连续**（1..45），统计列表不被日志页切片污染，日志仍是 15 行 / 第 2 页 |
| 路径 A′ | 日志先翻到第 3 页 → 统计 load-more 仍按自己游标续到 30 行，日志页码不受影响 |
| 路径 B | 统计 45 行后日志表格行数 ≤ PAGE_SIZE 且 `page`/`totalPages`/`logTotal` 自洽 |

**变异自检**：让 `goPage` 改写 `statLoaded`/`statSessions`（模拟旧串味）→ U3 两条变红 ✅

**验证**：`pnpm exec vitest run wasm-apps/agent-hub` 172/172 绿；tsc 0 error；eslint 0。
