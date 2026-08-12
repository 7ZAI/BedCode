# 移动端终端输入体验优化计划

> 状态: 计划 (未排期)
> 已实施: P0-1 一键中断、P0-2 `/` 补全、P0-4 @ 文件引用插入（移动端仅，延续 ADR-0014 边界）
> 背景: 本项目的初衷是通过移动端远程控制 agent CLI（Claude Code / pi / Codex / OpenCode）。
> 已落地（commit 392dcdeca）: Agent CLI 命令预设（12×4）+ 16 键默认快捷键 + 会话自动识别。
> 本文档记录下一轮输入体验优化方向，按优先级分组，不设实现期限。

## 优化原则

- 移动端最高频动作：中断生成、发送指令、补全命令、重发/修改 prompt
- 一切优化围绕"单手拇指操作 + agent CLI 交互语义对齐"，不做桌面端能力平移
- 命令/按键语义见 CONTEXT.md（快捷命令、命令预设、发送与执行、Agent CLI）

## P0 — 高价值（直击日常痛点，改动小体感大）

### 1. 生成中一键中断 ✅ 已实施

- 现状: agent 输出时中断需「展开面板 → 按 Esc」，路径过长
- 方案: 输入条在会话生成/等待时自动切换为常驻**中断按钮**（等价 Esc 特殊键发送），
  生成结束后恢复原按钮；按钮状态由会话 waiting/running 状态驱动
- 涉及: `TerminalInputBar.vue`、`TerminalView.vue`（状态透传）、会话状态事件
- 成本: 小
- 实施说明: 桌面端 waitingInput 检测与插件 taskStatus 均为预留未接入，通用 PTY 会话
  用「xterm 末行提示符 + 近期无输出」双条件推断空闲（`utils/terminalIdle.ts`，400ms 轮询），
  taskStatus 接入后自动优先生效；中断按钮复用 Esc 特殊键通道（`handleInterrupt`）

### 2. 输入框 `/` 命令补全 ✅ 已实施

- 现状: 命令预设只能点面板按钮发送，输入框内打 `/` 无任何提示
- 方案: 输入框输入 `/` 时弹出本地预设命令补全列表（复用 `agentPresets.ts` 数据），
  点选即填充输入框；与 agent 内部补全同构，本地零延迟
- 涉及: `TerminalInputBar.vue`、`agentPresets.ts`（导出命令文本列表）
- 成本: 中
- 实施说明: `agentPresets.ts` 新增 `getPresetCommandTexts` / `filterPresetCommands`
  （前缀过滤 + 排除裸 `/`），弹层仅在当前会话预设非空时出现（generic 不弹）

### 3. prompt 历史与草稿保护

- 现状: ↑ 方向键发送的是 agent CLI 自己的历史；BedCode 输入框无本地历史，
  且 Esc 打断/误触后已输入的 prompt 直接丢失
- 方案:
  - 输入框内 ↑/↓ 翻本地发送历史（与 CLI 历史错开：仅在输入框空闲/有内容时接管）
  - 打断后草稿保护：输入内容在面板收起、会话切换时不丢（临时草稿槽，类似 CC 的 stash）
- 涉及: `TerminalInputBar.vue`（草稿状态）、`inputAssistant.ts`（历史存储，localStorage）
- 成本: 中

### 4. @ 文件引用插入 ✅ 已实施

- 现状: CC/codex 支持 `@路径` 引用文件，移动端只能手打 Windows 路径
- 方案: 文件侧栏选中文件 → 「插入引用」把路径作为 `@引用` 填入输入框
- 涉及: `FileSidebar.vue` / 侧栏选中状态 → 输入框联动
- 成本: 中
- 实施说明: 终端侧栏启用 `ref-insert` 模式（点选文件 → `@路径` 填入输入框并聚焦，
  自动收起侧栏）；查看/复制改为长按操作面板，保留原能力

## P1 — 中价值（输入效率提升）

### 5. Agent CLI 手动覆盖 UI

- 现状: 覆盖映射只有 store API（`setAgentTypeOverride`），无 UI 入口；
  包装脚本启动的会话识别可能误判，无法修正
- 方案: 设置页或会话配置卡片上选择 Agent CLI，写入覆盖映射（JSON 文件）
- 涉及: 设置页/`SessionConfigCard.vue`、`inputAssistant.ts`
- 成本: 小

### 6. 输入法协作

- 方案:
  - `enterkeyhint="send"` 让键盘回车键变「发送」；多行 prompt 用显式换行按钮
  - 中文输入法组合键与 Enter 发送的冲突处理（compositionend 判定）
- 涉及: `TerminalInputBar.vue`
- 成本: 小

### 7. 排队发送

- 现状: agent 生成中发送会静默失败或直接写入，无排队语义
- 方案: 生成中点发送时弹「排队/丢弃」选择（对齐 pi 的 alt+enter 排队、codex 的 Tab 排队）
- 涉及: `TerminalView.vue`（队列状态）、`TerminalInputBar.vue`
- 成本: 中

## P2 — 低成本打磨

### 8. 命令预设复制为自定义

- 现状: builtin 预设不可编辑
- 方案: 预设命令支持「复制为自定义」后自由增删（`custom_commands` 落盘）
- 涉及: `TerminalInputBar.vue` 编辑模式
- 成本: 小

### 9. quick bar 手动置顶

- 现状: 纯频次排序，无法固定常用项
- 方案: 长按 quick bar 项 → pin（置顶且不参与频次排序淘汰）
- 涉及: `inputAssistant.ts`（QuickBarItem 加 pinned）、`TerminalInputBar.vue`
- 成本: 小

### 10. 长按连发

- 方案: Del/方向键长按连续发送（滚动长输出、连续删除有用），带触发间隔
- 涉及: `TerminalInputBar.vue`
- 成本: 小

### 11. 横屏精简按键行

- 现状: 横屏整个快捷键面板禁用（提示不可用）
- 方案: 横屏给一排精简按键（Esc/Ctrl+C/↑↓/Enter），不占竖屏布局
- 涉及: `TerminalInputBar.vue`
- 成本: 中

## 建议实施顺序

1. P0-1 一键中断 + P0-3 草稿保护（最小组合，先解决"误触丢输入 + 中断绕路"）
2. P0-2 `/` 补全（复用 agentPresets，收益直接）
3. P1-5 覆盖 UI（补齐识别闭环）
4. 其余按需

## 备注

- P0-3 的历史存储沿用 inputAssistant 的 localStorage 模式（与频次统计同源）
- P0-2 补全列表仅在当前会话 agent 预设非空时启用（generic 不弹出）
- 所有改动仅涉及移动端，桌面端不受影响（延续 ADR-0014 边界）
