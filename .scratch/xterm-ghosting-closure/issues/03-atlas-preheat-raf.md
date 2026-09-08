# 03 — atlas 预热改 rAF 迭代（P1）

**What to build:** 替代固定 700ms 猜数。当前 `ATLAS_PREHEAT_DELAY_MS = 700`（`terminalResizePolicy.ts`）是 resize 重建字符图集后等 idle 队列把非 ASCII 字形光栅化完的固定延时——低配机不够、高性能机浪费。

依据（plan.md §4）：`TextureAtlas.warmUp()` 只预热 ASCII 33-125（`for (let i = 33; i < 126; i++)`，含 33 不含 126；xterm 源码注释写作 33-126）且走 `IdleTaskQueue`；`WebglRenderer.handleResize()` 尾部调 `_refreshCharAtlas()` 清空并重建整个图集；而 `beginFrame()` 在 atlas 页合并时返回 true 会触发 `_clearModel(true)` + 全量重绘。因此**迭代刷新能自然跟上光栅化进度**。

改为 rAF 驱动的有界迭代（上限约 8 帧）：每次迭代 `terminal.refresh(0, rows-1)`，元素脱离 DOM 或组件销毁时停止，已有迭代在跑时不重复启动。

**Spec:** §D-4

**Blocked by:** None — can start immediately.

**Status:** open

- [ ] `terminalRendererPolicy.ts` 增加 `decideAtlasRefreshFrames(prev)` → 下一个迭代的帧预算判定（返回 `0` 表示停止）；或在既有 `terminalResizePolicy.ts` 增补，保持纯函数
- [ ] 对应单测：帧预算递减、到 0 停止、不重复启动
- [ ] `TerminalPreview.vue` 的 `scheduleAtlasPreheat` 改用 rAF 迭代，删除 `ATLAS_PREHEAT_DELAY_MS` 依赖
- [ ] 迭代在 `terminal.element?.isConnected === false` 或组件销毁时中止
- [ ] 回归：拖窗后中文 / box-drawing / emoji 一帧内逐步完整，无需点击刷新
- [ ] 验证命令：`cd bedcode-desktop && pnpm run test:run`

## Comments

- 依赖 Linux 走 DOM 渲染器（`LINUX_USE_DOM_RENDERER = true`）的既有事实：Linux 无 atlas，本票对 Linux 是 no-op，不应在 Linux 分支启动迭代。
- 迭代上限（8 帧 ≈ 133ms @60fps）是从「idle 队列首批任务 + 一次页合并」的经验值，属可调参数；若真机观察到字形恢复仍需更多帧，调上限而非改回固定延时。
