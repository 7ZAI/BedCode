# 调查记录：xterm 残影链路逐环验证（2026-09-09）

本文件是 `.scratch/xterm-ghosting-closure/spec.md` 的证据底座。**所有结论都带源码文件与行号**，实现者可直接复现，无需重新调查。

对照对象：`/home/binblink/Documents/vscode-main/src/vs/platform/terminal/` 与 `src/vs/workbench/contrib/terminal/`（VS Code 主干 checkout，2026-09-09 已换为该完整仓库；原 `/media/binblink/Data/vscode-main` 与 `Documents/terminal/common` 旧目录中的同名文件经 diff 确认与本目录逐字节一致），以及 BedCode 安装的 `@xterm/xterm@6.0.0` + `@xterm/addon-webgl@0.19.0`。

> **复核记录（2026-09-09，两轮）**：第一轮逐条对照旧 checkout；第二轮（本行）用户提供了完整仓库 `/home/binblink/Documents/vscode-main/`，对全部引用逐条重验——`xtermTerminal.ts` / `terminalInstance.ts` / `terminalResizeDebouncer.ts` 与新仓库逐字节一致，§3 的 `onScroll` 消费点（suggest :258 / stickyScroll :180 / links :179）、§6 的构造 options（:240-283 内 scrollOnEraseInDisplay=:270、windowOptions=:279-283、_getScrollbarOptions=:566-580 返回 `{ width, overviewRuler }`、updateConfig=:587-624）、D-1 的 `_enableWebglRenderer`=:888 / `_disposeOfWebglRenderer`=:1038 / loadId=:901/:1039、§5 防抖常量与不可见分支、wordSeparators 默认值（terminalConfiguration.ts:503-504）全部命中，无一漂移。修正两处实质性错误——§3 的「VS Code `terminalInstance.ts` 的 `onScroll` 重绘监听」引文**不存在于本 checkout**（已换为真实机制）；§6 的 `scrollbar` 选项值 `{ useOverlay, width }` 为误记（VS Code 实际是 `{ width, overviewRuler }`，且 xterm 6.0.0 根本没有该选项，spec D-5 相应修订）。其余结论（含 fork 评估的 npm 版本、beta300 死代码、allow-transparency 类机制、CSS 类 diff）全部复核成立，仅行号按实际文件微调。

---

## 1. 残影因果链（四环，逐环有源码依据）

### 环 1 — cell 清除方式取决于 alpha

`node_modules/@xterm/addon-webgl/src/renderLayer/BaseRenderLayer.ts:142-172`（`_clearAll` 142-156；`_clearCells` 158-172）

```ts
protected _clearAll(): void {
    if (this._alpha) {
        this._ctx.clearRect(0, 0, this._canvas.width, this._canvas.height);   // ← 清成透明
    } else {
        this._ctx.fillStyle = this._themeService.colors.background.css;
        this._ctx.fillRect(0, 0, this._canvas.width, this._canvas.height);    // ← 清成背景色
    }
}
```

`_clearCells(x, y, w, h)` 同款分支。`_alpha` 由构造时 `this._canvas.getContext('2d', { alpha: this._alpha })` 定型。

### 环 2 — 常规更新只重绘变化行，不是全屏

`node_modules/@xterm/addon-webgl/src/WebglRenderer.ts:323-361`（`beginFrame` 全量分支在 346-355）

```ts
public renderRows(start: number, end: number): void {
    for (const l of this._renderLayers) {
        l.handleGridChanged(this._terminal, start, end);          // ← 只处理 start..end
    }
    if (this._glyphRenderer.value.beginFrame()) {
        this._clearModel(true);
        this._updateModel(0, this._terminal.rows - 1);            // 仅 atlas 页合并 / 模型清空时全量
    } else {
        this._updateModel(start, end);                             // ← 常态：只更新变化行
    }
    this._rectangleRenderer.value.renderBackgrounds();
    this._glyphRenderer.value.render(this._model);
}
```

### 环 3 — atlas 纹理本身也带 alpha

`node_modules/@xterm/addon-webgl/src/TextureAtlas.ts:100`

```ts
alpha: this._config.allowTransparency,
```

### 环 4 — BedCode 自己的 CSS 让下面一层也透明

`bedcode-desktop/src/components/TerminalPreview.vue:1525-1527`

```css
:deep(.xterm .xterm-viewport) { background-color: transparent; }
```

该元素覆盖整个终端区域，位于背景图层之上、渲染画布之下（同处注释已说明）。

