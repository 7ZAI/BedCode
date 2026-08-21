# xterm 渲染输出优化 — 建议落地计划（对标 VS Code 实现）

> 来源：2026-08-21 探索 VS Code（D:/vscode-main）对 xterm 渲染输出的优化（`src/vs/workbench/contrib/terminal/browser/xterm/xtermTerminal.ts`、`terminalResizeDebouncer.ts`、`terminalInstance.ts`、`terminalProcessManager.ts`）后，对照 BedCode 桌面端现状整理的可落地优化项。
> 结论一句话：**WebGL 渲染器等核心渲染优化 BedCode 已在用，真正值得补的是「resize 分层」「背压反馈环」「像素/DPR 精度」这类节制层优化。**

## VS Code 源码基准路径

本文件所有「参考（VS Code 源码）」均相对：`D:/vscode-main/src/vs`（即 `vscode-main` 仓库的 `src/vs` 目录），行号为 2026-08-21 核对。

## 现状对照（BedCode 桌面端代码事实）

| 维度 | VS Code | BedCode 现状（TerminalPreview.vue / useTerminalOutputStream.ts / pty_reader.rs） |
|---|---|---|
| WebGL 渲染器 | addon-webgl + 纹理图集 + DOM 兜底 | ✅ 已用，且多做 context loss 1s 后重建 + 回退 DOM |
| unicode11 | 按需加载 | ✅ 已用（`unicode.activeVersion='11'`） |
| 写入管线 | 直写 xterm + 解析后 ack 背压 | ✅ 合并 write + 分块 + 让出主线程（`flushWriteQueue`/`writeInChunks`）；**无背压** |
| smoothScroll | 物理滚轮才开（125ms） | ⚠️ 关到 0（WebGL 滚动重影） |
| resize | 垂直立即 / 水平 100ms debounce | ⚠️ rAF 节流 + fit 后条件全量 refresh，无水平 reflow 防抖 |
| 像素精度 | getXtermScaledDimensions（DPR 感知） | ⚠️ fit addon 不感知 DPR，高分屏行列数略不精确 |

## 待办（按优先级）

### P1 — 低风险高回报（建议先做）

- [ ] **resize 分层（垂直立即 / 水平防抖）**：TerminalPreview.vue `ResizeObserver` 区块（当前 rAF 节流 + `fitAndRefresh` 全量重绘）。水平 resize 触发整屏 reflow（贵）→ 单独 100ms debounce；垂直 resize 立即；仅 cols/rows 实际变化才全量 refresh + syncTerminalSize。改动小、拖窗体验直接受益。
  **参考（VS Code 源码）**：
  - `workbench/contrib/terminal/browser/terminalResizeDebouncer.ts` — 整份 `TerminalResizeDebouncer` 类（L19-100）：分层常量 `StartDebouncingThreshold=200`（L15）/`DebounceResizeXDelay=100`（L16）、小 buffer 立即分支（L51）、不可见时 `runWhenWindowIdle` 延迟 X/Y 两路（L63-72）、`flush()` 保证最终尺寸必达（L90）
  - 调用/接线参考：`workbench/contrib/terminal/browser/terminalInstance.ts` `_resize()`（L2045-2088，`_resizeDebouncer.resize(...)` 于 L2086；`_layoutSettingsChanged` 时先重算字体的模式 L2049-2073 可借鉴）
- [ ] **custom glyphs 光栅化**：`initWebGL` 里 `new WebglAddon({ customGlyphs: true })`。更多 unicode 符号直接 GPU 光栅化，减少 canvas 光栅化开销、改善字体渲染一致。**注意**：VS Code 因辅助窗口 canvas 被 block 才禁用它，BedCode 单窗口无此限制，可直接开。需实测内置 glyph 覆盖差异。
  **参考（VS Code 源码）**：
  - `workbench/contrib/terminal/browser/xterm/xtermTerminal.ts` `_enableWebglRenderer()`（L888-957，`new Addon({ customGlyphs })` 于 L933；`_getWebglCustomGlyphs()` L959-963 展示 ownerDocument 判断）
- [ ] **font ligatures（可选）**：装 `@xterm/addon-ligatures` + 加载后**重建 WebGL renderer** 让纹理图集吸到 DOM 侧字体样式。若产品上无连字诉求可跳过。
  **参考（VS Code 源码）**：
  - `workbench/contrib/terminal/browser/xterm/xtermTerminal.ts` `_refreshLigaturesAddon()`（L965-1003，`@debounce(100)` 装饰器 + 加载/卸载 + 依赖项变化→`shouldRecreateWebglRenderer`→`_disposeOfWebglRenderer()`+`_enableWebglRenderer()` 重建，L1003 是重建触发点）

### P2 — 中等投入，收益需评估

