# 移动端宿主壳 Android 平台化改造

> 用户指令（2026-10-14）：移动端宿主前端改造，当作类似的安卓系统平台来改造——主页面和桌面端
> 前端类似、切换应用是类似安卓的卡片式切换、有平台全局的设置、授权管理、能够设置主界面背景图、
> 有独立的通知栏。
>
> 状态：实施中（v1 前端批次）。落点全部在 `bedcode-mobile/src/shell/**`，v1 零 Rust 改动。

---

## 0. 事实基线（2026-10-14 实测）

| 事实 | 证据 |
| --- | --- |
| 移动端当前前端 = 新壳（`b84767916` 整面覆盖后），旧前端已退役 | `git log -- src/shell`；`src/views/` 只剩设置子页 + 文件浏览器（standalone 路由） |
| 壳内已有：首页 / 应用管理 / 应用详情 / 运行面 / 多任务列表 / 权限总览 / 我的（7 屏） | `src/shell/composables/useShellNavigation.ts` SCREENS 表 |
| 壳内已有底部 Tab（首页/应用/我的） | `components/ShellTabbar.vue` |
| 多任务现状 = 运行中应用列表（无卡片、无预览、无手势） | `components/screens/ShellSwitcherScreen.vue` |
| 通知面现状 = 无；只有授权底部弹窗（`ShellPermissionPrompt`）与胶囊菜单 | `composables/useShellOverlays.ts` |
| 无壁纸 / 无壳 UI 偏好持久化（只有 `recentIds` 落 localStorage） | `composables/useShellRecent.ts` |
| 后端**不支持**逐项权限开关（`setPermissionGrant` 显性返回 false） | `adapters/pluginAppSource.ts:173` |
| 后端审批 = 整单（manifest 全量批准 + 内容哈希钉扎），命令 `plugin_approve` | `src-tauri/src/plugin/approval.rs` |
| 宿主事件（可喂通知栏）：`ws_connected` / `ws_reconnected` / `ws_reauth_rejected` | `src-tauri/src/connection/manager.rs` |
| 桌面壳（方案 C，另一会话在途）已定：主屏 = 运行卡片带 + 应用网格 + 系统网格；壁纸 token 派生；通知中心 | `.scratch/2026-10-08-desktop-shell-redesign/spec-c.md` v0.3 |
| 前端资源访问红线：壳内禁 fetch/fs/裸 invoke；资产协议 scope = `$APPDATA/$APPCONFIG/$RESOURCE` | 根 `eslint.config.js` + `src/__tests__/shell/shellConstraintLocks.test.ts` L1/L2 + `src-tauri/tauri.conf.json` |
| 壳内自足锁 L7：禁 `@/components|composables|views` import（白名单仅 `ShellSettingsScreen.vue`） | `shellConstraintLocks.test.ts` L7 |

---

## 1. 设计（v1）

### D1 主屏（对齐桌面方案 C 的主屏语义）

自上而下：**壁纸层**（渐变 + 遮罩）→ 头部（品牌 / 本机 / 通知铃铛）→ **正在运行卡片带**（MRU，
点切换、× 停止）→ 快捷卡片（应用 `registerSlot` 贡献，保留既有扩展点）→ **应用宫格**（全部应用，
状态点 + 审批角标）→ **系统宫格**（应用管理 / 权限管理 / 平台设置，桌面同构）。

- 桌面「运行卡片带 + 应用网格 + 系统网格」三段结构在移动端逐段对应（移动端应用少，网格 4 列）。
- 原「最近使用」chips 区删除：多任务屏承接（卡片式），主屏不再重复一份。

### D2 平台导航（去底部 Tab，改 Android 系统条）

- **删除底部 Tab 栏**（首页/应用/我的）——Android launcher 无底部页签；系统页入口进主屏系统宫格。
- 新增**底部系统条**（`ShellSystemBar.vue`，全局唯一）：点击 = 回主屏；上滑 = 打开多任务。
  - WCAG 2.2 Dragging Movements：拖拽**不是唯一路径**——主屏头部有「多任务」按钮、应用运行面
    胶囊右侧有 homebar，键盘/单指点击可达（`ui-ux-pro-max --domain ux` 检索结果 1）。
  - 手势只在系统条自身响应（垂直位移 > 阈值判上滑），不抢页面滚动（同检索结果 2：避免
    与系统/内容手势冲突）。
