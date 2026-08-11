# AI Chatbox 流式渲染管线 + 协议适配层 实施计划

> 依据：`.scratch/ai-chatbox-render-adapter/spec.md`（ready-for-agent）。术语遵循 `CONTEXT.md`；架构决策见 `docs/adr/0010`、`0011`。双端（desktop / mobile）同步实施，宿主与 SDK 零改动。

## 阶段总览

```
P1 协议适配层（桌面）  →  P2 渲染管线（桌面）  →  P3 思考模式+插件配置（桌面）
                                                    │
P4 移动端移植（含 Shiki 引擎） ←────────────────────┘
                                                    │
P5 双端收尾验证（测试全绿 + 对比实验验收）
```

依赖关系：P2/P3 依赖 P1 的 apiStyle 重命名（不依赖 adapter 全量完成，可先落地字段）；P4 依赖 P1–P3 的核心纯模块；P5 依赖全部。每阶段独立可测试、可回退。

---

## P1 协议适配层（桌面端）

**目标**：供应商差异收敛到前端 TS 适配层，流式路径改 raw 模式 + 前端 SSE 解析；`apiFormat`→`apiStyle` 重命名。

### 改动

**前端**
- `types.ts`：`ApiFormat` 联合类型扩为 `'openai' | 'anthropic' | 'gemini' | 'custom'`，字段重命名 `apiStyle`；`ChatMessage` 新增可选 `reasoning` 字段
- 新增 `src/adapters/`：`ProviderAdapter` 接口、`SseBuffer`（跨 chunk 断行缓冲，产出 SSE 事件）、适配器注册表（按 `apiStyle` 分派）、`openai.ts`（请求构建 + 解析：`delta.content` / `delta.reasoning_content` / usage / `[DONE]`）、`anthropic.ts`（`x-api-key` + `anthropic-version` 头、system 独立、`content_block_delta.delta.text`、`message_delta.usage`）、`gemini.ts`（`candidates[0].content.parts[0].text`、`usageMetadata`）、`custom.ts`（槽位，抛"未实现"或原样透传）
- `useAiConfig.ts`：`normalizeProvider` 映射旧键 `apiFormat`→`apiStyle`（缺失默认 openai）；写入一律新键
- `useAiChat.ts`：发送路径改为 `adapter.buildRequest(...)` → 调用命令（载荷变为 `{ streamId, request }`）；事件监听改为 raw 语义——`payload.chunk` 喂 `SseBuffer`，解析结果按 `{content, reasoning, usage, done}` 累积；`[DONE]` 行标记终结，宿主 `done` 事件作为兜底；**usage 改由前端从流中提取**（raw 模式下宿主不再透传 usage，依赖 `include_usage` 尾块）
- 发送前校验：apiKey 非空、baseUrl 合法（原 Rust 校验前移）

**Rust（变薄）**
- `client.rs`：删除 `build_chat_stream_request` / `build_chat_complete_request` / `build_fetch_models_request` 及 `ApiProvider` 请求构建特化；`chat_stream` / `chat_complete` / `fetch_models` 改为接收 `{ request }`（即 `http_fetch` 载荷 JSON），仅做最小校验（含 url/method/streamEvent）后透传 `host.http_fetch`
- `commands.rs`：命令签名与载荷同步调整（命令 id `ai-chatbox.chat-stream` 等保持不变，前端契约内变更）；`ApiProvider` serde 结构退役或降级为兼容壳
- `store.rs`：本期不动（save_message 的 reasoning 参数在 P3）

### 验证
- 新增接缝 1 测试：各方言请求形状断言、SseBuffer 断行、reasoning 提取、usage 透传、`[DONE]` 终结、thinking 参数映射（P3 配置就绪后补映射用例）、apiStyle 缺失默认
- 更新 `useAiChat.test.ts` 既有用例（事件 payload 语义变化）
- Rust：`cargo test`（client.rs 透传路径 + 既有 http 域测试不受影响）

---

## P2 渲染管线（桌面端）

**目标**：rAF 节流 + fence 补偿 + 延迟高亮，打字机流畅、布局不跳。

