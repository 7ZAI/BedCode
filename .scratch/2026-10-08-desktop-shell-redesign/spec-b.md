# 2026-10-08 桌面端前端宿主界面重构 — 方案 B：分屏平铺工作区

> 定位：与同目录 `shell-preview.html`（方案 A · 标签页工作台）**并列的第二个候选方案**。
> 两者不是同一条路线的两个版本，而是对同一个待决策点（`spec.md` 待决策点 2
> 「多 app 是『单窗标签页』还是『分屏/拼贴』」）的两种正面回答，供并排比较后裁决。

## 预览

```bash
xdg-open .scratch/2026-10-08-desktop-shell-redesign/shell-preview-b.html
```

单文件、零构建依赖。

## 自检证据

设计原型不是「画完就交」，逻辑部分已实跑验证：

| 检查 | 方法 | 结果 |
| --- | --- | --- |
| JS 语法 | `node --check` | 通过 |
| 分屏树结构正确性 | 抽出 `buildTree` 纯函数，遍历 5 预设 × 0–5 个应用 × 最大化开关 = **60 组合**，校验叶子数、比例区间、split id 唯一性 | 60/60 通过 |
| 渲染路径无运行时错误 | 极简 DOM 桩实跑 `renderAll()`，遍历 3 模式 × 5 预设 × 3 检查器页签 × 6 应用数 × 最大化 = **540 组合** | 540/540 通过，产物无 `undefined` / `NaN` / `[object Object]` 泄漏 |
| 拖拽比例持久化 | 写回槽位 → 重建树读回 | 通过（`focus.outer = 0.31` 原样读回） |
| 预设间槽位隔离 | `focus.outer` 的值不得被 `columns` 读到 | 通过 |
| token 绑定 | 扫描全部 hex 字面量 | 仅剩 `style.css` 既有 token 值 + 品牌图标资产色（`icon.svg` 的 `#16181C/#F5F7F9/#FF9E2C`） |
| 图标规范 | 扫描 emoji 区段 | 0 个 emoji（全 SVG） |
| 过渡规范 | 扫描 `transition-all` | 0 个（全部 property-specific） |

### 自审发现并已修的两个缺陷

1. **拖拽比例被静默丢弃**：`split()` 原本用自增 id（`sp1`/`sp2`…）作节点标识，拖拽按 `data-split` 写回 `ratios['1']`，而 `buildTree` 读的是 `ratios['sa'/'sb'/'sc']` —— 两套名字永不相交，表现是「拖完比例，切一下 app 就弹回默认」。改为 `split(key, dir, defRatio, a, b)`，`key` 即持久槽位，并带预设名前缀（`focus.outer` / `columns.left`）避免不同预设的内外层同名槽位串值。
2. **最大化时状态失同步**：A 被最大化时点左轨的 B，`active` 已切到 B 但树仍是单叶 A，面板与检查器/状态栏各说各话。改为目标与 `maximized` 不一致时先退出最大化。


## 与方案 A 的分野（一句话）

| | 方案 A（`shell-preview.html`） | **方案 B（本方案）** |
| --- | --- | --- |
| 并行的维度 | **时间**：实例在后台并行存活，同一时刻只显示一个（标签页） | **空间**：多个 app 界面同屏可见（分屏/平铺） |
| 「切换展示」的含义 | 换 tab（串行替换） | 换布局预设 / 最大化 / 交换（空间重排） |
| 权限呈现 | 右侧 Inspector 常驻显示当前 app 的权限位 | Inspector 三视图 + **全宿主权限矩阵**（应用 × 权限位） |
| 隐喻 | IDE / 浏览器 | 平铺窗口管理器 + 编排控制台 |

方案 A 适合「一个 app 干一件事」的效率场景；方案 B 适合 BedCode 的真实场景——终端会话、AI 对话、文件传输**需要同时看着**（传输进度、终端输出、AI 回复交叉参考），串行标签页会迫使用户反复切回。

## 布局结构

