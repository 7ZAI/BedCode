# Agent Hub 界面打磨：新建预设弹窗 + 顶部分段栏宽度稳定

- 日期：2026-09-27
- 范围：`bedcode-desktop/wasm-apps/agent-hub/`（仅前端：1 个组件 + 1 个样式表 + 3 个 i18n 文件 + 3 个测试文件）
- 触发：用户两条明确诉求（① 优化「新建预设」弹窗样式 ② 顶部 tab 区域宽度不应随滚动条出现/消失而变）
- 关联：`.scratch/2026-09-27-agent-hub-audit-fixes/`（其 §2 非目标写了「不做视觉改版」，故本次另立任务，不并入该 spec）

## 1. 诉求二：分段栏宽度随滚动条跳变

### 根因（实测定位，非推测）

`.ah-view` 是 `display: flex; flex-direction: column` 的**滚动容器**，`.ah-tabs` 作为 flex item
被 `align-items: stretch` 默认行为**拉成整行宽**；整行宽 = 内容区宽 − 纵向滚动条宽（WebKitGTK 实测 17px）。
于是任一分区内容一旦溢出、滚动条出现/消失，分段栏（连同底色块与描边）就整体变宽 / 横移。

原型 `.scratch/2026-09-13-agent-hub/prototype/index.html:166` 里 `.tabs` 是普通块级容器里的
`inline-flex`（**收窄形态**），实现时容器换成 flex 列后 stretch 语义丢失 —— 属实现与设计真源漂移。

### 改法

- `.ah-tabs` 加 `align-self: flex-start`（回到原型的内容宽），宽度**只由内容决定**，与滚动条状态完全无关
- `max-width: 100%` + `flex-wrap: wrap` + `.ah-tab { white-space: nowrap }`：窄面板兜底
  （最小窗口宽 800px + UI 放大档可能放不下 6 项）—— 整块换行，不压缩中文标签、不把整列撑出横向滚动条
- 保留 `.ah-view` 既有的 `scrollbar-gutter: stable`（整列内容不横移的第二道保险）

### 实测（headless Chrome 152，dev-shell `:5193`）

| 场景 | 改前分段栏宽 | 改后 |
| --- | --- | --- |
| 1280 窗口，无滚动条 | 992px | **441.59px** |
| 1280 窗口，有滚动条 | 992px | **441.59px**（Δ = 0） |
| 800 窗口（最小宽），scale 1 | 992→按容器 | 442px / 1 行 |
| 800 窗口，`--ui-scale: 1.6` | — | 512px / **2 行**，无自身溢出、整列无横向滚动 |
| 520 窗口，scale 1 | — | 3 行换行，无自身溢出 |

## 2. 诉求一：「新建预设」弹窗样式

### 改前实测到的问题

| # | 问题 | 证据 |
| --- | --- | --- |
| D1 | 字段行**零垂直间距**，名称 / Base URL 两个输入框直接贴在一起 | 截图 `before-editor-light.png` |
| D2 | 96px 定宽标签列装不下「模型列表（每行一个）」「API Key（中心凭据）」，标签溢出色列并与控件抢位 | 同上 |
| D3 | 保存 / 取消挤在 key 提示语正下方，无分隔、主操作在**左** | 同上 |
| D4 | 打开弹窗时初始焦点落在**关闭按钮**上（一打开就出现焦点环在 ✕ 上） | `focusables()[0]` 是 DOM 首位的 ✕ |
| D5 | 输入框只有 `<span>` 伪标签，无 `label[for]` 关联；关闭按钮 `aria-label="close"` 硬编码英文 | 源码 |
| D6 | 「同名预设已存在」错误飘在弹窗底部，与出错控件无关联 | 源码 |

### 改法（三段式弹窗 + 纵向字段 + 无障碍关联）

- **面板三段**：`.ah-modal-head`（标题 + 图标关闭）/ `.ah-modal-body`（唯一滚动区，`min-height: 0`）/
  `.ah-modal-foot`（顶部分隔线 + 右对齐动作）。表单再长，头与「保存」也不滚出视野
  （实测：窗口压到 520px 高 + 模型列表塞 40 行 → 体部滚动，头脚常驻）
- **字段改纵向堆叠**：`.ah-pv-field { flex-direction: column }`，控件占满字段宽。
  关键点：必须同时把 `.ah-pv-input` 改回 `flex: 0 0 auto` —— `.ah-input` 的 `flex: 1` 在**纵向** flex 里
  `flex-basis` 作用于高度，会把 36px 输入框压成 0 高（key 行内再用 `.ah-pv-keyrow .ah-pv-input` 恢复弹性）