### 改动
- 新增 `src/utils/markdown.ts`（纯函数）：
  - `patchIncompleteMarkdown(text)`：奇数个 ` ``` ` fence 追加闭合；行尾未闭合行内码 `` ` `` 补上
  - `getClosedCodeBlocks(text)`：闭合块状态检测（fence 配对 → 块索引/语言/是否闭合），供延迟高亮与头部注入判定
- `useAiChat.ts`：chunk 累积改为**缓冲 + rAF flush**——缓冲区持有待应用内容，`requestAnimationFrame` 回调批量写回 `streamingContent`（每帧至多一次）；`done`/停止/失败时取消待决 rAF 并立即 flush 终态
- `ChatMessage.vue`：
  - `rendered` 计算改为「`patchIncompleteMarkdown` 补偿后的文本」再过 marked + DOMPurify（breaks 选项保持）
  - 高亮收敛为 `HighlightEngine` seam（`src/utils/highlight.ts`：接口 + hljs 实现注入点）；`enhanceCodeBlocks` 只处理**已闭合块**（用 `getClosedCodeBlocks` 判定），未闭合块渲染纯文本 pre（fence 补偿已保证容器存在）
  - 语言标签/复制按钮仅在闭合块注入（幂等，复用现有 `md-code-header` 结构）
- 光标保持现状（不动）

### 验证
- 新增接缝 2 测试：补偿正确性（奇数 fence、行尾行内码）、补偿幂等、闭合块检测（含嵌套文本内 fence 干扰——如正文里出现 ``` 字面量，检测规则需与 marked 行为对齐）
- 接缝 3：fake timers 验证节流语义（多 chunk 一帧合并、done 立即 flush、停止时残留 flush）
- 手工：发一条带 ` ```python ` 开头长回复，观察流式过程布局稳定、闭合后高亮出现

---

## P3 思考模式 + 插件级配置（桌面端）

**目标**：思考过程可折叠展示；三项插件级配置生效并落到请求。

### 改动
- `plugin.json`：新增 `contributes.configuration`——`thinkingMode`（enum: default/enabled/disabled，默认 default）、`reasoningEffort`（enum: low/high/max，默认 high）、`showReasoning`（boolean，默认 true）
- 新增 `src/composables/usePluginConfig.ts`（或并入 useAiConfig）：读 storage key `config` + 默认值合并（宿主配置页保存的值可能缺项）
- `useAiChat.ts`：assistant 消息累积 `reasoning`；`saveMessage` 传 `reasoning`；`finishStream` 落盘含 reasoning（replaceLast 覆盖语义不变——正文与思考一并覆盖）
- **Rust `store.rs`**：`save_message` 命令参数新增 `reasoning`（可选），JSONL 行写入 `reasoning` 字段；`get_messages` 读回
- `ChatMessage.vue`：思考块 UI——可折叠、次级样式、流式期间默认展开、`showReasoning=false` 时不渲染；`reasoning` 从消息字段取（历史对话重开可见）
- openai adapter：`thinkingMode ≠ default` 时写入 `thinking: { type, reasoning_effort }`；`include_usage` 常开（usage 提取依赖它）
- i18n：zh-CN/en 同步新增 key（思考块标题/折叠文案；配置项 title/description 若宿主配置页支持 i18n key 则用 key，否则实施时确认宿主机制后定）

### 验证
- 接缝 3：配置读写 + 默认值合并、reasoning 落盘/读回/重生成覆盖
- 接缝 1 补：thinking 参数映射（default 不传 / enabled+effort / disabled）
- Rust：`cargo test`；手工：DeepSeek 思考模型对话，验证思考块、关闭展示开关、历史重开可见

---

## P4 移动端移植（含 Shiki 引擎）

**目标**：P1–P3 核心纯模块复制同步；高亮引擎换 Shiki（ADR-0011 双引擎对比）。

