# 终端输入导航条（Terminal Input Rail）— 设计规格

> 状态：已实现 · 2026-08-14
> 范围：桌面端终端窗口（TerminalWindowView / TerminalPreview）
> 参考：`.scratch/屏幕截图 2026-08-14 050853.png`（行级横线标记导航效果）

## 1. 背景与目标

终端滚动历史很长时，用户想快速跳回"某一次输入"的位置。本功能在终端区域**贴近右边框**提供一个悬浮导航条：

- **一根横线 = 一次用户输入**，横线的垂直位置映射该输入在终端 buffer 中的位置
- 鼠标未放上去时：只显示一列横线，**背景完全透明**，不遮挡终端内容
- 鼠标移上去：展开为**带背景色的列表卡片**，每条输入一行（内容 + 横线），点击任意一行滚动终端到该输入显示的区域
- **无输入时整体隐藏**（不占位、不渲染）

**约束**：这是独立的 Vue DOM 组件，导航效果本身不使用 xterm 渲染能力；与 xterm 的交互（记录位置、滚动）通过 TerminalPreview 已有的实例完成。

## 2. 交互设计

### 2.1 状态机

```
[无输入记录]  → 组件隐藏（v-if，不渲染任何元素）
        ↓ 出现第一条输入
[默认态] 右侧竖直居中、固定高度（h-56 = 224px）一列横线（透明背景，横线垂直位置 = 输入在 buffer 中的相对位置，全量 buffer 压缩进条带）
        ↓ 鼠标移入组件区域        ↓ 鼠标移出
[展开态] 背景色卡片 + 输入列表（每条输入一行：左侧截断文本 + 右侧主题色横线）
```

| 状态 | 背景 | 内容 | 触发 |
|------|------|------|------|
| 隐藏 | — | 无 | `markers.length === 0` |
| 默认态 | 全透明 | 一列主题色横线（垂直位置映射 buffer 行，条带竖直居中、高度固定 224px） | `markers.length > 0` |
| 展开态 | `bg-card` + 边框 + 阴影 | 每条输入一行：`$ 输入文本` + 右侧横线 | 鼠标进入组件区域（hover） |

### 2.2 交互细节

- **hover 展开**：鼠标进入组件整体区域（轨条宽度）即展开；移出即收起（`duration-200` 过渡）
- **点击导航 + 选中**：展开态点击列表行、默认态点击横线 → 记录选中（`selectedId`）并 `terminal.scrollToLine(marker.line)`，选中行保持高亮（浅色背景 + 亮色文字 + 实心横线），再次点击其他行切换选中
- **行悬停**：展开态列表行 hover 高亮 `bg-[var(--bg-hover)]`，横线同步加深
- **文本截断**：输入文本超长截断（`truncate` + `title` 完整内容）；多行粘贴的记录取首行文本显示
- **滚动位置实时性**：终端滚动时横线位置不变（映射 buffer 绝对行，非视口），仅当 buffer 长度变化（新输出/淘汰）时重算

## 3. 数据模型

```ts
/** 一次用户输入的位置标记 */
interface InputMarker {
  /** 自增 ID（组件内部维护） */
  id: number
  /** buffer 绝对行号（xterm 坐标，随 scrollback 淘汰自动校正，-1 = 已淘汰） */
  line: number
  /** 输入文本（不含提示符；多行粘贴取首行） */
  text: string
}
```

**记录时机**（TerminalPreview `terminal.onData`）：收到 `\r`/`\n` 时，在清空 `currentLineBuffer` **之前**保存 `{ text: currentLineBuffer, marker: registerMarker(0) }`。

**位置校正**：使用 xterm `registerMarker(cursorYOffset)` 返回的 `IMarker`（`marker.line` 随 scrollback trim 自动递减校正，`line < 0` 表示该行已被淘汰）。组件持有 `IMarker` 引用，渲染时过滤 `marker.line >= 0`。

