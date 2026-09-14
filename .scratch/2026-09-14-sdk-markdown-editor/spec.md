# SDK 公共 Markdown 编辑器 + agent-hub Skills 编辑区改造

**Status:** resolved（2026-09-14 实施完成）
**Type:** task
**领域:** plugin-sdk-desktop（前端） / agent-hub 插件（首个消费者）

## Problem Statement

agent-hub 的 Skills 管理里编辑 SKILL.md 目前是一个**裸 textarea**（`SkillEditor.vue`）：没有任何 markdown 辅助（无语法工具栏、无预览），编辑区高度固定 320px、不占满面板到底边、底部也无 padding 间隔，长文件编辑体验差。

同时，BedCode 生态（移动端 `FileViewerModal` 已有 marked + shiki 的 markdown **预览**能力，但无编辑器）没有可复用的 markdown **编辑器**组件。若每个插件各自引入/自研 markdown 编辑器，会重复选型、重复打包、风格漂移。

## Solution

- 在**前端 SDK**（`@binblink/bedcode-plugin-sdk-desktop`，位于 `bedcode-desktop/packages/plugin-sdk-desktop`）新增公共轻量 `MarkdownEditor` 组件：textarea 编辑内核 + markdown 语法工具栏（光标处插入）+ 可选预览切换，零框架依赖、源码级复用，任何插件 `import` 即可用。
- agent-hub `SkillEditor.vue` 改为使用该组件；编辑区**占满当前面板到底边的高度**，**底边保留 padding 间隔**。
- 移动端不动（仅有预览，无编辑器需求）。

## User Stories

1. 作为 agent-hub 用户，我想在编辑 SKILL.md 时看到 markdown 语法工具栏（加粗 / 斜体 / 标题 H1-H3 / 无序列表 / 有序列表 / 引用 / 代码块 / 行内代码 / 链接 / 分隔线），以便不用手打 markdown 语法就能快速写出结构化文档
2. 作为 agent-hub 用户，我想工具栏插入语法时**基于当前光标/选区**生效（无选区时插入带占位文本的模板并选中占位，有选区时包裹选区），以便编辑连续、不用重新定位光标
3. 作为 agent-hub 用户，我想编辑器**预览切换**（编辑 ↔ 渲染后的 markdown），以便不离开页面就能检查渲染效果（渲染质量与移动端一致：marked）
4. 作为 agent-hub 用户，我想编辑区**占满当前面板到底边的高度**，以便长文档获得最大的可视编辑区域
5. 作为 agent-hub 用户，我想编辑区**底边保留一定的 padding 间隔**，以便内容不贴边、视觉舒适
6. 作为 agent-hub 用户，我想现有的保存流程（保存前 diff 预览、mtime/hash 冲突检测、覆盖/重读）**完全不变**，以便编辑器替换不引入工作流回归
7. 作为 agent-hub 用户，我想编辑/预览切换时草稿内容不丢失，以便来回切换检查
8. 作为插件开发者，我想从 SDK `import { MarkdownEditor }`（或 `@binblink/bedcode-plugin-sdk-desktop/ui/markdown-editor`）直接获得编辑器组件，以便其他插件（如 ai-chatbox 的笔记、auto-task 的说明编辑）不用重复实现/选型 markdown 编辑器
9. 作为插件开发者，我想 SDK 组件样式**token-bound**（复用 BedCode 宿主设计 token），以便任何插件里视觉都跟随宿主主题（暗色/亮色一致）
10. 作为插件开发者，我想工具栏提示文案**不绑定某个插件的 i18n**（组件 props 可覆盖或内置双语/图标为主），以便 SDK 组件在任意 i18n 环境下可用

## Implementation Decisions

- **SDK 组件位置与导出模式**（本会话已确认的事实）：
  - 组件文件 `packages/plugin-sdk-desktop/src/ui/MarkdownEditor.vue` + 同名类型声明 `src/ui/markdown-editor.d.ts`（`DefineComponent` + props 类型，参照 `src/ui/Select.vue` / `Select.d.ts`）
  - `package.json` `exports` 增加 `"./ui/markdown-editor"` 子路径（`types` → d.ts、`import` → vue 源码；现有 `./ui` 子路径已有 Select 先例）
  - 复用模式是**源码级**（插件 vite build 时把组件编译进插件 bundle，参照现有 ui/ 组件），不需要重建 SDK `dist`
