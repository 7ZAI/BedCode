# 前端 composables 单测审核报告

审核范围：`bedcode-mobile/src/__tests__/composables/*.test.ts`（11 个文件）
审核标准：unit-test-discipline G1-G6

---

### 1. 模块概览

- **测试文件数**：11
- **总行数**：2247
- **测试用例数**：约 87（`it`/`it.each` 展开后）
- **覆盖的被测模块清单**：

| 被测模块 | 测试文件 | 用例数 |
|---|---|---|
| `useFileTree` | `useFileTree.test.ts` | 16 |
| `writeCoalescer` | `writeCoalescer.test.ts` | 14 |
| `useTuiCompat`（sniffer / seq / compat） | `useTuiCompat.test.ts` | 11 |
| `useTerminalBuffer` | `useTerminalBuffer.test.ts` | 12 |
| `useTerminalScroll` | `useTerminalScroll.test.ts` | 6 |
| `useNotification` | `useNotification.test.ts` | 9（含 `it.each` 4 展开） |
| `useLinkEncryption` | `useLinkEncryption.test.ts` | 9 |
| `presetTaskState` | `presetTaskState.test.ts` | 18 |
| `connectionProbe / httpProbe` | `connectionProbe.test.ts` | 9 |
| `useViewportPanGuard` | `useViewportPanGuard.test.ts` | 8 |
| `useAppStartup` | `useAppStartup.test.ts` | 11 |

整体质量优秀：契约追溯注释清晰、正反例完整、几乎全部使用强断言（返回值精确值、字段值、异常类型）、独立性强、mock 边界基本正确（只 mock 跨进程/Tauri 桥）。以下问题按严重级别列出。

---

### 2. 问题清单

#### Blocker

- 无。

#### Major

- `connectionProbe.test.ts:117-136`｜**恒真断言 + 无行为追溯**｜G4、G6｜三条 `it('unreachable/timeout/refused 错误应匹配 DevicesView 的错误处理')` 测试体仅 `const errorMsg = 'mobile.connection.unreachable'; expect(errorMsg.includes('unreachable')).toBe(true)`——被测对象是本地字面量常量，与被测代码 `useHttpApi` 完全无关；即使 `DevicesView` 里字符串常量被误改为 `'foo'`，此测试也不会失败。属于复制实现字符串为字面量的反模式。**建议**：删除这三条测试，或者改为对真实错误处理路径的验证——在 mock invoke 中依次触发 404/timeout/refused 三类响应，断言 `httpProbe` 返回的 `error` 字符串与 DevicesView 中 i18n key 建立映射（例如断言 `result.error` 落入某个白名单，或断言 DevicesView 的 `switch/case` 表包含该 key）。
- `useTerminalScroll.test.ts:197-214`（`handleShortcutsPanelToggle`）｜**变异无法杀死**｜G6、G3｜第二次调用 `scroll.handleShortcutsPanelToggle(120)` 期望 `shortcutsPanelHeight.value === 120`——但新传入值恰好也是 120，即使实现被改成"任何情况下都赋值"，断言依然通过。变异 `if (isAtBottom()) {...}` 变为无条件赋值，测试不失败。**建议**：非底部场景改为 `scroll.handleShortcutsPanelToggle(500)`，再断言 `shortcutsPanelHeight.value === 120`（旧值），才能真正杀死"忘记 isAtBottom 守卫"的变异。

#### Minor