- 各非根屏保留顶部返回（应用详情 / 权限总览已有；应用管理 / 平台设置补返回行）。

### D3 多任务（Android 卡片式切换）

`ShellSwitcherScreen.vue` 重写：

- **运行中应用**在前（MRU：最近打开的在前），**最近使用但未运行**的应用在后（灰态），
  两者都是大卡片；卡片含：应用图标 + 名称 + 状态 chip + 关闭按钮 + 预览位。
- **预览位诚实留白**：平台不持有应用内部快照，卡片给「运行中 / 未运行 + 权限数 + 应用 id」
  而不是伪造截图（与既有实现同口径，AGENTS §5.1 B4）。
- 交互：点卡片进入；`×` 关闭（运行中 = 停用实例，未运行 = 从最近列表移除）；「全部关闭」；
  卡片上滑手势关闭（拖拽替代路径 = `×` 按钮）。
- 空态保留（说明 + 出口），不再出现「上滑卡片停止应用」这类与实际能力不符的文案。

### D4 通知栏（独立面）

新增 `components/ShellNotificationShade.vue` + `composables/useShellNotifications.ts`：

- **独立覆盖层**：从屏幕顶部下拉展开（Teleport 到壳根，遮罩 + 面板），再次下拉/上滑/点击遮罩/
  Esc 关闭；`role="dialog"` + `aria-modal` + 焦点回到入口。
- **入口**：① 壳状态条（系统状态栏不可见时渲染的那条）点击 ② 主屏头部铃铛（带未读角标）。
  两处都带 `aria-label` 与条目数，键盘可达。
- **内容**（全部有真实来源，无假开关）：
  - **待处理**：待审批应用（由应用状态 `needsApproval` 派生，批准后自然消失；「去审批」直达详情）。
  - **通知**：宿主事件卡片——应用生命周期（启动 / 停止 / 审批通过 / 安装 / 卸载 / 启动失败）、
    连接事件（`ws_connected` / `ws_reconnected` / `ws_reauth_rejected`）；来源图标 + 标题 + 内容 +
    相对时间 + 单条关闭；「清除全部」只清非驻留项。
- **v1 不做快捷开关**：能真驱动的平台开关（主题 / 防休眠 / 链路加密）都在 `@/composables/**`，
  壳按 L7 不可直连；做假 toggle 违反诚实原则。后续若要，先给壳补机制副本或白名单桥接再上。
- 通知**不落库**：内存态（生命周期/连接事件的可见面），刷新即清空——避免宿主持业务真源（B3）。

### D5 平台设置（我的）

新增**「背景」分组**：壁纸四选（无 / 极光 / 沙丘 / 礁湖，token 派生渐变预览）+ 遮罩强度滑杆
（0–70%，默认 30%）。其余分组（平台 / 应用设置 / 危险区）保持。偏好落 localStorage
（与桌面壳同口径：壳 UI 偏好不写宿主 AppConfig，引擎面零壳词汇）。

### D6 授权管理（权限）

- 权限总览页新增**待审批**分区（有待审批应用时置顶，一键「批准并启动」= `plugin_approve` + `launch`）。
- 权限分组保留（终端与会话 / 文件与网络 / 设备与链路 / 界面与消息 / 其他），高危权限词加标记。
- 逐项开关维持只读并显性说明（后端无该能力，`permissionUnsupported`）——不做假成功。

### D7 壁纸

- 内置四档：`none`（纯 `--mobile-bg-primary`）/ `aurora` / `dune` / `reef`，全部 `color-mix()`
  自既有语义 token（零新 hex），双主题自动适配；定义落 `src/shell/styles/shell.css`（壳 token
  唯一落点，L6 已豁免该文件）。
- 遮罩：`--wall-dim` 0–70%，主屏内容层之下、壁纸之上。
- **自定义图片（开放项）**：需新增宿主命令（SAF 选图 → 拷贝到 `$APPDATA` → 资产协议 URL 下发），
  v1 不做——前端零资源访问红线（AGENTS §6）下壳内无法自行读盘。待用户裁决后再立 Rust 批次。

