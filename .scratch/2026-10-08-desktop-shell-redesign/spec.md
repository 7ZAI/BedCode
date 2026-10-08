# 2026-10-08 桌面端前端界面重构 — 方案草案 v0.1

## 预览

直接用浏览器打开（无构建依赖）：

```bash
xdg-open .scratch/2026-10-08-desktop-shell-redesign/shell-preview.html
```

## 设计定位

WasmApp 桌面端定位为 **wasm-app 运行平台壳**：

- 宿主内核只提供运行时、权限、资源、消息总线等原语（呼应无业务内核红线 §5）
- 每个 wasm-app 是独立运行实例，拥有独立界面展示面（中间 stage 的 apppane）
- 壳子负责：安装/启动/停止、权限管理、资源监控、运行检查、全局命令入口

## 布局结构

```
┌────────────────────────────────────────────────────────────┐
│ TitleBar: 品牌 · 全局搜索/命令(Ctrl+K) · 插件标题栏扩展 · 窗口控制 │
├────┬───────────────────────────────────────────┬───────────┤
│App │ Tabs（多 app 同时运行）  │   Inspector     │
│Rail│ ┌─────────────────────────────────────┐ │ 实例状态   │
│ ⌘  │ │ 当前活动 app 的独立界面                │ │ 权限详情   │
│ ✦  │ │（terminal / ai / file / agent mock） │ │ CPU/内存   │
│ ⇅  │ └─────────────────────────────────────┘ │ 日志/停止  │
│ ＋ │                                           │            │
├────┴───────────────────────────────────────────┴───────────┤
│ StatusBar: 运行时状态 · 应用数 · 总线速率 · 快捷键 · wasmtime 版本 │
└────────────────────────────────────────────────────────────┘
```

- **左 rail**：已安装 wasm-app 图标 + 运行态圆点（running 绿 / dormant 灰）+ 安装入口
- **Tab 栏**：并行运行的 app 以标签页表达，强调「多 app 同时运行」
- **Stage**：活动 app 的独立界面（iframe/沙箱槽位占位，mock 四种 app 形态）
- **Inspector**：权限位（`pty:spawn` / `net:egress` / 未授权项）、资源用量、日志/停止操作
- **StatusBar**：宿主级健康度总览

## 设计决策与 ui-ux-pro-max 追溯

- 整体 token 直接复用 `style.css` 既有 warm 色板（light/dark），未引入新色值/字体
- 密集型桌面效率工具布局：固定三栏 + 状态栏，检索 `--domain ux "desktop productivity dense layout sidebar status bar"` 第 3 条（fixed positioning 用固定布局 + 内部滚动，无层叠遮挡）
- 可点击元素均 `cursor:pointer`、hover 过渡 150ms 内、焦点可见、`prefers-reduced-motion` 待正式实现时补
- 终端/AI/文件/应用中心四个 mock 覆盖「独立界面展示」的典型形态

## 与现有架构的对应

- 左 rail ↔ 现 `Sidebar` 插件面板（统一排序），但视觉弱化为图标轨
- Tab 栏 ↔ 现 `KeepAlive` 路由缓存语义的可视化：多 app 并行存活
- Stage ↔ `PluginViewHost` 的替身：plugin surface 挂载点
- Inspector ↔ 现 `PluginStatusBar` / 权限审批弹窗能力的常驻化
- 权限模型 ↔ manifest 权限位仲裁（加载期）+ `host-*` 闸门

## 待决策点（供讨论）

1. 左 rail 是否保留文字标签（可折叠到现 Sidebar 形态）
2. 多 app 是「单窗标签页」还是「分屏/拼贴」（参考平铺窗口管理器）
3. Inspector 是否默认收起（窄屏友好）
4. 应用安装来源：本地导入 / 远端 registry / dev 热重载
5. 与现有 `PluginsView` / `PluginDetailView` 的关系：并入壳还是作为独立"应用商店"app
