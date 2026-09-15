# 前端基础层测试审核报告（utils / config / plugin / services）

审核范围：12 个测试文件，877 行测试代码，覆盖 `src/utils/*`、`src/config/*`、`src/plugin/dialog-host.ts`、`src/services/linkCrypto.ts`。

---

## 1. 模块概览

| 指标 | 值 |
|---|---|
| 测试文件数 | 12 |
| 测试用例数 | ~54（含子循环） |
| 总行数 | 877 |
| 覆盖的被测模块 | `terminalResizePolicy`, `terminalResizeDebouncer`, `terminalRowClip`, `terminalDimensions`, `terminalIdle`, `frontendLogger`, `terminalThemes.resolveTerminalTheme`, `terminalOnboardingSteps`, `agentPresets`, `pluginDialogHost`, `PluginIcon`, `linkCrypto` |

整体观察：工具类断言风格统一、无 `.only/.skip`、无快照滥用、无真实网络/真实时间（均使用 `vi.useFakeTimers` + `vi.spyOn`），基础卫生良好。主要风险集中在 **G3 弱断言**、**G6 变异无法杀死** 与 **G1/G2 覆盖缺口**（`plugin/permission.ts` 权限仲裁、`plugin/events.ts` 订阅/清理事件、`linkCrypto.decryptText/Binary` 反解路径、`terminalRowClip.attachRowBackgroundClipper` 观察器生命周期、`agentPresets.getAllPresetCommandTexts`、`terminalThemes.resolveThemeLabel` 完全无测试）。

---

## 2. 问题清单

### Blocker

- 无。

### Major

1. `src/__tests__/config/terminalOnboardingSteps.test.ts:26-34` | **Major** | 
   弱断言模板 `expect({ key, value: typeof value === 'string' ? value : undefined }, ...).toBeDefined()` 恒真——对象字面量永远 defined，`toBeDefined` 无法失败。真正的校验放在第二行 `typeof value === 'string' && value.length > 0`，但**当 i18n key 缺失（value=undefined）或为空串时**，第二行 `.toBe(true)` 仍会因 `false === true` 失败——看似 OK，然而**当 `key` 为 `undefined`（如某步骤遗漏了 `tryHintKey`）时** `if (!key) continue` 会跳过，导致**该 locale 缺失的 key 被静默放行**（数据契约里 `tryHintKey` 是 optional，代码路径允许缺省；测试却把「不存在 = 跳过」当成正确行为）。**反向断言缺失**：如果新增一个 step 但只在 zh-CN 加 key 未加 en，本测试确实会失败——但那是靠 `if (!key) continue` 之外的循环结构兜底，一旦有人把 `if (!key) continue` 改成 `if (!key) fail` 就会引入无谓失败。建议改为 `expect(value, \`${locale}: ${key}\`).toBeTypeOf('string')` + `expect(value.length).toBeGreaterThan(0)`，并对每个 step 明确枚举 `[titleKey, descKey, tryHintKey]` 分别断言（不允许 `continue`）。 | **G3/G4/G6** | 强断言替换；对 optional `tryHintKey` 做显式数据契约断言（要么强制非空，要么用类型收窄而非 `if (!key) continue` 静默）。

2. `src/__tests__/plugin/pluginIcon.test.ts:35-37` | **Major** | 
   「内联 `<svg>` 标记走消毒渲染分支」测试只 `expect(wrapper.find('svg').exists()).toBe(true)`——**未断言任何消毒效果**。变异测试：删除 `sanitizedSvg` 计算属性里的 4 个 `.replace()` 调用，测试仍然通过。**高危场景**：`<script>`、`<foreignObject>`、`on*=` 事件属性、`href="javascript:..."` 注入全部未被覆盖。这是插件扩展点的**纵深防御**（源码注释明说「防止 icon 字段被第三方 manifest 滥用」），必须覆盖负例。 | **G2/G3/G6** | 补 3-4 条断言：含 `<script>` 输入断言 `wrapper.html()` 不含 `"<script"`；含 `onclick="..."` 断言 `wrapper.html()` 不含 `"onclick"`；含 `href="javascript:..."` 断言无 `javascript:`；反向断言合法 `<path d="...">` 属性保留。