```
┌────────────────────────────────────────────────────────────────────────┐
│ TitleBar  品牌 │ 布局预设 ▾ 单窗·主从·三栏·双栏·横排 │ 全局搜索 │ 窗口控制 │
├────┬──────────────────────────────────────────────┬────────────────────┤
│Rail│ Workspace（分屏平铺区 · 可拖拽分隔条）           │ Inspector          │
│ ⊞ │  ┌────────────────────────┬─────────────────┐  │ ┌────┬────┬────┐  │
│ ▦ │  │ terminal-session       │ file-transfer   │  │ │实例│权限│通道│  │
│ ⛨ │  │ （独立界面 · 终端形态）  ├─────────────────┤  │ ├────┴────┴────┤  │
│ ──│  │                        │ ai-chatbox      │  │ │ 状态 / 资源   │  │
│ ⌘ │  └────────────────────────┴─────────────────┘  │ │ 扩展点 / 配额  │  │
│ ✦ │                                             ▸   │ │ 权限位 + 开关  │  │
│ ⇅ │  ▏= 可拖拽分隔条（鼠标拖拽 / 方向键，比例持久）    │ │ 通道 + 同驻    │  │
│ ◈ │                                             ▸   │ └───────────────┘  │
│ ＋│  ← 左轨另有两个宿主自有面：应用总览 / 权限矩阵      │                    │
├────┴──────────────────────────────────────────────┴────────────────────┤
│ StatusBar  运行时就绪 │ 4 应用运行中 · 1 未启动 │ 1 待审批 │ 总线 │ 布局 │ wasmtime 48 │
└────────────────────────────────────────────────────────────────────────┘
```

### 五个区域各自的职责

| 区域 | 职责 | 归属判定（AGENTS §5.1） |
| --- | --- | --- |
| **App Rail**（左轨） | 已安装 wasm-app 入口 + 运行态圆点；顶部两个**宿主自有面**入口（总览 / 权限矩阵） | ③ 通用注册表与寻址——按注册表渲染，不含产品语义 |
| **Workspace**（中舞台） | 分屏树：每个叶子 = 一个 app 的独立界面承载位 | ① 引擎实现（布局引擎是通用机制） |
| **Inspector**（右） | 实例 / 权限 / 通道三视图，针对当前活动面板 | ② 安全闸门 + ③ 通用注册表 |
| **Overview**（宿主自有面） | 运行中 app 的卡片总览，点击进入工作区 | ③ 通用注册表与寻址 |
| **Permission Matrix**（宿主自有面） | 应用 × 权限位矩阵，授予/撤销、批量审批 | ② 安全闸门（权限位是**机制词汇**，非产品概念） |
| **StatusBar**（底） | 宿主级健康度与运行计数 | ① 引擎实现 |

## 关键设计决策

### 1. 分屏引擎 = 通用二叉分裂树，不是 per-app 硬编码

```js
{ k:'leaf',  app:'terminal-session' }
{ k:'split', dir:'h'|'v', id, ratio:0.64, c:[nodeA, nodeB] }
```

渲染器对树递归，**不认识任何具体 app**。布局预设只是「树构造函数」：

| 预设 | 树形 | 适用 |
| --- | --- | --- |
| 单窗 | `leaf(active)` | 专注单任务 |
| 主从 | `split(h,.64)[ leaf, split(v,.5)[leaf,leaf] ]` | 一个主角 + 两个侧栏 |
| 三栏 | `split(h,⅓)[ leaf, split(h,⅓)[leaf,leaf] ]` | 横向对照（如三个终端） |
| 双栏网格 | `split(h,.5)[ split(v,.5)[a,b], split(v,.5)[c,d] ]` | 密度优先 |
| 横排网格 | `split(v,.5)[ split(h,.5)[a,b], split(h,.5)[c,d] ]` | 纵向对照 |

比例存 `state.ratios[splitId]`，重建树时复用 → **布局在切 tab / 启停 app 后不丢失**。

### 2. 「独立界面展示」= 面板承载位，不塞 iframe

每个面板 = 独立挂载槽位 + 自己的头部（状态点 / 版本 / badge / 三个动作）。面板内容直接挂插件注册的视图组件（现状 `PluginViewHost` 的形态），**不引入 iframe 沙箱**——因为 wasm-app 的前端资源由宿主经 Vite 构建注入（`plugin-sdk-desktop/src` 的共享模块运行时代理），组件级挂载即可拿到共享的 pinia / vue-i18n / Tauri invoke 通道。iframe 会切断这条链路，是退步。

