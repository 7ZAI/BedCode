# 移动端终端 opencode TUI 滚动残影 — 根因取证与修复交接

> 日期：2026-09-07　|　涉及端：bedcode-mobile　|　状态：已修复并真机回归

## 一、问题现象

opencode TUI（alt screen + SGR 鼠标上报）在移动端展示时，**向上滑动滚动过程产生渲染残影**：

- 成片字符重叠/乱码块（如 `A4QxpBeOf8x9QB...`，实为搜索高亮条与文本叠加）
- 中文重叠、"缺字方块"
- 输入框白块覆盖（半行 ghosting）
- 滚动**停止后不消失**；点工具栏「刷新」按钮可全清

截图证据：`.scratch/adb-screenshots/`（screen-20260907-000516.png 重度残影基线；exp1~exp7 为取证过程；截图文件体积较大仅本地留存、未入库，交接后可按需自保）。

## 二、TUI 滚动链路（关键认知）

TUI 模式滚动**不走本地 scrollToLine**（alt buffer 无 scrollback）：

```
手指上滑 → onTouchMove → TUI 模式 → sendWheel (useTuiCompat)
  → SGR 滚轮序列 ESC[<64/65;col;rowM → WS → 桌面端 → opencode PTY
  → opencode 重绘整个 alt screen（大块输出）
  → 桌面端转发回 WS → 移动端 onOutput
  → writeCoalescer → terminal.write() → xterm 解析 → canvas renderer
```

项目既有防重影措施（`will-change: auto`、`smoothScrollDuration: 0`、GPU hint no-op、`forceCompositorRepaint`）全部针对**本地滚动**场景，对这条回写渲染链路无覆盖。

## 三、取证实验（真机 + adb 截图 + vision 逐帧分析）

| 实验 | 操作 | 结果 | 结论 |
|------|------|------|------|
| 1 | 滚动停止后仅 `terminal.refresh(0, rows-1)` | 清主体，剩左右边缘 2-3 字符残字 | renderer 位图残留，refresh 部分有效 |
| 2 | 滚动停止后仅 transform 往返（=forceCompositorRepaint） | 无效；残字位置随内容迁移 | 排除合成器滞留 |
| 3 | **rAF 合并写入全局开启**（`ENABLE_RAF_COALESCE=true`） | **成片残影 100% 消除**，无卡顿 | 根因一确认：**写入批次交错** |
| 后续 A | 合并开启后滚动 → 高亮条局部残留（exp5，y≈2195-2230）；点刷新（forceReplay）→ 全清（exp6，重放数据干净） | buffer 数据正确 | 根因二确认：**渲染层漏绘** |
| 后续 B | 合并开启 + 滚动停止补 `refresh` | 高亮残影**自动消失**（exp7） | 新增兜底方案验证通过 |

## 四、根因（两层，独立修复）

### 根因一：写入批次交错 → canvas 位图成片残留

opencode 一次滚轮重绘被拆成多个 WS 消息；移动端**直写**（每消息一次 `terminal.write`）→ xterm 按 write 边界多次提交渲染 → 同一逻辑屏幕更新的多批内容在不同帧渲染 → 中间态残留固化在 canvas 位图（停止不自愈=位图残留而非瞬时中间态；transform 清不掉=非合成层问题；refresh 能清主体=渲染层）。桌面端始终 rAF 合并，移动端默认直写是缺口。

**修复**：`writeCoalescer.ts` 默认开启 rAF 合并（保序合并同帧写入 → 单次渲染提交）；`MAX_COALESCED_BYTES` 256KB→512KB（防滚动重绘脉冲在逻辑更新中途截断合并，重蹈批次交错）。

### 根因二：高亮行渲染层漏绘 → 局部残留

含背景色/反色的行（opencode 高亮选中条/确认框）滚动更新时，xterm 收到清背景序列但 canvas 像素未更新（刷新=forceReplay 能清 ⇒ buffer 数据正确、属渲染层残留；rAF 合并解决成片残影后此局部残留仍在）。

**修复**：`useTerminalScroll.ts` 新增 `schedulePostScrollRefresh()`——TUI 模式下 `touchend` 后 500ms（覆盖惯性滑行尾段）补一次 `refresh(0, rows-1)`；仅 TUI 模式触发，一次性不做节流。

## 五、改动文件清单

| 文件 | 改动 |
|------|------|
| `bedcode-mobile/src/composables/writeCoalescer.ts` | 默认 rAF 合并 + 512KB 阈值 + 文件头根因注释（指向本交接文档）；置 `ENABLE_RAF_COALESCE=false` 可回退直写做 A/B |
| `bedcode-mobile/src/composables/useTerminalScroll.ts` | 新增 `schedulePostScrollRefresh()`（含 dispose 清理）；顺带修既有 LSP 阻断项 `applySettings` 未用参数 `theme`→`_theme` |
| `bedcode-mobile/src/__tests__/composables/writeCoalescer.test.ts` | 默认语义反转（默认合并 + 显式 `false` 测直写回退）；512KB 阈值用例（600KB→10 块） |
| `bedcode-mobile/src/__tests__/integration/terminal-flow.test.ts` | 显式 rAF 驱动（取代依赖 jsdom 时序的隐式 flush）；`Record<string, any>`→`TerminalSocketHandlers`；文件头"直写管线"→"rAF 合并写入管线" |

## 六、验证

- `pnpm run test:run` → **40 files / 344 tests 全绿**
- 真机回归（用户确认）：滚动停下 1 秒内画面自动变干净，无需点刷新；实时输出无卡顿
- 移动端常驻提示：**opencode 的深蓝/青色 Build 横幅是正常 UI**（命令运行时显示、结束消失），非渲染残留——判断依据：`refresh(0, rows-1)` 全量重绘后仍在 ⇒ buffer 真实内容

## 七、注意事项 / 后续建议

1. **热更新陷阱**：移动端 dev 模式 HMR 可能断开（锁屏/网络切换），改代码后重进终端页或重启 app 验证，勿直接复现下结论
2. **rAF 合并的已知边界**：合并依赖 rAF，页面后台时由 `FALLBACK_FLUSH_MS=100ms` 兜底定时器清队列；如未来出现"后台写入延迟/黑屏"类回归，优先查此路径
3. **未处理项**：`terminal-flow.test.ts` 的 LSP warning 全部为跨 tsconfig 边界假阳性（`@/` 别名由 vitest 配置解析）；`useTerminalScroll` 的 L779/780 非空断言、L804 console.warn 为既有代码，未在本次范围内
4. 若后续切换 WebGL 渲染器（`USE_WEBGL_RENDERER`），本修复仍适用，但 WebGL 双缓冲在移动端更易重影，需重新评估（项目历史结论：canvas 更稳）