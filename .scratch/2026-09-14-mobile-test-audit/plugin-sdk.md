# ai-chatbox 插件 + plugin-sdk-mobile 测试审查

## 1. 模块概览

- **测试文件数**：10（插件 7 + SDK 3）
- **测试用例数**（`it(...)` 精确计数）：121
  - ai-chatbox: 38(adapters) + 23(markdown) + 11(sse) + 22(highlight) + 33(useAiChat) + 21(useAiConfig) + 7(usePluginConfig) = 155 处 it 关键字
  - plugin-sdk-mobile: 4(types) + 8(runtime) + 12(vite-plugin) = 24
  - 合计约 **179 个用例**（含循环体内多次断言）
- **总行数**：2418 行（+ 107 行 mockContext 共享工具）
- **覆盖被测模块**：
  - `adapters/`: openai / anthropic / gemini / custom / registry / sse
  - `utils/`: markdown（fence 补偿 + 闭合检测）、highlight（Shiki 单例 + 缓存 + in-flight 去重）
  - `composables/`: useAiChat（发送/流/停止/重生成/限流重试/rAF 节流）、useAiConfig（CRUD + storage）、usePluginConfig（配置归一化）
  - `plugin-sdk-mobile/`: runtime 代理、vite 插件（共享模块外置 + CSS 内联）、类型契约

---

## 2. 问题清单

### Blocker

（无）

### Major

| # | 位置 | 问题 | 违反门禁 | 建议 |
|---|---|---|---|---|
| M1 | `plugins/ai-chatbox/src/__tests__/highlight.test.ts:52-58` | 「懒加载单例」测试仅断言两次调用输出含主题名，**未验证任何缓存/单例行为**（WASM 是否只加载一次、highlighter 是否同一实例）。变异分析：把 `getHighlighter` 中的 `if (!highlighterPromise)` 删掉，两次都会重新 `createHighlighterCore`，此测试仍全绿。 | G6（变异杀不死） | 注入 `createHighlighterCore` mock 或断言 `highlighterPromise` 模块级不变；或将单例逻辑抽出可测 |
| M2 | `packages/plugin-sdk-mobile/__tests__/types.test.ts` 整文件 | 全为 `expectTypeOf` 编译期断言，**零运行时断言**。若编译失败 vitest 会跳过整个文件（无 it 失败信号）；实际价值等同 tsconfig 本身。 | G5（无法独立运行产生失败信号） | 补充对真实类型守卫 / runtime discriminator 的运行时断言；或至少声明「本文件只作 TS 类型检查的额外保险，不计入用例覆盖」 |
| M3 | `packages/plugin-sdk-mobile/__tests__/vite-plugin.test.ts:125-141` | 「无入口 chunk → 不改动 bundle」用例的 `bundle['style.css']` 只是 `toBeDefined()`，但源码 `if (!entry || entry.type !== 'chunk') return` 会**在收集 CSS 前 return**，因此 `delete bundle[fileName]` 不会执行。断言方向对但断言值太弱（`toBeDefined()`）。 | G3（弱断言） | 断言 `bundle['style.css'].type === 'asset'` 与 `bundle['vendor.js'].code === 'x'`（后者已有，前者建议加强） |
| M4 | `plugins/ai-chatbox/src/__tests__/useAiChat.test.ts:670-683` | 「封顶」测试只跑到第 5 次重试调度（`countdownSec=10`），**未推进触发第 5 次重试 fire**，也未验证 `maxDelay` 之外的第 6/7 次重试仍封顶在 10s（如 maxRetries=5 上限触发后 `rateLimitExhausted` 收尾）。第 5 次重试若实际调度 16s，测试仍全绿。 | G2（边界越界未覆盖） | 追加：多次触发验证 `delayMs` 恒为 `min` 上限；再触发耗尽收尾验证 `streamCallCount = maxRetries+1` |
| M5 | `plugins/ai-chatbox/src/__tests__/useAiChat.test.ts:449-455`（限流重试「已收到部分内容」用例） | 仅断言「不重试」，**未验证已接收 chunk 已通过 `flushStreamingNow` 落盘**（源码 `finishStream` 内 `flushStreamingNow()` 在 saveMessage 之前）。若实现顺序倒过来（落盘→flush），用户会看到未落盘的 chunk 丢失但测试仍绿。 | G6（副作用顺序未验证） | 加断言：`assistantSave.args.content === '部分'` |
| M6 | `plugins/ai-chatbox/src/__tests__/highlight.test.ts:20-21` | `waitFor` 使用真实 `Date.now()` + 10ms `setTimeout` 轮询、2s 超时。**违反"无真实时间/sleep"** 门禁；在 WASM 首载超阈值时会以 `waitFor 超时` 抛出，掩盖真实缺陷。 | G4（真实时间） | 注入 fake highlight（如第二个用例的做法）以测试单例缓存路径，或 mock `highlightCode` |
| M7 | `plugins/ai-chatbox/src/__tests__/adapters.test.ts:190-194`（anthropic `message_start` 缺省 output_tokens） | 断言 `{ promptTokens: 25 }` 精确形状，但源码 `parseStreamEvent` 对 `output_tokens` 缺失时**仍会写 `completionTokens: undefined`**（未 `?? 0`），依赖前端 `mergeUsage` 兜底。若前端合并逻辑回归为 NaN 传播，此测试不覆盖。 | G2（异常路径不完整） | 增加 mergeUsage 层单测（`adapters/usage.ts` 完全无单测），验证 `undefined` 不会串成 NaN |

