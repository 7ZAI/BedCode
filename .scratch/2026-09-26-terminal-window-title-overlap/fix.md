# 终端窗口内容侵占 40px 工具条（窗体标题栏）修复

**日期**：2026-09-26
**报告**：桌面端终端窗口，用户反馈「迁移为 wasm 应用后第一次测试就这样」——
xterm 内容向上溢出到顶部 40px 工具条，第一行被工具条裁掉（工具条本身显示正常）。

## 根因（三条独立路径，同一症状：内容高于可视区 → 视口保持"滚动到底" → 顶部被裁）

### A. 字体测量口径：预测与渲染不一致（Linux）

- **预测**：`src/utils/terminalInitialSize.ts` 用**设置原值字号**（`terminal_font_size`，默认 12）
  + **Windows 字体栈**（Cascadia Mono / Consolas / Monaco…）测量 cell；
- **渲染**：插件 `TerminalPreview.initTerminal` 在 Linux 上用**字号 × PLATFORM_UI_SCALE(1.15)**
  + **系统等宽栈**（`LINUX_FONT_STACK`，DejaVu Sans Mono 优先）。

后果：预测 cell 偏小 → 行数偏多。实测日志（2026-09-25 18:38:10）PTY 初始
`cols=88 rows=84`，而 1080px 窗口扣 64px chrome 后按真实 cell（13.8px 行高）只容 ~73 行
→ 首帧内容高于容器 → 第一行被工具条裁掉。

### B. 首帧字体重测后未重算网格（Linux 专项缺口）

`useTerminalRenderer.scheduleInitialFontRemeasure`（WebKitGTK 首帧用回退字体指标，
300ms 后重测修正 cell 尺寸，原为修"首帧文字发蒙"）旧实现只 `measure()` + `refresh()`，
**不重算网格**：行列数永久停留在按错误（偏小）cell 算出的偏多值。ResizeObserver 只在
容器尺寸变化时触发——用户不动窗口则**永不自愈**，与"一直如此"吻合。

### C. 双高度声明歧义（结构性隐患）

`TerminalWindowView` 给 `TerminalPreview` 传 `class="flex-1 min-h-0"`，而组件根自带
`h-full`（`height:100%`）——同一元素双高度声明。规范上 flex item 主轴尺寸由 `flex-basis`
（`flex-1` → `0%`）决定、`height` 应被忽略，但 WebKit 实测行为不可依赖。包一层定高容器后
内层 `h-full` 的参照系恒为「剩余高度」，A/C 两类根因同时收敛。

## 修复（3 处）

1. **`src/utils/terminalInitialSize.ts`**：新增纯函数 `resolveTerminalCellFont(fontSize, isLinux)`
   —— Linux 字号 × `PLATFORM_UI_SCALE` + `LINUX_FONT_STACK`（**按值复制**插件
   `terminalThemes.ts` 真源，宿主不 import 插件）；`computeDesktopInitialTerminalSize`
   经 `@tauri-apps/plugin-os` 的 `platform()` 判定平台后走该口径；文件头补口径说明。
2. **`wasm-apps/terminal-session/src/composables/terminal/useTerminalRenderer.ts`**：
   `scheduleInitialFontRemeasure` 末尾 `terminal.refresh(...)` → `ctx.callbacks.fitAndRefresh()`
   （内含 applyDprFit 重算行列 + 整屏重绘；行列变化经 `onResize` → PTY resize 同步）。
3. **`wasm-apps/terminal-session/src/views/terminal/TerminalWindowView.vue`**：
   终端区包一层 `div.flex-1.min-h-0.relative`，`TerminalPreview` 传 `h-full`
   （该组件在插件内唯一使用点就是这里，改动面最小）。

## 验证

