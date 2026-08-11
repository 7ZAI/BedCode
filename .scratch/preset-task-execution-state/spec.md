# 预设任务执行状态闭环（移动端 auto-task 插件）

Status: ready-for-agent

> 本规格由 grilling 会话（grill-with-docs + domain-modeling）全程决策汇编而成，实现者无需再做重大决策。领域词汇与决策依据见 `CONTEXT.md`「自动任务 (Auto Task)」节与 `docs/adr/0008-preset-task-execution-state-local.md`。

## Problem Statement

移动端 auto-task 插件的"从预设添加"与预设任务之间没有闭环：同一个预设任务可以无限次加入自动任务队列（服务端 `task_queue` 的 `add` 接口按 prompt 原样入队、不做任何来源判断），页面上也看不到预设任务"是否已经用过"。用户希望：**不可重复的预设任务执行过后不能再加入队列，并在页面上以状态标明**。

约束（用户明确）：预设任务是移动端本地轻量数据（卸载/重置即清空），**不入服务端表**；桌面端"预存任务"与移动端"预设任务"是两个独立概念，互不干扰。

## Solution

- 预设任务新增 **可重复 / 不可重复** 属性（创建时设置）
- 预设任务获得 **执行状态**：未使用 → 执行中 → 已完成 / 已中断；状态全部记录在移动端本地（localStorage 扩展），服务端不做任何持久化
- **入队动作即标记"已执行"**（锁定依据 = 用户本地操作，可靠）；服务端完成广播只做状态细化（收到 → 已完成，对账无 → 已中断），不参与锁定判定
- 锁定规则：不可重复 + 已执行（执行中/已完成/已中断）→ 不能再入队；可重复 + 执行中 → 不能重复入队（同一时刻至多一个实例）；可重复完成/中断后恢复可入队
- 解锁路径：编辑预设内容（重置为未使用）、从队列删除/清空该项（回退未使用）
- 展示：auto-task 面板"从预设添加"区与 toolbox 预设任务列表页均显示四态状态标签，锁定的预设禁用添加

## User Stories

1. 作为用户，我希望创建预设任务时选择"可重复 / 不可重复"，这样我能决定这个任务是一次性的还是可反复使用的
2. 作为不可重复预设的用户，我希望它一旦加入自动任务队列就立即锁定（chips 置灰、不可再添加），这样即使应用重启、通知丢失，也不会被重复加入队列
3. 作为用户，我希望收到任务完成通知的预设显示"已完成"，这样我知道它成功执行完了
4. 作为用户，我希望未收到完成通知（中断/通知丢失）的预设显示"已中断"，这样我知道上次没跑完
5. 作为用户，我希望"已中断"的不可重复预设同样锁定、不能再次加入队列，这样不会造成重复执行
6. 作为可重复预设的用户，我希望它执行完成后仍能再次加入队列，这样重复性任务可以反复使用
7. 作为可重复预设的用户，我希望它在队列执行期间不能再次加入，这样同一任务不会在队列里出现两个实例
8. 作为用户，我希望把已入队的预设从队列中删除/清空后，它恢复为可添加状态，这样加错了可以反悔
9. 作为用户，我希望编辑预设内容后其执行状态重置为未使用，这样改过的任务可以重新执行
10. 作为用户，我希望在 auto-task 面板的"从预设添加"区域看到每个预设的状态标签（未使用/执行中/已完成/已中断），这样不用点进去就能判断哪个还能用
11. 作为用户，我希望锁定的预设 chips 置灰禁用，这样不会误触
12. 作为用户，我希望 toolbox 预设任务列表页每行显示状态徽章，这样两个入口看到的状态一致
13. 作为用户，我希望手动执行（toolbox 直接发送）预设成功后它也标记为已执行（不可重复即锁定），这样手动使用过的一次性任务不会被后续加入队列
14. 作为用户，我希望应用重启后打开面板（或进入工具箱预设页），未收到完成通知的"执行中"预设显示为"已中断"，这样状态不会永远卡在执行中
15. 作为用户，我希望收到完成广播时面板上预设的状态实时更新为"已完成"，这样不用手动刷新
16. 作为用户，我希望手动输入（非预设来源）不受任何预设状态约束，这样临时任务照常添加
17. 作为可重复预设的用户，我希望任务完成广播丢失、对账落"已中断"后仍能再次加入队列，这样重复性任务不受通知丢失影响
18. 作为用户，我希望清除/删除队列时，与该队列项关联的预设状态回退为未使用，这样不会误锁定

## Implementation Decisions

### 领域模型（状态机，来自 grilling 决策）

预设任务类型扩展：`{ id, content, createdAt, updatedAt, repeatable: boolean, status: 'unused'|'executing'|'completed'|'interrupted', pendingTaskId: string|null }`。`pendingTaskId` 为本地记录的队列项关联（入队时来自 add 接口返回值），用于广播/移除事件的 task_id 匹配。

状态转换（事件 → 结果）：

| 事件 | 条件 | 结果状态 |
|------|------|---------|
| 入队（enqueue） | 任意可入队预设 | executing（记录新 taskId，覆盖旧记录） |
| 完成广播（taskDone） | taskId 匹配 pendingTaskId | completed |
| 完成广播 | taskId 不匹配（孤儿/手动输入项） | 不变，忽略 |
| 对账（reconcile） | status = executing | interrupted（幂等：interrupted 再对账不变） |
| 手动执行成功（manualExecute） | 任意预设 | completed（清除 pendingTaskId） |
| 队列项移除（queueItemRemoved） | taskId 匹配 pendingTaskId | unused（清除记录） |
| 队列项移除 | 不匹配 | 不变，忽略 |
| 编辑内容（edit） | 内容变化（仅改 repeatable 属性不触发，见 User Story 9 注） | unused（清除记录） |

