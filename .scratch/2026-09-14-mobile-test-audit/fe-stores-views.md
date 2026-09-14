# 前端 stores / components / views 测试审查（unit-test-discipline G1-G6）

## 1. 模块概览
- 测试文件 10 个 / 用例 60 个 / 总行数 2038
- 覆盖被测模块：
  - `stores/terminalBuffer.ts`（878 行，Rust 驱动）— 24 用例
  - `stores/inputAssistant.ts`（499 行，localStorage）— 20 用例
  - `stores/codeViewer.ts`（107 行，localStorage + 纯函数）— 12 用例
  - `stores/settings.ts`（95 行，Tauri invoke）— 7 用例
  - `components/EgressConsentDialog.vue`（219 行）— 9 用例
  - `components/TerminalInputBar.vue`（经 fixture host）— 3 用例
  - `views/ToolboxView.vue`（205 行）— 6 用例（3 文件）
  - `views/PluginView.vue`（858 行，仅 toggle 失败路径）— 2 用例

## 2. 问题清单（按严重级别）

### Major
- `terminalBuffer.test.ts:378` | Major | `markAllUnsubscribed` 只断言 `terminalUnsubscribeAll` 被调用（`toHaveBeenCalled`），未断言 state 清理（buffers 清空/subscribed=false）。若实现只发命令不做状态重置，测试仍通过。 | G4 (仅断言 mock 调用) / G3 | 追加 `expect(store.buffers.size).toBe(0)` 或 `expect(store.getBuffer('s1')).toBeUndefined()`
- `terminalBuffer.test.ts:187-188` | Major | `onReplayDone` 只断言 `toHaveBeenCalled`，未检查调用次数或参数；`resetCursor` 后无游标验证。若超时兜底多次触发或参数错误也测不出。 | G4 / G6 | 追加 `.toHaveBeenCalledTimes(1)`
- `pluginToggleConvergence.test.ts:104` | Major | 启用失败测试最后一步只断言 `h.deactivate` 被调用，未断言其返回值/异常被吞掉。若实现里 deactivate 抛错但不捕获（当前依赖 logger spy 掩盖），测试仍过。 | G4 / G6 | 断言 `logger.error` 被调用；断言 `pluginEnabledStates` 已 false（下方已断言但重复冗余）
- `pluginToggleConvergence.test.ts:141-142` | Major | 拆解也失败的场景下，只断言 `h.deactivate` 被调用 + state 为 false；未断言错误已被吞（无抛错冒泡）。若实现把 deactivate 错误抛出，前端弹窗会被阻塞但测试仍绿。 | G6 | 追加 `expect(async () => await flushPromises()).not.toThrow()` 或断言 logger.error 收到拆解错误
- `terminalInputBarBlur.test.ts:83-95` | Major | "未聚焦时 blurInput 为 no-op" 只断言 `not.toThrow` + `isFocused()===false`。若实现里 `blurInput` 直接抛错或返回错误 state 都可测出，但当前 happy-dom 下 `textarea.element.blur` 未被 spy 时也无从校验 no-op 语义。 | G3 / G6 | 追加 `expect(blurSpy).not.toHaveBeenCalled()`