- **组件接口**：
  - props：`modelValue: string`、`placeholder?: string`、`toolbar?: boolean`（默认 true）、`preview?: boolean`（默认 true，允许关闭预览切换）、`labels?: Partial<Record<'bold'|'italic'|'h1'|'h2'|'h3'|'list'|'olist'|'quote'|'codeBlock'|'codeInline'|'link'|'divider'|'preview'|'edit', string>>`（可选文案覆盖，默认图标 + 内置英文 title）
  - emits：`update:modelValue`（v-model 契约）
  - 编辑内核：`<textarea>`（轻量可靠、保持现有 diff/保存流兼容），语法插入通过**纯函数**作用于光标/选区
- **语法插入纯函数**（建议放 `src/ui/markdown-editor-syntax.ts` 导出，便于单测）：
  - 输入 `(text, selectionStart, selectionEnd, action)` → 输出 `{ text, selectionStart, selectionEnd }`
  - 行为：无选区 → 插入模板并选中占位（如 `**加粗**` 选中"加粗"）；有选区 → 包裹（`**选区**`、`` `选区` ``）；行级动作（标题/列表/引用/代码块/分隔线）作用于光标所在行
- **预览渲染**：`marked`（SDK 已 `pnpm add marked@^18.0.11`，实装 18.0.11；**双端已确认同一库同一版本**——移动端 `marked ^18.0.5` 在 `bedcode-mobile/pnpm-lock.yaml` 解析同为 18.0.11，桌面 ai-chatbox 为 `^18.0.9`，全生态统一 marked v18，渲染行为一致，无需额外选型）。SKILL.md 为宿主自管的本地可信文件，但预览仍建议基础处理（如 marked 默认不转义 HTML 时对未知协议链接/脚本做最小防护，或明确记录接受本地可信内容的取舍）
- **与移动端的能力边界**：移动端 markdown 预览（`FileViewerModal.vue` 等 4 处 marked 用法）仅**预览**、无编辑器需求，且其代码高亮走 `shiki`（代码查看器能力，非 markdown 渲染库）；SDK 编辑器只复用 `marked` 预览渲染，**不引入 shiki**（重型依赖，代码块高亮另行评估）——两端的 markdown 渲染开源组件因此保持同源（marked）
- **样式**：Tailwind utility + 宿主 token 变量（参照 `src/ui/Select.vue`：`--text-primary` / `--text-secondary` / `--border-input` / `--input-height` / `--radius-button` / `--bg-input` 等），禁止硬编码色值；暗色/亮色随 token 自动跟随
- **agent-hub 消费**：
  - `SkillEditor.vue`：`<textarea>` 替换为 `<MarkdownEditor v-model="draft" />`；`draft`、diff 预览、冲突流、保存流**全部不动**（组件只负责编辑交互）
  - 布局（用户明确要求）：编辑区**占满面板到底边高度** + **底边 padding**。实现建议：`.ah-sk-editor` 容器 flex column，编辑区 `flex: 1`；为撑满到底边，给编辑区外层以 `calc(100vh - <头部固定高度>)` 兜底（顶部含面板 padding、tabs、编辑器头部、meta 行），并在编辑区底部留 12–16px padding（以实际渲染微调；参考 SkillEditor 当前 DOM 结构：`ah-inst-head` + `ah-sk-meta` + 编辑区）
- **SDK i18n 中立**：工具栏按钮以图标为主（SVG，参照 Select 的 inline SVG 风格），`title` 文案走 `labels` props 覆盖，默认内置英文；SDK 不引入 vue-i18n
- **依赖现状**（本会话已完成）：SDK `marked@18.0.11` 已安装（`packages/plugin-sdk-desktop/node_modules`，`bedcode-desktop/pnpm-lock.yaml` 已更新，importer `packages/plugin-sdk-desktop` 声明 `marked ^18.0.11`）；本次不动移动端

## Testing Decisions

