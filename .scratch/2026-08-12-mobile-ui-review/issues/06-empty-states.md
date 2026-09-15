# 06 空态批：工具箱 / 会话 / 终端审查体验

**Status:** ready-for-agent
**Type:** task

## 问题（vision 第一/二/五批，P0/P1）

`toolbox-dark.png`：
1. 工具箱只有"预设任务"一张卡，下方 60% 空白（插件区空时为孤岛）——至少给空态占位或次级入口
2. 卡片无 chevron，可点击性弱

`sessions-dark.png`：
3. 空态仅一行文字，无图标/引导
4. 顶部两个 icon 按钮触达区 < 44px 且间距小

`terminal-dark.png`：
5. mock 输出 5 秒才 4-8 行，截图/审查时输出过少（可加快 mock 频率，审查体验）
6. 底部快捷键条 6 键拥挤；输入框 placeholder 与标题重复

## 涉及

`src/views/ToolboxView.vue`、`src/views/SessionsView.vue`、
`src/composables/useMockTerminal.ts`、`src/components/TerminalInputBar.vue`、`src/components/MobileNav.vue`（icon 按钮间距）

## 建议

- 空态统一模式：图标 + 主文案 + 引导动作（按钮/链接）
- mock 输出间隔参数化（如 `mock-terminal-speed` localStorage 已支持，默认加快）
- i18n key 同步

## 验证

harness 截 `toolbox`、`sessions`、`terminal/__mock_terminal__`。

## Comments

- 2026-08-12：vision 第一/二/五批评审发现。
- 2026-08-12（已修复）：工具箱无插件时加虚线空态占位（图标+文案+「插件管理」入口），预设任务卡补 chevron；会话页未连接/无会话空态统一 图标+文案+引导动作（未连接加「二维码连接」CTA）；头部 icon 按钮扩到 44px；mock 输出间隔支持 `mock-terminal-speed` localStorage（默认 1.5s，首帧 0.8s）；mock 会话输入框 placeholder 不再与标题重复。底部快捷键条保留横向滚动 + 数量可配（quickBarCount），未改布局。
