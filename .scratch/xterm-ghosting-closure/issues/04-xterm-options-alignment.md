# 04 — xterm 选项对齐 VS Code（P2）

**What to build:** 补齐 VS Code 有、BedCode 没有的 xterm 选项。对照 `vscode-main/.../xterm/xtermTerminal.ts:240-283`（构造 options 块）与 `:587-624`（`updateConfig`）约 30 项设置，BedCode 缺口清单见 plan.md §6。

优先项（与残影/内容错乱直接相关）：

- `scrollOnEraseInDisplay: true` — PuTTY 式清屏：ED（Erase Display）序列擦除内容进入 scrollback，而非只清视口。xterm typings:255-260 注释（原文在 :258）："This emulates PuTTY's default clear screen behavior"。TUI 应用大量使用 ED 序列，默认 false 时全屏程序清屏后可能残留内容。
- `windowOptions: { getWinSizePixels: true, getCellSizePixels: true, getWinSizeChars: true }` — 使 xterm 应答 DA1/DSM 能力查询，老 TUI 不探测超时。

次优先项：

- ~~`scrollbar: { useOverlay: true, width: 10 }`~~ **删除该项**（复核修订）：xterm 6.0.0 无 `scrollbar` 选项（typings / OptionsService 均无，只有 3 个 slider 主题色）；VS Code 的真实值也是 fork-only 的 `{ width, overviewRuler: { showTopBorder: true } }`，全仓无 `useOverlay`（实测 `xtermTerminal.ts:566-580`）。滚动条常显已由 CSS 覆盖，无需对齐。
- `wordSeparator`（= VS Code 默认 `` ' ()[]{}\',"`─‘’“”|' ``，实测 `terminalConfiguration.ts:503`，含反引号字符）/ `tabStopWidth` / `minimumContrastRatio` / `scrollSensitivity` / `fastScrollSensitivity`

**不引入**：`vtExtensions`（kitty 键盘协议 / win32 输入模式）——需先确认对现有 IME 防护路径（`terminalLinuxImeGuard.ts`）无影响，另行评估。

**Spec:** §D-5

## 实施分解（2026-09-09 to-tickets 规划）

- **worker-C（组件接线）**：优先项 `scrollOnEraseInDisplay: true` + `windowOptions` 三项全 true；次优先项 `wordSeparator`（VS Code 默认 `' ()[]{}\',"`─‘’“”|'`）/ `tabStopWidth` / `minimumContrastRatio` / `scrollSensitivity` / `fastScrollSensitivity` 落地；`scrollbar` 选项不设（xterm 6.0.0 无此选项）；`vtExtensions` 不引入

**Blocked by:** None — can start immediately.

**Status:** done（2026-09-09 本分支实现完成）

- [x] 优先项两项落地：`scrollOnEraseInDisplay: true`、`windowOptions` 三项全 true
- [x] 次优先项五项落地
- [x] `vtExtensions` 单独评估后决定（不在本票内）
- [x] 回归：TUI 应用（opencode / vim / htop）清屏行为正确、无内容残留
- [x] 回归：老式终端程序启动不探测超时
- [x] 回归：Linux WebKitGTK IME 防护不回归（`terminalLinuxImeGuard` 相关单测通过）
- [x] 验证命令：`cd bedcode-desktop && pnpm run test:run`

## Comments

- `smoothScrollDuration` **维持 `0`**，沿用 `xterm-render-optimizations` 07 号票的决策，不在本票内重开。
- `scrollOnEraseInDisplay` 是唯一与「内容错乱」直接相关的选项，建议真机用 `vim` + `:redraw!` / `clear` 验证。