- **好测试的标准**：只测外部行为——语法插入函数的输入输出映射、组件的 v-model 契约与工具栏交互，不测内部实现细节
- **SDK 侧**（`packages/plugin-sdk-desktop`，vitest + happy-dom 已配置）：
  - `markdown-editor-syntax` 纯函数单测：每种动作 ×（无选区/有选区/光标在行中）的插入结果与选区落点；行级动作（标题/列表/引用）作用于行的行为
  - `MarkdownEditor` 组件测试：`modelValue` 双向绑定、工具栏按钮触发对应插入（断言 textarea value 变化与光标位置）、预览切换渲染 `v-html` 输出
- **agent-hub 侧**：现有 `src/__tests__/` 22 用例保持全绿（diff/保存流不受组件替换影响）；如组件替换涉及 SkillEditor 交互可补 1 条冒烟（挂载渲染）
- **先例参照**：SDK `__tests__/`（happy-dom 组件测试）、移动端 `FileViewerModal.vue` 的 marked 使用方式

## Implementation Log（2026-09-14 完成）

- **SDK**：`src/ui/MarkdownEditor.vue` + `markdown-editor-syntax.ts`（语法插入纯函数）+ `markdown-editor.d.ts` + `package.json` `exports["./ui/markdown-editor"]` 子路径；`marked@18.0.11` 渲染预览（与移动端同版本），link 协议白名单最小防护（不引入 sanitizer，本地可信取舍已注释）；组件额外加 `disabled` prop（保存流锁定编辑区用）；工具栏默认英文 title + `labels` props 覆盖（i18n 中立）
- **agent-hub 消费**：`SkillEditor.vue` textarea → `<MarkdownEditor v-model="draft" :disabled="previewing || saving" />`；布局 flex 链（`.ah-view` flex column → `.ah-sk` flex:1 → `.ah-sk-editor` flex:1 → `.ah-sk-editor-body` flex:1）撑满面板到底边，textarea/预览内容自带 padding 不贴边；diff 预览/冲突流/保存流未动
- **验证**：SDK `test:run` 110 全绿（新增 syntax 22 + 组件 10）；agent-hub 22 全绿；agent-hub `tsc --noEmit` 0 error；根 `eslint .` 0 error（新增文件 0 warning；`__tests__` 在既有 ignore 内）；lens 收尾无 blocker；前端重建 `index.js` 并双目录同步（resources + target/debug）
- **注意**：桌面全量 vitest 在该环境 OOM（heap limit），分文件跑；残留 vite --watch 进程为历史 dev 环境（非本任务开启）

## Out of Scope

- IDE 级编辑能力：语法高亮、自动补全、所见即所得、拖拽/粘贴图片——轻量是刻意取舍，不做
- 移动端 markdown 编辑器（移动端仅预览需求，不动）
- 其他插件 UI 改造（本次只交付 SDK 能力 + agent-hub 先行消费；其他插件后续按需接入）
- 富文本/HTML 粘贴清洗
- 预览样式库（只用 marked 基础渲染，代码高亮不引入 shiki——避免 SDK 重型依赖；如需代码块高亮另行评估）

## Further Notes

- 前置事实（本会话已探明，可直接沿用）：
  - SDK 前端包实际位置 `bedcode-desktop/packages/plugin-sdk-desktop`（仓库根 `packages/` 下没有它，注意路径）
  - 插件前端独立 vite build（`inlineDynamicImports` 单文件），import SDK 组件源码会被打进插件 bundle；改 SDK 组件**不需要重建 SDK dist**，但 agent-hub 需重建 `index.js` 并**双目录同步**（`src-tauri/resources/plugins/desktop/com.bedcode.agent-hub/` 与 `src-tauri/target/debug/resources/plugins/desktop/com.bedcode.agent-hub/`，dev 运行时读后者）
  - agent-hub 生效需重启 BedCode 或停用再启用插件
- agent-hub 完成验证清单：`cargo test`（插件，当前 87 全绿基线）、前端 `vitest run`（22 全绿基线）、`pnpm exec tsc --noEmit` 0 error（i18n MessageSchema 同步）、根目录 `pnpm exec eslint .` 0 error；SDK 侧 `pnpm run test:run` 全绿
- 若后续其他插件接入同一组件，统一从 SDK `./ui/markdown-editor` 导入，不各自复制
