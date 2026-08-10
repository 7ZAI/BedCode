# AI Chatbox 供应商配置页与输入区 UX 重构规格（桌面）

Status: ready-for-agent

> 本规格由 grilling 会话全程决策汇编而成（grill-with-docs + domain-modeling）。实现者无需再做重大决策。上一轮 v2 重构规格见 `.scratch/ai-chatbox-rebuild/spec.md`（已完成），本规格是它之上的 UX 重构。

## Problem Statement

用户在 v2（AI Chatbox 插件重写）基础上提出三个体验问题：

1. **供应商配置页设计逻辑不合理**：左侧"预设目录 + 自定义供应商"双列表。预设点击后表单实际是空的——`ProviderConfigPage.selectPreset()` 只设置死状态 `selectedPresetName`，`ProviderForm` 的预设回填依赖 `initialValues?.name` 匹配，而 add 模式下 initialValues 恒为 undefined，回填永远不触发（"预设没办法直接设置"）。预设是 `types.ts` 硬编码常量，列表项无删除入口；删除按钮藏在编辑表单底部且用 3 秒双确认（"无法删除配置"）。模板与实例混排同一层，用户分不清哪些能删、哪些是模板。
2. **对话输入框与主流 AI 对话（ChatGPT / DeepSeek / Chatbox）观感不符**：模型选择器是表单控件语言的 pill（边框/高度/chevron 均为 Select 骨架），发送图标是 `↑` 而非主流 `↗`。
3. **模型下拉在底部被遮挡**：SDK `Select.vue` 面板固定向下展开（`top: rect.bottom + 4`），无视口空间检测；输入框贴侧边栏底部时面板超出窗口底边被裁掉。

## Solution

- **供应商配置改为单一实例列表 + 两步添加流程**（Cherry Studio / Chatbox 通用做法）：列表每项 = 已保存的配置实例（图标 + 名称 + 激活圆点 + hover 删除）；"添加供应商"→ 模板选择步骤（4 内置预设 + 空白自定义）→ 回填表单 → 填 API Key → 保存
- **预设 = 只读添加模板**，不再是列表项，不可删除
- **删除入口**：列表行 hover 删除图标 + 编辑表单内删除按钮，统一走自绘确认弹窗（替换 3 秒双确认；禁原生 confirm）
- **供应商图标**：内置预设品牌 SVG（DeepSeek / 通义千问 / OpenAI / Anthropic，自官网爬取简化后打包）；自定义与旧数据用首字母彩色头像（哈希取色）
- **ChatInput 对齐主流**：模型选择器重做为无边框圆角 chip，发送图标改 `↗`
- **SDK Select 智能翻转**：下方空间不足时向上展开、两侧不足时收缩 maxHeight、水平 clamp 视口——共享组件一次修复，所有插件受益

## User Stories

