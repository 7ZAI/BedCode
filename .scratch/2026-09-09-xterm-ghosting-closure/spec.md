# xterm 透明模式残影收敛与渲染质量对齐 — Spec

Status: ready-for-agent

> 本规格是 `.scratch/xterm-render-optimizations/spec.md`（01-06 已落地、08 作废）的**后续专项**。其 07 号票（平滑滚动重影根因调查，已 resolved）在 Answer 中明确留了遗留项：「透明模式（背景图开启）下滚动/刷新的残影问题未根除」。本规格收敛该遗留项，并纳入本轮深挖新发现的 `allowTransparency` 运行时不生效缺陷、xterm 选项缺口与 VS Code fork 依赖评估。
>
> 完整调查证据（含 xterm 源码文件与行号、npm 版本对照、fork 实测输出）见同目录 `plan.md`。**实现者读 plan.md 即可复现全部结论，无需重新调查。**

## Problem Statement

桌面端终端的默认（无背景图）场景已通过上一轮规格消除残影：`allowTransparency` 改为条件开启，WebGL 在不透明模式下每帧正常清帧。但**只要用户开启终端背景图，三个问题同时出现**，且其中两个是本轮深挖才发现的真缺陷：

1. **背景图透不出来（功能性 bug，非观感）**：`allowTransparency` 在构造时读取一次，渲染层的 alpha 标志与 canvas 的 `{ alpha }` 属性在 `getContext('2d', {...})` 之后不可变。xterm 里确实有一个处理运行时切换的方法，但它在 BedCode 使用的 addon-webgl 0.19.0 中**没有任何调用者**（编译产物中该标识符只出现 1 次，仅定义）。因此"无图 → 开图"切换后渲染层仍是 non-alpha，`getTheme()` 返回的 `rgba(0, 0, 0, 0)` 背景在 non-alpha canvas 上被 premultiply 成黑色 → **背景图完全不可见**。反向切换"开图 → 关图"则 layer 仍是 alpha，`clearRect` 继续产生透明洞，仅靠容器底色兜底才看不出。
2. **透明模式下仍有残影（07 遗留项）**：`.xterm-viewport` 被无条件设为 `background-color: transparent`。该元素覆盖整个终端区域、位于背景图层之上、渲染画布之下。当 layer 是 alpha 模式时，任何"被清但未被完全覆盖"的 cell 变成透明洞，透出该层——叠加背景图即为残影与"行入侵"。
3. **`refresh(0, rows-1)` 只覆盖视口**：xterm 的整屏重绘请求是 `{ start: 0, end: rows-1 }`，scrollback 行不在其内。所有现有补丁（`armReplayRefresh`、`scheduleAtlasPreheat`、刷新按钮）都只修可见行，向上滚动的历史行里的透明洞原样保留。

另有三项非阻塞的渲染质量缺口：atlas 预热用固定 700ms 猜数替代状态判定；xterm 有 8 个选项未设置（含 PuTTY 式清屏语义、DA1 能力应答）；resize 防抖缺少 VS Code 的"窗口不可见时走 idle 回调"分支。

最后是依赖问题：**VS Code 自家 fork 的 xterm 能否下载直接使用、是否推荐**。

## Solution

不采用 VS Code fork 作为产品依赖，改为在应用层修根：

- **P0 `allowTransparency` 切换重建渲染器**：透明度状态变化时 dispose WebGL addon 并重建，让 canvas 的 alpha 属性与 layer 的 alpha 标志真正改变。同时把"是否需要透明"收敛成一个纯决策函数，使判定可测。
- **P0 viewport 背景按透明状态条件化**：不再无条件透明，改为随透明度状态切换（镜像 xterm 6.1 的 `allow-transparency` 类机制，在 6.0 结构上实现），彻底切断第二条残影通路。
- **P1 背景图场景改走 DOM 渲染器（备选根治方案，需用户选择）**：DOM 渲染器无 texture atlas、无 alpha 分支、无帧缓冲优化，透明天然正确。代价是 TUI 输出吞吐下降。两条路二选一，见 Implementation Decisions §D-2。
- **P1 atlas 预热改 rAF 迭代**：替代 700ms 固定延时，迭代刷新直到稳定。
- **P2 xterm 选项对齐**：补齐 `scrollOnEraseInDisplay`、`windowOptions`、`scrollbar` 等 8 项。
- **P2 resize 防抖补不可见窗口分支**：对齐 VS Code 的 `runWhenWindowIdle` 路径。
- **依赖决策**：fork 不采纳；其透明度 CSS 改进的**思路**吸收进 P0（见上），其依赖不引入。理由与实测见 plan.md「fork 评估」。