**四环叠加**：`allowTransparency: true` 时，任何"被清但未被完全覆盖"的 cell 变成透明洞，透出背景图层或旧内容 → 残影 + 行入侵。与 `xterm-render-optimizations` 07 号票的调查结论一致，本轮把每一环都定位到源码。

---

## 2. 新缺陷 A：`allowTransparency` 运行时切换不生效（P0）

### 2.1 事实

`bedcode-desktop/src/components/TerminalPreview.vue:704-714`

```ts
watch([bgImageUrl, bgOpacity], () => {
  if (terminal) {
    terminal.options.theme = getTheme()
    terminal.options.allowTransparency = !!bgImageUrl.value
    terminal.refresh(0, terminal.rows - 1)
  }
})
```

### 2.2 xterm 里确实有处理该切换的方法——但没有调用者

`node_modules/@xterm/addon-webgl/src/renderLayer/BaseRenderLayer.ts:70-91`

```ts
protected _setTransparency(terminal: Terminal, alpha: boolean): void {
    if (alpha === this._alpha) return
    const oldCanvas = this._canvas
    this._alpha = alpha
    this._canvas = this._canvas.cloneNode() as HTMLCanvasElement
    this._initCanvas()
    this._container.replaceChild(this._canvas, oldCanvas)
    this._refreshCharAtlas(terminal, this._themeService.colors)
    this.handleGridChanged(terminal, 0, terminal.rows - 1)
}
```

该方法会正确换 canvas、重建 atlas、全量重绘——**但它是死代码**。

**实测（addon-webgl 0.19.0，BedCode 当前版本）**：

```
$ rg -c "setTransparency" node_modules/@xterm/addon-webgl/lib/addon-webgl.mjs
1        ← 只有定义，零调用
```

xterm core 侧（`@xterm/xterm@6.0.0`）`allowTransparency` 只出现在三处（类型声明 ×2 + 默认值 ×1），且都不是监听器：

- `typings/xterm.d.ts:40` — 公共 API 类型声明
- `src/common/services/Services.ts:225` — 内部类型声明
- `src/common/services/OptionsService.ts:45` — 默认值 `false`

### 2.3 后果

canvas 的 `{ alpha }` 属性在 `getContext('2d', {...})` 之后不可变。

- **无图 → 开图**：layer 仍 `_alpha=false`、canvas 是 `alpha:false`。`getTheme()` 返回 `background: 'rgba(0, 0, 0, 0)'`（`TerminalPreview.vue:649-656`），在 non-alpha canvas 上被 premultiply 成黑色 → **背景图完全不可见**（功能性 bug，非观感）。
- **开图 → 关图**：layer 仍 `_alpha=true`，`clearRect` 继续产生透明洞，仅靠容器 `containerBgColor`（`TerminalPreview.vue:659-662`，不透明主题色）兜底才看不出。

### 2.4 正确修法

必须重建渲染器（dispose WebGL addon → 重设 options → 重新 initWebGL → 重设 theme → 重算尺寸 → 全量重绘）。现有 `initWebGL` 的 context-loss 恢复路径（`TerminalPreview.vue:730-752`）已有同款"重建后重算尺寸"处理，可复用。重建时必须同步 `xterm-hidden-cursor` 的加/删，否则出现双光标或光标消失。

---

## 3. 新缺陷 B：`refresh(0, rows-1)` 不覆盖 scrollback（07 遗留）

`node_modules/@xterm/addon-webgl/src/WebglRenderer.ts:621-623`

```ts
private _requestRedrawViewport(): void {
    this._onRequestRedraw.fire({ start: 0, end: this._terminal.rows - 1 });
}
```

整屏重绘请求的边界就是视口行。`terminal.refresh(0, rows-1)` 同理。因此：

- `armReplayRefresh`（回放静止补刷）只修可见行
- `scheduleAtlasPreheat`（atlas 预热补刷）只修可见行
- "刷新格式"按钮（`TerminalPreview.vue:1188-1192`）只修可见行
- **scrollback 的透明洞无法被任何补丁触达**（机制澄清，修订初稿的表述）：滚入视口的历史行其实会被 xterm 内部滚动重绘重建（见上 CoreBrowserTerminal 的 onRequestScrollLines → refresh 全视口），但透明主题下 buffer 空白 cell 的默认背景就是 `rgba(0,0,0,0)`，RectangleRenderer 照画透明——洞是 buffer/主题层面的属性，重建多少次都是洞，不是 refresh 边界能修的；未滚入视口的行则连重建都不会发生。