3. `src/__tests__/plugin/dialogHost.test.ts:22-28` | **Major** | 
   「showPrompt 确认时返回输入值」测试**只覆盖 `resolveTop('confirm', 'my-plugin')`** 的正例。**未覆盖 `resolveTop('confirm')`（无 value 参数）分支**——源码 `pluginDialogHost.showPrompt` 内部 `.then(r => (r.action === 'confirm' ? (r.value ?? '') : null))`，若 `value === undefined` 会返回空串 `''`（非 `null`）。这是一个数据契约歧义：`showPrompt` 的返回类型应该是 `string | null` 还是 `string`？测试静默接受当前行为，等于**用测试固化了空串 vs null 的实现选择**。同时**完全缺失 `resolveById`** 的测试（源码注释说「resolveTop 只适用于调用方确知自己即队首的场景；跨插件共享队列下队首可能是他插件对话框，误结算会关错窗（首连确认 30s 超时关闭用 resolveById 定点）」），这是**跨插件安全关键路径**（首连 30s 超时靠它定点结算），未覆盖 = 一旦误结算 bug 引入无法回归。 | **G2/G4/G6** | 补：`resolveTop('confirm')` 断言返回值（`''` 或 `null`，明确契约）；`resolveById(id, 'cancel')` 断言只结算指定 id 且其它队首不动；`resolveById(nonexistent)` 断言 no-op；`showDialog` 未测试 `resolveTop('confirm', value)` 正例。

4. `src/services/linkCrypto.test.ts:79-116` | **Major** | 
   两个 roundtrip 测试**都止步于加密侧**：`text roundtrip` 只断言信封 `{v:1, seq:0}` 与第二帧 `seq===1`（结构纪律），**从未调用 `decryptText` 验证对称性**；`binary roundtrip` 只断言 `sealed[0]===1` 与长度公式 `9+len+16`（长度正确不代表密文正确）。**变异无法杀死**：把 `aesGcmEncrypt` 换成恒返回随机字节的函数，测试仍通过；把 `aesGcmDecrypt` 逻辑反过来，测试仍然全绿（未触达）。另外「对称性白盒断言」直接读 `client as unknown as { c2s: ... s2s: ... }` 私有字段，**测试和实现耦合到内部类结构**——一旦重构把 `c2s/s2c` 抽到内部模块或封装成 getter，测试立刻失效。此外 `expect(Number.isFinite(r.cols)).toBe(true)` 在 `terminalDimensions.test.ts:64` 属于同类型弱断言。`parseCryptoEcho` / `generateEphemeral` / `decryptText` / `decryptBinary` **完全无测试**，特别是**反向路径（错误 seq、错误 nonce、错误 tag）的异常分支**——安全敏感代码必须对称覆盖。 | **G1/G2/G3/G4/G6** | 补：客户端 `encryptText` → 独立服务端复刻 `decryptText` 对称回读；`decryptText` 传错 seq 断言抛 `seq mismatch`；传损坏 ct 断言抛 GCM 错；`decryptBinary` 长度不足断言抛 `too short`；`parseCryptoEcho` 缺 crypto 字段返回 null、`ek` 空串返回 null、缺 v 兜底 1。白盒 `as any` 断言保留一份作对称性锚点，但**主断言必须是端到端 encrypt→decrypt 回读**。

### Minor

5. `src/__tests__/config/agentPresets.test.ts:33-42` | **Minor** | 
   `expect(AGENT_TYPES).toHaveLength(4)` 只测数组长度，**未测 `Object.keys(AGENT_PRESETS).sort()` 是否恰好等于 AGENT_TYPES**。若新增第五个 preset（如 `aider`）但未同步 AGENT_TYPES 数组，后续 `for (const type of AGENT_TYPES)` 循环会漏测该 preset 的 12 条命令、skills 位、模式——静默回归。`getAllPresetCommandTexts()` **完全无测试**（去重与「跨 CLI 合集」语义无锚）。 | **G2/G6** | 补 `expect(Object.keys(AGENT_PRESETS).sort()).toEqual([...AGENT_TYPES].sort())`；`getAllPresetCommandTexts` 断言去重、覆盖所有 AGENT_PRESETS 命令、`/compact` 只出现一次。