## User Stories

1. 作为桌面终端用户，我希望在设置里开启终端背景图后背景图能正确显示，这样装饰功能可用而不是设置一个看不见的开关。
2. 作为桌面终端用户，我希望开启背景图后终端文字依然清晰、没有残留的旧内容，这样背景图不会带来观感退化。
3. 作为桌面终端用户，我希望在背景图开启状态下运行 opencode / vim 等 TUI 应用时整屏重绘不留下残影，这样全屏交互程序可用。
4. 作为桌面终端用户，我希望在背景图开启状态下执行 `cat` 大文件、构建输出等大流量输出时不出现残影，这样大输出场景与默认场景体验一致。
5. 作为桌面终端用户，我希望关闭背景图后终端立刻回到干净的不透明渲染，没有背景图透出的痕迹，这样开关切换是干净往返的。
6. 作为桌面终端用户，我希望从"有背景图"切到"无背景图"后向上滚动查看历史不看到残影，这样开关切换不留后患。
7. 作为桌面终端用户，我希望默认（无背景图）场景继续保持零残影，这样上一轮的修复不被本轮改动回归。
8. 作为桌面终端用户，我希望切换终端主题（default / dracula / oneDark / ...）时在透明与不透明两种模式下都正确显示，这样主题选择不受背景图状态影响。
9. 作为桌面终端用户，我希望调整背景图不透明度滑块时终端实时反映变化，这样不透明度是可预览的。
10. 作为桌面终端用户，我希望拖动窗口尺寸后中文、box-drawing（╔═╗║╚╝）、emoji 等非 ASCII 字符立即完整显示，不需要多点几次刷新，这样拖窗后终端立刻可用。
11. 作为桌面终端用户，我希望点击"刷新格式"按钮后字形一次到位，这样刷新按钮的语义与我的预期一致。
12. 作为桌面终端用户，我希望用 Ctrl+滚轮切换字号后字符立即完整、行尾不截断，这样字号调节即时可用。
13. 作为桌面终端用户，我希望把窗口拖到不同缩放比例的屏幕（或调整系统缩放）后字符完整、行列数精确，这样多显示器工作流不损坏显示。
14. 作为桌面终端用户，我希望 TUI 应用的清屏行为与 VS Code / PuTTY 一致（擦除内容进入 scrollback 而非只清视口），这样全屏程序清屏后不会出现内容错乱。
15. 作为桌面终端用户，我希望老式终端程序的能力探测（DA1 / DSM 查询）得到正确应答，这样它们不会探测超时进入降级路径。
16. 作为桌面终端用户，我希望终端滚动条始终可见且可拖动，这样我能发现并使用真正的滚动条。
17. 作为桌面终端用户，我希望在窗口被最小化或切到后台后再恢复时终端尺寸正确、没有残留旧尺寸，这样切换焦点不需要手动刷新。
18. 作为桌面终端用户，我希望 WebGL 上下文丢失后自动恢复期间光标行为一致、不出现双光标或光标消失，这样渲染器故障对用户不可见。
19. 作为桌面终端用户，我希望在滚动到历史区时旧行没有透明洞或错位字符，这样回看历史是干净的。
20. 作为桌面终端用户，我希望上述优化对移动端远程终端无副作用（尺寸裁决与正统渲染端语义不变），这样桌面端优化不破坏移动端体验。
21. 作为维护者，我希望"是否需要透明"与"该用哪个渲染器"是可单测的纯判定，这样这类渲染缺陷不会以隐性方式回归。
22. 作为维护者，我希望保留一份带源码行号的调查记录，这样未来 xterm 升级后能判断结论是否仍成立。
23. 作为维护者，我希望明确记录 VS Code fork 不采纳的理由，这样后续讨论依赖时不必重新评估。

## Implementation Decisions

### D-1 `allowTransparency` 切换必须重建渲染器（P0）