VS Code 侧对照（vscode-main 主干 checkout 实测复核，2026-09-09）：**并不存在** `terminalInstance.ts` 的 `onScroll` 补丁式重绘——初稿记录的这段引文有误，vscode-main 全仓检索无 `scrollDisposable` 且 terminal 组件无此注释。事实是：

- `terminalInstance.ts` 只在视图位置变更（`:913` `xterm.refresh()`）与 `forceRefresh`（`:1079`）时手动 refresh，没有滚动补丁；
- `onScroll` 在 VS Code 中仅被三个 contrib addon 消费：`terminalContrib/suggest/browser/terminalSuggestAddon.ts:258`（滚动隐藏 suggest 浮层）、`terminalContrib/stickyScroll/browser/terminalStickyScrollOverlay.ts:180`（sticky scroll 联动）、`terminalContrib/links/browser/terminalLinkManager.ts:179`（滚动清除链接 hover）——全部是 UI 联动，不是重绘；
- 滚动后的视口重绘由 xterm 内部自驱动：`@xterm/xterm/src/browser/CoreBrowserTerminal.ts:513-517` 里 `_viewport.onRequestScrollLines → scrollLines + refresh(0, rows-1)`（`Viewport.ts:188` 在滚动时 fire 该事件）。

BedCode 现状一致（不手动 refresh，交给渲染循环）——差异不在补丁，而在于**透明模式下渲染循环本身就会产生洞**，两者都无法补救。根治只能走"不透明"或"DOM 渲染器"。

---

## 4. atlas 预热：700ms 猜数 vs 迭代

### 4.1 xterm 只预热 ASCII

`node_modules/@xterm/addon-webgl/src/TextureAtlas.ts:113-133`（`warmUp` 113-118；`_doWarmUp` 120-133）

```ts
public warmUp(): void {
    if (!this._didWarmUp) { this._doWarmUp(); this._didWarmUp = true; }
}

private _doWarmUp(): void {
    // Pre-fill with ASCII 33-126, this is not urgent and done in idle callbacks
    const queue = new IdleTaskQueue();
    for (let i = 33; i < 126; i++) { ... }
}
```

CJK / box-drawing / emoji 全部按需异步光栅化，走 idle 队列。

### 4.2 resize 会重建整个 atlas

`node_modules/@xterm/addon-webgl/src/WebglRenderer.ts:199-203`（`handleResize` 尾部：`:199` `_refreshCharAtlas()` + `:203` `_clearModel(false)`）

```ts
this._refreshCharAtlas();
this._clearModel(false);
```

即每次真实 resize 都清空并重建字符图集。`terminalResizePolicy.ts` 的 `ATLAS_PREHEAT_DELAY_MS = 700` 是为此猜的固定延时。

### 4.3 更好的做法

`beginFrame()` 的机制（见 §1 环 2）：atlas 页合并时返回 true → 触发 `_clearModel(true)` + 全量重绘。所以**迭代刷新能自然跟上光栅化进度**，比固定 700ms 稳。改为 rAF 驱动的有界迭代（上限约 8 帧），元素脱离 DOM 或组件销毁时停止。

---

## 5. resize 防抖：BedCode 缺 VS Code 的不可见窗口分支

`vscode-main/src/vs/workbench/contrib/terminal/browser/terminalResizeDebouncer.ts`

```ts
// Resize in an idle callback if the terminal is not visible
const win = getWindow(this._getXterm()!.raw.element);
if (win && !this._isVisible()) {
    if (!this._resizeXJob.value) {
        this._resizeXJob.value = runWhenWindowIdle(win, async () => {
            if (this._store.isDisposed) return
            this._resizeXCallback(this._latestX)
            this._resizeXJob.clear()
        });
    }
    if (!this._resizeYJob.value) {
        this._resizeYJob.value = runWhenWindowIdle(win, async () => { ... });
    }
    return
}
```

BedCode 的 `terminalResizeDebouncer.ts` 有三条分支（小缓冲立即 / 高度变化立即 / 宽度 100ms 防抖），**没有不可见窗口分支**。已有 `visibilitychange` / `focus` 的 `flush()` 兜底（`TerminalPreview.vue:849-856` 附近），恢复可见时尺寸会兑现；但不可见期间的中间状态仍会触发 `onApply`，属无谓开销。

`StartDebouncingThreshold = 200` 与 `DebounceResizeXDelay = 100` 两个参数 BedCode 已对齐（注释中明确标注）。

---

## 6. BedCode 未设置的 xterm 选项（对照 VS Code）

