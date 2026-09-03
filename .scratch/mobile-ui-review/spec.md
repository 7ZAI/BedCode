# Mobile UI 审查（mock harness + vision 评审）

**Status:** ready-for-agent
**Type:** task
**创建:** 2026-08-12

## 背景

移动端主色已统一为桌面端 warm 色板（墨纸体系：深色 `#ECE8DC` / 浅色 `#1D1A14`），
并实施了连接首页三项改进（主按钮 CTA 层级、未连接状态徽章、Tab 激活指示条）与
导航图标更换（连接=MonitorSmartphone、工具箱=Wrench）。

为系统性验证所有页面的配色/布局/对比度，构建了纯前端 mock harness，对全部 16 个
页面（深色 16 张 + 浅色 5 张）截图后由 vision subagent 逐页评审。

## Mock Harness 用法（复验用）

- 入口：`bedcode-mobile/public/mock-harness.html`（仅 dev server 可访问）
- 原理：同源 iframe 加载应用（视口严格 390px 左对齐，避免 headless 顶层视口 bug），
  预填 localStorage mock 数据，支持 URL 参数控制
- URL 参数：
  - `route=/mobile/devices` — 目标路由
  - `theme=dark|light` — 主题（延迟 1s 应用，等应用异步初始化完成）
  - `hidebar=1` — 隐藏工具条（截图用）
  - `connected=0` — 未连接态（默认 mock_connected=1 已连接）
- localStorage 预填：`connection_history`（5 条）、`preset-tasks`（3 条）、
  `mock_terminal_enabled`、`mock_connected`
- 应用侧 dev-only mock：`useMobileConnection.ts` 中 `import.meta.env.DEV &&
  localStorage.getItem('mock_connected') === '1'` 注入已连接状态
- 截图命令示例：
  ```
  chrome --headless=new --screenshot=out.png --window-size=390,900 --hide-scrollbars \
    --virtual-time-budget=15000 "http://localhost:1423/mock-harness.html?route=/mobile/settings&theme=dark&hidebar=1"
  ```
- 截图存档：`.scratch/mock-review/*.png`

## 审查结论摘要

### 已甄别为误判/设计决策（无需修复）

| 项 | 说明 |
|---|---|
| "背景纯黑" | 实为 token `#0a0a0f`，非纯黑 |
| "字体 Inter" | 系统字体栈，非 Inter |
| 终端字符硬换行 | mock 生成器随机断行，非渲染 bug |
| mock 卡 + 空态并存（会话页） | dev-only 特性（`useMockTerminal`），生产不渲染 |
| 插件页 + 按钮"白色突兀" | 实为 `var(--mobile-accent)` 米白主色，正确应用 |
| 会话卡红色方块"语义不明" | 为停止按钮（有 `:title`），可作 P2 打磨 |

### 待修复清单

见 `issues/01-*.md` ~ `issues/07-*.md`，按 ticket 逐项修复。

## 截图存档

- `.scratch/mock-review/` — 21 张（16 深色 + 5 浅色），命名 `<page>-<theme>.png`
- 修复后建议复用 harness 重截对应页面，vision 复验

## Comments

- 2026-08-12：全部 5 批评审完成（连接/终端组、功能组、浏览组、设置组、浅色组）。
- 2026-08-12：7 个 ticket 全部修复完成（01~07），改动见各 issue Comments；`vue-tsc` 通过、82 个 vitest 通过。修复后截图存档待补：`scan`、`settings`、`settings-connection`、`settings-appearance`、`settings-authentication`、`preset-tasks`、`devices`、`toolbox`、`sessions`、`files` 深色 + 浅色。
- 2026-08-12（复验）：按 harness 重截 10 页深/浅色（22 张，含 devices 断连态）交 vision 复核。vision 发现 2 个真实 P0：
  - ① `ScanView.vue` 模板多余 `</div>`（Invalid end tag）→ vite 转换 500 → 扫码页整页空白（只剩 tab bar）。已删除多余闭合标签，恢复 200，重截后取景器 UI（header/256px 取景窗/四角标/四块遮罩/扫描线/提示文案/底部工具栏）完整，深浅色对称，vision 确认闭环。
  - ② 文件页「前往连接设置」引导只覆盖了 FileExplorer 右侧代码区错误分支，左侧 FileSidebar 文件树错误态（主路径）缺失。已在 FileSidebar 错误态加 base URL 识别（`no base url|not connected`）+ 主按钮（accent 实底）+ 重试降级，FileExplorer 转发事件，vision 确认闭环。
  - 误判甄别：浅色主题 segment/Toggle 激活态 vision 报“纯黑 #000”，实为 `--mobile-accent` 墨色 `#1D1A14`（像素验证 0 处 #000，397/779 处 accent），设计正确；Toggle 深色态白 thumb 在米白轨道上对比弱为 P2 打磨项。
  - 其余 P2 打磨建议（断开/清除按钮触控区、辅助文字对比度、滑块档位点、浅色重置按钮区分度）未处理，记录待后续。
  - 复验后 `vitest run` 82 通过、`vue-tsc` 通过。