- `useTuiCompat.test.ts:196-231`（`积压超过 MAX_PENDING_DELTA`）｜**依赖时序细节，可读性差**｜G5｜用例用 `for (let i = 0; i < 60; i++) await vi.advanceTimersByTimeAsync(17)` 排空积压，最终 `totalEvents === 122`（2 首窗 + 120 积压上限）依赖实现内部"每窗口 2 个"的调度细节。契约可读但脆弱：若实现把每窗口事件数从 2 改成 1，需要同时修改用例与断言，无注释解释 122 的构成。**建议**：在断言上方加一行注释 `// 2 (首窗) + 120 (MAX_PENDING_DELTA 上限) = 122`；或把测试简化为只断言"总发送量 ≤ 122 且 > 120"，让内部调度细节不锁定测试。
- `writeCoalescer.test.ts:28-42`（`waitForWrites`）｜**真实时间循环，无 fake timers 兜底**｜G5、G6｜用 `Date.now() + 2000` 轮询 5ms 间隔等待写入完成。虽然大多数情况立即返回，但依赖真实定时器；CI 极端慢速下 2s 截止可能被击穿。**建议**：把让出点的 `setTimeout(0)` 也改为 fake timers 驱动，或至少把截止值提升到 5000ms 并记录一次 `performance.now()` 诊断日志。
- `useAppStartup.test.ts:71-78`（`重复打点幂等`）｜**真实 sleep 3ms 依赖时钟**｜G4、G5｜`await new Promise(r => setTimeout(r, 3))` 用来保证两次 `completeStartupTask` 时间戳不同——用 fake timers 更稳。**建议**：`vi.useFakeTimers()` + `vi.setSystemTime()` 或直接注入时钟。
- `useTerminalBuffer.test.ts:109-115`（`prepareSession 超时`）｜**超时边界只测到 9s**｜G2｜用例用 `9000`（>8000），但没有在 7999/8000/8001 的边界上验证——若实现把超时误改成 10s，此测试仍会通。**建议**：再加一条 `advanceTimersByTimeAsync(7999)` 应仍处于 pending，`8001` 才超时。
- `useNotification.test.ts:156-160`（`idle / in_progress 状态一律不发`）｜**未断言错误路径**｜G2｜仅断言"不发"，没有断言调用参数不会包含这些 status。当前实现走早返回，但测试未杀死"误走默认分支发通知"的变异（因为早返回后 `args` 为 undefined 不会被访问，此问题实际不成立；只是建议增加一次调用参数断言 `args.taskStatus`）。可保持现状。
- `useFileTree.test.ts:241-264`（`expandAll in lazy mode`）｜**依赖内部并发调度顺序**｜G5、G6｜断言 `httpMocks.httpGetFileTreeChildren).toHaveBeenCalledTimes(4)` 依赖 src/docs 并行度。实现若把 `Promise.all` 改成 `Promise.allSettled` 或反过来串行，仍会通过总数；但若改成串行+提前返回，会失败。目前可接受，建议在断言前加注释说明"root + 2 并行 + 1 递归"。

#### Nit

- `useTuiCompat.test.ts:169` 用例末尾 `compat.dispose()` 是好事，但 `attach` 后未断言 `isTuiMode.value === true`（第 4 条用例漏掉）——可读性上略欠。
- `useLinkEncryption.test.ts:28-36` `beforeEach` 只重置了部分单例字段（enabled/strictMode/3 个子开关），未重置 `settings.value` 上未来可能新增的字段——若后续新增子开关且默认值非 true，本套测试会静默失效。建议把 `useLinkEncryptionSettings` 内部的默认值对象抽出为具名常量，`beforeEach` 用 `Object.assign` 整体重置。
- `useFileTree.test.ts:103-108` `cache: remount with same session id reuses module cache without refetching` 使用 `httpGetFileTree` 只被调用一次来证明"缓存命中"——但若实现"每次都调用但返回旧数据"，测试仍通过。属实现约定问题，非测试缺陷，仅作提示。
- 全部文件中未发现 `toMatchSnapshot`、`.skip`、`.only`、快照替代行为断言。合规。

---

### 3. 评分卡

| 维度 | 分数 | 说明 |
|---|---|---|
| 需求/行为契约追溯性（G1） | **82** | 每个 `describe` 顶部都有中文契约注释；`presetTaskState` 追溯 spec 21 条清单；个别场景（`expandAll lazy`）依赖内部并发细节 |
| 正反例覆盖（G2） | **85** | 每个业务规则普遍有正+反例（通知开关、加密通道判定、TUI 双条件门控、fileTree 缓存/失败、viewport panGuard 上/下边界）；`prepareSession` 超时边界覆盖不足 |
| 边界+异常覆盖（G3） | **80** | `writeCoalescer` 512KB 阈值、TUI 单次/窗口双上限、fileTree 懒加载失败置空防重试、加密畸形公钥拒绝写入——异常路径覆盖到位；`prepareSession` 8000ms 边界未精确断言 |
| 断言强度（G3、G6） | **70** | 85% 测试为强断言（精确值/字段/异常/消息）；`connectionProbe` 3 条为恒真断言；`useTerminalScroll.handleShortcutsPanelToggle` 变异无法杀死 |
| 独立性+确定性（G5） | **75** | `beforeEach` 清理模块单例做得好（fileTree 用唯一 sessionId、linkEncryption 显式重置、appStartup 用 `resetModules`）；`writeCoalescer.waitForWrites` 用真实时间轮询，`useAppStartup` 用真实 `sleep(3)` |
| 可读性+可维护性 | **88** | 每个 `it` 有中文描述；关键阈值以常量形式声明；测试辅助函数（`flushAsync`、`makeMockTerminal`、`seedSettings`）复用得当 |
| **总分（加权平均）** | **79** | **< 80，需要重写（局部）：主要是 `connectionProbe` 尾部 3 条 + `useTerminalScroll` 1 条用例，共 4 处，占 ~5% 用例数** |