1. 作为用户，我希望供应商配置页只显示一个"已保存配置"列表，这样一眼看清我配了哪些供应商
2. 作为用户，我希望点击"添加供应商"后先看到模板选择页（DeepSeek / 通义千问 / OpenAI / Anthropic + 自定义），这样不需要记住各家 baseUrl 和模型名
3. 作为用户，我希望选择预设模板后表单自动回填名称/baseUrl/模型列表，这样只需粘贴 API Key 即可保存
4. 作为用户，我希望"自定义"模板提供全空表单，这样中转站/私有网关也能接入
5. 作为用户，我希望模板选择页展示各家品牌图标，这样一眼认出我要配的服务
6. 作为用户，我希望保存后实例出现在列表中并带品牌图标，这样识别直观
7. 作为用户，我希望自定义供应商显示首字母彩色头像（非品牌图标），这样与内置供应商同样有视觉识别
8. 作为用户，我希望点击列表行进入编辑表单，这样改 Key/模型不需要删除重建
9. 作为用户，我希望列表行 hover 时出现删除按钮，点击后弹出确认弹窗，这样误删有挽回余地
10. 作为用户，我希望编辑表单里也保留删除入口（同一确认弹窗），这样不习惯行操作的我也能删除
11. 作为用户，我希望删除激活供应商后自动回退到列表第一个，这样不会出现"无供应商"悬空状态
12. 作为用户，我希望新增供应商不自动切换当前对话的激活供应商（第一个除外），这样调试新配置不打断当前对话
13. 作为用户，我希望列表行显示激活圆点标记，这样知道当前对话正在用哪个供应商
14. 作为用户，我希望旧版本保存的配置（无 presetId）照常可用并显示默认头像，这样升级无感、零迁移
15. 作为用户，我希望输入框里的模型选择器是无边框圆角 pill（浅底、小 chevron、hover 微深），这样与 ChatGPT/DeepSeek 观感一致
16. 作为用户，我希望发送按钮是右上箭头（↗）图标，这样符合主流 AI 对话交互语言
17. 作为用户，我希望输入框在窗口底部时模型下拉自动向上展开，这样选项不被窗口边缘遮挡
18. 作为用户，我希望下拉在上下空间都不足时收缩高度并可滚动，这样极端小窗口也能选到模型
19. 作为用户，我希望下拉水平方向不超出视口，这样窄侧边栏里面板完整可见
20. 作为用户，我希望宿主其他页面（设置页等）的下拉行为与现在一致，这样共享组件修复不引入回归
21. 作为用户，我希望界面文案随宿主语言切换（zh-CN / en）且无裸 key，这样中英文体验一致

## Implementation Decisions

### 插件 `types.ts`

- `ProviderPreset` 增加 `id` 字段（`'deepseek' | 'qwen' | 'openai' | 'anthropic'`），`PROVIDER_PRESETS` 同步补齐
- `ApiProvider` 增加可选字段 `presetId?: string`（旧数据缺失走默认图标，向后兼容）

### 供应商图标

- 品牌 SVG：自各官网爬取官方图标，简化后打包为插件静态资源（`src/assets/providers/`，deepseek / qwen / openai / anthropic 四个）
- 纯函数 `resolveProviderIcon(presetId)`：返回内置图标引用；无 presetId 返回 null（走首字母头像）
- 首字母头像组件：取名称首字符 + 按名称哈希从主题色板取色，圆形底色；名称为空时极简兜底（通用 bot SVG）

### `ProviderConfigPage.vue` 重写

- 单一列表：行 = 图标 + 名称 + 激活圆点（现有 activeProviderId 语义保留）+ hover 删除按钮；点击行进入编辑；"添加供应商"按钮置顶
- 添加流程两步页内流转（配置页保持现有全宽覆盖模式，不新增弹窗层）：模板选择步骤（4 预设图标卡片 + "自定义"项）→ ProviderForm 步骤（预设回填 name/baseUrl/models，自定义全空）
- 保存成功回到列表；**新增不自动激活**（首个自动激活的现有逻辑保留）
- 排序按添加顺序；不分组、不搜索、不拖拽

### `ProviderForm.vue`

- 字段全保留：名称、BaseURL、API Key、模型列表（ModelListEditor）、拉取模型、测试连接
- 保存时写入 `presetId`（从预设进入时取模板 id；编辑已有实例保持原值）
- 删除按钮保留（编辑模式），与行删除共用确认弹窗；移除 3 秒双确认交互
- API Key 仍明文存储（已知限制，本次不动）

### 新增确认弹窗（插件本地组件）

- 自绘覆盖层（Teleport），标题 + 正文（含供应商名称）+ 删除/取消按钮；禁原生 `confirm()`
- 放在插件本地，不新增 SDK 组件（等第二个插件需要时再提升）

### `ChatInput.vue`