- **表单语义**：面板改为 `<form>`，保存按钮 `type="submit"` → 文本字段回车即保存；名称为空时按钮 disabled 拦得住
- **初始焦点**：`[data-autofocus]` 标记（名称字段），不再是 DOM 首位的 ✕
- **无障碍**：4 个控件全部 `label[for]` 关联；名称标必填（`*` + `aria-required` + 星号带「必填」可读名）；
  错误落回名称字段下方并 `role="alert"` + `aria-invalid` + `aria-describedby`；改动名称即作废上一轮错误；
  chip 与 tab 补 `:focus-visible` token 描边（选中项用 `--color-primary-contrast`，否则同色不可见）
- **i18n**：新增 `hub.pv.editor.close` / `hub.pv.editor.required`（zh-CN + en + `MessageSchema` 三处同步）

### 检索可追溯（frontend-styles 强制前置）

| 决策 | 检索来源 |
| --- | --- |
| 标签必须与控件程序关联（不用 placeholder 当标签） | `--domain ux`「form field label input vertical stack required」→ *Input Labels*（High） |
| 必填项要有标记 | 同上 → *Required Indicators* |
| 错误必须落在出错字段下方并 `aria-describedby` 关联 | 同上 → *Error Placement*（High） |
| 标题要与正文有明确层级差 | `--domain ux`「modal dialog form layout clarity」→ *Heading Clarity* |
| 窄面板不得出现横向滚动条 → 选「换行」而非「横向滚动条」 | `--domain ux`「modal dialog long form scroll actions footer」→ *Horizontal Scroll*（High） |
| 分段栏宽度不由容器决定 | `--domain ux`「layout shift scrollbar content width」→ *Content Jumping*（High：异步状态更新不得推移邻近内容） |

结论未覆盖 skill 规则：全部走宿主 token，无第三方色板/字体；`scrollbar-gutter` 检索无库命中（`--stack html-tailwind`
返回 0 条），按 WebKitGTK ≥ 2.48 支持的事实保留既有声明。

## 3. 验证证据

- 插件前端测试 `pnpm exec vitest run wasm-apps/agent-hub`：**213 passed**（新增 A7 组 8 例 + 改写 A2 焦点 trap 1 例）
- 桌面端全量 `pnpm run test:run`：**101 files / 1175 tests passed**
- 样式护栏 S1 对比度矩阵（6 palette × 明暗 = 12 套主题）：新增 `.ah-pv-req` 最差 **7.28:1**、
  `.ah-pv-label`（改 12px 后）最差 **7.13:1**，门槛 4.5 —— 全绿
- `pnpm exec tsc --noEmit` 0 error；根 `pnpm exec eslint bedcode-desktop/wasm-apps/agent-hub` 0 error 0 warning
- 新增样式守门 S10（分段栏宽度解耦）/ S11（弹窗三段 + 纵向字段），共 9 例
- 变异自检 8 项**全部杀死**：去掉 `align-self` / 去掉 `scrollbar-gutter` / `.ah-pv-input` 退回 `flex: 1` /
  面板退回自滚动 / 去掉 `data-autofocus` / 去掉 `aria-describedby` / 删掉「改名称作废错误」watch /
  保存按钮退回 `type="button"`（4 例红）
- 实机截图（dev-shell `:5193` + LingLong Chrome 152）：`/tmp/ah-shots/final-*.png`
  （明/暗弹窗、同名错误态、超长内容滚动态、明暗分段栏、窄窗口分段栏）

## 4. 遗留 / 未覆盖

1. **`ah-contrast.mjs` 浏览器实测脚本未跑**：本机无 `puppeteer-core`（上一轮装过、现已不在），
   该脚本依赖它。自动化替代是 vitest 的 S1/S2 矩阵（同一 12 套主题，从宿主 token 合成）。
   要跑实测需先 `pnpm add -D puppeteer-core` 到 dev-shell。
2. **WebKitGTK 实机未验**：本轮实测在 Chromium 152。`scrollbar-gutter: stable` 在 WebKitGTK 2.52 支持，
   但分段栏宽度现在**不再依赖**该属性（`align-self: flex-start` 即已解耦），残余风险仅为「整列内容横移」。
3. 520px 窗口下整列仍有 25px 横向溢出（改前改后一致，来自其它卡片而非分段栏）；
   应用 `minWidth: 800`，不在本票范围，未处理。