**最大数量**：`maxMarkers` 可配置（Prop），**默认 10**。超过时淘汰最旧记录（FIFO），淘汰时 `marker.dispose()`。

## 4. 组件规格

### 4.1 文件与职责

| 文件 | 职责 |
|------|------|
| `packages/plugin-sdk-desktop/src/ui/TerminalInputRail.vue` | 纯 UI 组件（SDK 共享）：接收 markers + bufferLength + 导航回调，渲染默认态/展开态；`InputMarker` 类型内置于组件导出（宿主 composable 产出同构对象） |
| `bedcode-desktop/src/composables/useTerminalInputMarkers.ts` | 数据逻辑：接收 terminal 引用，记录输入、FIFO 淘汰、淘汰过滤、buffer 类型检测 |

> 宿主引用：`import TerminalInputRail from '@binblink/plugin-sdk-desktop/ui/terminal-input-rail'`（SDK `exports` 子路径）

### 4.2 组件 Props / Emits

```ts
interface Props {
  /** 输入标记（已过滤 line >= 0，按时间正序） */
  markers: InputMarker[]
  /** 当前 buffer 总行数（用于横线位置百分比） */
  bufferLength: number
  /** 是否处于 alternate buffer（TUI 全屏程序），true 时内部隐藏 */
  isAltBuffer: boolean
  /** 最大横线数量，默认 10 */
  maxMarkers?: number
}
// Emits
(navigate, line: number) => void  // 点击横线/列表行 → 父组件执行 scrollToLine
```

### 4.3 布局与定位

- 容器：`absolute right-0 top-1/2 -translate-y-1/2 z-20 h-56 max-h-full`，**竖直居中 + 固定高度 224px**，不占布局流（终端宿主 `terminalHostRef` 为 `relative`）；短终端（< 224px）由 `max-h-full` 收缩；容器宽度随状态切换（默认 18px 轨距 / 展开 284px 覆盖卡片），保证 hover 命中区域非零
- **横线轨道独立于卡片**：`absolute inset-0 z-[1] pointer-events-none`（按钮 `pointer-events-auto`），默认态/展开态共用——展开时横线位置与收起时逐像素一致，不随卡片出现跳变；横线 `top = line / (bufferLength - 1) * 100%`（clamp 2px–容器高-2px，贴边输入行完整显示）
- 展开态：卡片右对齐贴边，`right-1 inset-y-0`（与条带同高同居中），宽 `w-[280px]`，内容超出 `overflow-y-auto`
- z-index 按 safe-stack：`z-20`（Local overlay，与 `remote-size-hint` 同层，二者不重叠）

### 4.4 样式规范（token-bound，遵循 frontend-styles skill）

| 元素 | 样式 |
|------|------|
| 横线 | `bg-[var(--color-primary)]`（**主题色**），`w-[10px] h-[2px] rounded-full`，默认不透明度 0.45，hover/当前导航目标时 `opacity-100` |
| 展开态卡片 | `bg-card border border-[var(--border)] rounded-card shadow-card`，`p-2`，`overflow-y-auto` |
| 列表行 | `flex items-center gap-2 px-2 py-1 rounded-btn text-xs`（紧凑行距，行高 ≈26px），hover `bg-[var(--bg-hover)]`，文本 `text-[var(--text-secondary)]`（`min-w-0` + `truncate`），横线 `flex-shrink-0` opacity-60（行 hover 或选中时 100）；**选中态**：`bg-[color-mix(in_srgb,var(--color-primary)_15%,transparent)]` + 文本 `text-[var(--text-primary)]` + 横线实心 |
| 过渡 | `transition-colors duration-200`（背景/文字色）；展开/收起整体用 `opacity + transform` 过渡（GPU 合成），`transition-opacity duration-200` |
| 字体 | 输入文本 `font-mono`（与终端一致） |