面板头部三个动作对应三条真实路由/能力：

| 动作 | 落到哪 |
| --- | --- |
| ⛶ 最大化 | 工作区临时变成单叶节点（比例保留，可退出还原） |
| ⧉ 剥出独立窗口 | 现有 `bareWindow` 路由 `/plugin/window/:pluginId/:viewId`（`meta.bareWindow` → 不套壳） |
| ✕ 关闭 | 停用实例，面板槽位回收，rail 上回落为 dormant |

### 3. 权限管理做成两个面（单 app + 全局）

- **Inspector › 权限**：当前 app 的权限位清单，每个带开关。高危位（`pty:spawn` / `process:run` / `terminal:input` / `database:main`，取自 `contributionKinds.ts` 的 `HIGH_RISK_PERMISSIONS`）红色描边 + `高危` chip + 后果文案，且开关用 danger 色。
- **权限矩阵**：行 = 应用，列 = 权限位并集（从 manifest 聚合），单元格 = 授予状态。三态可视化：已授予 / 待审批（黄）/ 未授予。高危位列头红色竖排。

**为什么值得单独做矩阵**：单 app 视图只能逐个检查；矩阵让「谁拿了 `pty:spawn`」一眼可见，且**新增 app = 新增一行、新增权限位 = 新增一列，壳的代码零改动**——这是扩展性的直接证据。

原型里的开关是**演示态**（本地改 `granted` 数组）。正式实现的落点：授权策略真源在 `packages/bedcode-wasm-core/src/security/{auth_policy,strategy}.rs`，授予/撤销必须走策略表 + 实例重启生效，不能只在 UI 层改状态。

### 4. 扩展性：三条「加东西不加代码」的路径

| 要加的东西 | 需要改壳吗 | 机制 |
| --- | --- | --- |
| 新的 wasm-app | **不改** | manifest 声明 `permissions` + `contributes.views`，自动进 rail / 矩阵 / 总览；拖一个面板进工作区即可 |
| 新的权限位 | **不改** | 权限位来自生成的词汇表（`src/plugin/permission-vocabulary.ts`，真源 SDK `permission.rs`），矩阵列自动扩展 |
| 新的扩展点种类 | 加一条注册表条目 | `contributionKinds.ts` 的 kind 表 + 对应 i18n key，渲染分支不增（沿用现状设计） |
| 新的面板形态 | **不改** | 面板是通用承载位；插件视图自己决定内部形态（终端 / 对话 / 列表 / 表单） |

### 5. 架构红线自检（AGENTS §5.1 B1–B6）

| 判据 | 本方案是否命中 | 说明 |
| --- | --- | --- |
| B1 产品类型/字段 | 否 | 壳的数据形状是 `leaf/split`（布局机制）+ 权限位字符串（机制词汇）+ 实例 id。**没有**会话、传输任务、对话等业务类型 |
| B2 业务编排/状态机 | 否 | 只推进「布局树 / 实例启停」两个机制状态机，不按产品语义推进任何流程 |
| B3 业务真源 | 否 | 壳不持有任何业务事实；权限真源在策略表，会话真源在 `wasm-apps/terminal-session` |
| B4 业务投影 | 否 | 不把原语结果翻译成产品 wire 形状 |
| B5 业务默认值 | 否 | 布局默认值（比例 0.64）是**机制布局参数**，不是替插件决定业务；权限默认一律 fail-closed（待审批态） |
| B6 业务生命周期挂钩 | 否 | 壳解释的是「实例 running/dormant」这一运行时状态，不解释产品事件 |

落在 §5.1.3 允许的四类薄壳内：① 引擎实现（布局引擎、树渲染）② 安全闸门（权限矩阵 fail-closed、高危位整单批准）③ 通用注册表与寻址（rail / 总览 / 扩展点计数）④ 无。