### Minor

| # | 位置 | 问题 | 违反门禁 | 建议 |
|---|---|---|---|---|
| m1 | `plugins/ai-chatbox/src/__tests__/markdown.test.ts:17-56` (`markedFencedBlocks`) | 测试辅助函数 `markedFencedBlocks` **在测试里重写了 marked 的 fence 解析逻辑**（openRun + 反引用正则），与 `patchIncompleteMarkdown`/`getClosedCodeBlocks` 的实现近乎同构。若两处同时改错，对拍测试无法捕获。 | G4（复制实现逻辑作为预期） | 用真实 `marked.parse` 生成 HTML 后正则提取 `<code class="language-X">` 作为黑盒期望；或引入独立 tokenizer |
| m2 | `packages/plugin-sdk-mobile/__tests__/vite-plugin.test.ts:44-46` | 断言 `expect(css.apply).toBe('build')` / `expect(css.enforce).toBe('post')` 是**结构性快照**（配置值），不是行为契约。若源码改成 `apply: 'serve'`（错误）此测试会失败——但这属于配置常量而非行为。 | G3（弱断言/结构替代行为） | 保留但对每个 hook 至少有一个行为断言（其他用例已覆盖，可接受） |
| m3 | `packages/plugin-sdk-mobile/__tests__/vite-plugin.test.ts:65-78` (renderChunk default) | 期望值 `const Vue = window.__BEDCODE_SHARED__["vue"]` 里 `__BEDCODE_SHARED__["vue"]` 的双引号形式来自源码 `SHARED_MODULES` 定义。测试对源码字符串的**转义风格过度耦合**，源码改成单引号或模板字符串即失败，但行为等价。 | G3（过强快照） | 断言 `result.code.match(/const Vue = window\.__BEDCODE_SHARED__\.vue\|window\.__BEDCODE_SHARED__\["vue"\]/)` 或行为等价校验 |
| m4 | `packages/plugin-sdk-mobile/__tests__/vite-plugin.test.ts:81-84` (named import) | 期望 `'const {  ref, computed  } = ...'`（花括号内两个空格）与源码 regex `const { ${imports} }` 空格绑定，源码微调即失败。 | G3（弱变异） | 使用 `toMatchInlineSnapshot` + 注释说明格式，或正则匹配解构形式 |
| m5 | `packages/plugin-sdk-mobile/__tests__/vite-plugin.test.ts` 整文件 | `config` hook 只测 `external: array` 与 `undefined`；源码明确处理 `external: string` 与 `external: function`（`typeof === 'string' ? [x] : []`），**未测试 string 分支**。变异分析：把 `typeof existingExternal === 'string'` 分支删掉，测试仍全绿。 | G1（分支未追溯） / G6 | 追加：`config` 传入 `external: 'foo'` 用例 |
| m6 | `packages/plugin-sdk-mobile/__tests__/vite-plugin.test.ts` 整文件 | renderChunk 未测：(a) 同一 chunk 内**多个共享模块**混合（`import Vue from 'vue'; import { pinia } from 'pinia'`）；(b) 共享模块**同时被 default + named import**；(c) `import type { X } from 'vue'`（类型导入，编译后被擦除，若被替换会污染）。 | G2（正/反例不全） | 补 3 条正/反例 |
| m7 | `packages/plugin-sdk-mobile/__tests__/runtime.test.ts:76` | `getPluginContext` 「vue 共享模块缺失」用例只 `expect(() => getPluginContext()).toThrow()`，未匹配错误消息；其他用例都用 `toThrow(/.../)`。 | G3（弱断言） | 改为 `toThrow(/Shared module "vue" not found/)` |
| m8 | `packages/plugin-sdk-mobile/__tests__/runtime.test.ts` 整文件 | `getMobileApi()` 类型声明为 `MobileHostApi`，但测试只验证"能取到对象"，**未验证形状契约**（因为 runtime 不做形状校验，属 SDK 设计契约，但缺一个"形状错时行为如何"的反例）。 | G2（无异常路径） | 追加：`shared.mobileApi = {}` 时 `getMobileApi().httpRequest` 抛 TypeError 或明确契约说明「SDK 不校验形状」 |
| m9 | `plugins/ai-chatbox/src/__tests__/adapters.test.ts` 整文件 | 三个 adapter 都**无 baseUrl 尾部空格/相对路径/非法 URL 的负例**（`joinUrl` 未测试）；`apiKey` 空、`activeModel` 缺失走 `effectiveModel` 兜底、`messages` 为空数组的场景也无覆盖。 | G2（异常路径缺） | 补 3-5 条负例（非法 URL 抛错、空 messages body 形状、model 兜底为空串） |
| m10 | `plugins/ai-chatbox/src/__tests__/adapters.test.ts` | `parseStreamEvent` 三个 adapter 都**只测正例，无空/畸形数据反例**：choices 空数组、delta 为 null、usage 部分字段缺失。 | G2 | 每个 adapter 加 1 条 `parseStreamEvent('{"choices":[]}')` 断言 `=== null` |
| m11 | `plugins/ai-chatbox/src/__tests__/useAiChat.test.ts:147-160` | 「插件配置：thinkingMode=default 不写 thinking」测试通过 `toEqual(DEFAULT_PLUGIN_CONFIG, ...)` 构造，**默认 effort='high' 会因 default 分支忽略而生效**，测试未明确断言 effort 字段被忽略（源码 `applyThinking` 在 default 时直接 return，忽略 effort）。 | G6 | 显式断言 `body.thinking === undefined`（已有），同时补一条 `thinkingMode=default, effort=max` 的 effort 忽略断言 |
| m12 | `plugins/ai-chatbox/src/__tests__/useAiConfig.test.ts:214-225` | 「setActiveModel 非法复合键」用例覆盖 3 种非法形式（无 `::`、空 provider、空 model），**缺 `p1::deepseek-chat` 指向不存在的 provider id 的负例**（源码第 184 行有 `parsed.providerId !== activeProviderId.value` 分支，但 provider 不存在时行为未定义）。 | G2 | 追加：`setActiveModel('nonexistent::model')` |
| m13 | `plugins/ai-chatbox/src/__tests__/useAiConfig.test.ts` | `removeProvider` 只测删除当前 active；未测删除非 active、删除不存在的 id（源码 `idx === -1` 静默 return，无副作用）。 | G2 | 追加 2 条 |
| m14 | `plugins/ai-chatbox/src/__tests__/useAiConfig.test.ts` | `fetchModels` / `testConnection` 在 `status !== 200` 时抛 `API error ${status}`，**未测**（源码 useAiConfig.ts:216-217 / 232-233）。 | G2 | 各追加 1 条 mock 返回 500 的用例 |
| m15 | `plugins/ai-chatbox/src/__tests__/useAiConfig.test.ts` | `updateProvider` 对**不存在的 id** 静默 return（源码 `idx === -1` 分支），无测试。 | G2 | 追加 1 条 |
| m16 | `plugins/ai-chatbox/src/__tests__/useAiChat.test.ts:513-520` | `renameConversation` 断言 `currentConversation.value?.title === '新标题'`，但源码 `if (!conv || !title.trim()) return` 的**空标题/不存在 conv** 早返回路径无测试。 | G2 | 追加 2 条 |
| m17 | `plugins/ai-chatbox/src/__tests__/useAiChat.test.ts` | `switchConversation` 早返回（`sending.value` 或 `convId === currentConvId.value`）路径无测试；`regenerate` 早返回（`sending.value` 或 `lastUserIdx === -1`）路径无测试。 | G2 | 追加 3-4 条 |
| m18 | `plugins/ai-chatbox/src/__tests__/useAiChat.test.ts` | `classifyError` 只测两个正向关键词（context length / permission），**未测反例**：普通错误消息应返回 null（走原始文本透传）。测试「命令抛错 → authRevoked」和「上下文超限 → contextLimitExceeded」都测到了分类，但未测"未命中分类时 raw text 透传"（源码 line 531-532 有 `classified \|\| errorText` 分支）。 | G2 | 追加 1 条：error 事件带普通文本 → `lastError` 等于原文 |
| m19 | `plugins/ai-chatbox/src/__tests__/useAiChat.test.ts` | `isRateLimitError` 的两种路径（status ∈ {429,503,529} vs 正则 `/rate.?limit|too many requests/`）：仅测了 429/503，**未测 529**；也**未测**「文本提到 rate limit 但无 status code」的纯正则路径。 | G2 | 追加 529 用例 + 「too many requests」无 status 用例 |
| m20 | `plugins/ai-chatbox/src/__tests__/useAiChat.test.ts` | rAF 节流测试中「rAF 回调执行后再来 chunk」断言 `stub.pending.length === 1`（重新挂起），但**未验证第一次回调已清除 rafId**（源码 `flushStreamingState` 内 `rafId = null`）。若 rafId 未清零，第二次 `scheduleFlush` 会静默跳过。 | G6 | 断言 `stub.pending.length === 1` 已有，但再加一次 `stub.fire()` 断言内容包含前次 + 后次 |
| m21 | `plugins/ai-chatbox/src/__tests__/usePluginConfig.test.ts` | 未测 `saveConfig`（源码 line 91-97 有 try/catch 包裹 storage.set）。写入抛错时是否静默吞掉？ | G2 | 追加 1 条 |
| m22 | `plugins/ai-chatbox/src/__tests__/usePluginConfig.test.ts:94-95` | 「storage 读取抛错」用例断言 `pluginConfig.config.value` 仍为 DEFAULT，但**未断言 `pluginConfig.loading` 状态**（源码 catch 分支是否复位 loading 未验证）。 | G3 | 断言 `expect(pluginConfig.loading.value).toBe(false)` |
| m23 | `plugins/ai-chatbox/src/__tests__/highlight.test.ts:52-58`（重复 M1，另列） | 「不同语言产出不同 token 结构」断言 `expect(py).not.toBe(js)`，**依赖 Shiki 输出确定性**。若 Shiki 版本升级导致两者恰相同或都降级 plaintext，测试失败原因模糊。 | G4（快照替代行为） | 断言 js 有 `>const</span>`（已有），py 无（`expect(py).not.toContain('>const</span>')`），把「不同」具体化为「python 无 const 着色」 |
| m24 | `plugins/ai-chatbox/src/__tests__/highlight.test.ts` | 未测：`code.isConnected === false`（applyHighlight 早返回，不更新 innerHTML）、`code.textContent === ''`（`highlightElement` 早返回）、`template.content.querySelector('code')` 为 null（不写 dataset）、`highlight` 抛错（console.warn 且不注入） | G2 | 追加 4 条 engine 层用例 |
| m25 | `plugins/ai-chatbox/src/__tests__/highlight.test.ts` | 未测 `CACHE_MAX` 触发清空逻辑（源码 line 309）；未测 `clearHighlightCacheForTest` 清空 in-flight Map（当前只 clear 两个 map，逻辑简单，但无覆盖） | G1 | 追加：调用 N+1 次不同 key 后断言缓存 size 未超 CACHE_MAX；或用小 CACHE_MAX 常量注入 |
| m26 | `plugins/ai-chatbox/src/__tests__/markdown.test.ts` | `patchInlineCode` 是模块私有（未 export），仅通过 `patchIncompleteMarkdown` 间接测试。测试覆盖了 1/2/3+ 长度 run、跨行、fence 内反引号等，覆盖较全；但**未测反引号夹带空格的边界**：`text \`abc` def \`xyz` ghi \`（行尾 run 长度 1，前一个已闭合 1）`。 | G2 | 追加 1-2 条极端行内码 |
| m27 | `packages/plugin-sdk-mobile/__tests__/vite-plugin.test.ts` | 未测：`bedcodePlugin` 在 `enforce: 'pre'` 的 config 阶段与用户自定义 config 的**合并顺序**（Vite 会串联多个 config 返回值）；未测 `renderChunk` 收到 `chunk` 参数时是否依赖 chunk 字段（源码忽略 chunk）。 | G2 | 追加 1 条「chunk 参数被忽略」用例 |
| m28 | `packages/plugin-sdk-mobile/__tests__/runtime.test.ts` | `beforeEach` 内赋值 `(globalThis as any).window`、`afterEach` delete。跨文件测试若同时测 SDK 与插件，**globalThis.window 污染风险**（vitest 默认 isolate 但同文件内多 describe 依赖清理）。 | G5（隔离风险） | 使用 vitest `spyOn` + mockRestore，或 `vi.stubGlobal` |