6. `src/__tests__/config/terminalThemes.test.ts:15-17` | **Minor** | 
   `expect(resolveTerminalTheme('system', true)).toBe(TERMINAL_THEMES.dark)` 是**恒真**（同一对象引用相等）。变异：把 `resolveTerminalTheme` 改成 `return TERMINAL_THEMES.system`（错误的原 `system` 条目），`.toBe` 失败，但同时断言 `background.match(/^#[0-9a-f]{6}$/i)` 也会失败——但**如果只测 6 位 hex 正则，测试只覆盖 background 一列**，前景色 / 光标 / 16 色全部未锚。`resolveThemeLabel` **完全无测试**（i18n 前缀判定 + `t()` 透传是 UI 显示关键路径）。 | **G1/G4** | 断言具体颜色值（`background === '#0a0a0f'`）而非对象引用相等；补 `resolveThemeLabel` 三个用例：`settings.appearance.lightMode` 走 t()、纯文本 `'Dracula'` 直接返回、`settings` 缺 t() 键兜底。

7. `src/__tests__/utils/terminalDimensions.test.ts:59-66` | **Minor** | 
   `it('非法 DPR（≤0）按 1 兜底，不产生 NaN')` 只 `toBeGreaterThan(0)` + `Number.isFinite`——**未断言精确值**，变异把 `dpr <= 0 ? 1 : dpr` 改成 `dpr || 1`（NaN 输入仍会返回 NaN）也不会被杀死。同文件最后一个用例 `it('恒为非零合法维度（极小容器也拿 1 行 1 列）')` 断言 `>=1` 而非精确 `=== 1`，同样弱。**边界条件（containerWidthCss 恰 = TERMINAL_SCROLLBAR_GUTTER_PX=6 时可用宽 = 0）未测试**——这个边界是"扣除滚动条预留宽后可能为负"的关键路径。 | **G3/G4/G6** | 断言精确 `toEqual({ cols: 99, rows: 25 })`（DPR=0 应等同 DPR=1）；补 `containerWidthCss === 6`（可用宽恰为 0）断言 `cols === 1`（`Math.max(..., 1)` 兜底路径）。

8. `src/__tests__/utils/terminalIdle.test.ts:19-25` | **Minor** | 
   正例列表未覆盖：`> ` 已带输入内容（`> ls -la`）——源码注释说"含提示符上已输入的内容"，但测试没锚。此外 `C:\Users\binblink>` 后带空格的**负例**（如 `C:\Users\binblink> C:` 显示盘符切换提示而非等待输入）未测。行内 `❯` 后无空格变体未测。空串 `''` 断言了 false 但**只有空串一个反例**——应至少补「纯空白 `"  "`」、「只有 emoji `"✅"`」、「行内有 `$` 但在中段（如 `foo$ bar`）」等。 | **G2/G6** | 补 4-5 条正例变体（提示符后带用户输入、无空格、Windows 全路径带空格）与 3-4 条反例变体（`$` 出现在中段、emoji、纯空白、含 `$` 的 git diff 行）。

9. `src/__tests__/utils/terminalResizeDebouncer.test.ts:37-44` | **Minor** | 
   未覆盖 `horizontalDelayMs` 自定义参数——所有用例都用默认 100ms。若有人误把默认值改成 0 或 500，除了默认 100ms 相关断言外无保护。另外「等值喂入不触发、不重置计时器」测试断言 `toHaveBeenCalledTimes(1)` 后未 `vi.advanceTimersByTime`，无法区分「计时器被提前重置但窗口内」与「计时器按原计时器触发」。 | **G2/G3** | 构造 `new TerminalResizeDebouncer({ onApply, horizontalDelayMs: 50 })`，断言 `vi.advanceTimersByTime(49)` 未触发、`vi.advanceTimersByTime(1)` 后触发。

10. `src/__tests__/utils/frontendLogger.test.ts:46-56` | **Minor** | 
    「循环引用对象回退 visited-set 序列化」断言 `rendered.toContain('[Circular]')` **耦合到源码里的字面字符串**——变异把 `[Circular]` 换成 `"<circular>"` 会失败，这是**实现细节耦合而非行为断言**。正确断言应为「输出可 JSON.parse 或包含循环引用标记」而非特定字符串。 | **G4/G6** | 改为断言输出包含 `'"name":"loop"'` 且不抛错 + `String(rendered).length > 0`；或对循环引用场景直接断言「JSON.stringify(arg) 会抛错但 formatLogArgs 不会」。