锁定判定（canEnqueue）：

| repeatable | status | 可入队 |
|-----------|--------|-------|
| false | unused | ✅ |
| false | executing / completed / interrupted | ❌ 锁定 |
| true | unused / completed / interrupted | ✅ |
| true | executing | ❌ 防重复（同刻一实例） |

筛选：面板预设区 = 全部预设 + 各自状态标签；`canEnqueue` 为 false 的置灰禁用。toolbox 列表页仅展示状态徽章（不限制其他操作）。

### 服务端广播补丁（桌面端 auto-task 插件）

- 任务终态 done 时补一次广播：复用现有 `TaskQueueChanged` 事件（SyncEvent 与 bus/emit 双通道），**扩展携带 `task_id` + `status` 字段**（action 沿用现有语义，done 时 action 可取 'done'）
- 向后兼容：新增字段为可选，桌面端既有监听者（TaskHistoryView）不受影响；不修改 `list-task-queue` 返回结构（不改 SQL）
- 不做任何预设任务持久化、不加列、不改入队接口签名（`add` 已返回 `{ task_id, position }`，够用）

### 移动端实现

- 状态机抽成**纯函数模块**（无 Vue / localStorage 依赖：事件 → 新状态，纯输入输出），供面板与 toolbox 列表共享；localStorage 持久化与事件订阅在外层薄封装
- 入队流程：面板 `handleAddFromPreset` → add 接口 → 用返回的 task_id 落本地记录（状态 → executing）→ 刷新面板
- 事件链路：移动端连接层需解析并转发桌面端 `TaskQueueChanged` 广播（含新增字段）到前端；前端订阅后按 task_id 匹配本地记录
- 对账时机：面板打开 + 应用启动后首次进入工具箱预设页（两处均调 `reconcileWithQueue(sessionId)`；仅判定入队会话与该会话一致的执行中预设，防多会话误中断）
- 手动执行（toolbox）：`executeTask` HTTP 成功（code 0）→ 本地标记 completed
- 删除/清空：面板删除/清空队列成功后，按 task_id 反查本地记录回退 unused
- 编辑：预设编辑保存后重置 unused

### 展示

- 面板"从预设添加"chips：状态标签（未使用/执行中/已完成/已中断，沿用队列状态的颜色语义：进行中 accent、完成绿、中断橙/错误色）+ 锁定置灰
- toolbox 预设列表页：每行状态徽章，同一状态源
- i18n：新增 key 同步 zh-CN 与 en（属性命名、状态标签、锁定提示）

## Testing Decisions

**好测试的标准**：只测外部行为——给定预设状态与事件，断言状态转换结果与锁定判定结果；不测 localStorage 调用次数、不测 Vue 组件内部、不 mock 状态机自身。

**被测模块（TDD，红 → 绿 → 重构）**：状态机纯函数模块（唯一新测试 seam）。测试先于实现编写，覆盖全部状态转换与判定。

**测试用例清单（穷举）**：

状态转换（每个断言：给定 {repeatable, status, pendingTaskId} + 事件 → 期望状态）：
1. 入队：unused → executing，记录新 taskId
2. 入队：可重复 completed 预设 → executing，新 taskId 覆盖旧
3. 入队：可重复 interrupted 预设 → executing，新 taskId 覆盖旧
4. 完成广播：executing + taskId 匹配 → completed，清除 pendingTaskId
5. 完成广播：taskId 不匹配 → 状态与记录不变（忽略孤儿）
6. 对账：executing → interrupted
7. 对账：interrupted 再对账 → interrupted（幂等）
8. 对账：unused / completed → 不变
9. 手动执行：任意状态 → completed，清除 pendingTaskId
10. 队列项移除：taskId 匹配 → unused，清除记录
11. 队列项移除：不匹配 → 不变
12. 编辑：任意状态 → unused，清除记录

锁定判定（canEnqueue，覆盖 2×4 矩阵）：
13-20. 不可重复 × {unused, executing, completed, interrupted} 与 可重复 × {unused, executing, completed, interrupted} 全组合

筛选：
21. 混合预设列表 → 仅返回可入队子集（含不可重复未使用、可重复完成/中断；排除全部锁定项）

**Prior art**：`plugins/ai-chatbox/src/__tests__/useAiChat.test.ts`（插件内 composable 纯逻辑 vitest 先例）；测试命令 `npm run test:run`（vitest run）。

**不设测试**：桌面端 Rust 广播补丁（插件无 Rust 测试先例，WasmHost mock 成本高，靠移动端集成验证）；UI 组件层（薄展示，逻辑已由状态机测试覆盖）。

## Out of Scope

- 桌面端"预存任务"（Stored Preset）：保持独立，任何改动
- 服务端 schema 变更（task_queue 不加列、预设任务不入表）
- 广播的可靠重传/消息确认：通知是尽力而为，不做丢失补偿
- 队列项与预设的持久化关联（不依赖服务端）
- 定时任务（Scheduled Auto Task）与预设任务的关联
- 预设任务状态的服务端同步（多设备）

## Further Notes

- ADR-0008 记录"本地记录 + 入队即锁定 + 广播细化"的权衡与拒绝方案；CONTEXT.md 收录「预设任务 / 预存任务 / 可重复 / 已执行 / 执行状态」术语
- 移动端连接层若已有通用的 SyncPayload 转发机制（如文件服务、会话模式变更事件），TaskQueueChanged 转发应复用同一通道，不新开链路
- 手机重置/卸载后本地状态清零，服务端残留队列项成为孤儿记录，不影响新循环
- 可重复预设"执行中"的占用判定依赖本地记录（无广播也会在重启对账后落 interrupted 而解锁），这是接受"尽力而为"后的固有特性
