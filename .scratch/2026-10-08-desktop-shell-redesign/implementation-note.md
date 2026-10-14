# 方案 C 设备壳落地实现记录（2026-10-14）

> 依据 `spec-c.md`（v0.3）与 `shell-preview-c.html` 原型，用桌面端既有技术栈
> （Vue 3 `<script setup>` + TS + Tailwind + vue-i18n）实现新宿主前端。
> 新壳与旧工作台（TitleBar + Sidebar）**并存**：路由 `/shell` 进入，`bareWindow` 不套旧外壳。

## 代码落点

```
bedcode-desktop/src/shell/
├── types.ts                共享类型（两屏 / 卡片键 / 通知 / 弹窗 / 壳偏好）
├── state.ts                纯逻辑助手（MRU / 展示态归并 / 待审批派生 / tone 哈希 / 常量）
├── useShellApps.ts         应用注册表投影 + 生命周期动作（模块单例；plugin_list_loaded 等真实通道）
├── useShellState.ts        两屏模型 + 卡片 MRU + 覆层 + 通知机制（模块单例）
├── useShellSettings.ts     壳本地 UI 偏好（showNav / wallpaper / wallDim → localStorage）
├── shell.css               共享组件样式（tone / switch / seg / chip / 壁纸 / 动效；token-bound）
├── ShellView.vue           路由根（键盘 / type-to-search 隐藏 input / 点外部关覆层）
├── components/             ShellToolbar · ShellHome · ShellCardScreen · ShellAppTile ·
│                           ShellIcon · ShellCard · ShellSeg · ShellNotificationPanel ·
│                           NotifCard · ShellContextMenu · ShellCommandPalette ·
│                           ShellDialogLayer · ShellBootScreen
└── sys/                    SysAppsView（应用管理）· SysPermsView（权限与授权）·
                            SysSettingsView（设置）
```

配套改动：
- `style.css`：新增 token `--radius-app-icon` / `--radius-sheet` / `--toolbar-h` +
  三张 token 派生壁纸渐变 `--wall-aurora/dune/reef`（零新 hex）
- `locales/zh-CN/shell.ts` + `en/shell.ts`：`shell.*` 命名空间（新增 key 双语同步）
- `router/index.ts`：`/shell` 路由（`meta.bareWindow`）
- `main.ts`：导入 `@/shell/shell.css`
- `App.vue`：Ctrl+, 快捷键在 `/shell` 让位（壳自带设置卡快捷键）

## 真实数据接线（非 mock）

| 壳区域 | 数据源 |
| --- | --- |
| 应用清单 / 状态 / 权限位 / 来源 / 体积 | `plugin_list_loaded`（PluginInfo） |
| 启停 / 审批 / 卸载 | `pluginPreauthorize + pluginLoader.activate/deactivate` · `pluginApprove` · `pluginUninstall` |
| 权限与授权（策略 / 四分区记录 / 内置免询问） | `plugin_auth_overview` + `authPolicy.ts` 分区口径（复用 AuthSection/AuthRecordRow/AuthFirstPartyRow） |
| 设置 | 复用既有设置分组组件（链路加密 / 系统 / 日志 / 关于）+ 壳专属界面/背景组 |
| 主题 / 色板 / 语言 / 字号 / 动画 | 既有 settings store + useTheme / useFontSize / useI18nStore |
| 应用配置 | manifest `contributes.configuration` schema 驱动 + 插件存储 `config` key（与 PluginConfigView 同通道） |
| 命令面板 | `plugin_list_commands` + 宿主动作 |

## 边界裁定（AGENTS §5.1）

- 壳数据形状 = 卡片 MRU（机制）+ 注册表投影（机制词汇）+ 授权读模型（机制词汇），落薄壳 ①②③
- 图标色调 / 图标名 = **插件 id 确定性哈希**，不做产品 id 映射（B5 不命中）
- `showNav / wallpaper / wallDim` 是壳 UI 偏好 → localStorage，**不加宿主 AppConfig**
  （引擎面纯净性；改 Rust 契约属越线，未动）
- 通知中心 = 审批动态派生 + 壳生命周期事件；宿主只承载机制面，文案由来源方给出

## 验证

- `vue-tsc --noEmit` 0 error；`eslint`（壳 + 修改面）0 error；`vite build` 通过
- 新增单测 26 用例（shellState 纯逻辑 + useShellState 两屏/卡片/覆层/通知）全绿
- 全量 vitest 1653/1654 通过，唯一失败 = `terminalPreview.test.ts` resize 重试
  （**预存在 flaky**，该树本次零改动，隔离运行也红，见 2026-10-14 记忆）
- 浏览器冒烟（vite + chromium-headless）：主屏（应用/系统网格 + 壁纸）· 开设置卡
  （界面 seg / 开关 / 壁纸 swatch / 滑杆）· 系统页三 tab 互切 · 通知面板 2×2 快捷开关 ·
  Esc 逐层收 · Ctrl+Shift+P 命令面板 · 主题切换 · type-to-search（ASCII 路径 + Backspace + Esc）全数通过

## 已知边界

- 应用卡内容 = 插件注册视图（PluginViewHost 挂第一 sidebar/toolbox 视图）；无视图给占位。
  卡片静态缩略为占位文案（真源需离屏渲染或只读镜像，spec-c 待决策 3）
- 通知中心数据源 = 审批派生 + 壳生命周期事件（宿主无统一通知面，spec-c 待决策 5）
- 自定义壁纸链路（fs:pick → 宿主读盘下发）待接入（前端零资源访问红线）
- 旧界面（/plugins /settings 等）原样保留；`/shell` 与旧面并存，默认落地页未改