11. `src/__tests__/utils/terminalRowClip.test.ts` 整体 | **Minor** | 
    只测了 `scanRowBackgroundOverflow`（同步扫描），**`attachRowBackgroundClipper` 完全无测试**。后者承担了 64ms 节流、250ms 静默补扫、脏行增量、`MutationObserver` + `ResizeObserver` 挂接与 `dispose()` 清理——**关键性能约束（120Hz 帧预算 8.3ms，见源码注释）与资源泄漏**（未 dispose 会留 observer）均未验证。 | **G1/G6** | 补 `attachRowBackgroundClipper` 测试：注入 mock `MutationObserver` / `ResizeObserver`，触发 mutation 断言 64ms 后调用扫描、250ms 后触发全量、`dispose()` 后不再响应。

12. `src/__tests__/utils/terminalResizePolicy.test.ts:19-30` | **Nit** | 
    「行变化（双向）立即生效」用例里 `shouldApplyGridResize(80, 24, 79, 25)` 和 `(80, 24, 79, 23)` 断言 `true`，但**列漂移 ±1 且行变化 ±1 的合成边界**只在其中一方向断言（列 79 = ±1、行 25 = +1）。缺 `(80, 24, 81, 25)`（列 +1、行 +1）和 `(80, 24, 81, 23)`（列 +1、行 -1）的对称覆盖。 | **G2** | 补对称用例（列 ±1 × 行 ±1 的 4 种组合）。

### 其他观察（Nit）

- `linkCrypto.test.ts:103` `void sealed` 是无用语句，属**死代码**——`sealed` 已在前一行通过 `JSON.parse(sealed)` 使用，`void sealed` 无副作用。 | **G4** | 删除。
- `frontendLogger.test.ts:87-107`「转发失败仅静默一次警告」测试**断言 `expect(errorSpy).toHaveBeenCalledTimes(2)`**——`errorSpy` 是 `console.error` 的 spy，但源码走的是 `console.warn`（`reportedFailure` 分支），这 2 次 errorSpy 调用**来自 `logger.error` 本身**（`fn.apply(console, args)` 转发到 console.error）而非 flush 逻辑。测试作者用 `logger.error` 做 flush 载荷，`errorSpy` 计数被两次 `logger.error` 累加，而非断言"flush 失败后是否递归调 console.error"。断言逻辑勉强成立，但**意图与实现错位**，维护者读代码会误解。 | **G4/G6** | 改用 `logger.warn('will fail')` 触发 flush（不干扰 errorSpy 计数），或明确分开 `console.error` 计数与 `logger.error` 调用计数。
- `terminalRowClip.test.ts:52` `ROW_RIGHT = 389` 硬编码——`OVERFLOW_EPSILON_PX` 边界（正好等于 epsilon、稍超 epsilon）未测。 | **G2** | 补 `rect.right === 390`（溢出 1px = epsilon）与 `rect.right === 390.5`（溢出 1.5px）断言前者不裁、后者裁。
- `pluginIcon.test.ts:38-43` emoji 与无 icon 用例都只断言"存在"、`wrapper.text()` 含 emoji——**未验证 SVG path 输入被拒为 text**（源码 `SVG_PATH_RE` 命中就渲染成 path，测试已覆盖）、**也未验证「path-like 但非 `M/m` 开头」（如 `L4 4`）回退到 emoji 分支**。 | **G2** | 补 `wrapper.text().includes('L4 4') === true` 反向断言。

---

## 3. 评分卡