- 模型 pill 重做：无边框圆角 chip（浅底、小圆角、文字 + 小 chevron、hover 微深），由父组件注入的 SDK Select 承担交互（样式经 `:deep()` 收敛），视觉语言对齐 DeepSeek/ChatGPT
- 发送按钮：图标改右上箭头 `↗`；形状保持圆形（用户允许圆角方形，实现取最小改动）
- 其余骨架不动：textarea 自适应（1~8 行）、Enter 发送 / Shift+Enter 换行、流式停止按钮、toolbar 插槽

### SDK `Select.vue`（共享组件，桌面 SDK）

- `computePosition()` 重构为纯函数 `computeSelectPosition(triggerRect, viewport, panelHeight) → { top, left, maxHeight }`
- 翻转规则：下方空间足够 → 向下（现状）；下方不足且上方足够 → 向上展开；两侧都不足 → maxHeight 收缩到可用空间（面板内滚动）；水平 clamp 不出视口
- 行为变更仅在空间不足时触发，既有使用点（页面中部）行为不变
- SDK 改动后需在 `packages/plugin-sdk-desktop` 内 `npm run build` 重新产出 dist，插件（`file:` 依赖 dist）才能引用

### `useAiConfig.ts`

- `addProvider`：从预设模板创建时写入 `presetId`；新增不自动激活（首个除外）
- `normalizeProvider`：旧数据无 `presetId` 时保持 undefined（走默认头像），不强制补默认值

### i18n（zh-CN + en 同步）

- 移除 key：`presetProviders`、`customProviders`、`addCustomProvider`
- 新增 key：添加供应商、选择模板、自定义（模板项标签）、确认删除标题/正文、删除、取消等
- 术语语义：供应商列表 / 预设模板 / 激活供应商 / 自定义（见 CONTEXT.md「AI 对话」小节）

### CONTEXT.md

- 已在本会话写入「AI 对话」小节新增术语：供应商 (Provider)、预设模板 (Preset Template)、激活供应商 (Active Provider)、自定义 (Custom)

## Testing Decisions

好的测试只测外部行为（给定输入 → 断言输出），不测实现细节；纯视觉部分（布局/配色）不进单测，走视觉审查。

**接缝 1 — SDK Select 定位纯函数（SDK 包 vitest）**：`computeSelectPosition` 规则表驱动单测——下方空间足够向下、下方不足上方足够向上、两侧不足 maxHeight 收缩、水平 clamp 各象限。先例：SDK 既有 `__tests__/manifest-gen.test.ts`（同一 vitest 设施）。

**接缝 2 — 插件配置逻辑 + 图标解析（插件 vitest）**：`useAiConfig`——从预设添加写入 presetId、新增不自动激活（首个除外）、删除激活供应商回退、旧数据（无 presetId）归一化不崩溃；`resolveProviderIcon`——presetId → 对应图标、无 → null；首字母取色——同名称同色（确定性）。先例：`__tests__/useAiConfig.test.ts` + `__tests__/mockContext.ts`。

**接缝 3 — UI 视觉审查（dev-shell + vision，半人工）**：dev-shell 运行插件（既有 `dev-mock.ts` 机制）→ Chrome headless 截图（配置页列表 / 模板选择 / 表单、输入区 pill 常态与 focus 态、下拉向上展开态）→ vision subagent 审查（对照宿主 CSS token 与主流 AI 输入框形态）→ 迭代至通过。先例：v2 Step 4/6 六轮 vision 迭代。

**接缝 4 — 迁移与回归手动验证**：旧 storage 数据（无 presetId）加载后配置可用、显示默认头像；宿主其他 Select 使用点（设置页等）行为不变；SDK 重建后插件构建通过。

运行方式：SDK 包 `npm run test:run` + 插件 `npm run test:run`（vitest run，禁 watch）。

## Out of Scope

