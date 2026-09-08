# 05 — resize 防抖补不可见窗口分支（P2）

**What to build:** 对齐 VS Code `TerminalResizeDebouncer` 的第三条分支。BedCode 的 `terminalResizeDebouncer.ts` 现有三条分支（小缓冲立即 / 高度变化立即 / 宽度 100ms 防抖），缺少 VS Code 的「窗口不可见时 X 与 Y 各自走 `runWhenWindowIdle`」分支（plan.md §5）。

现状：不可见时尺寸变化照旧应用（高度立即 `onApply`、宽度 100ms 计时器）。虽有 `visibilitychange` / `focus` 的 `flush()` 兜底保证恢复可见时尺寸兑现，但**不可见期间的中间状态仍会触发 `onApply`**（含 `applyDprFit` 的测量与可能的 resize），属无谓开销，且正确性依赖恢复时的 flush。

改为：在 `TerminalResizeDebouncer` 增加注入式 `isVisible` 判定（组件传入），不可见时挂起应用并推迟到窗口空闲，可见时保持现有分层逻辑不变。

**Spec:** §D-6

**Blocked by:** None — can start immediately.

**Status:** open

- [ ] `TerminalResizeDebouncer` 增加可选 `isVisible?: () => boolean` 注入（照既有 `getBufferLength` 注入模式）
- [ ] 不可见时挂起应用；不可见分支下 X / Y 的挂起语义与恢复兑现路径明确（对齐 VS Code 的 `_resizeXJob` / `_resizeYJob` 分离语义）
- [ ] `flush()` 语义不变：挂起的最终尺寸必达
- [ ] 未注入 `isVisible` 时行为与现状完全一致（向后兼容，现有单测全绿）
- [ ] 扩展 `src/__tests__/utils/terminalResizeDebouncer.test.ts` 覆盖不可见分支
- [ ] `TerminalPreview.vue` 接线：传入基于 `document.visibilityState` / 窗口聚焦状态的判定
- [ ] 保持纯模块零 DOM 依赖约定（Seam A）——`isVisible` 由组件注入，模块内不读 `document`
- [ ] 验证命令：`cd bedcode-desktop && pnpm run test:run`

## Comments

- 本票是纯内部开销优化，无可感知行为变化（`flush()` 兜底已保证正确性）。若优先级紧张可延后。
- **不要**把 `document.visibilityState` 读进 `terminalResizeDebouncer.ts`——既有的 Seam A 约定（零 DOM 依赖）必须保持，判定由组件注入。这与 `terminalRendererPolicy.ts`（01 号票）的注入式设计一致。
