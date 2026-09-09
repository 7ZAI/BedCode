# 01 — `allowTransparency` 切换重建渲染器（P0）

**What to build:** 修复「背景图透不出来」的功能性 bug。透明度状态变化（`bgImageUrl` 有无切换）时，不能只改 `terminal.options.allowTransparency` + `refresh()`——addon-webgl 0.19.0 中该选项无运行时监听，渲染层 alpha 标志与 canvas 的 `{ alpha }` 属性在 `getContext('2d', {...})` 之后不可变（xterm 里处理该切换的 `_setTransparency` 是零调用的死代码，见 plan.md §2.2 实测）。

改为：dispose WebGL addon → 重设 `terminal.options.allowTransparency` → 重新初始化 WebGL addon → 重设 theme → 重算尺寸 → 全量重绘。现有 `initWebGL` 的 context-loss 恢复路径已有同款「重建后重算尺寸」处理，复用之。重建时必须同步 `xterm-hidden-cursor` 的加/删，否则出现双光标或光标消失。

同时把「是否需要透明 / 该用哪个渲染器」抽为纯函数（Seam A，见 spec Testing Decisions），使判定可单测。

**Spec:** §D-1、§D-2（若用户选路线 B，`hasBackgroundImage === true` 时 `useWebgl = false` 一并落地）

## 实施分解（2026-09-09 to-tickets 规划）

- **worker-A（纯函数层，独立文件）**：新建 `src/utils/terminalRendererPolicy.ts`（`decideRenderer` + `decideAtlasRefreshFrames`）+ `src/__tests__/utils/terminalRendererPolicy.test.ts`（isLinux × hasBackgroundImage × route 全组合 + 帧预算递减/停止）
- **worker-C（组件接线，TerminalPreview.vue 唯一写者）**：初始化改用 decideRenderer 结果；透明度状态变化时重建渲染器（dispose → 重设 options → 重新 initWebGL/移除 → 重设 theme → applyResize → 全量重绘）；重建序列号竞态保护；同步 xterm-hidden-cursor；组件卸载清理

**Blocked by:** None — can start immediately.

**Status:** done（2026-09-09 本分支实现完成）

- [x] 新增 `src/utils/terminalRendererPolicy.ts`：`decideRenderer({ isLinux, hasBackgroundImage, linuxUseDomRenderer, route })` → `{ useWebgl, allowTransparency, useDom }`，零 DOM 依赖
- [x] 新增 `src/__tests__/utils/terminalRendererPolicy.test.ts`：覆盖 isLinux × hasBackgroundImage × route 全组合
- [x] `TerminalPreview.vue` 初始化时改用 `decideRenderer` 的结果驱动 `allowTransparency` 与 `initWebGL` 调用
- [x] 透明度状态变化时重建渲染器（dispose → 重设 options → 重新 initWebGL → 重设 theme → 重算尺寸 → 全量重绘）
- [x] 重建路径同步 `xterm-hidden-cursor` 的加/删
- [x] 组件卸载/会话切换时清理重建产生的 addon 引用，不泄漏
- [x] 回归：默认（无背景图）场景零残影不变
- [x] 验证命令：`cd bedcode-desktop && pnpm run test:run`

## Comments

- 前置决策：**spec §D-2 的路线 A / 路线 B 需用户拍板**。两条路线下本票都必须做（见 spec Further Notes）。
- 升级 xterm 前必须重新核对 `_setTransparency` 是否已被接线（`rg -c "setTransparency" node_modules/@xterm/addon-webgl/lib/addon-webgl.mjs`，当前返回 `1` = 仅定义）。那是本票修法的前提。
- **竞态保护必做**（spec D-1 复核补充）：快速连续切换背景图会并发两次重建，context-loss 的 1s 异步恢复回调可能覆盖新 addon。照抄 VS Code `_webglAddonLoadId` 递增守卫（`xtermTerminal.ts:901` / `:1039`）；实现时也可参考 VS Code `_enableWebglRenderer`（`:888-947`）的「先 dispose 再重建」模式。