VS Code 在 `workbench/contrib/terminal/browser/xterm/xtermTerminal.ts:240-283`（构造 options 块）与 `:587-624`（`updateConfig`）设置约 30 项。BedCode 未设的关键项：

| 选项 | VS Code 值 | 作用 |
|---|---|---|
| `scrollOnEraseInDisplay` | `true` | PuTTY 式清屏：ED 序列擦除内容进入 scrollback，而非只清视口。xterm 6 typings:255-260 注释（原文在 :258）："This emulates PuTTY's default clear screen behavior" |
| `windowOptions` | `{ getWinSizePixels, getCellSizePixels, getWinSizeChars }` 全 true | 使 xterm 应答 DA1/DSM 能力查询，老 TUI 不探测超时 |
| `scrollbar` | `{ width, overviewRuler: { showTopBorder: true } }`（实测 `xtermTerminal.ts:566-580` `_getScrollbarOptions`，**无 `useOverlay`**） | 仅 fork 6.1+ 才有该选项；**xterm 6.0.0 无 `scrollbar` 选项**（typings 与 OptionsService 均无，只有 3 个 slider 主题色 `scrollbarSliderBackground/Hover/Active`，见 `Viewport.ts:84-91`）——BedCode 6.0 下不可落地，滚动条常显已由 CSS 覆盖（§7.4 类名审计不受影响） |
| `wordSeparator` | `config.wordSeparators` | 右键选词边界 |
| `tabStopWidth` | `config.tabStopWidth` | tab 宽度 |
| `minimumContrastRatio` | `config.minimumContrastRatio` | 最低对比度 |
| `scrollSensitivity` | `config.mouseWheelScrollSensitivity` | 滚轮灵敏度 |
| `fastScrollSensitivity` | `config.fastScrollSensitivity` | 快速滚动灵敏度 |
| `vtExtensions` | `{ kittyKeyboard, win32InputMode }` | kitty 键盘协议 / win32 输入模式 |
| `rescaleOverlappingGlyphs` | `config.rescaleOverlappingGlyphs` | 重叠字形缩放 |

已对齐项：`allowTransparency`（条件开启）、`customGlyphs: true`、`unicode11`、`allowProposedApi: true`、`rightClickSelectsWord`、`altClickMovesCursor`、`drawBoldTextInBrightColors`、`scrollback`。

### `scrollOnEraseInDisplay` 的特殊意义

这是唯一与"残影/内容错乱"直接相关的选项。TUI 应用大量使用 ED（Erase Display）序列，默认 false 时只清视口，全屏程序清屏后可能残留内容。VS Code 显式设 true（`xtermTerminal.ts:270`）；xterm 6.0.0 typings 注释在 `:255-260`（原文 `:258`）。

---

## 7. VS Code fork 的 xterm 评估

### 7.1 可以下载

`npm view @xterm/xterm versions` 显示 `-beta.NNN` 系列公开发布在公共 npm：

```
6.1.0-beta.281 ... 6.1.0-beta.304   （VS Code pin 6.1.0-beta.292）
0.20.0-beta.282 ... 0.20.0-beta.300 （addon-webgl，VS Code pin 0.20.0-beta.291）
```

`npm pack @xterm/addon-webgl@0.20.0-beta.300` 成功（537KB）。LICENSE 与上游一致（MIT / The xterm.js authors）。

### 7.2 实测：它没有修我们要修的 bug

```
$ rg -c "setTransparency" pkg-webgl-beta300/lib/addon-webgl.mjs
1        ← 仍然只有定义、零调用
```

`0.20.0-beta.300` 中该 addon 唯一的选项变更监听：

```
his._optionsService.onSpecificOptionChange("blinkIntervalDuration",a=>{this.setIntervalDuration(a)})
```

只有 `blinkIntervalDuration`，**没有 `allowTransparency`**。WebGL 层的 `_alpha` 在 fork 中同样是构造时定型。

`_clearCells` 的 alpha 分支在 fork 中完全不变：

```
_clearCells(t,r,s,a){this._alpha?this._ctx.clearRect(...):(this._ctx.fillStyle=this._themeService.colors.background.c...)}
```

**结论：升级 fork 不解决 D-1。**

### 7.3 fork 真正有价值的改进：CSS 类驱动的透明度

xterm `6.1.0-beta.304` 在 core 里注册了监听（且 `open()` 时按初始值 toggle 一次，两处均在 minified `lib/xterm.mjs` 中实测可见）：