### 改动
- 复制同步（沿用两插件独立代码库惯例，不抽共享包）：`adapters/`、`utils/markdown.ts`、节流逻辑、`usePluginConfig`、思考块 UI、`plugin.json` configuration、`store.rs` reasoning 参数（移动端 Rust 同名改动）
- **Shiki 高亮引擎**（移动端特有）：复制宿主接入模式——`createHighlighterCore` + `createOnigurumaEngine(import('shiki/wasm'))` + 静态语言导入（`@shikijs/langs`，Tauri WebView 不支持动态 import）+ 懒加载单例；按深浅色模式切换两套内置主题（不映射 CSS token，如 vitesse 对或 github 对，与宿主文件查看器观感对齐）；`HighlightEngine` seam 注入 Shiki 实现（异步高亮：闭合块高亮结果异步回填，需缓存已高亮块避免流式期间重复请求）
- 依赖：移动插件 package.json 新增 `shiki` / `@shikijs/langs` / `@shikijs/themes`（版本与宿主对齐）

### 验证
- 移动端同步全部接缝测试（mockContext 同步适配）
- Shiki adapter node 集成测试：给定代码+语言产出高亮 HTML；未支持语言降级 plaintext
- 移动端 `npm run test:run`（注意移动端包名路径）+ Rust `cargo test`
- 手工：真机/模拟器流式对话，验证 Shiki 主题深浅切换、WASM 加载无报错

---

## P5 双端收尾验证

- 双端 `npm run test:run`（vitest run，禁止 watch）全绿；双端 Rust `cargo test` 全绿
- **对比实验验收**（ADR-0011）：桌面/移动各跑一条带多代码块的长回复（同一 prompt），记录：打字机流畅度（有无掉帧）、流式过程布局稳定性（fence 未闭合期）、代码块闭合后高亮观感（hljs 低饱和 vs Shiki IDE 级）；结论决定后续统一引擎方向，记入 ADR-0011 状态
- 回归检查：token 用量展示、停止生成/重新生成、会话切换拦截、复制按钮、目录授权失败提示等既有行为不受影响
- i18n：zh-CN/en key 同步清单核对

---

## 风险与注意点

- **raw 模式 usage 回归**：宿主 raw 模式 `done` 事件不带 usage（usage 提取原属 openai 解析分支）——必须依赖 `include_usage` 尾块 + 前端解析，P1 测试重点覆盖；若某供应商不回传 usage，保持"无 usage 不显示"的既有降级（usage 字段可选）
- **`[DONE]` 双重终结**：adapter 遇 `[DONE]` 行终结 + 宿主 done 事件兜底，`finishStream` 必须幂等（防双重落盘/重复 flush）
- **fence 检测与 marked 行为对齐**：补偿逻辑的 fence 计数规则要与 marked 的代码围栏规则一致（如 fence 长度、行内 ``` 干扰），否则补偿会改坏正文；接缝 2 用真实 marked 解析做对拍用例
- **rAF 与测试**：vitest 环境无 rAF——用 fake timers / 注入 rAF polyfill；节流函数需可注入调度器便于测试
- **DOM 重建 vs 高亮幂等**：每帧 `v-html` 重建 DOM，hljs 的 `classList.contains('hljs')` 幂等检查失效（现状已是如此）——P2 改为按闭合块状态驱动，避免全量重扫
- **Shiki 在插件 bundle**：移动插件独立打包一份 Shiki（包体 + WASM），与宿主已有实例并存（bundle 隔离）；若加载失败需降级纯文本而非白屏
- **配置页 title 国际化**：宿主配置页 schema 渲染是否支持 i18n key 需实施时确认；不支持则与宿主配置机制对齐后统一
- **`ApiProvider` serde 退役**：Rust 侧结构体退役时确认 `fetch-models` / `chat-complete` 的既有调用方（测试连接按钮）同步走新载荷，避免半套迁移

## 验收标准（对齐 AGENTS.md Done When）

- 双端所有修改代码测试通过（vitest run + cargo test）
- i18n key 同步出现在 zh-CN 与 en
- 公开项（adapter 接口、纯函数）有文档注释；错误处理用 AppError/分类提示，无裸字符串
- 无注释掉的代码；无原生 UI 控件新增（思考块折叠用自绘实现）
- 对比实验结论记录（ADR-0011 状态更新或注释说明）
