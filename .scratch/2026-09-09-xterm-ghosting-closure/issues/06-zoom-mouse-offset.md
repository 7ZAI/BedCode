# Issue 06：opencode TUI 鼠标坐标偏移 —— 根因 = Linux 全局 CSS zoom，方案 = 去 zoom 换 --ui-scale/font-size 体系

> 状态：根因已实锤（实测数据），方案 1（治本）已在 dev 分支实施（style.css + useFontSize.ts + TerminalPreview.vue，另同步 zoom-compensation.ts / main.ts 注释），待真机走查（验证清单 2-6）。
> 发现时间：2026-09-09。实测环境：Deepin 25 (X11) + WebKitGTK，屏幕 1.25x 分数缩放（`~/.config/deepin/qt-theme.ini: ScreenScaleFactors=1.25`）。

## 现象

opencode TUI 支持鼠标操作：鼠标悬停位置与 TUI 选中高亮不一致，**高亮恒在鼠标下方 1~2 行**（用户在 opencode 列表区观察；VS Code 内置终端中同一 opencode 完全正常 → opencode 自身无问题，问题在本 app 渲染管线）。

## 根因（已实锤）

### 一句话

`bedcode-desktop/src/style.css` 的 `html.platform-linux { zoom: 1.15; }`（Linux-only 全局 CSS zoom，观感补偿 hack）让 xterm.js 的**鼠标 hit-test 坐标系**与**字符测量坐标系**进入不同空间，行号 = 真实视觉行 × 1.15。

### 机制（xterm v6.0.0 源码级）

1. **字符测量无视布局缩放**：`CharSizeService` 优先走 `OffscreenCanvas + ctx.measureText("W")`（纯字体度量，`fontBoundingBoxAscent + fontBoundingBoxDescent`），完全不受 CSS zoom/transform 影响 → `dimensions.css.cell.height = 14.4`（12px 字号的 raw 度量，未缩放）。实测 `cssCellH: 14.4`。
2. **渲染行高在未缩放空间，视觉被 zoom 放大**：DOM 渲染器按 `css.cell.height`（14.4px）布局行，html zoom 1.15 把整棵渲染树放大 → 屏幕上每个视觉行高 = `14.4 × 1.15 = 16.56`。实测行 div `rowDivH: 16.56`。
3. **鼠标 hit-test 用视觉空间**：`getCoordsRelativeToElement` = `clientY - screenElement.getBoundingClientRect().top - paddingTop`，除以 `css.cell.height`（未缩放的 14.4）→ `row = 视觉偏移 / 14.4 = 真实行 × 1.15`。rect 与 clientY 都是视觉空间（含 zoom），分母是布局空间 → **错位因子恒 = zoom 值**。
4. 方向与用户观察一致：公式行 > 视觉行 → 高亮在鼠标下方。

### 实测证据（frontend 日志 [mouseDiag] 插桩，worktree 会话）

```
cssCellH=14.4  rowDivH=16.56  zoom="1.15"  dpr=1.25  rows=47
formulaRow=17.36  visualRow=15.09  deltaRows=2.26   ← 中部
formulaRow=28.53  visualRow=24.82  deltaRows=3.72   ← 下部
formulaRow=3.89   visualRow=3.38   deltaRows=0.51    ← 顶部
```
- `deltaRows ≈ 0.15 × visualRow` **严格线性**（16.56/14.4 = 1.15 精确等于 zoom）→ 比例偏移，用户感知"固定 1~2 行"是 opencode 列表集中在屏幕上中部的假象。
- **zoom=1 实验**（临时改 css）→ `deltaRows→0`，用户确认鼠标与高亮完全对齐 → **zoom 是唯一因素**（dpr=1.25 分数缩放不参与；fit/测量/渲染在 zoom 下自洽，断点只在鼠标公式分母）。

### 同类问题外部佐证

- xterm.js issue #6023：字符测量在 #2488 后 transform-agnostic，但鼠标 hit-test 仍用 post-transform `getBoundingClientRect` → 祖先 transform/zoom 下坐标偏移。
- clawpier commit：`document.documentElement.style.zoom` 全局缩放破坏 xterm 鼠标坐标（选中/光标偏移）。
- WebKit bug 77998：CSS `zoom` 下 `getBoundingClientRect` 返回值异常。

## 方案选型（已选方案 1：治本）

| 方案 | 内容 | 取舍 |
| --- | --- | --- |
| 1（选） | 去掉全局 zoom，UI 缩放改走既有体系：`--ui-scale`（px token 字号）×1.15 + `html font-size: 115%`（rem 间距/控件）×1.15 | 根治整类坐标 bug（不只终端）；观感 ≈ zoom 前（等价性 ~97%，仅 40 处硬编码 px 尺寸不放大）；改动集中 3 文件 |
| 2 | 保留 zoom，仅 monkey-patch xterm MouseService 坐标除以 zoom | 改动最小但 hack 内部 API（升级脆弱），只修终端，其他像素命中场景仍踩坑 |
| 3 | 保留 zoom，terminalHostRef 反 zoom（zoom: 1/1.15）+ fontSize×1.15 | 视觉零变化但 WebKit offsetWidth 语义依赖实测（fit 行数可能漂移），仍只修终端 |