- [ ] **背压反馈环（output flow control）**：当前链路是单向洪水：`pty_reader.rs` 无限 `while` 读 → 环形队列 → WS 推送 → 前端 `writeQueue`（仅有 `MAX_PENDING_FRAME_BYTES` 防御上限 + `setTimeout(0)` 让步）。VS Code 精髓：`terminal.write(data, cb)` 在 cb（解析完）里 `acknowledgeDataEvent(len)`，源端收到 ack 才发下一批 → 渲染解析速度反向钳制数据源流速。**BedCode 有天然载体**：TB v2 帧 `seq` —— 前端 `onWriteParsed` 后回发 ack 帧携 `last_rendered_seq`；Rust 侧收到前暂停从 PTY read。链路双向 WS 现成，改动集中可测。
  - ⚠️ **收益判断**：BedCode 是**本地环回 WS**，吞吐远高于 VS Code remote 场景，背压增益较小；但能压住 cat 大文件 / tail -f / 构建输出的峰值内存与 writeQueue 积压。接受"本地场景收益有限"再排期。
  - 若能落地，`writeInChunks` 的 `setTimeout(0)` 让出可顺带简化。
  **参考（VS Code 源码，背压链路整条）**：
  - 前端触发点：`workbench/contrib/terminal/browser/terminalInstance.ts` `_onProcessData()`（L1663-1692，OSC 133 序列拆分写入）/`_writeProcessData()`（L1694-1704，`raw.write(data, cb)` + cb 内 `acknowledgeDataEvent(data.length)` 于 L1699）
  - 前端缓冲/转发：`workbench/contrib/terminal/browser/terminalProcessManager.ts` `AckDataBufferer` 类（L742-756，攒满 `CharCountAckSize` 才发一次回调）+ 构造（L167）+ `acknowledgeDataEvent()`（L696）
  - 常量：`platform/terminal/common/terminal.ts` `FlowControlConstants.CharCountAckSize = 5000`（L896）；`ITerminalChildProcess.acknowledgeDataEvent(id, charCount)`（L845）
  - （BedCode 需自行设计 seq 复用的 ack 帧，VS Code 的"字符数"维度换成"帧 seq"维度）
- [ ] **DPR 感知 cols/rows 计算**：移植 `getXtermScaledDimensions`（`window.devicePixelRatio` 换算 canvas 像素，避免高分屏/多屏 DPI 变化时行列数不精确导致的模糊/截断）。fit addon 不感知 DPR，Windows 150%/200% 缩放用户受益。
  **参考（VS Code 源码）**：
  - `workbench/contrib/terminal/browser/xterm/xtermTerminal.ts` `getXtermScaledDimensions()`（L1178-1192，纯函数可整体移植：`w.devicePixelRatio` 乘宽高、ceil 行高、floor 列宽）
  - 配套字体测量：`workbench/contrib/terminal/browser/terminalConfigurationService.ts` `getFont()`（L175-189，优先取 xterm `_renderService.dimensions.css.cell` 实测 cell 尺寸）/`_measureFont()`（L223-260，GPU on/off 分支不同的 charWidth 算法 L245-251 + `_lastFontMeasurement` 失败回退缓存 L227-231）
  - 私有类型穿透声明：`workbench/contrib/terminal/browser/xterm-private.d.ts`（`IXtermCore._renderService.dimensions.css.cell`）
  - 像素尺寸下传 PTY 参考：`workbench/contrib/terminal/browser/terminalInstance.ts` `_updatePtyDimensions()`（L2093-2100，把 canvas 像素宽高传给后端）

### P3 — 谨慎（先解决前置问题）

- [ ] **平滑滚动**：先查清 BedCode WebGL 滚动重影根因（`addon-webgl 0.19` 版本问题 or alpha 缓冲？）。VS Code 方案：`MouseWheelClassifier` 区分物理滚轮/触控板，**只对物理滚轮**开 `smoothScrollDuration=125`（触控板保持即时滚动防跟手延迟）。若重影是版本 bug，升级 xterm/webgl 后可引入；否则维持 0 是正确决定。
  **参考（VS Code 源码）**：
  - `workbench/contrib/terminal/browser/xterm/xtermTerminal.ts`：常量 `SmoothScrollDuration = 125`（L54）、初始判定 `_isPhysicalMouseWheel()`（L128）、wheel 监听喂分类器（L522-530，`{ passive: true }`）、`_updateSmoothScrolling()`（L629-630）
  - 分类器本体：`base/browser/ui/scrollbar/scrollableElement.ts` `MouseWheelClassifier`（L63-107 类 + `acceptStandardWheelEvent()`；内部样本实现 `MouseWheelClassifierItem` L49-61；引用示例 L467）—— 可整体提取成独立 class

## 明确跳过（不适用 / 已被覆盖）