### Minor
- `settings.test.ts:75-77` | Minor | `getMaxCachedTerminals` 未测负数、Infinity、NaN；只测 0/undefined。若实现把负数当有效值回退逻辑会漏。 | G1 / G2 | 追加 `store.settings.ui.max_cached_terminals = -5` 断言回退
- `codeViewer.test.ts:158-165` | Minor | CODE_THEMES 元数据测试用 `meta.label` 做 `toBeTruthy`（弱断言）；对 `system.background` 断言 CSS 变量字符串是"复制实现逻辑"，一旦实现改为具体色值测试需同步改（脆弱耦合）。 | G4 / G6 | 改成 `expect(meta.label.length).toBeGreaterThan(0)` + 类型判定；颜色改为 hex 正则
- `terminalBuffer.test.ts:433` | Minor | `terminalSetMode` 只断言 `toHaveBeenCalledWith('s1','realtime')`，未断言 unregister 后 batch 模式的调用参数。若实现把 realtime 误发为 batch（或反之），只有一边能测出。 | G2 | 断言两次调用次序 `['realtime','batch']`
- `toolboxDeepChild.test.ts:1-79` | Minor | 单测试文件仅 1 个用例，覆盖 `enable→disable→enable→reactivate` 一种组合；与 `toolboxKeepAlive` 高度重叠，仅换 fixture 层级，未验证深层子组件特有场景（v-for key 复用、多 ToolboxView 实例）。 | G1 / G2 | 补一个"仅 disable 无 re-enable 返回空态"或对偶用例
- `EgressConsentDialog.test.ts:230-247` | Minor | 30s 超时测试只测"到点自动收起"，未测临界值 29.999s 未触发 / 30.001s 触发，也未测弹窗已关闭后定时器是否被清理（资源泄漏风险）。 | G2 | 补一次 29s 未触发断言；补卸载后无 pending timer
- `toolboxViewSync.test.ts:137-149` | Minor | 断言 `wrapper.text()` 包含 `VIEW_TITLE` 作为"入口重现"的验证——若二级页残留但恰好同标题，此断言会误判为通过。 | G3 | 定位具体入口卡片元素（如 `button`）而非全文本匹配
- `inputAssistant.test.ts:226-238` | Minor | `getQuickBarItems: quickBarCount clamped` 只测 2 与 20，未测下限 3 / 上限 10 精确边界。 | G2 | 补 `quickBarCount=3`、`quickBarCount=10` 精确断言
- `inputAssistant.test.ts:143-146` | Minor | `topShortcuts` 只测 3 个不同计数（2/1/3）排序；未测同频次稳定排序、无计数时的默认列表。 | G2 | 补同频场景
- `settings.test.ts:37-52` | Minor | 默认值断言里 `terminal_font_family: 'Consolas'`、`default_environment: 'windows'` 等硬编码"复制实现"，未来改默认值只改实现会让测试红——属实现变更信号可接受，但 `notify_in_background`、`palette` 属 UI 语义值，建议集中常量导入以明确契约来源。 | G4 (复制实现逻辑作为预期) | 从 store 导出 `DEFAULTS` 常量并对比
- `codeViewer.test.ts:68-78` | Minor | 断言 `store.settings` 精确等于默认对象，但默认值在实现和测试里各写一份。若实现改默认值，测试必须同步修改才能定位差异。 | G4 | 从 store 导出 DEFAULT 常量
- `terminalBuffer.test.ts:54-58` | Minor | `flushAsync(n=3)` 用固定次数 `await setTimeout(0)` 代替 vitest 的 `vi.waitFor` 或精确微任务排空，多帧时序敏感处（P5 冷却测试）脆弱。 | G5 | 关键处改用 `await vi.advanceTimersByTimeAsync(0)` + 显式 flush

### Nit
- `terminalBuffer.test.ts:405-423` | Nit | 输出活动通知测试通过 emit 事件间接触发 `emitMock`，未验证节流 `ACTIVITY_THROTTLE_MS=200` 的间隔语义。 | G2 | 补两次连续帧第二次不 emit 断言
- `toolboxKeepAlive.test.ts:120-129` | Nit | 通过 `toolbox.vm.$ setupState.activePluginView` 直接读内部 computed，属实现细节耦合。 | G4 | 改为纯外部行为断言（title 文本）已具备
- `EgressConsentDialog.test.ts:177-193` | Nit | 背板点击测试用 `.fixed.inset-0` 定位——若组件 CSS 类调整，测试需同步改。 | 可维护性 | 加 data-testid 或语义选择器
- `pluginToggleConvergence.test.ts:100-145` | Nit | 使用 `(wrapper.vm.$ as any).setupState.pluginEnabledStates` 直接读私有状态；虽必要（v-model 已断言），但脆弱于命名变更。 | 可维护性 | 已通过 toggle 的 `data-on` 属性间接断言，可去掉重复的 setupState 断言

## 3. 评分卡

| 维度 | 分数 | 依据 |
|---|---|---|
| 需求/行为契约追溯性 | 88 | 每个 describe 顶部注释明确契约来源（ticket/spec/bug），G1 达标良好 |
| 正反例覆盖 | 85 | 权限拒绝、未知 code、corrupt JSON、失败回退等反例普遍存在 |
| 边界+异常覆盖 | 80 | GAP 冷却、截断、超时兜底覆盖；但节流临界值、maxCachedTerminals 负数、quickBarCount 精确边界缺失 |
| 断言强度 | 70 | 5 处 major + 若干 minor 弱断言（`toHaveBeenCalled` 单独使用、`toBeTruthy`、`not.toThrow`） |
| 独立性+确定性 | 85 | 每文件独立 `beforeEach`、无 `.skip/.only`、无真实网络/时间；但固定次 `setTimeout(0)` flush 略脆弱 |
| 可读性+可维护性 | 82 | 注释清晰、辅助函数抽取到位；但实现常量与默认值在测试/实现两侧重复 |
| **总分（加权 30/20/15/20/10/5）** | **81.8** | 高于 80 阈值，不需重写，但弱断言需定点修补 |

## 4. 高风险未覆盖清单