透明度状态变化时（`bgImageUrl` 有无变化），不能只改 `terminal.options.allowTransparency` + `refresh()`——该选项在 addon-webgl 0.19.0 中无运行时监听，layer 的 alpha 标志与 canvas 的 `{ alpha }` 属性在 `getContext` 之后不可变。

正确路径：**dispose WebGL addon → 重设 `terminal.options.allowTransparency` → 重新初始化 WebGL addon → 重设 theme → 重算尺寸 → 全量重绘**。WebGL 与 DOM 渲染器 cell 尺寸不同，重建后必须重算行列（现有代码在 context loss 恢复路径已有同款处理，可复用）。

**VS Code 先例（实测）**：`xtermTerminal.ts:888-947` 的 `_enableWebglRenderer` 正是「先 `_disposeOfWebglRenderer()`（`:1038-1054`）再重建」的模式——dispose 注释原文 "Dispose of existing addon before creating a new one to avoid leaking WebGL contexts"，重建后 fire `_onDidRequestRefreshDimensions`，注释原文 "WebGL renderer cell dimensions differ from the DOM renderer, make sure the terminal gets resized after the webgl addon is loaded"。本票的重建路径与之一致。

**竞态保护（初稿遗漏，必须补）**：快速连续切换背景图（无图→开→关）会并发两次重建，而 context-loss 的 1s 异步恢复回调（`TerminalPreview.vue:730-752`）可能与新 addon 互相覆盖。照抄 VS Code 的 `_webglAddonLoadId` 递增守卫（`xtermTerminal.ts:901` / `:1039`）：组件持重建序列号，旧序列号的回调一律丢弃。

重建时必须同步 `xterm-hidden-cursor` 的加/删，否则出现双光标或光标消失。

**判定逻辑抽为纯函数**：输入 `{ isLinux, hasBackgroundImage, linuxUseDomRenderer }`，输出 `{ useWebgl, allowTransparency }`。见 Testing Decisions 的缝合点。

### D-2 背景图场景渲染器选择：两条路线二选一（P1，需用户决策）

- **路线 A（保留透明 + 修 D-1）**：背景图场景继续用 WebGL，靠 D-1 的重建修复 alpha 切换。优点：大输出吞吐不变。风险：透明模式下的残影本质是 alpha 帧缓冲 + 部分行更新（`renderRows` 只 `_updateModel(start, end)`，不全屏）的结构问题，`refresh(0, rows-1)` 覆盖不到 scrollback，无法完全根除。
- **路线 B（背景图场景强制 DOM 渲染器）**：扩展 D-1 的纯函数，`hasBackgroundImage === true` 时 `useWebgl = false`。DOM 渲染器无 texture atlas、无 alpha 分支、无帧缓冲优化，透明天然正确，**两条残影通路一次性消失**，代码从"条件透明"简化为"二选一"。代价：背景图场景下 TUI 输出吞吐下降（背景图是装饰功能，可接受）。

**建议路线 B**：它把结构性的渲染器问题变成确定性的选择，且让 D-1 只剩一个边界（构造时的初始透明度），维护成本显著更低。默认场景（无背景图）不受影响，仍走 WebGL。

### D-3 viewport 背景按透明状态条件化（P0）

当前 `:deep(.xterm .xterm-viewport) { background-color: transparent }` 无条件透明，在 xterm 6.0 下是被迫的（6.0 的 xterm.css 对 `.xterm-viewport` 恒设 `background-color:#000`）。改为**随透明度状态切换**，镜像 xterm 6.1 的 `.xterm:not(.allow-transparency) .xterm-viewport { background-color:#000 }` 机制，在 6.0 结构上实现。

实现方式：在终端容器上由 Vue `:class` 绑定一个语义类（如 `terminal-transparent`），CSS 用 `:deep(.terminal-transparent .xterm-viewport) { background-color: transparent }`，非透明时不覆盖（继承 xterm.css 的 `#000`）。**不要**给 xterm 自己创建的 `terminal.element` 手动 `classList.toggle`——容器类由 Vue 管理，组件重建后自动正确，不需要在初始化/watch 里同步 DOM 状态。

`scoped` + `:deep()` 用于第三方 DOM 覆盖是本项目允许的用法（frontend-styles skill 明确"scoped `<style>` for animations and third-party overrides"）；禁用的是对自己子组件用 `:deep()`，这里不适用。

