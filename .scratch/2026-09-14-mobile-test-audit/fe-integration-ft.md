# 前端集成测试 + file-transfer 插件测试审计报告

审计范围：bedcode-mobile 前端集成测试（8 文件）+ file-transfer 插件前端测试（7 文件）
审计方式：静态代码审查，对照 unit-test-discipline G1-G6 硬性门禁
审计日期：2025-11

---

### 1. 模块概览

- **测试文件数**：15（integration 8 + file-transfer 7）
- **测试用例总数**：107
- **总行数**：3748
- **覆盖的被测模块**：
  - `useMobileConnection`（连接/配对/会话/终端 4 个 L2 场景）
  - `useHttpApi`（HTTP 代理层）
  - `terminalBuffer` store + `useTerminalBuffer` + `writeCoalescer` + xterm
  - `pluginLoader` / `pluginRegistry` / `plugin/events`
  - file-transfer 插件 composables：`useConsent`、`useTasks`、`usePeerDevices`、`useRemoteFs`、`useTrustedPeers`、`useSettings`、`deriveDeviceRows`

- **测试文件行数分布**：connection-flow 356 / session-flow 321 / useConsent 456 / useTasks 362 / usePeerDevices 320 / useRemoteFs 236 / plugin-lifecycle-teardown 234 / pairing-flow 213 / terminal-flow 205 / useTrustedPeers 186 / plugin-loader-gating 178 / useSettings 155 / deriveDeviceRows 141 / pluginReactivate 121 / plugin-flow 163

---

### 2. 问题清单（按严重级别）

#### Major

- **usePeerDevices.test.ts:145** | Major | `await new Promise((r) => setTimeout(r, 2100))` — 真实 2.1s 睡眠等待 debounce 落盘，违反 G4「真实时间/sleep」。整个文件未使用 fake timers。测试慢且潜在 flaky（CI 慢机可能超 debounce 窗口或反之） | **G4** | 改为 `vi.useFakeTimers()` + `vi.advanceTimersByTimeAsync(2100)`；`beforeEach` 加 `vi.useFakeTimers()`、`afterEach` 加 `vi.useRealTimers()`。

- **usePeerDevices.test.ts:109** | Major | `await new Promise((r) => setTimeout(r, 5))` — 快照恢复后真实 5ms 微任务延迟，同样是真实时间依赖 | **G4** | 并入上一条 fake timers 修复，改为 `vi.advanceTimersByTimeAsync(5)` 或直接 `flushAsync()`。

- **plugin-loader-gating.test.ts:155-156** | Major | 两次 `await new Promise((r) => setTimeout(r, 0))` 等待 `plugin_mark_error` 微任务；测试整体未使用 `vi.useFakeTimers()`。虽然 setTimeout(0) 通常无害，但与 `helpers.ts` 的 `flushAsync()` 惯例不一致，且当被测逻辑引入真实 setTimeout（如降级重试）时会立刻 flaky | **G4** | 改为 `await flushAsync(4)` 或 `await vi.waitFor(() => expect(markErrorCount(...)).toBe(1))`。

- **plugin-loader-gating.test.ts 全文（G1/G2 缺失）** | Major | 门禁测试只覆盖 `plugin_is_enabled=true` 的 6 态放行路径；测试文件顶部注释明确说明「isEnabled=false → 跳过」，但无任何用例验证该分支。该意图门禁是 spec §3.5 移动端的**核心裁决**（区别于桌面端 2 态门禁），漏测等于核心契约未保护 | **G1/G2** | 追加 1-2 个用例：`isEnabled=false` 时 Activated/Degraded 插件也走跳过路径（`markErrorCount===0`）；或按插件 id 返回 false 验证逐条判断而非统一返回。

#### Minor

- **plugin-loader-gating.test.ts:36-42** | Minor | `import { pluginLoader } from '@/plugin/loader'` 直连模块级单例（区别于本目录下 plugin-flow / plugin-lifecycle-teardown / pluginReactivate 均用 `loadFreshModule` 或 `vi.resetModules()` + `await import()`）。当前只有 1 个用例不受影响，但若新增用例，前一个用例的 `pluginLoader.loaded` Set / `plugins` Map 会残留污染后续用例 | **G5** | 顶部 import 改为测试内 `vi.resetModules()` + `const { pluginLoader } = await import('@/plugin/loader')`；或明确在文件头注释说明「本文件只允许单用例，新增须迁移到 fresh-module 模式」。