### 已有测试但明显缺失的关键场景
- **terminalBuffer**：`MAX_BUFFERED_LIVE_BYTES` (8MB) 背压溢出分支（源码 274 附近）无测试；`headTrimmed` 首次 trim 通知仅测了 truncate 场景；`onStateEvent` 收到 `detail='unsubscribed'`/`'session_missing'` 分支未测；`base64ToBytes` 在 `typeof atob !== 'function'` 时的兜底路径未测；`markPageLeft` / `markPageEntered` 未直接测试；`terminalGetHistory` 抛异常（非 writeParsed 抛错）路径未测。
- **inputAssistant**：`recordShortcut` 传入空串/超长 code 无验证；`loadFromStorage` 对 stats/cmdStats/position 的 corrupt JSON 只测了 settings；手势 `gestures` 部分字段缺失的降级行为未测。
- **EgressConsentDialog**：payload 缺 `request_id`、`url` 为空、`host` 与 `url` 不一致的畸形请求无测试；连续快速打开-关闭时的定时器泄漏未测。
- **ToolboxView**：多插件并发注册/停用的顺序稳定性未测；二级页内插件被停用但用户未离开页面的 UI 状态迁移仅通过标题间接断言。
- **PluginView**：只测启用方向；停用失败（`deactivate` 抛错）路径未测；toggle 成功路径未测（可能被其他测试覆盖，但本文件缺失）。

### 完全没有测试但应有关键路径
- `TerminalInputBar.vue` 除了 blur 契约外无独立测试（`InputBar.vue`、`InputAssistant.vue`、`ShortcutPanel.vue` 等交互组件也未被本次审核清单覆盖，但作为输入链路关键组件应补测试）。
- `CodeExplorerView.vue`、`SessionsView.vue`、`PresetTasksView.vue`、`DevicesView.vue` 等主视图未在本次审核清单，建议核实是否有对应测试文件。
- `settings.ts` 的 `getMaxCachedTerminals` 在真实 UI 中被用于裁剪会话缓存，缺失场景下的行为（负数/超大值）应被验证。

## 5. 改进优先级建议

### P0（必须修，弱断言 → 强断言）
- `terminalBuffer.test.ts:378` — `markAllUnsubscribed` 追加 buffers/state 清空断言
- `terminalBuffer.test.ts:187-188` — `onReplayDone` 补 `.toHaveBeenCalledTimes(1)`
- `pluginToggleConvergence.test.ts:104` — 补 `logger.error` 断言与 `pluginSetEnabled` 单次调用次数断言
- `pluginToggleConvergence.test.ts:141-142` — 断言拆解失败不冒泡（`not.toThrow` + `logger.error` 收到拆解错误）
- `terminalInputBarBlur.test.ts:83-95` — 追加 `blurSpy` 未被调用断言以证明 no-op

### P1（建议修）
- `settings.test.ts:75-77` — 补 `max_cached_terminals` 负数/Infinity 分支
- `codeViewer.test.ts:158-165` — 移除 `meta.label` 的 `toBeTruthy`，改精确类型判定；颜色断言改正则或结构
- `terminalBuffer.test.ts:433` — 补 `terminalSetMode` 两次调用参数序列断言
- `toolboxDeepChild.test.ts` — 增加至少 1 个互补用例（如 disable-only 空态）
- `EgressConsentDialog.test.ts:230-247` — 补 29s 未触发 + 卸载后定时器清理
- `toolboxViewSync.test.ts:137-149` — 断言改为定位具体入口卡片元素
- `inputAssistant.test.ts:226-238` — 补 quickBarCount 精确边界 3/10

### P2（可选）
- 默认值统一从 store 导出 `DEFAULTS` 常量，测试引用而非硬编码（消除实现/测试双侧漂移）
- 节流 `ACTIVITY_THROTTLE_MS` 补两次连续帧的第二次不 emit 断言
- `TerminalBuffer` MAX_BUFFERED_LIVE_BYTES 溢出分支补测试
- `EgressConsentDialog` payload 畸形数据（缺 request_id 等）补测试
- 组件选择器 `.fixed.inset-0` 等 CSS 类改为 `data-testid`
- PluginView 停用方向 toggle 失败、toggle 成功路径补测试

## 6. 门禁符合性小结（G1-G6）

- **G1 行为契约**：达标（88）。每个 describe 顶部注释明确契约来源；`terminalBuffer` 头部详列覆盖分支；`EgressConsentDialog` 明确 ticket 10 引用。
- **G2 正反例覆盖**：良好（85）。权限拒绝、corrupt JSON、失败回退、边界钳制普遍存在；临界值场景有缺口。
- **G3 强断言**：达标（多数用例），但 5 处弱断言需修（见 Major 清单）。
- **G4 反模式**：合格。无 `toMatchSnapshot`、无 `expect(true).toBe(true)`、无 `.skip/.only`、无 `sleep`、无真实网络/时间；少量"复制实现逻辑作为预期"（codeViewer/settings 默认值）建议改为导出常量。
- **G5 独立性+确定性**：达标。每文件独立 `beforeEach` 重置 pinia/localStorage/mocks；仅 `flushAsync(n=3)` 固定次数 flush 略脆弱。
- **G6 变异分析**：多数测试能杀死关键变异（反转 if、删除副作用、返回 null、抛异常吞掉）；弱断言处（`toHaveBeenCalled` 单独使用）无法杀死"实现未做状态重置"类变异。

**结论**：整体质量优秀（81.8/100，高于 80 阈值），无需重写。P0 修完 5 处弱断言后可达 85+。