### D8 动效与无障碍

- 全部 property-specific（`transform` / `opacity`），≤300ms；`prefers-reduced-motion` 压平。
- 触控目标 ≥44px（`--mobile-touch-target-min`），相邻目标间距 ≥8px（ui-ux-pro-max `--domain ux`
  Touch Target Size / Touch Spacing）。
- 通知栏 / 多任务卡片 / 壁纸选择全部有可访问名称与键盘路径。

---

## 2. 批次

| 批次 | 内容 | 状态 |
| --- | --- | --- |
| B1 | 壳 UI 偏好（壁纸 / 遮罩）+ 壁纸 token + 主屏重排（运行卡片带 / 应用宫格 / 系统宫格） | 已完成 |
| B2 | 底部系统条（去 Tab）+ 多任务卡片式重写 | 已完成 |
| B3 | 通知栏（状态 + 组件 + 事件桥 + 状态条/铃铛入口） | 已完成 |
| B4 | 平台设置「背景」分组 + 授权管理待审批分区 + 应用详情批准动作 | 已完成 |
| B5 | 测试（壳锁更新 + 新增六套单测）与 i18n 双语 | 已完成 |
| B6 | 自定义壁纸图片（宿主命令 + SAF + 资产 URL） | 待裁决（需 Rust） |
| B7 | `packages/plugin-sdk-mobile/dev-shell` 同步新壳形态 | 未开始（开发预览用，另行对齐） |

### 验证证据（2026-10-14）

| 项 | 命令 | 结果 |
| --- | --- | --- |
| 类型 | `npx vue-tsc --noEmit -p tsconfig.json`（移动端） | 0 error |
| 全量前端测试 | `pnpm run test:run`（移动端） | 84 文件 / 968 用例全绿 |
| Lint | 根 `pnpm exec eslint .` | 0 error（98 warning 均为既有；壳与新增测试零 warning） |
| 新增测试 | `src/__tests__/shell/{shellNotifications,shellPreferences,shellRecents,shellSystemBar,shellHomeScreen,shellHostEvents,shellAppActions}.test.ts` | 79 用例 |
| 更新测试 | `pluginAppSource`（approval / install 返回 id）、`shellSettingsScreen`（背景分组 + 返回键 + 按 key 定位）、`retiredHostUIRetirementLocks`（壳面清单）、`shellConstraintLocks`（L4 注释遮罩） | 全绿 |
| 未跑 | wasm 完整构建（wasmHash）/ 真机核验 / cross-end-tests | 本批零 Rust、零跨端协议改动，故未跑 |

---

## 3. 归属自检（AGENTS §5.1 B1–B6）

| 判据 | 结论 |
| --- | --- |
| B1 产品类型/字段 | 无。壳数据形状 = 应用注册表投影 + 壁纸/遮罩 UI 偏好 + 通知事件（机制词汇） |
| B2 业务编排 | 无。多任务卡片 MRU 是平台机制（时间序），不解释业务状态 |
| B3 业务真源 | 无。通知不落库；壁纸偏好是壳 UI 偏好（localStorage，非业务事实） |
| B4 业务投影 | 无。通知文案由来源方（宿主事件 / 壳动作）给出；卡片预览不伪造应用内容 |
| B5 业务默认值 | 无。壁纸默认档是视觉默认，不替插件决定业务行为 |
| B6 业务生命周期挂钩 | 无。壳只对**自身动作**（启动/停止/审批/安装）落通知；不解释插件业务事件 |

---

## 4. 开放项（需用户裁决）

1. **自定义壁纸图片**（D7）：是否立 Rust 批次（宿主命令 + SAF 选图 + `$APPDATA` 落盘 + 资产 URL）。
2. **通知栏快捷开关**（D4）：是否允许壳桥接 `@/composables/**` 的平台开关（主题/防休眠/链路加密），
   或先给壳做机制副本。
3. **底部系统条的手势强度**：v1 = 点击回主屏 + 上滑多任务；是否再加「从屏幕左右边缘滑动返回」
   （Android 手势导航，WebView 内与系统返回手势可能冲突，需真机验证）。