**用户观察支持方案 1**：Chrome/VS Code 等成熟产品在 Linux 下初始 UI 也偏小、不做全局 zoom 放大——"Linux UI 小"是可接受的平台常态，用字号体系补偿即可。

## 实施细节（dev 分支）

### 改动 1：`bedcode-desktop/src/style.css`

```css
/* 删除 */
html.platform-linux {
  zoom: 1.15;
}
/* 替换为：rem 体系（Tailwind 间距/控件/图标）放大，等效 zoom 对 rem 的效果 */
html.platform-linux {
  font-size: 115%;
  --ui-scale: 1.15; /* JS important 生效前的 CSS 默认（splash 期），JS 就绪后覆盖一致 */
}
```
注释同步更新（zoom 破坏 xterm 鼠标坐标系，改用 font-size + --ui-scale 体系）。

### 改动 2：`bedcode-desktop/src/composables/useFontSize.ts`

- 导出 `export const PLATFORM_UI_SCALE = 1.15`（Linux 平台基线因子；TerminalPreview 复用）
- `applyFontSize`：`scale = (clamped / NORMAL_FONT_SIZE) × (platformInfo.value.isLinux ? PLATFORM_UI_SCALE : 1)`
- **竞态根除**：`platformInfo`（`usePlatform` 模块级 ref）未初始化（platform===null）时**不设置** `--ui-scale`（交给 CSS 默认），watch `platformInfo.value.isLinux` 就绪后重算。`useFontSize.setupFontSize` 在 App.vue onMounted 调用，早于 main.ts 异步 `initPlatform` 的 then —— 不可依赖调用顺序。

### 改动 3：`bedcode-desktop/src/components/TerminalPreview.vue`（基于 dev 版，无残影修复）

- xterm `fontSize` 传参在 Linux ×`PLATFORM_UI_SCALE`（视觉字号 = 设置值 × 1.15 × 无 zoom = 原 zoom 等效；行高度量 16.56 → 视觉行数保持 47 不变）
- 所有 fontSize 使用点同步 ×scale：`initTerminal` 构造、`watch(fontSize)` 更新 `terminal.options.fontSize`、`computeDesktopInitialTerminalSize(fontSize.value)` 调用处
- 用户设置值（settingsStore `terminal_font_size`）与 Select 显示保持原值，不做乘法

### 不放大项（可接受，走查确认）

- 硬编码 px 尺寸类仅 40 处（`p-[3px]`×9、`w-[76px]`、`w-[168px]` 等零星）不随体系放大；明显不协调处走查时个别微调。
- xterm 自带 CSS 全 px，不受 font-size 影响。

## 验证清单

1. `pnpm run test:run` 通过（主 worktree 全量前端测试）
2. 启动 `pnpm run tauri:dev`（主 worktree，先确认 8765 空闲）：主界面/设置页/终端窗口观感走查（字号/间距/控件整体 vs zoom 前，无溢出）
3. 终端窗口：`rows` 保持 47、字符清晰（无位图放大模糊）
4. 终端内跑 opencode，悬停列表：**高亮与鼠标对齐**（用户确认，终验）
5. 临时插桩 `[mouseDiag]`（若复用）确认 `deltaRows≈0`
6. 用户调 `ui.font_size` 档位（10/14/16）与终端字号档位，缩放比例仍正确

## 收尾事项

- worktree `BedCode-xterm-ghosting-closure`（feature/xterm-ghosting-closure）：删除 TerminalPreview.vue 的 `[mouseDiag]`/`[mouseDiagSgr]` 临时插桩；该 worktree 的 26 个未提交文件（残影修复 + Channel 传输 + 调试代码）按原计划另行合并，与本 issue 改动无重叠（style.css 在 dev 是原始版，无冲突）
- 本 issue 全部改动在 dev 分支完成，与残影专项合并时互不干扰

## 排查中的有用命令/路径

- 前端日志（debug 构建 console 转发）：`~/.local/share/com.bedcode.app/logs/frontend.YYYY-MM-DD.log`
- xterm 鼠标公式（v6.0.0）：`node_modules/@xterm/xterm/lib/xterm.js` 模块 5251 `getCoordsRelativeToElement` / `getCoords`；`MouseService.getCoords` 用 `dimensions.css.cell.*`
- 窗口操作（X11）：`xdotool search --name BedCode` / `getwindowgeometry` / `mousemove`
- 屏幕分数缩放：`~/.config/deepin/qt-theme.ini` `ScreenScaleFactors`