加权方案：契约 20%、正反例 15%、边界异常 15%、断言强度 25%、独立性 15%、可读性 10%。

---

### 4. 高风险未覆盖清单

**模块有测试但明显缺失关键场景**：

- **`useTuiCompat`**：未测 `feedOutput` 收到超大 chunk（>64KB）时的 CSI sniffer 内存占用/截断行为；未测 `sendWheel` 在 `MAX_PENDING_DELTA` 上限**恰好等于**时的边界（现在只测 130 > 120 的截断）。
- **`useTerminalBuffer`**：未测 `terminalGetHistory` 返回空/失败时 `registerRealtimeHandler` 的降级行为；未测 `prepareSession` 边界（7999/8000/8001ms）。
- **`useFileTree`**：未测 `refresh` 失败时的错误处理（当前只测成功）；未测 `updateSettings` 传入未知字段时被安全忽略。
- **`writeCoalescer`**：未测"rAF 已调度但 coalescer 在帧间隙被 dispose 后又立即 write"的竞态；未测 `MAX_WRITE_CHUNK` 恰好等于阈值（64KB）时的分块数量。
- **`useNotification`**：未测 `showTaskNotification` 收到 `args.taskStatus` 为 `undefined` 或非预期字符串时的默认行为。
- **`useLinkEncryption`**：未测 `applyPin` 收到 fingerprint 但公钥为空的组合；未测 `setStrictMode` 变更后的即时生效（只测了 `syncLinkCryptoContextToNative` 推送参数）。

**模块没有测试但应该有的关键路径**（本次审核范围内已覆盖，无需新增）：
- 本次审核清单已覆盖全部 11 个 composable 测试文件，未发现"完全无测试"的 composable。

---

### 5. 改进优先级建议

**P0（必须修，直接违反 G4/G6）**：
- `connectionProbe.test.ts:117-136` — 删除 3 条恒真断言，或改造为真实错误路径验证（触发 `httpProbe` 的 404/timeout/refused 分支）。
- `useTerminalScroll.test.ts:205-208` — `handleShortcutsPanelToggle(120)` 改为 `handleShortcutsPanelToggle(500)`，验证"非底部忽略新值"。

**P1（建议修，提升确定性/边界覆盖）**：
- `useTerminalBuffer.test.ts:109-115` — 增加 7999/8001ms 超时边界用例。
- `useAppStartup.test.ts:74` — 真实 `sleep(3)` 改为 `vi.setSystemTime` + fake timers。
- `writeCoalescer.test.ts:36-42` — `waitForWrites` 截止 2s 提升到 5s 或改为全 fake timers 驱动。
- `useTuiCompat.test.ts:196-231` — 在断言前加注释解释 `122 = 2 + 120` 的构成，降低对内部调度细节的耦合。

**P2（可选）**：
- `useLinkEncryption.test.ts` 的 `beforeEach` 建议改为 `Object.assign` 整体重置单例默认值对象。
- `useFileTree.test.ts:103-108` 缓存命中用例可补充断言"实现未调用 fetchTree"（当前只断言调用次数）。
- 全部 `it` 描述建议保持中文注释风格一致（当前混用中英文，可接受）。

---

**总结**：这批测试整体质量在 BedCode 移动端代码库中处于上游水平——契约追溯、正反例、mock 边界、异步确定性处理都做得扎实。主要问题是 4 处弱测试（3 条恒真断言 + 1 条变异无法杀死），修复后预计加权总分可从 79 提升至 88-90。建议本次 CI 前完成 P0 修复，P1 排入下一个迭代。