- addon 懒加载（AMD 按需导入）：VS Code 为 web 端启动体积，BedCode 本地 Tauri 无谓。参考（若未来需要）：`workbench/contrib/terminal/browser/xterm/xtermAddonImporter.ts` 整份 `XtermAddonImporter`（L30-70，`importAMDNodeModule` + 缓存 Map，可换 Vite 动态 import）
- 无缝重启缓冲（`SeamlessRelaunchDataFilter`）：已有「快照重订阅 + 截断→清屏全量重播」模型覆盖。参考：`workbench/contrib/terminal/browser/terminalProcessManager.ts` L767 起（`TerminalRecorder` 缓冲 + 延迟交换播放）
- OSC 133 序列拆分写入：只在 shell-integration 场景有价值，BedCode 无此功能。参考：`terminalInstance.ts` `_onProcessData()` L1666-1691
- 不可见时 idle resize：桌面常驻窗口不划算。参考：`terminalResizeDebouncer.ts` L63-72（若未来做多窗口/后台再取）
- DOM renderer 懒初始化：无此需求（优先 WebGL）。参考：`xtermTerminal.ts` `attachToElement()` L502 附近 `TODO: Move before open so the DOM renderer doesn't initialize`
- WebGL 纹理图集截图（`textureAtlas`→`createImageBitmap`）：BedCode 无截图需求。参考：`xtermTerminal.ts` L194-198

## 验收与回归

- 涉及文件：`bedcode-desktop/src/components/TerminalPreview.vue`（resize/custom glyphs/背压前端侧）、`src/composables/useTerminalOutputStream.ts`（ack 帧）、`src-tauri/src/pty/pty_reader.rs`（暂停读/水位）
- 现有测试：`src/__tests__/` 下 `useTerminalOutputStream.test.ts`、`terminalScrollback.test.ts`、`integration/terminal-flow.test.ts` 需保持全绿
- 背压/DPR 若涉及 Rust 侧，跑 `cargo test`（pty 模块已有 `reads_output_and_reports_stopped_on_eof` 等用例模式）
- 真机/真实场景：高分屏缩放、拖动窗口连续 resize、`cat` 大文件、`tail -f` 日志风暴、WebGL context loss 恢复路径回归

## Comments

- 2026-08-21：创建。源自 VS Code xterm 渲染优化探索（工作目录 D:/vscode-main/src/vs/platform/terminal → 主文件 `workbench/contrib/terminal/browser/xterm/xtermTerminal.ts`）。**实施前注意**：TerminalPreview.vue / pty_reader.rs / system.rs / lib.rs 曾被并发会话改动（见 [[desktop file-transfer]] 遗留），开工前先 `git status` 确认无并发在途改动，避免冲突。
- 2026-08-21：补充「VS Code 源码基准路径」= `D:/vscode-main/src/vs`，并为 P1/P2/P3 各优化项 + 跳过项逐一补充参考文件与行号（均已 grep 核对）。

- 2026-08-21（接手后）：**P1 全部落地并验证**（resize 分层 01 / DPR 感知 02 / custom glyphs 03）：
  - 01：`src/utils/terminalResizeDebouncer.ts` 纯模块 + 9 单测，组件 ResizeObserver→防抖→applyResize（仅 cols/rows 变化才全量重绘+PTY 同步）已接线
  - 02：`src/utils/terminalDimensions.ts` 纯函数 `getXtermScaledDimensions` + 7 单测（100%/150%/200% 缩放/ceil 行高/floor 列宽/退化兜底）；组件 `applyDprFit`/`measureCellSize`/`watchDprChanges`（matchMedia 递归监听跨屏 DPI 变化）已接线。**接手时该文件缺失 → 组件 import 断裂构建失败，已补齐修复**
  - 03：`customGlyphs: true`（0.19 走 Terminal 选项）+ Unicode11Addon（宽度表升级，TUI 边框光标漂移修复）已启用
  - 一并落地：`allowTransparency` 条件化（背景图才透明）——修 WebGL alpha 帧缓冲滚动残影根因（07 结论）
  - 验证：`vue-tsc --noEmit` 干净；全量 vitest 431 通过；Rust `cargo test --lib` pty/forward 用例绿（集成测试二进制 ws_session_route 在 Windows 有预存 rlib 链接问题，与本次无关）
  - 未做：04-06 背压（下步候选）；03/01/02 真机回归（高分屏缩放/拖窗/context loss）
- 2026-08-21（背压落地）：**P2 背压反馈环（04-06）实现完成**，wire 契约见 issue 04 Answer 段：
  - 04：前端 ack（TB v2 头 + ACK 标志位 0x02 + acked_seq + session_id 负载），`onWriteParsed → confirmWriteParsed()`，64KB 阈值 + 250ms 空闲节流；05 前端测试
  - 05：Rust 侧账本（`unacked_bytes` 原子 + `unacked_fifo` 8K 上限）+ `GlobalOutputManager::ack/should_pause` + `PtyReader` 每次 read 前查水位暂停（5ms 轮询、零字节丢失）+ `start_with_pause` 注入点；3 模块单测
  - 06：writeInChunks 的 `setTimeout(0)` **暂不简化**（保留——另一会话 WRITE_YIELD_THRESHOLD 水线对「水位以上/阈值以下」短时暴发仍有效，删让步需真机数据支撑）
  - 验证：`cargo test --lib` 567 全绿；前端 `vue-tsc` 干净 + vitest 436 全绿（曾遇 vitest worker OOM，根因=测试内 70KB spread 数组，改 Uint8Array 后消失）
  - 遗留：e2e 真机回归（cat/tail -f 峰值内存与 writeQueue 积压）+ 本地环回收益验证
