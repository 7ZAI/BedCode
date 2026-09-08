# 02 — viewport 背景按透明状态条件化（P0）

**What to build:** 切断第二条残影通路。当前 `:deep(.xterm .xterm-viewport) { background-color: transparent }` 无条件透明；该元素覆盖整个终端区域、位于背景图层之上、渲染画布之下。当渲染层处于 alpha 模式时，任何「被清但未被完全覆盖」的 cell 变成透明洞（`_clearCells` 的 alpha 分支走 `clearRect`，见 plan.md §1 环 1；常规更新只重绘变化行，见 §1 环 2），洞透出该层即残影。

改为随透明度状态切换（镜像 xterm 6.1 的 `.xterm:not(.allow-transparency) .xterm-viewport { background-color:#000 }` 机制，在 6.0 结构上实现，见 plan.md §7.3）：在终端容器上由 Vue `:class` 绑定一个语义类，CSS 用 `:deep(<class> .xterm-viewport) { background-color: transparent }`，非透明时不覆盖（继承 xterm.css 6.0 的 `#000`）。

**不要**给 xterm 自己创建的 `terminal.element` 手动 `classList.toggle`——容器类由 Vue 管理，组件重建后自动正确，不需要在初始化/watch 里同步 DOM 状态。

**Spec:** §D-3

**Blocked by:** 01（复用其 `decideRenderer` 输出的 `allowTransparency` 判定；串行避免组件并发编辑冲突）

**Status:** open

- [ ] 终端容器 `:class` 绑定语义类（如 `terminal-transparent`），键为 `decideRenderer(...).allowTransparency`
- [ ] CSS 改为条件选择器，删除无条件 `background-color: transparent` 覆盖
- [ ] 更新 `TerminalPreview.vue` 中该 CSS 块的注释（当前注释说明 6.0 下无条件透明是被迫的，需同步为条件化语义）
- [ ] 加载 `frontend-styles` skill 并按其规范自检：`:deep()` 仅用于第三方 DOM 覆盖（本场景符合）、token-bound、无反模式
- [ ] 回归：无背景图场景背景不透明、文字清晰
- [ ] 验证命令：`cd bedcode-desktop && pnpm run test:run`

## Comments

- 本票与 01 号票共同覆盖 07 号票遗留项「透明模式（背景图开启）下滚动/刷新的残影问题未根除」。
- **注意**：即使本票落地，alpha 模式下的透明洞依然存在（只是不再透出第二层）。scrollback 行不在 `refresh(0, rows-1)` 覆盖范围内（plan.md §3），无法用补丁补救。因此本票在路线 A 下只能减轻、不能根除——这也是 spec §D-2 建议路线 B 的核心理由。若选路线 B（背景图强制 DOM 渲染器），本票的 CSS 条件化仍应保留（作为防御性一致性），但透明模式下的残影问题随之消失。