- **plugin-flow.test.ts:99-108** | Minor | 首个用例断言 `plugin_is_enabled` 只被调用 1 次（对 `frontend-disabled`），但**没有断言 `plugin_list_loaded` 被调用 1 次**——rust-only 分支跳过是否发生在 list_loaded 之前未被验证；变异「list_loaded 循环 2 次」不会杀死测试 | **G6** | 追加 `expect(invokeCalls('plugin_list_loaded')).toHaveLength(1)`。

- **plugin-flow.test.ts:130-133** | Minor | 「空清单扫描轮询」只断言 `plugin_list_loaded` 调用 2 次与 `plugin_is_enabled` 为 0；若第一次返回空列表后轮询**永不触发**（`setTimeout` 未调度），测试仍会因 `expect(...).toHaveLength(2)` 失败，但失败信息不能定位是「轮询未启动」还是「轮询启动但没查」 | **G6** | 追加中间态断言：第一次 `loadAll` 完成时 `plugin_list_loaded` 为 1，250ms 之后为 2（分两段断言）。

- **connection-flow.test.ts:156** | Minor | `expect(probeCall).toBeTruthy()` 是弱断言（任何非 null/undefined 都过）；虽随后紧跟 3 条精确断言（url/kind/timeoutMs），但 `probeCall` 本身可能为 `[]`（空数组）时 `toBeTruthy()` 也会通过 | **G3** | 改为 `expect(probeCall).toBeDefined()` 或直接 `expect(probeCall![0].request.url).toBe(...)` 并依赖 `!` 非空断言。

- **plugin-lifecycle-teardown.test.ts:126** | Minor | `expect(pluginLoader.getActivePlugin('mock-plugin')).toBeDefined()` 是弱断言（任何非 undefined 对象都过）；未断言返回的插件条目 `id` 是否正确 | **G3** | 改为 `expect(pluginLoader.getActivePlugin('mock-plugin')?.id).toBe('mock-plugin')`。

- **plugin-loader-gating.test.ts:175** | Minor | `expect(degradedWarn).toBeTruthy()` 弱断言，虽随后 `toContain(DEGRADED_REASON)` 会兜底，但 `degradedWarn` 可能匹配到不相关 warn 行 | **G3** | 先 `expect(warns.filter((w) => w.includes('com.bedcode.gate-degraded'))).toHaveLength(1)`，再对首元素 `toContain`。

- **useTasks.test.ts:175** | Minor | `expect(env.calls).toContainEqual({ id: 'file-transfer.resume-all', args: {} })` — 断言 `args: {}` 依赖调用侧严格传空对象；若实现改为「无参调用」(args undefined)，测试失败但业务无意义 | **G4** | 弱化为 `expect(env.calls.some((c) => c.id === 'file-transfer.resume-all')).toBe(true)`，或明确文档化「resume-all 契约要求传空对象」。

- **useSettings.test.ts:97** | Minor | `expect(env.calls.at(-1)!.args).toEqual({...})` 假设最后一次调用就是本次 setReceivingPolicy——若被测代码在失败路径下又调用了别的命令，断言会误绑 | **G6** | 改为 `expect(env.calls.filter((c) => c.id === 'file-transfer.set-settings').at(-1)!.args).toEqual(...)`。

- **useSettings.test.ts 全文** | Minor | `addRoot` 用例未断言成功路径 `env.calls.some(c => c.id === 'file-transfer.mount-local')`；只断言失败路径不 reload，成功路径的 mount-local 命令是否路由未被验证 | **G1/G6** | 追加 `expect(env.calls.some((c) => c.id === 'file-transfer.mount-local')).toBe(true)`。

- **useTrustedPeers.test.ts:176-183** | Minor | `formatTrustedDate('2026-08-01T10:30:00+08:00', 'zh-CN')` 只断言 `toContain('2026')` 和 `not.toBe` 原字符串——**未断言具体日期格式**（如是否含月/日/时/分/秒、是否含中文/星期）。变异「格式化只保留年份」不会杀死该测试 | **G3/G6** | 断言更具体：`expect(out).toMatch(/\d{4}年\d{2}月\d{2}日/)` 或固定格式字符串。