- 新增 `src/__tests__/utils/terminalInitialSize.test.ts`（4 用例）：Linux 口径正例 /
  非 Linux 反例（Linux 因子不得泄漏）/ **宿主 ↔ 插件 `LINUX_FONT_STACK` 逐字一致锁**
  （读源码正则提取，防空串恒真；防双份真源漂移）/ 非法字号边界（0、负数、NaN → null）。
- `wasm-apps/terminal-session/src/__tests__/terminalRendererResize.test.ts` 增 3 用例：
  Linux 首帧重测后**重算网格**（回归锁：断言 `terminal.resize(100, 25)`，而非只 refresh）/
  非 Linux 不重测（反例）/ element 断开（边界）。
- **变异自检**：① 重测后改回只 `refresh` → 回归锁转红；② Linux 字号不乘 1.15 → C1 转红；
  ③ 宿主字体栈改一字符 → C3 一致性锁转红；还原后 16/16 绿。
- 相关面：`wasm-apps/terminal-session/src/__tests__` 20 文件 / 199 用例绿；
  `src/__tests__/utils` + `src/__tests__/plugin` 在范围内同跑绿；前端全量（`--maxWorkers=2`）
  结果见当日 memory `.codebuddy/memory/2026-09-26.md`。

## 待用户复验

本修复覆盖「预测口径 / 首帧重测 / 容器高度」三条路径，但布局效果需真机目视。若复验后
仍有顶行被裁，下一步在 `applyDprFit` 加一次性诊断日志（`host.clientHeight` / xterm cell /
`cols×rows`）落盘比对，区分容器高度与 cell 尺寸的残余偏差。

## 顺带核实（未改）

- `wasm-apps/terminal-session/src/utils/terminal/terminalInitialSize.ts` 是一份**无生产引用**
  的宿主副本（插件实际走 `context.session.predictTerminalSize()` → 宿主实现）。本次未同步
  口径修复；如将来被误用会重新引入同类偏差，建议后续删除或加「以宿主实现为准」注释。

---

## 第二轮（用户复验反馈）：按钮文字换行 + 顶行仍被裁

**用户截图现象**：① 标题栏右侧插件扩展点按钮「自动任务」文字换行、溢出 40px 工具条；
② 终端第一行顶部仍被工具条裁掉约 0.3 行高（三条路径修复后残余）。

### 按钮文字 / 布局（根因明确）

- `TerminalWindowView.vue`：会话名块 `shrink-0`（不可收缩）→ 窗口偏窄时被压缩的是右侧按钮区
  → 多字标签换行。修复：左信息区可收缩（`truncate` 生效）、右操作区 `shrink-0`；
  扩展点按钮统一 **24px 高 + `calc(11px*var(--ui-scale))`**（原固定 `text-xs`=12px，不随
  `--ui-scale` 缩放）+ `whitespace-nowrap` + `shrink-0`；颜色 token 化
  （`text-slate-500 dark:text-dark-400` → `text-[var(--text-secondary)]` 等）。
- 宿主 `PluginPageToolbar.vue` / `PluginTitleBarItems.vue` 同步同款（主窗口标题栏共用同一渲染面）。

### 视口—容器高度收敛（残余顶行被裁加固）

- 第一轮覆盖「预测口径 / 首帧重测 / 双高度声明」三条路径，但不覆盖**cell 测量值与渲染实际
  行高的亚像素差**（Linux 分数缩放 / DPR 取整）：`rows × 实际行高` 仍可能比容器高零点几到几像素，
  视口保持「滚动到底」即把顶行挤出可视区。
- `useTerminalRenderer.ts` 新增 `convergeViewportOverflow()`，接入 `applyDprFit`（含
  `fitAddon.fit()` 降级分支与 ±1 漂移抑制分支，均不短路）：实测 `.xterm-screen` 高 vs 容器高，
  超出容差 1px 且视口在底部 → 逐行削减 rows（≤2 行/次）自愈；用户上滚查看历史时不干预；
  收敛发生时 `logger.warn` 输出 `host 高 / rows / cols / dpr`（真机复验定位用）。
