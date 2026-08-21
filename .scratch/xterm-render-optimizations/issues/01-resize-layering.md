# 01 — resize 分层（垂直立即 / 水平防抖）

**What to build:** 拖动窗口时终端 resize 的行为分层——水平方向改变宽度被 100ms 防抖合并，不再每帧触发整屏 reflow；垂直方向改变高度立即生效；拖动结束后最终尺寸必达。resize 决策逻辑抽为纯模块（Seam A），组件只留接线。仅当 cols/rows 实际变化才全量 refresh + 同步 PTY。

**Blocked by:** None — can start immediately.

**Status:** resolved

- [x] 水平方向快速拖动窗口时 resize 被 100ms 防抖合并，拖窗过程中不再每帧触发整屏 reflow（`TerminalResizeDebouncer` 水平 100ms 防抖）
- [x] 垂直方向 resize 立即生效，行数即时更新
- [x] 防抖窗口空闲后（拖动结束）最终 cols/rows 一定送达 PTY 并完成全量重绘（flush 保证）
- [x] 仅 cols/rows 实际变化才触发全量 refresh + PTY 尺寸同步；subpixel 抖动不触发
- [x] resize 决策逻辑抽为纯模块，vitest fake timers 单测覆盖立即/防抖/flush 分支
- [x] 现有终端测试全绿（`useTerminalOutputStream` / `terminalScrollback` / `terminal-flow`）：全量 vitest 431 通过
- [ ] 拖窗真机回归无卡顿（需真机执行）