| 维度 | 分数 | 主要扣分依据 |
|---|---|---|
| 需求/行为契约追溯性 (G1) | **72** | 部分关键路径完全无测试（`resolveById`、`getAllPresetCommandTexts`、`resolveThemeLabel`、`attachRowBackgroundClipper`、`decryptText/Binary`、`parseCryptoEcho`），模块边界契约未锚 |
| 正反例覆盖 (G2) | **68** | `terminalIdle` 反例仅 4 条、`PluginIcon` 无消毒负例、`dialogHost.showPrompt` 无 `resolveTop('confirm')` 无值分支、`plugin/permission.ts` 完全无测试 |
| 边界+异常覆盖 (G2/G6) | **65** | `terminalDimensions` 弱断言、`linkCrypto` 反解路径缺失、`terminalRowClip` epsilon 边界未锚、`frontendLogger.flush` 空 entries 与 invoke 抛异常分支未覆盖 |
| 断言强度 (G3) | **62** | `terminalOnboardingSteps` 存在恒真 `toBeDefined` + `if (!key) continue` 静默跳过；`terminalThemes` `.toBe(TERMINAL_THEMES.dark)` 对象引用相等恒真；`terminalDimensions` `toBeGreaterThan(0)` 多处；`pluginIcon` 消毒断言缺失 |
| 独立性+确定性 (G5) | **88** | 全部测试独立无顺序耦合、`vi.useFakeTimers` + `vi.spyOn` 规范使用、无真实网络/时间；仅 `frontendLogger` 依赖模块级单例状态（`entries/flushTimer/reportedFailure`），`resetLoggerState` 每次调用能正确重置，属可接受的边界 |
| 可读性+可维护性 | **80** | 测试命名清晰、场景注释对齐源码；扣分：`linkCrypto` 白盒 `as unknown as { c2s: ... }` 与 `[Circular]` 字面量耦合实现细节、`void sealed` 死代码、`terminalOnboardingSteps` 弱断言模板易被复制扩散 |
| **总分（加权平均）** | **71.7 → 需重写** | 加权：G1×0.2 + G2×0.15 + 边界×0.15 + G3×0.2 + G5×0.15 + 可读×0.15 |

**判定：< 80 分，标记为需重写（部分模块级）。** 工具类（`terminalResizePolicy` / `terminalResizeDebouncer` / `terminalRowClip.scanRowBackgroundOverflow` / `terminalDimensions`）质量高、可保留；**需重写的是 `terminalOnboardingSteps` 弱断言模板、`pluginIcon` 消毒负例、`dialogHost` 关键分支、`linkCrypto` 反解路径**——这四处是 Major，占总分下探主要贡献。

---

## 4. 高风险未覆盖清单

### 4.1 有测试但明显缺失的关键场景

| 模块 | 缺失场景 | 影响 |
|---|---|---|
| `linkCrypto.decryptText/decryptBinary` | seq 不连续、GCM tag 篡改、nonce 长度错、frame 过短、错误 version 全部异常分支 | **安全**：攻击者篡改密文/序号无法被回归捕获 |
| `pluginDialogHost.resolveById` | 定点结算、跨插件并发队列、非存在 id no-op、30s 超时首连确认 | **跨插件串扰**：误结算关错窗，30s 超时逻辑无回归 |
| `pluginIcon.sanitizedSvg` | `<script>`/`<foreignObject>`/`on*=` 事件属性/`href="javascript:"` 消毒 | **XSS/扩展点安全**：第三方插件 manifest 注入 |
| `terminalRowClip.attachRowBackgroundClipper` | 64ms 节流、250ms 静默补扫、`dispose()` 后无 observer 泄漏、MutationObserver 无 rowsEl 早退 | **性能 & 内存泄漏**：真机 120Hz 帧预算 |
| `terminalIdle` | `> ls -la` 提示符后带用户输入、`C:\...> C:` 负例、`$` 出现在行中段 | 忙闲误判（P0-1 已取消但工具保留待复用） |
| `terminalThemes.resolveThemeLabel` | i18n 前缀分支、`t()` 透传、纯文本回退 | UI 显示错乱 |
| `agentPresets.getAllPresetCommandTexts` | 去重、跨 CLI 合集语义 | `/` 补全数据源回归 |

### 4.2 完全无测试但应该有的关键路径

| 模块 | 关键路径 | 影响 |
|---|---|---|
| `src/plugin/permission.ts` | `hasPermissionForApi`（18 个权限 × N API 方法） | **权限仲裁**：错配即插件越权 |
| `src/plugin/registry.ts` | `getPluginRegistry` 单例、注册/反注册/冲突 | 生命周期 |
| `src/plugin/events.ts` | 订阅/触发/`clearPluginEvents` 清理 | 内存泄漏、跨插件事件污染 |
| `src/plugin/routes.ts` | `registerPluginRoute` / `openPluginRoute` / `pluginRouteName` 命名规则 | 路由冲突 |
| `src/plugin/loader.ts` | 加载流程（275 行） | 插件启动 |
| `src/plugin/context.ts` | `createPluginContext`（426 行） | 插件 API 注入 |
| `src/plugin/commands.ts` | 命令注册 | 终端命令注入 |
| `src/utils/terminalMetrics.ts` | `measureCellSize` / `computeGridSize` / `computeDeviceDefaultGridSize` | 网格计算上游 |
| `src/utils/clipboard.ts` | 剪贴板读写 | UI 交互 |
| `src/utils/terminalScrollback.ts` | 常量 | 低风险 |
| `src/config/splash.ts` | 启动画面候选 | 低风险 |
| `linkCrypto.generateEphemeral` / `parseCryptoEcho` | 密钥对生成、crypto 回执解析 | **握手安全** |