### D-4 atlas 预热改 rAF 迭代（P1）

`ATLAS_PREHEAT_DELAY_MS = 700` 是固定猜数。xterm 的 atlas 在页合并时会通过 `beginFrame()` 触发全量重绘，因此**迭代刷新能自然跟上光栅化进度**。改为 rAF 驱动的有界迭代（上限约 8 帧），组件销毁/元素脱离 DOM 时停止，已有迭代在跑时不重复启动。

### D-5 xterm 选项对齐（P2）

补齐 VS Code 有、BedCode 没有的选项：

```
scrollOnEraseInDisplay: true      // PuTTY 式清屏：ED 序列擦除内容进入 scrollback，而非只清视口
windowOptions: {                  // 使 xterm 应答 DA1/DSM 能力查询，老 TUI 不探测超时
  getWinSizePixels: true,
  getCellSizePixels: true,
  getWinSizeChars: true,
}
wordSeparator: ' ()[]{}\',"`─‘’“”|'   // = VS Code 默认值（实测 terminalConfiguration.ts:503，修订：初稿值少了 ─‘’“”|）
tabStopWidth: 8
minimumContrastRatio: 1
scrollSensitivity: 1.0
fastScrollSensitivity: 5.0
```

**`scrollbar` 选项不在本票范围（修订）**：xterm 6.0.0 **没有** `scrollbar` 选项（typings 与 OptionsService 默认值均无；只有 3 个 slider 主题色 `scrollbarSliderBackground/Hover/Active`，xterm `Viewport.ts:84-91` 自绘滚动条实际使用）。初稿建议的 `{ useOverlay: true, width: 10 }` 在 6.0 下是静默 no-op；且 VS Code 的真实值也不是 `{ useOverlay, width }`，而是 `{ width, overviewRuler: { showTopBorder: true } }`（fork-only，`xtermTerminal.ts:566-580`，全仓无 `useOverlay`）。BedCode 滚动条常显已由 CSS 覆盖，本项不可落地也不必要；若未来升级 xterm 6.1+ 再评估 `scrollbar.width` 与 overview ruler。

`smoothScrollDuration` 维持 `0`——沿用 07 号票的决策，不在本规格内重开。

### D-6 resize 防抖补不可见窗口分支（P2）

VS Code 的 `TerminalResizeDebouncer.resize()` 有一条 BedCode 没有的分支：**窗口不可见时，X 与 Y 各自走 `runWhenWindowIdle`**，而非立即应用或普通计时器防抖。

BedCode 现状：不可见时尺寸变化照旧（高度立即 `onApply`、宽度 100ms 计时器）。虽然已有 `visibilitychange` / `focus` 的 `flush()` 兜底，恢复可见时尺寸会兑现，但**不可见期间的中间状态仍会触发 `onApply`**（含 `applyDprFit` 的测量与可能的 resize），属无谓开销且依赖恢复时的 flush 正确。

对齐做法：在 `TerminalResizeDebouncer` 增加一个注入式 `isVisible` 判定（组件传入），不可见时挂起应用并推迟到窗口空闲，可见时保持现有分层逻辑不变。保持纯模块零 DOM 依赖的既有约定（Seam A）。

### D-7 不采用 VS Code fork 的 xterm（依赖决策）

**可以下载吗？可以。** VS Code 的 xterm 以 `-beta.NNN` 标签公开发布在公共 npm：

| 包 | BedCode 现状 | VS Code pin | npm 最新 |
|---|---|---|---|
| `@xterm/xterm` | `6.0.0` | `6.1.0-beta.292` | `6.1.0-beta.304` |
| `@xterm/addon-webgl` | `0.19.0` | `0.20.0-beta.291` | `0.20.0-beta.300` |

`npm pack @xterm/addon-webgl@0.20.0-beta.300` 可正常下载，LICENSE 与上游一致（MIT / The xterm.js authors）。

**推荐吗？不推荐作为产品依赖。** 五条理由：

1. **它没修我们要修的 bug**。实测 `0.20.0-beta.300` 编译产物中 `_setTransparency` 仍只出现 1 次（仅定义、零调用）；该 addon 唯一的选项变更监听是 `blinkIntervalDuration`，没有 `allowTransparency`。WebGL 层的 alpha 仍然是构造时定型。升级不解决核心问题。
2. **是公共 prerelease 的内部产物**。`-beta.292` → `-beta.304` 说明发布节奏活跃，但也意味着没有 semver 稳定承诺；它是 VS Code 的内部构建产物恰好公开在 npm，可能随时改发布方式。锁死一个别人的 prerelease 内部产物，依赖风险不成比例。
3. **有破坏性 CSS 变更需全量审计**。对比 xterm 6.0.0 与 6.1.0-beta.304 的 css 类名集合：`.xterm-char-measure-element` 被移除（换 `.xterm-scra`），新增 `.xterm-scrollbar` / `.xterm-shadow*` / `.xterm-arrow-down` / `.xterm-fade` / `.xterm-visible` / `.xterm-invisible`。BedCode 现有的 `:deep(.xterm-scrollable-element > .scrollbar.vertical)` 与 `:deep(.xterm-viewport)` 覆盖需要逐一核对。
4. **addon 必须整线同升**。`@xterm/addon-*` 与 core 版本强耦合，升级 xterm 意味着 fit / unicode11 / web-links / webgl 四个 addon 全部同升，回归面扩大。
5. **不解决 Linux WebKitGTK IME bug**（第 5 条，2026-09-09 复核补充，证据见 plan.md §7.6）。fork 的 CompositionHelper 改进（selection 光标定位、`_compositionSuffix` 后缀截断、229 差值补发防重入）全是 **Chromium 世界观**——VS Code 跑 Electron/Chromium（Linux 下也是 Chromium/Ozone），**从不经过 WebKitGTK**；而 Tauri 2 Linux 的 webview 是 webkit2gtk，BedCode 的三个 IME bug（compositionstart 事件丢失 → start 位置漂移重复发送、textarea 不在提交后清空、229 差值补发把已提交内容当新输入）fork **一个都没根治**（丢 start 时 suffix 也全错、textarea 照样不清、差值补发只是防重入非消除）。升级到 6.1+ 只可能让 `terminalLinuxImeGuard` 的③（清空 textarea）与 fork 的 suffix 机制协同简化，guard 本身必须保留。

**吸收思路，不引入依赖**：fork 真正有价值的改进是**把 viewport 背景做成 `allow-transparency` 类驱动的条件逻辑**（xterm core 注册 `onSpecificOptionChange("allowTransparency")` 切换类，css 用 `.xterm:not(.allow-transparency) .xterm-viewport` 条件着色）。D-3 在 6.0 结构上实现同一机制。

### D-8 不做的事

- 不引入 `@xterm/addon-serialize` / `@xterm/headless`：VS Code 用它做本地会话恢复（headless xterm 重放输出 + serialize dump）。BedCode 的历史回放走服务端 TB v2 `seq` 快照重播 + 环形队列，能力更强且已实现，不需要抄。
- 不引入 `@xterm/addon-search` / `@xterm/addon-clipboard` / `@xterm/addon-progress` / `@xterm/addon-image` / `@xterm/addon-ligatures`：属功能差距而非显示问题，各自独立评估，不混入本规格。
- 不重开 `smoothScrollDuration`（沿用 07 号票决策）。
- 不动移动端。桌面端与移动端共用 PTY 尺寸，但尺寸裁决（正统渲染端）与 `requestResize` 语义不变。

## Testing Decisions

**什么算好测试**：只测外部可观察行为与纯判定逻辑，不测 xterm 内部实现。渲染正确性无法在无头环境断言像素，因此测试聚焦"决策是否正确"——即给定平台与背景图状态，是否选了正确的渲染器与透明度。像素级验证归真机冒烟（见 plan.md 的验证清单）。

**缝合点（seams）**：优先复用既有 Seam A（`src/utils/terminal*.{ts}` 纯逻辑模块 + `src/__tests__/utils/*.test.ts`），**不新增接缝层**。

- **Seam A（既有，最高）**：新增 `src/utils/terminalRendererPolicy.ts`——纯函数，零 DOM 依赖，含：
  - `decideRenderer({ isLinux, hasBackgroundImage, linuxUseDomRenderer, route })` → `{ useWebgl, allowTransparency, useDom }`
  - `decideAtlasRefreshFrames(prev)` → 下一个迭代的帧预算判定（返回 `0` 表示停止）
  - 复用既有 `shouldApplyGridResize`（±1 钳制）与 `terminalDimensions.ts` 的 `getXtermScaledDimensions`，不重复实现
- **Seam B（既有）**：`TerminalResizeDebouncer` 已有单测 `src/__tests__/utils/terminalResizeDebouncer.test.ts`，扩展覆盖注入式 `isVisible` 分支（不可见时挂起、可见时兑现、flush 语义不变）。
- **不做组件级渲染测试**：`TerminalPreview.vue` 的重建渲染器、`classList` 切换、rAF 迭代属接线，归真机验证，不写组件测试（现有 `terminal-flow.test.ts` 是集成风格，不扩它）。

**prior art**：

- `src/utils/terminalResizePolicy.ts` + `src/__tests__/utils/terminalResizePolicy.test.ts`——纯决策函数 + 单测，本规格 D-1/D-2/D-4 的新模块直接照此模式。
- `src/utils/terminalDimensions.ts` + `terminalDimensions.test.ts`——纯函数几何计算。
- `src/utils/terminalLinuxImeGuard.ts` + `terminalLinuxImeGuard.test.ts`——xterm 内部行为补丁的纯逻辑抽取（与本次 D-1 的"判定抽纯函数"同款手法）。
- `src/utils/terminalResizeDebouncer.ts` + 对应单测——带可选注入（`getBufferLength`）的纯模块，D-6 的 `isVisible` 注入照此扩展。

**验证命令**（AGENTS.md 强制字眼）：前端 `cd bedcode-desktop && pnpm run test:run`；涉及 Rust 背压路径才跑 `cargo test`（本规格不动 Rust，除非 D-5 的 `windowOptions` 引发尺寸协商变化）。

## Out of Scope

- **移动端**：全部优化仅桌面端。移动端的远程渲染、尺寸仲裁、正统渲染端裁决语义不变。
- **平滑滚动**：沿用 07 号票「维持 `smoothScrollDuration: 0`」的决策，不重开讨论。若未来要引入，需先有透明模式平滑滚动的专项处理（07 号票 Answer 已注明）。
- **Rust 侧 PTY 背压**：04/05/06 号票已落地，本规格不动。
- **功能类 addon**（search / clipboard / progress / image / ligatures）：独立评估，不并入。
- **xterm fork 的实际接入**：本规格只做决策记录（D-7），不做升级实施。若要实施，需独立 spec 覆盖全量 addon 同升与 CSS 覆盖审计。
- **背景图的加载链路**（本地静态端点 `/static/terminal-bg`、预加载校验）：已实现，不动。

## Further Notes

- **证据密度高，见 `plan.md`**：所有结论都带 xterm 源码文件与行号，以及 npm 版本实测输出。实现者不需要重新调查；但**升级 xterm 后必须重新核对** `_setTransparency` 是否被接线——那是本规格 D-1 的前提。
- **两个真缺陷的严重度不对称**：D-1（背景图透不出，功能性 bug）> D-3（透明模式残影，观感）。D-1 即使选路线 B（背景图强制 DOM）也需要修——因为初始构造时的 `allowTransparency` 仍要正确，且"开图→关图"后 layer 的 alpha 残留依然需要重建才能清除。**D-1 在两条路线下都必须做。**
- **D-2 需要用户拍板**：路线 A（保留透明 + 修切换）与路线 B（背景图强制 DOM）是产品取舍，不是技术取舍。实施前需确认。**建议 B**，理由见 D-2 末段。
- **`terminal.clear()` 语义已核实**：xterm 6 的 `clear()` 文档为 "Clear the entire buffer, making the prompt line the new first line"——清的是整个 buffer 含 scrollback，**不是** scrollback 残影来源。`onReset` 时的 `terminal.clear()` + 全量重播是安全的，无需改动。
- **前端 UI/CSS 改动前置要求**：D-3 涉及 `:deep()` 覆盖与类绑定，实施前必须加载 `frontend-styles` skill 并按其规范自检（token-bound、无反模式、无 `:deep()` 用于自身子组件）。
- **AGENTS.md 纪律**：本规格未改任何产品代码，仅新增 `.scratch/` 文档。目标文件均 >200 行，探索过程走的是 `module_report` / `read_enclosing` / `read` 定点读，未整文件吞读。