**未决**：§5.1.2 三问第 1 问「离宿主能实现吗」——布局引擎**只有宿主能实现**（插件之间没有共享布局的通道，也不需要），故宿主侧成立；第 3 问要求「WIT 纯增量 + 权限位有门禁落点 + 停用可回收」——三者均满足（新增扩展点走 manifest，不新增 host-* 原语；权限面复用现有闸门；面板关闭即回收）。**但这一裁决建议在落地前与用户确认**，因为它触及「宿主是否该拥有布局态」这个边界问题。

## 设计规范追溯（frontend-styles 强制 + ui-ux-pro-max 检索）

| 决策 | 依据 |
| --- | --- |
| 全部色值 / 字体走 `style.css` 既有 token，**零新增 hex、零新字体族、零 Google Fonts** | `frontend-styles` token-bound 铁律。`--design-system` 检索返回的蓝色板（`#2563EB`）与 Outfit 字体**被真源裁决否决**，未采用 |
| 视觉基调：网格化、功能优先、高对比、双主题 | `--design-system` 命中 `Minimalism & Swiss Style`（Light/Dark 均支持，适配 professional tools / dashboard） |
| 三栏 + 底部状态栏固定、内部滚动 | `--domain ux "desktop productivity dense layout sidebar status bar"` → Layout/Fixed Positioning：固定元素不得无序堆叠。已加 `<1180px` 收起 Inspector 的兜底 |
| 状态栏计数用语义整句 + `role="status" aria-atomic="true"`，不播报裸数字 | 同上检索 → Accessibility/Contextual Live Badge Updates（High） |
| 提交类操作有 loading→success 反馈（toast） | 同上检索 → Forms/Submit Feedback（High） |
| 图标全 SVG，**零 emoji**；可点元素 `cursor:pointer`；hover 过渡 150ms；焦点可见；`prefers-reduced-motion` 降级 | `--design-system` pre-delivery checklist + `frontend-styles` Checklist |
| 过渡全部 property-specific（`transition-colors` / `-transform`），无 `transition-all` | `frontend-styles` Tight Transitions |
| 分屏条既是 `separator` 也可 Tab 聚焦 + 方向键调节 | 原生无障碍：拖拽不能是唯一途径 |

## 交互清单（原型内可玩）

- 顶栏 5 个布局预设按钮 → 树重建，比例沿用
- 分屏分隔条**鼠标拖拽**（实时改 flex）+ **方向键**（步进 4%，比例持久）
- 面板头 ⛶ 最大化 / ⧉ 剥出窗口 / ✕ 停用
- 左轨图标点击 → 打开并设为活动；dormant 应用点击即启动
- 左轨 ⊞ 回到工作区 · ▦ 应用总览 · ⛨ 权限矩阵
- Inspector 三视图切换（实例 / 权限 / 通道）
- 权限开关与矩阵单元格点击 → 本地授予/撤销 + toast；`Agent Hub` 待审批可单批或批量批准
- 顶栏 ◐ 切换 light/dark；全部交互元素键盘可达

## 待决策点（沿用方案 A 的清单，B 额外两条）

1. ~~多 app 是单窗标签页还是分屏~~ —— 本方案明确选**分屏**，与 A 并排裁决
2. Inspector 是否默认收起（窄屏友好）
3. 应用安装来源：本地导入 / 远端 registry / dev 热重载
4. 与现有 `PluginsView` / `PluginDetailView` / `AuthorizationView` 的关系：并入壳（总览 + 矩阵）还是保留为独立管理面
5. **布局态是否持久化**：存 `host-storage` 原语（宿主机制面）还是插件 `storage`？—— 涉及 §5.1 归属，需用户裁定
6. **同一 app 能否开多个面板**（如同开三个终端窗口）：现状 manifest 一个 view 一个面板；支持多实例需要新的寻址维度（`appId + instanceId`），属 WIT 变更，须走 ABI bump 流程

## 未纳入（明确排除，避免范围蔓延）

- 分屏内嵌 iframe 沙箱（会切断共享模块链路，见决策 2）
- 面板拖拽排序 / 平铺工作区自动布局（`i3`-style 递归分割）——可作后续增强，当前预设已够用
- 真实的实例启停与权限落库（原型为演示态，落点已在决策 3 写明）
