# 移动端宿主壳 · 第二轮修订（用户指令 2026-10-14）

> 用户原始指令（逐条编号，交付时逐条对账）：
> R1 主界面删「平台自带」字样；删底部导航栏；多任务按钮进各应用运行页右上角胶囊
> R2 快捷卡片按原型改左右轮播（非上下堆叠）；删「由应用提供」字样
> R3 管理按钮太丑，改图标展示
> R4 运行页左上角放应用图标 + 名称；右侧胶囊加多任务图标；小圆圈改 home 图标
> R5 宿主内「方案设计期」文案全删（例：「平台叠加的控制项，与小程序胶囊一致」）
> R6 多任务页重做：卡片固定宽高居中、多卡片等差叠放、动画重做
> R7 平台设置的危险操作迁到「关于 WasmApp」
> R8 设置二级页与安全区紧贴 → 参考权限总览标题栏间距
> R9 平台设置补齐：空白 / 点不进的项
> R10 背景可设壁纸（自定义图片）
> R11（追加）平台设置里的「连接」「认证」删除——不属于宿主

## 落点与裁决

| 项 | 落点 | 裁决 |
|---|---|---|
| R1 | `ShellHomeScreen.vue` / `ShellHost.vue` / 删 `ShellSystemBar.vue` | 底部系统条整条退役（R3 锁同步改）；返回入口由各非根屏页头承担，多任务页补返回行 |
| R2 | `ShellHomeScreen.vue` | 横向 scroll-snap 轨道（原型 `.slots{display:flex;overflow-x:auto}`），卡片定宽 |
| R3 | `ShellHomeScreen.vue` | icon-only 按钮（adjustments-horizontal），44px 触控目标 + aria-label |
| R4 | `ShellAppRunScreen.vue` | 顶栏（左：图标+名称；右：胶囊 ··· / 多任务 / home） |
| R5 | i18n + 各壳组件 | 删「方案/设计期」文案；保留事后后果说明（如权限拒绝后果） |
| R6 | `ShellSwitcherScreen.vue` + `shell.css` | 固定宽高居中卡片 + 等差叠放（deck），横向滑动切换 / 上滑关闭 / 页头上一个-下一个（拖拽替代路径） |
| R7 | `ShellSettingsScreen.vue` → `AboutSettingsView.vue` | 危险区整段迁移；壳侧清掉 `clearAllData` 桥接（L7 白名单可删） |
| R8 | `components/SettingsSubPage.vue` | inject safeArea，页头补顶部安全区（五个设置二级页 + 插件路由页统一受益） |
| R9 | `ShellSettingsScreen.vue` + 各二级页 | hint 与实际页内容对齐；应用设置项无 handler 时落应用详情（不留死点击） |
| R10 | 壳偏好 + `system/shell_ui.rs` | 宿主命令 `shell_import_image`（选图 + 落 `$APPDATA`，SAF content:// 经 SafIo 中转）；壳只存路径偏好，URL 经 asset protocol |
| R11 | `ShellSettingsScreen.vue` | 「连接」「认证」两项从平台设置删除；底层二级页/路由本批保留（CodeExplorerView 仍引用 + 无应用侧替代），待用户裁决是否整面删 |

## 检索依据（ui-ux-pro-max）

- `--domain ux` "card stack deck overlap z-index" → Z-Index Management（10/20/30/50 标尺）、Stacking Context
- `--domain ux` "horizontal carousel scroll snap peek" → Horizontal Scroll（避免页面级横向滚动；轨道级 snap 属组件内滚动）、Motion Sensitivity（prefers-reduced-motion）
- `--domain icons` "icon button accessible label" → 图标控件必须有可访问名；单图标家族
- `--domain web` "safe area header inset notch" → Safe Area Insets（不得让可点内容压系统栏）
- `--domain ux` "list row action button affordance" → 诚实可用性：可点即须有反馈与去处

## 验证口径

- 移动端 `pnpm run test:run` 全量 + 根 `pnpm exec eslint .` 0 error
- 移动宿主 `cargo test`（bedcode-mobile/src-tauri 根）
- 未跑：真机（自定义壁纸 SAF 选图）/ 完整 wasm 构建（本批零 WIT/ABI 变更）