### Nit

| # | 位置 | 问题 |
|---|---|---|
| n1 | `useAiConfig.test.ts:24, 38, 55, 93` | 多处 `await expect(...).rejects` 无 `.not.toThrow` 双断言，风格一致即可；但 `.rejects.toThrow(/invalid base url/)` 大小写敏感，与源码 `'invalid base url'` 一致 |
| n2 | `useAiChat.test.ts:1-9` 头部注释提及"接缝 3/4"，但未在测试标题中体现；读者需读注释才知道测的是哪条契约 |
| n3 | `mockContext.ts:63-66` 事件 `on` 只支持每 event 单 handler（`listeners[event] = handler`），若 composable 内多次订阅同一 event 后一次 dispose 会误删。当前用例无此场景，但为潜在陷阱 |
| n4 | `adapters.test.ts:8-11` provider 工厂 id 固定 `'p1'`，跨用例无隔离风险（各 setup 独立 mock），但 `useAiConfig.test.ts:38` 手工构造第二 provider id='p2'，若误写 `'p1'` 会导致覆盖 |
| n5 | 全项目无 `console.*` mock；`highlight.ts:316` `console.warn` 分支未测试副作用 |

---

## 3. 评分卡

| 维度 | 分 | 依据 |
|---|---|---|
| 需求/行为契约追溯性 | 88 | 文件头注释明确列出被测接缝（1-4），用例命名贴合契约；个别分支（vite-plugin external:string、classifyError 反例）未追溯 |
| 正反例覆盖 | 72 | 大多数 adapter 只测正例；composables 早返回分支普遍未测；`parseModelsResponse` 反例较完整 |
| 边界+异常覆盖 | 74 | usePluginConfig 边界覆盖优秀（夹取/枚举回退/类型不符）；其他模块异常路径偏少 |
| 断言强度 | 85 | 大量 `toEqual`/`toBe` 精确值断言，几乎无 toHaveBeenCalled；个别（M3、m7、m23）偏弱 |
| 独立性+确定性 | 82 | mock 干净隔离、fake timers 使用得当；唯一真实时间点是 highlight.test.ts 的 waitFor（M6） |
| 可读性+可维护性 | 90 | 注释详尽、断言意图说明清楚、语料对拍设计（ALIGNED_CORPUS）优秀；辅助工具封装清晰 |
| **总分（等权平均）** | **81.8** | 略高于 80 分阈值；主要扣分在正反例覆盖与分支追溯 |