- **terminal-flow.test.ts:162-172** | Minor | 「输入回传」测试的 `sendInput('s1', 'ls -la\n', 'enter')` 断言 `ok === true` 与 invoke 精确参数，但未验证 `sendInput` 在**订阅但未 live** 时的行为（`emitState('s1', 'live')` 之前调用）——按 store 逻辑此时应拒绝 | **G2** | 追加断言：不 emit live 前 `sendInput` 返回 false 且不路由 invoke。

- **session-flow.test.ts:190** | Minor | 「删除会话」先 emit `ws_sync_session_removed` 后断言 `bufferStore.getBuffer('session-1')).toBeUndefined()`，但**未验证**在删除前 store 确实持有 buffer（防御：若 `ensureBuffer` 未被调用，`toBeUndefined()` 恒真） | **G6** | 追加中间断言 `expect(bufferStore.getBuffer('session-1')).toBeDefined()` 在 removed 事件之前。

- **pluginReactivate.test.ts:60-66** | Minor | `mockRouterRemove = vi.fn()` 和 `mockRouterAddRoute = vi.fn(() => mockRouterRemove)` 定义了但**全文未断言其被调用**——`addRoute` 是否真的注册了路由无测试证据 | **G3/G6** | 追加 `expect(mockRouterAddRoute).toHaveBeenCalledTimes(2)`（1 次 activate + 1 次再激活）或清理未使用的 mock。

- **useConsent.test.ts:340** | Minor | 「重复事件幂等」用例的 3 次 `emitConsent({ ...request })` 未验证 `pendingCount.value` 在第 2 次后仍为 1（只验证最终值 1），若实现「首次入队、第 2 次不入队但第 3 次入队」这种非单调 bug 会漏测 | **G6** | 在每次 emit 后追加 `expect(consent.pendingCount.value).toBe(1)`。

#### Nit

- **helpers.ts:18-24** | Nit | `flushAsync` 中 `await new Promise((r) => setTimeout(r, 0))` 在真实 timer 下是微任务延迟而非 macrotask；注释说「微任务队列排空后触发」但实际语义是「等 macrotask 边界」——描述与实现不精确 | — | 注释改为「让出微任务队列并跨越一个 macrotask 边界」。

- **useTasks.test.ts:83、usePeerDevices.test.ts:65** | Nit | 局部 `const flush = () => new Promise((r) => setTimeout(r, 0))` 与 `helpers.ts` 的 `flushAsync()` 语义重叠，可在文件内直接 `import { flushAsync } from '../../integration/helpers'` 减少重复 | — | 复用共享 helper。

- **useRemoteFs.test.ts:80-82** | Nit | 「zero roots keeps the chooser」用例把两个独立场景（空 roots + 目录 notice 透传）塞进 1 个用例，可读性一般 | — | 拆为 2 个用例。

---

### 3. 评分卡

| 维度 | 分数 | 依据 |
|---|---|---|
| 需求/行为契约追溯性 | **86** | 顶部注释几乎每个用例都可追溯到 issue/ticket/spec 章节；插件 lifecycle teardown 甚至对齐 spec §5；主要扣分：plugin-loader-gating 只 1 个用例覆盖 6+2 态、无 isEnabled=false 分支 |
| 正反例覆盖 | **88** | 几乎每个业务规则都有正例 + 反例：consent 覆盖 accept/deny/timeout/malformed/dup/stop；tasks 覆盖 send 空选/失败、clearHistory 确认/取消、failed-only 通知/全取消静默；remoteFs 覆盖 loadRoots 成功/失败/空 |
| 边界+异常覆盖 | **85** | 大量边界：normalizePairedNames 7 种畸形输入、CONSENT_TIMEOUT_MS 超时先结算 + 迟到静默、dial denied/unreachable、TTL 清扫已连接保护、dirId 兜底 |
| 断言强度 | **89** | 极少数弱断言（toBeTruthy/toBeDefined，均已列出）；主断言全部使用精确值/`toMatchObject`/`toContainEqual`/`toHaveLength`；无恒真断言、无快照替代行为 |
| 独立性+确定性 | **72** | 主要扣分：usePeerDevices 未用 fake timers + 2.1s 真实 sleep、plugin-loader-gating 用 module-level 单例无 resetModules、plugin-loader-gating 用真实 setTimeout(0)。其余文件独立性优秀（beforeEach/afterEach 严格清理 + loadFreshModule + 独立 mock context） |
| 可读性+可维护性 | **90** | 每个文件顶部有清晰 seam 说明 + 覆盖范围注释；helpers.ts 抽出的 flushAsync/loadFreshModule/mockProxyResponse 复用良好；注释解释「为什么」而非「是什么」（如「修复边界验证：aborted 复位前的窗口期」）；只有 useTasks 一个用例塞了 3 个子场景（failed-only）轻微扣分 |
| **总分（等权）** | **86** | — |