- 单测 +6：正例（溢出 → 削 1 行）/ 无溢出反例 / 上滚边界 / rows=1 边界 / 防抖上限 /
  `element` 无 `querySelector` 环境降级。变异自检：去掉「视口在底部」检查、放开 rows≤1 保护
  → 两条边界用例精确转红，还原后 18/18 绿。
- 回归：终端插件前端 20 文件 205 用例绿；宿主 `src/__tests__/plugin` + `terminalInitialSize`
  22 文件 180 用例绿；`eslint .` 0 error（改动文件无新增告警）。

### 构建链踩坑（本次触发，未改脚本）

`wasm-apps/terminal-session/scripts/build.js --frontend-only` 的 `copyArtifacts()` **先
`rmSync(RESOURCES_DIR)` 再复制**；WASM 缺失时 `process.exit(1)` → 随包目录被留在
「有前端、无 WASM」半残状态（`src-tauri/resources/plugins/**` 被 .gitignore 忽略，git 无法恢复）。
**重建务必跑全量 `node scripts/build.js`（或先 `--rust-only`）**，勿单独跑 `--frontend-only`。

### 真机复验观测点

重跑 `pnpm run tauri:dev`（默认插件 watch 会重建前端产物；WASM 需按上节重建）。若顶行仍被裁，
控制台会出现 `[TerminalPreview] 视口画布高于容器 Xpx，削减 1 行收敛 (host=… rows=… cols=… dpr=…)`
——把该行数值回传即可定位残余偏差；若**没有**该日志且仍被裁，说明偏差未被实测口径识别，
需改用其他实测口径（如比对 `.xterm-rows` 最后一行元素底边）。

---

## 第三轮（用户复验 04:2x）：底部黑缝 + 内容上移

**现象**：① 终端内容与窗口下边框之间多出空隙（黑缝）；② 应显示在最后几行的文字被推到
视口顶部（标题栏区域）；③ IDE 面板（webview console 转发）出现
`[terminal-session] resize 命令失败，回退 applied: {}`。

**像素级实测**（X11 抓屏 2560×1440，dpr≈1.5，`ffmpeg -f x11grab` + raw 灰阶逐行统计）：

| 区域 | y 范围（物理 px） | 换算 |
| --- | --- | --- |
| 标题栏 | 151..208（58px） | ≈40 逻辑 × 1.45 |
| 终端区 | 209..1335（1127px） | 容器 ≈751 逻辑 |
| 状态条 | 1336..1369（34px） | ≈24 逻辑 |
| 终端行距 | 22px | ≈14.7 逻辑（字号 12×1.15 + DejaVu fontBoundingBox ≈1.164em） |

关键测量：首行文字顶部距终端区顶 **6px**（≈0.27 行，非整行）→ 内容整体上移。

**判定（两个独立根因）**：

1. **非整行的像素级滚动残留**：xterm 6 的滚动容器是像素级 ScrollableElement
   （`smoothScrollDuration` 的存在即证据），清屏（`scrollOnEraseInDisplay`）/ resize 组合下
   可残留非整行 scrollTop（此处 ≈6px）→ 内容整体上移 → 首行被容器上沿裁掉、底部空出同宽。
   **上一轮的"削行"是错误方向**：削行不修滚动偏移，只把"顶部被裁"变成"底部多一整行空白"
   （= 用户看到的黑缝），且每轮 fit 可能继续削 → 上移 + 黑缝并存。
2. **resize 命令偶发失败**：`resize_via_host` 依赖插件侧会话登记 / PTY 句柄（窗口挂载与
   登记落库存在竞态）；旧实现首次失败即回退 applied → PTY 永久停留初始预测尺寸，
   前端格网与 PTY 不一致（底部多空白行）。

**修复（2 处）**：

