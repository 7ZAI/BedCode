# 07 杂项 P1：文件页 CTA / 发现页 / 徽章对比度 / 触达区

**Status:** ready-for-agent
**Type:** task

## 问题（vision 第二/三批，P0/P1）

`files-dark.png`：
1. 错误态"重试"CTA 错配——"no base URL set"是配置缺失，重试无意义，应引导去设置页（router.push 到连接设置或扫码页）

`discover-dark.png`：
2. 空态与"重新扫描"按钮间 ~200px 真空；扫描进行中无状态指示（脉冲/进度）

全局：
3. 状态徽章小字号对比度临界（"未使用" `preset-tasks`、"运行中" `sessions`、"未连接" `devices`）——小字号下需 WCAG AA ≥ 4.5:1
4. 次要文字（IP、时间戳、placeholder）对比度偏低，统一走 token（`--mobile-text-muted` 等）核对
5. 可点击卡片缺 active 反馈（`active:opacity-90` 类）

## 涉及

`src/views/CodeExplorerView.vue`（或文件树组件）、`src/views/DiscoverView.vue`、
`src/styles/mobile.css`（token 核对）、相关卡片组件

## 建议

- files 错误态：主 CTA 改为"前往设置/重新连接"，重试降为次按钮
- discover：空态垂直居中（`min-h` + justify-center），扫描中显示脉冲指示
- 徽章/次要文字对比度按 token 走查调整

## 验证

harness 截 `files`、`discover` 深色 + 浅色。

## Comments

- 2026-08-12：vision 第二/三批评审发现。
- 2026-08-12（已修复）：FileExplorer 错误态识别「no base URL」→ 主 CTA 改为「前往连接设置」（CodeExplorerView / PresetTasksView 已接线），重试降为次按钮；discover 查证已有扫描状态行 + 雷达动画 + 垂直居中空态，无需修改；「未使用」徽章对比度已在 04 修复（chip-zinc + 去 opacity）；次要文字已全部走 token 核对无遗漏；可点击卡片均已带 active 反馈。