```
// open() 时：this.element.classList.toggle("allow-transparency", this.options.allowTransparency)
this._register(this.optionsService.onSpecificOptionChange("allowTransparency",
    l=>this.element.classList.toggle("allow-transparency",l)))
```

配套 CSS（`pkg-xterm-beta304/css/xterm.css:103-106`）：

```css
.xterm:not(.allow-transparency) .xterm-viewport {
    /* On OS X this is required in order for the scroll bar to appear fully opaque */
    background-color: #000;
}
```

即：**viewport 背景随透明度状态条件化**，不再无条件透明。这正是 BedCode 的 D-3 要吸收的思路——在 6.0 结构上自行实现同一机制（`:class` 绑定 + `:deep()` 条件选择器）。

BedCode 当前的无条件透明覆盖：

```css
:deep(.xterm .xterm-viewport) { background-color: transparent; }
```

在 6.0 下是被迫的（6.0 的 xterm.css 对 `.xterm-viewport` 恒设 `#000`，见 `TerminalPreview.vue:1519-1524` 注释）。但它把"透明模式才该透明"变成了"永远透明"，是第二条残影通路的放大器。

### 7.4 破坏性 CSS 变更（升级需全量审计）

`diff <(rg -o "\.xterm[a-z-]*" xterm 6.0.0/css/xterm.css | sort -u) <(rg -o "\.xterm[a-z-]*" 6.1.0-beta.304/css/xterm.css | sort -u)`：

```
< .xterm-char-measure-element      ← 移除
> .xterm-scra                      ← 替代（measure element 改名）
> .xterm-arrow-down
> .xterm-fade
> .xterm-invisible
> .xterm-scrollbar                 ← 新滚动条类
> .xterm-shadow
> .xterm-shadow-left
> .xterm-shadow-top
> .xterm-shadow-top-left-corner
> .xterm-visible
```

BedCode 现有的 `:deep(.xterm-scrollable-element > .scrollbar.vertical)`（`TerminalPreview.vue:1539-1542`）针对的是 6.0 的自绘滚动条类结构，在 6.1 下可能失效。

### 7.5 不推荐的理由（汇总）

1. **没修目标 bug**（§7.2）
2. **公共 prerelease 的内部产物**：`-beta.292` → `-beta.304` 说明发布活跃，但也无 semver 稳定承诺；锁死别人的 prerelease 内部产物，依赖风险不成比例
3. **CSS 破坏性变更需全量审计**（§7.4）
4. **addon 必须整线同升**：fit / unicode11 / web-links / webgl 四个 addon 全部同升，回归面扩大

**决策：不引入依赖，吸收透明度 CSS 的思路（D-3）。**

---

## 8. 顺带核实（避免误判）

### 8.1 `terminal.clear()` 不是 scrollback 残影来源

xterm 6 typings `Terminal.d.ts:1235-1238`：

```
/**
 * Clear the entire buffer, making the prompt line the new first line.
 */
clear(): void;
```

清的是整个 buffer 含 scrollback。`onReset` 时的 `terminal.clear()` + 全量重播安全，无需改动。

### 8.2 主题背景全为不透明 hex

`TerminalPreview.vue:490-545` 的 `terminalThemes`（default / dracula / oneDark / ...）全部用 `#RRGGBB`。只有 `getTheme()` 在 `bgImageUrl` 存在时覆盖为 `rgba(0, 0, 0, 0)`。所以不透明路径的 `_clearCells` → `fillRect(不透明色)` 是干净的。

### 8.3 `LINUX_USE_DOM_RENDERER = true`

`TerminalPreview.vue:219`。Linux 已走 DOM 渲染器，彻底绕开 WebGL atlas 发蒙问题。这是路线 B（背景图强制 DOM）的先例。

---

## 9. 真机验证清单（像素级无法单测，需人工/冒烟）

D-1 / D-3 落地后按此顺序验证：

1. 无背景图（默认）：大输出、拖窗、字号切换、向上滚动历史 → 应零残影（回归）
2. 开启背景图 → 背景图应可见（当前是黑的，这是 bug 修复的可观测标志）
3. 背景图 + TUI（opencode / vim / htop）整屏重绘 → 无残影
4. 背景图 + `cat` 大文件 → 无残影
5. 关闭背景图 → 立即回到干净不透明渲染，向上滚动历史无残留
6. 主题切换（两种透明度状态下各切一次）
7. 背景图不透明度滑块调节 → 实时反映
8. Windows / macOS / Linux（Linux 走 DOM，验证不回归）