> 加权（G4 反模式 × 1.5、G1/G2 覆盖 × 1.2 其他 × 1）：
> `(86×1.2 + 88×1.2 + 85 + 89 + 72×1.5 + 90) / 6.9 ≈ 84.3` → **≈ 84 / 100（通过，不需重写）**

---

### 4. 高风险未覆盖清单

#### 有测试但明显缺失的关键场景

- **plugin-loader-gating 的 isEnabled=false 意图门禁**：spec §3.5 的核心裁决，测试注释明确提及但未验证。若实现回归为「不看 isEnabled 一律加载」，此测试不会失败。
- **plugin-loader-gating 的降级原因 warn 内容**：只断言 warn 包含插件 id 和「degraded」字样，未断言 warn 的**完整错误串结构**（是否含插件名、是否含原始 error.message、是否含重试建议）。
- **useConsent 的 30s 超时对**「已排队但未展示」的 request 的处理**：只测了「当前展示中的 request 超时」，未测「第一个 request 超时后，队列中第二个 request 是否立即展示且拥有独立 30s 窗口」。
- **useConsent 的 respond-consent 命令失败路径**：只测了命令成功路径（`hit: true`），未测「后端拒绝受理」（如请求已过期、nodeId 不匹配）。
- **useTasks 的 batch 状态转换**：`batches.value` 只测了初始加载，未测 `plugin:file-transfer:batches-changed` 事件（虽然 `listenerCount` 验证已订阅）。
- **useTasks 的 cancel-receiving 失败路径**：命令路由只测了 happy path。
- **usePeerDevices 的 dial-peer 命令拒绝后 dialError 的**自动清理**：`deriveDeviceRows.test.ts` 覆盖了「已连接时清除 dialError」，但 usePeerDevices 侧未测「第二次 dial 成功时旧的 dialError 是否被清除」。
- **useRemoteFs 的深层路径 cd 失败**：只测了 loadRoots 失败，未测 enterRoot / cd 中途网络失败。
- **useTrustedPeers 的 revokingIds 并发保护**：只测了单个 revoke 的 revokingIds 状态，未测并发 revoke 同一 nodeId 时是否只发一次命令。
- **useSettings 的 load 失败**：`load()` 只测了成功路径，未测命令 reject 时的 loading/error 状态。
- **plugin-flow 的 scan loop 边界**：`plugin_list_loaded` 抛错时的行为未测。

#### 完全没有测试但应该有的关键路径

- **plugin/events 的 `emit` 对已 dispose 后重发的行为**：只在 lifecycle-teardown 中通过 `clearPluginEvents` 侧面测过，未直接测 `emit` 的 idempotent / no-op 语义。
- **useMobileConnection 的**多设备切换**：所有连接测试都是单设备场景，未测切换配对设备时的凭据/状态清理。
- **useMobileConnection 的**localStorage 写入失败**（QuotaExceeded）：`resetLocalStorage` 只测 happy path。
- **file-transfer 插件的 `permission denied` 路径**：`plugin_context.commands.execute` 抛「未授权」错误时各 composable 的降级行为无测试。
- **file-transfer 插件的 i18n key 未翻译**（fallback 到 key 本身）的 UI 呈现：consent 只断言 key 存在，未测 t() 未翻译时的用户可见性（可选，属 i18n 层测试范畴）。
- **file-transfer 的 `file-transfer.respond-consent` 幂等性**：consent 侧只测前端幂等，未测后端返回 `hit: false` 时的处理。

---

### 5. 改进优先级建议

#### P0（必须修）

1. **usePeerDevices.test.ts:109, 145** — 真实 setTimeout(5) 和 setTimeout(2100) 改为 `vi.useFakeTimers()` + `vi.advanceTimersByTimeAsync()`，`beforeEach` 加 `vi.useFakeTimers()`、`afterEach` 加 `vi.useRealTimers()`。当前测试最耗时且最 flaky。
2. **plugin-loader-gating.test.ts 追加用例** — 覆盖 `plugin_is_enabled=false` 时 Activated/Degraded 插件也跳过（spec §3.5 意图门禁核心裁决，当前完全漏测）。
3. **plugin-loader-gating.test.ts:155-156** — 两次 `setTimeout(0)` 替换为 `await flushAsync(4)` 或 `vi.waitFor`。