---

## 4. 高风险未覆盖清单

### 已有测试但缺失关键场景

1. **`adapters/usage.ts`**：完全无单测文件。`mergeUsage` 是所有 usage 合并的单一入口（openai 单块、anthropic 分块、gemini 覆盖式），NaN/undefined 传播风险最高，却零覆盖。**建议 P0 新增**。
2. **`adapters/utils.ts`**（`joinUrl` / `effectiveModel` / `parseDataIdModels` / `tryParseJson`）：无独立单测。`isValidBaseUrl` 被 useAiConfig / useAiChat 间接调用，但 URL 拼接（尾空格、相对路径、非法协议）与 JSON 解析异常只覆盖 happy path。
3. **`useAiChat.ts` 分类器**：`classifyError` 反例（普通错误文本透传）与 `isRateLimitError` 的正则路径（无 status code 场景）未测。
4. **`useAiChat.ts` 早返回**：`switchConversation` / `regenerate` / `stopGeneration` / `abortRateLimitRetry` 各有多条早返回，几乎全未覆盖。
5. **`useAiConfig.loadConfig` 异常路径**：storage 返回非 JSON、activeProvider 指向已删除 id、`restored.models` 不含 activeModel（line 70-71 有兜底回退，无测试）。
6. **`vite-plugin.ts` 边界**：`config.external` 为 string/function、CSS 文件名匹配 `.css` 但非 asset 类型、CSS source 为 Buffer（非 Uint8Array）。