**禁止**：硬编码色值、`text-white`（主题色背景上文字用对比 token 时才需要，此处无主色背景）、`transition-all`。

### 4.5 横线垂直位置的计算

```
top% = clamp( marker.line / max(bufferLength - 1, 1) * 100, 0, 100 )
```

`bufferLength` 由父组件在每次输出 flush / onScroll 后更新（普通 ref，成本极低）。多条横线间距 < 4px 时保留最新一条（避免重叠）。

## 5. 与 TerminalPreview 集成

```vue
<!-- TerminalPreview.vue 模板（terminalHostRef 容器内） -->
<TerminalInputRail
  :markers="visibleMarkers"
  :buffer-length="bufferLength"
  :is-alt-buffer="isAltBuffer"
  @navigate="handleNavigate"
/>
```

> 组件来自 SDK：`import TerminalInputRail from '@binblink/plugin-sdk-desktop/ui/terminal-input-rail'`

- `visibleMarkers`：computed，过滤 `marker.line >= 0`，按时间正序，截取最近 `maxMarkers` 条
- `handleNavigate(line)`：`terminal.scrollToLine(line)`（xterm API 语义已确认：参数为 buffer 绝对行号，`scrollAmount = line - ydisp`），滚动触发现有 `onScroll` → 自动出现"滚动到底"指示器
- `isAltBuffer`：在输出 flush 后检查 `terminal.buffer.active.type === 'alternate'`
- 清屏（`clearTerminal`）/ 会话切换（sessionId watch）：`dispose()` 全部 marker，清空数组 → 组件自动隐藏
- 记录逻辑复用现有 `currentLineBuffer` 追踪（onData 中已有），仅在 `\r`/`\n` 分支增加"先保存再清空"

## 6. 边界情况

| 场景 | 行为 |
|------|------|
| 无输入 | 组件 `v-if` 不渲染，零侵入 |
| 输入超过 maxMarkers（默认 10） | FIFO 淘汰最旧，`marker.dispose()` |
| scrollback 淘汰（行被顶掉） | `marker.line < 0` → 过滤；全部淘汰则隐藏 |
| 清屏 / 会话切换 | dispose 全部，组件隐藏 |
| TUI 全屏程序（vim 等 alternate buffer） | `isAltBuffer` → 内部隐藏（恢复 normal buffer 后重新显示） |
| 多行粘贴 | 记录拆分为多条，每条取对应行文本；或 MVP 取首行（spec 定案：拆分为多条，逐行记录） |
| 横线重叠 | 间距 < 4px 保留最新 |
| 终端尺寸变化（resize/字体缩放） | 百分比定位自适应，无需额外处理 |
| 横线数量为 0 但 markers 非空（全部被淘汰） | 隐藏 |
| 展开态 + 滚动到底按钮 | 导航条竖直居中（z-20），与底部按钮不在同一高度区域，天然不重叠 |

## 7. 测试

- **composable 单测**（`useTerminalInputMarkers`）：
  - 记录：onData 回车时正确保存 text + line
  - FIFO：超过 maxMarkers 淘汰最旧
  - 淘汰过滤：`line < 0` 被过滤
  - 清屏/会话切换：全部 dispose
- **组件测试**（可选）：markers 空 → 不渲染；有 markers → 渲染 `maxMarkers` 条横线；hover 展开列表；点击 emit `navigate`
- 运行：`npm run test:run`（vitest run）

## 8. 实现计划

1. `useTerminalInputMarkers.ts`：记录 / 淘汰 / 过滤 / dispose 逻辑 + 单测
2. `TerminalInputRail.vue`：默认态 + 展开态渲染，token-bound 样式
3. TerminalPreview 集成：onData 记录、bufferLength 更新、isAltBuffer 检测、清屏/会话切换清理、`@navigate` 处理
4. `npm run test:run` 验证 + 手动验证（真实终端输入/滚动/清屏/TUI 程序）