- API Key 加密存储（明文保持，已知限制）
- 预设模板管理（增删改模板本身）——模板保持只读硬编码
- SDK Select 图标插槽（聊天头部供应商下拉暂不加图标，记入后续增强）
- 列表排序/分组/搜索/拖拽
- 模型选择器升级为搜索式重型面板（Cherry Studio 风格，380px 侧边栏放不下）
- 输入框新元素：附件按钮、上下文/清空按钮、字符计数、Enter 发送提示小字
- 移动端插件（本次仅桌面；移动端输入区已在 v2 Step 6 重构，如有同类问题另行立项）
- Rust 侧任何改动（本次为纯前端 + SDK Select）

## Further Notes

- 品牌图标自官网爬取后简化打包，仅本地展示用途
- 修改样式时加载 `frontend-styles` skill
- i18n key 双语言同步；禁原生 confirm；禁注释掉的代码；禁中文硬编码字符串（composable 内）
- SDK 包 `packages/plugin-sdk-desktop` 有独立的 vitest 设施与 `test:run` 脚本，Select 定位函数单测放 SDK 侧

## 验证记录（2025-08 实施后）

### 代码审查修复（review 后补充，未在原规格中）

- `ProviderForm`：添加模式 `activeModel` 随 models 回填首个（原为 ''，靠运行时兑底）
- `ChatInput` pill hover：`--bg-input` 与 `--bg-card` 同值（四个主题均验证），hover 改 `color-mix` 朝 `--text-primary` 混入一档——浅色变深、深色变亮
- `select-position.ts`：两侧不足分支 maxHeight 让出一个间距，面板与触发器/视口边缘保持 4px 留白（原上方分支间距 0）；同步更新 2 个单测期望
- 品牌 SVG 改 `?raw` 内联 + `currentColor`（`<img>` 加载落入隔离文档时 currentColor 恒为黑，深色主题不可见）
- `ConfirmDialog`：遮罩 `@mousedown` 关闭（原 `@mousedown.self` 被遮罩子元素遮挡永不触发）；删除无人使用的 `confirmText/cancelText` 可选 props
- 清除未定义 Tailwind 类 `hover:bg-brand-hover`（tailwind 仅 DEFAULT/light 两键）→ `hover:opacity-90`（本次 diff 内 3 处）
- `ProviderAvatar` 删除无定义死类 `avatar-circle`；`useAiConfig` 提取 `isApiProvider` 显式类型守卫

### 接缝 3（视觉审查）执行记录

dev-shell（BEDCODE_DEV_PLUGINS 指向插件，port 5173）+ puppeteer-core 驱动 + Chrome headless 截图 8 张（`.scratch/ai-chatbox-config-ui/shots/`），vision subagent 审查：

- 聊天输入区：模型 chip 无边框圆角浅底 ✅、发送按钮 ↗ ✅、整体接近主流形态（8/10）
- 模型下拉向上翻转：面板 top=743/bottom=813 位于 pill（bottom=845）上方，完整可见无裁剪 ✅
- 配置列表：行结构（图标+名称+激活圆点）✅；OpenRouter 显示首字母头像为设计意图（自定义 fallback）
- 模板选择页：4 预设卡片 + 自定义卡片、品牌图标齐全 ✅
- 表单回填：DeepSeek 名称/baseUrl/模型已回填、API Key 为空 ✅
- 深色主题验证：`html.dark` 下品牌图标 fill=rgb(236,232,220)（浅色文字色），深底可见 ✅（截图 07/08）

vision 提出的优化建议（单行工具行、添加按钮降级、5 卡片网格、表单取消按钮等）经裁决均不采纳：超出规格范围（骨架保留、不分组不搜索）或与设计意图冲突（主 CTA、overlay 语义）。

### 接缝 4（迁移与回归）执行记录

- 旧数据兼容：`normalizeProvider` 对无 `presetId` 数据保持 undefined → 默认头像，单测覆盖（providerIcons 6 用例 + useAiConfig 13 用例）
- dev-mock seed 含无 presetId 的 OpenRouter 实例，验证列表/下拉正常渲染
- 回归：插件 27 + SDK 21 单测全过；`build:frontend` 通过（238 modules）
- Rust 零改动，无需 cargo test