### 完全没有测试但应有

1. **`plugins/ai-chatbox/src/dev-mock.ts`**：dev shell 场景，可跳过（不算生产代码）。
2. **`plugins/ai-chatbox/src/utils/providerIcons.ts`**：68 行、纯映射表；建议至少 1 条 fallback 用例（未知 provider.id 返回默认图标）。
3. **`packages/plugin-sdk-mobile/src/global-dialog.ts`**：SDK 对外 API，无测试；若为运行时 API 需覆盖。
4. **`packages/plugin-sdk-mobile/src/ui/**`**：UI 子模块，若为组件库需快照或行为测试。

---

## 5. 改进优先级建议

### P0（必须修，破坏变异防护）

- **M1** — `plugins/ai-chatbox/src/__tests__/highlight.test.ts:52-58`：单例测试无法杀死变异。
- **M7** — `plugins/ai-chatbox/src/__tests__/adapters.test.ts:190-194` + 新增 `usage.test.ts`：mergeUsage 层无测试，NaN 传播风险。
- **M5** — `plugins/ai-chatbox/src/__tests__/useAiChat.test.ts:449-455`：补断言已接收 chunk 落盘。
- **m5** — `packages/plugin-sdk-mobile/__tests__/vite-plugin.test.ts`：补 `external: string` 分支用例。
- **m18** — `plugins/ai-chatbox/src/__tests__/useAiChat.test.ts`：补 `classifyError` 反例（未命中分类时原文透传）。

