# 移动端终端 UI 调试会话产物 — 归档

> 关联：`.scratch/mobile-input-experience/plan.md`（方向计划）、`.scratch/terminal-input-navigation/spec.md`
> 日期：2026-08-15 · 会话：代码审查后清理
> 状态：已实现，随工作区未提交改动一并落地；本文档补 plan/spec 未覆盖的调试产物记录

## 背景

终端视图（TerminalView.vue）在真机调试中暴露的尺寸/滚轮/输出时序问题，本轮修复产物不在
`mobile-input-experience/plan.md` 与 `terminal-input-navigation/spec.md` 的原始范围内，属调试会话演进，特此归档。

## 改动清单

### 1. PTY 尺寸同步：HTTP 串行队列（TerminalView.vue）

- 问题实证：HTTP 与 WS 双通道并发会把不同尺寸请求乱序送达服务端——fit 前的 80x24 默认值若后到会覆盖实际尺寸，PTY 停在 80x24 → opencode 按 24 行渲染，显示区下半黑（半屏黑）
- 方案：所有 resize 请求（onResize / fit / 重连后显式同步）收敛到 HTTP 单通道串行发送，同一时刻仅一个在途请求；过滤 80x24 默认尺寸 + 单通道保序

### 2. 余量 fit（useTerminalScroll.ts `fitTerminal`）

- 官方 FitAddon 填满容器后各减 1 格（行尾/列尾留白），行数精确 → 内容底部不溢出输入栏，历史可滚范围正确

### 3. TUI 滚轮重写（useTerminalScroll.ts）

- TUI（opencode/pi 等全屏应用）滚轮语义与普通输出不同，按行距/方向重写滚动行为

### 4. 输出冷却计数修复（terminalBuffer.ts）

- 冷却计数 bug：注释含实测日期证据；`terminalMetrics.ts`（新文件）承载尺寸/性能指标，文件头 JSDoc 合规

### 5. 高频调试日志清理（本次审查收尾）

- `console.info` 高频路径（onResize 每次、syncSize 每次、fit probe 每次）全部移除，保留失败路径 `console.warn`

## 验证

- 移动端 `vitest run` 176 全绿（含 useTerminalScroll / useTuiCompat / writeCoalescer / terminalBuffer 同步更新的测试）
- 移动端 `vue-tsc --noEmit` 0 error

## 遗留

- `writeCoalescer.ts` 的 `ENABLE_RAF_COALESCE=false` 默认关闭 rAF 合并（生产默认每事件直写）：行为反转需真机高频输出场景回归，确认无卡顿后可恢复默认开启或删除双开关冗余
- `useTerminalScroll` 与 `.terminal-input-bar` 类名的耦合（fit 依赖输入栏存在）为已知脆弱点，后续可改为尺寸注入解耦