#### P1（建议修）

4. **plugin-loader-gating.test.ts:36-42** — `pluginLoader` 顶部 import 改为 `vi.resetModules()` + `await import()`，防止未来新增用例的模块级状态污染。
5. **useTrustedPeers.test.ts:176-183** — `formatTrustedDate` 断言从「包含 2026」升级为匹配具体日期格式正则。
6. **connection-flow.test.ts:156** — `probeCall).toBeTruthy()` 改为 `toBeDefined()`。
7. **plugin-lifecycle-teardown.test.ts:126** — `toBeDefined()` 改为断言 `.id === 'mock-plugin'`。
8. **plugin-loader-gating.test.ts:175** — `degradedWarn).toBeTruthy()` 前先断言 `warns.filter(...).length === 1`。
9. **useTasks.test.ts:175、useSettings.test.ts:97** — 弱化的 last-call 断言改为按 id 过滤后取末位。
10. **plugin-flow.test.ts:99-108** — 追加 `plugin_list_loaded` 调用次数断言（补 G6 变异缺口）。
11. **pluginReactivate.test.ts:60-66** — 断言 `mockRouterAddRoute` 调用次数或删除未用 mock。
12. **session-flow.test.ts:190** — 删除会话用例中追加「removed 之前 buffer 已存在」的中间断言。
13. **terminal-flow.test.ts:162-172** — 追加「订阅未 live 时 sendInput 拒绝」的边界用例。

#### P2（可选）

14. 抽取各 file-transfer 测试文件的 `makeContext()`（结构几乎一致）到共享 fixture 模块，减少 600+ 行重复。
15. `useTasks` 覆盖 `batches-changed` 事件的实际状态更新（当前只测 listenerCount）。
16. `useConsent` 追加「队列中第二个 request 的独立超时窗口」用例。
17. `useSettings.load` 追加失败路径用例。
18. `useRemoteFs` 拆分「zero roots + notice 透传」为 2 个独立用例。
19. 增加 file-transfer 插件层 `permission denied` 场景测试（跨 composable）。
20. `helpers.ts` 的 `flushAsync` 注释精确化（微任务 vs macrotask 边界）。

---

## 附录：Standards Compliance 总结（对照 BedCode 规范检查清单）

- **错误处理**：文件传输层所有失败路径都测了 toast/errorKey/reject 行为，composable 层错误未吞（`useTrustedPeers.revoke` 失败保留条目 + 上报 key）。符合。
- **`let _ =` 静默忽略**：`plugin-loader-gating.test.ts` afterEach 的 `await pluginLoader.deactivate(id).catch(() => {})` 是**测试清理**语境下的合理忽略（cleanup 失败不影响测试结论），符合。
- **`tokio::spawn` / `spawn_with_error_boundary`**：前端测试不适用（Rust 侧关注点），无违反。
- **panic hook**：前端测试不适用。
- **日志用 tracing 且级别得当**：`plugin-loader-gating.test.ts` 用 `vi.spyOn(logger, 'warn'/'log'/'error')` mock 静音预期日志，未在测试代码中直接 `tracing::error!`。符合。
- **composable 中文硬编码**：所有 file-transfer 测试断言的都是 i18n key（如 `transfer.consent.title`），composable 层没有中文硬编码（i18n 由 context.i18n.t() 提供）。符合。
- **i18n key 中英双语**：本次审计不含 i18n 资源文件；composable 侧 key 引用一致（`transfer.*`、`file-transfer.*`），未观察到未定义 key。
- **平台检测误用屏幕宽度**：本次审计范围内不涉及平台检测逻辑。
- **组件混入业务逻辑**：本审计不含组件测试，只覆盖 composables 与集成层；composables 逻辑真实执行、渲染层未测（明确声明不测渲染），职责分离清晰。
- **注释掉的代码、冗余注释**：未发现注释代码；注释质量高（解释「为什么」），无冗余。
- **JWT / token / 敏感信息**：`makeAuthCredentials()` fixture 使用固定假 token（`'test-jwt-token'`），测试代码无真实凭据；localStorage mock 中写入的 token 均为测试常量。安全。

**总体合规**：✅ 无违反硬性规范，主要问题是测试基础设施层的 fake timers 一致性与个别弱断言。