### P1（建议修，提升覆盖率）

- **M2** — `types.test.ts`：补充运行时断言或声明"仅作类型保险"。
- **M6** — `highlight.test.ts`：消除真实时间依赖（注入 fake）。
- **M4** — 限流「封顶」用例：补推进第 5 次重试 fire。
- **M3** — `vite-plugin.test.ts:125-141`：`toBeDefined()` 加严。
- **m9** — adapters 各补 baseUrl/apiKey/messages 负例（3-5 条）。
- **m10** — adapters `parseStreamEvent` 各补空/畸形数据反例。
- **m12-m15** — `useAiConfig.test.ts`：补 setActiveModel 不存在 provider、removeProvider 边界、fetchModels/testConnection status≠200、updateProvider 不存在 id。
- **m16-m17** — `useAiChat.test.ts`：补 switch/regenerate/stop/abort 早返回。
- **m19** — 补 isRateLimitError 的 529 与纯正则路径。
- **m24-m25** — highlight engine 补 isConnected 早返回、空内容、highlight 抛错、CACHE_MAX。
- **m28** — runtime.test.ts：用 `vi.stubGlobal` 替代直接赋值 `globalThis.window`。

### P2（可选，风格/健壮性）

- **m1** — markdown 对拍辅助函数 `markedFencedBlocks` 与实现同构，考虑黑盒化。
- **m2, m3, m4** — vite-plugin 断言过强快照，改为行为等价校验。
- **m8** — SDK 声明"不校验 mobileApi 形状"契约注释或补类型守卫测试。
- **m11, m20, m22** — 断言强度小幅加强。
- **m26** — markdown 行内码极端边界。
- **m27** — vite-plugin `chunk` 参数被忽略用例。
- **n1-n5** — Nit 层清理。

---

## Standards Compliance

对照 unit-test-discipline G1-G6：

- **G1 行为契约追溯**：✅ 文件头注释明确接缝；⚠️ vite-plugin external:string、classifyError 反例、多处早返回未追溯。
- **G2 正反例覆盖**：⚠️ 大多数 adapter 与 composable 早返回只测正例；usePluginConfig 边界优秀。
- **G3 强断言**：✅ 几乎全部使用精确值断言；⚠️ 3 处弱断言（M3、m3、m7）。
- **G4 反模式检测**：✅ 无 `.only`/`.skip`/snapshot；⚠️ M6 真实时间（waitFor 轮询）、m1 复制实现逻辑、m23 快照替代行为。
- **G5 独立运行**：✅ mock 干净隔离、无顺序依赖、fake timers 正确使用；⚠️ m28 globalThis.window 污染风险、M6 真实时间。
- **G6 变异分析**：⚠️ M1 单例测试、M3 定义即测、M5 副作用顺序、M4 边界未推进均杀不死变异。

**结论**：整体测试质量优秀（总分 81.8），达到通过阈值。主要短板集中在**分支/异常反例覆盖**（尤其 adapters 与 composables 早返回路径）和**个别测试的变异杀伤力不足**（highlight 单例、vite-plugin external:string、限流封顶推进）。建议 P0 项落地后可提升至 85+。