1. `useTerminalRenderer.convergeViewportOverflow` 重构为**两层、先软后硬**：
   - 层 1（软）：实测 `host.top - screen.top > 1px` 且视口在底部 → `terminal.scrollToBottom()`
     把 scrollTop 重置为「整行 × cellH」（不改变行数）；
   - 层 2（硬）：实测 `.xterm-screen` 高 > 容器高 → 才逐行削减 rows（≤2 行），每轮后重新钉底；
   - 视口不在底部（用户看历史）一律不干预；诊断日志升级为完整快照
     （`offsetTop / overflowHeight / host / rows / cols / dpr / bufferLen / viewportY / baseY`）。
2. `TerminalPreview.requestResizeImpl`：首次失败 → **300ms 后重试一次**；错误描述函数
   `describeCommandError` 把非 Error 对象渲染为 message / JSON（旧日志只显示 `{}` 无法定位）。

**验证**：渲染域 20 用例 + 组装层 8 用例全绿（新增 3 条：滚动残留校正正例 / 用户上滚反例 /
resize 重试进入裁决链路→弹窗）；**变异自检 2 条**（去掉层 1 校正 → 滚动残留用例红；
去掉重试 → resize 用例红；还原后全绿）；终端插件前端全量 **20 文件 208 用例**绿；
产物 `--frontend-only` 重建（WASM 就位，复制阶段安全）。

**复验观察点**：devtools / IDE 面板出现 `[TerminalPreview] 视口收敛(滚动残留|画布超容) …`
快照日志 → 回传即定位；若无该日志但现象仍在，说明偏差未被实测口径捕获，需换判据。

---

## 第四轮（用户复验 04:4x：现象依旧「顶部裁切 + 底部空白」）

现象与第三轮同形（新开窗口、运行 22s），用户猜测「PTY 输出渲染顺序混乱」。两个推断：

1. **输出期间校正缺失**：层 1 只在 fit（applyDprFit）时执行；持续输出（pi 每帧写屏 +
   `scrollOnEraseInDisplay`）会把滚动位置反复推回非整行残留态，fit 之后的输出立即重建偏移。
2. **实测口径/接线未知**：需要运行时快照才能判断「快照未触发（口径没捕获）vs 反复触发
   （校正被覆盖）vs resize 失败（PTY 与前端网格不一致）」。

修复/加固（3 处）：

- `useTerminalRenderer`：层 1 拆为 `settleViewport(force)`（节流 250ms），`convergeViewportOverflow`
  以 force 调用它 + 层 2；快照日志补 `scrollTop`（读 `.xterm-scrollable-element`）。
- `terminalKernel` + `TerminalPreview`：callbacks 新增 `settleViewport` 槽，
  `terminal.onWriteParsed` → `kernel.callbacks.settleViewport()`（写入解析后按节流校正）。
- `scripts/dev-run.js`：子进程 `stdio: inherit` → 管道 + tee，落盘 `.dev-logs/dev-run.YYYY-MM-DD.log`
  —— webview 裸 console（Tauri dev 转发 stdout）从此可回读，后续定位无需截图。

验证：新增 settleViewport 节流用例（首次 true / 窗口内 false / force true）；
**20 文件 209 用例**绿；`node --check dev-run.js` ✓；eslint 0 error；产物重建（04:44）。

判读指引（下次复验读 `.dev-logs/dev-run.YYYY-MM-DD.log`）：

| 日志形态 | 推断 | 下一步 |
| --- | --- | --- |
| `视口收敛(滚动残留…)` 反复出现 | 输出持续推回偏移，校正被覆盖 | 深入 xterm 滚动实现（ScrollableElement）找根因 |
| 无快照但现象仍在 | 实测口径未捕获 | 换判据：`.xterm-rows` 首行/末行元素 rect 比对 |
| `resize 命令失败（已重试）` | PTY 与前端网格不一致 | 查 `resize_via_host` 失败原因（登记竞态/句柄/权限） |