---

## 5. 改进优先级建议

### P0（必须修，Major 级别，涉及安全/关键契约）

- `src/__tests__/config/terminalOnboardingSteps.test.ts:26-34` — 弱断言模板 + `if (!key) continue` 静默放行
- `src/__tests__/plugin/pluginIcon.test.ts:35-37` — 消毒分支无负例断言
- `src/__tests__/plugin/dialogHost.test.ts:22-28` — `showPrompt` 无 value 分支 + `resolveById` 完全无测
- `src/services/linkCrypto.test.ts:79-116` — encrypt-only 无 decrypt 对称验证 + 反解路径异常分支缺失

### P1（建议修，Minor 级别）

- `src/__tests__/config/agentPresets.test.ts:33-42` — 补 `Object.keys(AGENT_PRESETS)` 与 `getAllPresetCommandTexts` 锚点
- `src/__tests__/config/terminalThemes.test.ts:15-17` — 断言精确色值而非对象引用相等；补 `resolveThemeLabel`
- `src/__tests__/utils/terminalDimensions.test.ts:59-66` — 精确断言 DPR=0 兜底、可用宽=0 边界
- `src/__tests__/utils/terminalIdle.test.ts:19-25` — 补提示符带用户输入、`$` 在中段负例
- `src/__tests__/utils/terminalResizeDebouncer.test.ts:37-44` — 补 `horizontalDelayMs` 自定义参数
- `src/__tests__/utils/frontendLogger.test.ts:46-56` — 断言脱耦 `[Circular]` 字面量
- `src/__tests__/utils/terminalRowClip.test.ts` — 补 `attachRowBackgroundClipper` observer 生命周期

### P2（可选）

- 补 `src/plugin/permission.ts` / `events.ts` / `routes.ts` / `registry.ts` / `loader.ts` / `context.ts` 单元测试（跨插件安全关键路径，建议单独一轮任务）
- 删除 `linkCrypto.test.ts:103` 的 `void sealed` 死语句
- 修正 `frontendLogger.test.ts:87-107` `errorSpy` 与 `logger.error` 计数耦合的意图错位
- 补 `terminalResizePolicy` 列 ±1 × 行 ±1 的对称边界（Nit）
- 补 `terminalRowClip` `OVERFLOW_EPSILON_PX` 精确边界（`rect.right === 390` 不裁 vs `390.5` 裁）
- 补 `pluginIcon` 非 `M/m` 开头的 SVG-like 字符串回退到 emoji 分支

---

## 6. 关键判断：整体健康度

- **工具类基础层（utils/）**：质量高，可作为标杆——`terminalResizePolicy`、`terminalResizeDebouncer`、`terminalRowClip.scanRowBackgroundOverflow` 达到 G1-G6 全部通过的水平。**保留并推广这套风格**（`vi.useFakeTimers` + 场景注释对齐源码 + 精确值断言）。
- **配置层（config/）**：分化严重——`agentPresets` 覆盖最完整，`terminalOnboardingSteps` 与 `terminalThemes` 存在结构性弱断言，需修。
- **服务层（services/linkCrypto）**：**最需要重写**——安全敏感代码只测加密不测解密、白盒耦合内部类字段、多条函数完全无测。是本轮最高优先级。
- **插件宿主（plugin/）**：**最需要补测**——`permission.ts` 权限仲裁零测试、`dialogHost.resolveById` 跨插件关键路径零测试、`pluginIcon` 消毒纵深防御零负例。

**重写触发阈值**：总分 71.7 < 80。但重写不是整体重写——工具类可直接复用；**重点重写 linkCrypto 反解 + plugin/permission + pluginIcon 消毒 + dialogHost.resolveById 四处**即可让总分回拉到 85+ 区间。
