# 票 15 · 终端 UI 域下沉 + host-terminal/terminal-hooks 退役

Status: **已实施（阶段 A + 阶段 B，2026-10-08）**（§3 裁决点已拍板并落 D-15a 用户修正案「插件内部全部迁移」；实施与门禁记录见 §4 / §5；阶段 B = host-terminal / terminal-hooks 整面退役 + ABI 16→17 + `terminal:input` 权限位与 manifest-gen 宽推导线清理 + 防回接锁）
专项: `.scratch/2026-10-07-mobile-wasm-core-refactor`（阶段 3 第三票）
依赖: 票 12（`terminal_link` 协议客户端已迁插件，宿主窄转发 `terminal_stream_gateway.rs` + `host-terminal-stream` 为**保留面**）、票 14B（插件 auth 域）、票 16（任务域并入）。
后续: 票 13（会话控制迁插件，复用 `host-connection.primary-target`）会与本票的「默认网格预算」临时桥收口；票 21（真机验收 + 文档收口）。

---

## 0. 一句话目标

把终端消费 UI 域（`TerminalView` / terminalBuffer store / `composables/terminal/*` / 终端组件 / 样式 / 字体 / 文案，约 **9.6k 行 / 34 文件**）从宿主前端整体迁入 `com.bedcode.terminal-session` 插件前端；宿主保留 `/mobile/terminal/:id` **路由薄壳**与**页面 Channel 传输机制**（对插件新增窄通道）；`host-terminal`（send）+ `terminal-hooks` 整面退役（**ABI 16→17**），`TerminalOutput` 生命周期链退役。

---

## 1. 现状实测（2026-10-08 工作区，全部为只读核查）

### 1.1 退役面零消费者证据

| 断言 | 证据 |
| --- | --- |
| `host_terminal::send` WASM 调用点 = 1 | SDK `wasm_host.rs:194-200`（唯一）；全仓其余命中均为定义/注释/文档 |
| 宿主 `terminal_send` 调用点 = 1 | `component.rs:114-118`（WIT Host impl → `host_impl/terminal.rs`）；`terminal:input` 权限门唯一使用点 `host_impl/terminal.rs:12-17` |
| 无任何插件调用 host-terminal | `plugins/{file-transfer,ai-chatbox,terminal-session}` 对 `HostTerminal`/`terminal_send` **0 命中**（terminal-session 只用 `HostTerminalStream`，`link.rs:24`） |
| 无任何插件覆写 terminal-hooks | plugins/ 对 `fn on_terminal_input/on_terminal_output` 0 命中；仅 SDK 默认 `None`（`wasm.rs:42-43`）与组件夹具（`plugin-component-test/src/lib.rs:331-341`） |
| `PluginLifecycleEvent::TerminalInput` 零生产构造 | 全部 dispatch 调用点（`manager.rs:439` / `handler/auth.rs:36` / `session.rs:103,148` / `connection/manager.rs:466` / `router/event.rs:417`）无一处构造；唯一构造是测试 `component.rs:1684` |
| `TerminalOutput` 仅 1 条触发链 | `router/event.rs:401-424` ← 前端 `terminal_output_activity`（`stores/terminalBuffer.ts:306-312` 节流 200ms）← `lib.rs:157` 注册 |
| 前端 `plugin:lifecycle:terminalInput/Output` 监听者 = 1 处 | `src/plugin/context.ts:355/360`（LifecycleAPI）；仓内插件零调用；测试零命中 |
| `TerminalAPI.onOutput` 监听 `terminal:output` **无宿主发射点** | 全仓 `emit('terminal:output')` 0 命中（仅 dev-shell mock 伪造）——悬挂监听（文档承诺兑现不了，按用户偏好退役） |
| `terminal:input` 权限位无代码消费 | 插件声明位 `terminal-session/plugin.json:16` 无代码消费；manifest-gen 由 `/\.terminal\b/` 宽规则自动推导（`bin/manifest-gen.js:24-26/33`）——**终端 UI 迁入插件后源码必然命中该规则 ⇒ 必须清理该规则** |

### 1.2 前端迁移面（域重组清单）

目标目录 `plugins/terminal-session/src/terminal/**`，与 `src/task/**` 并列（入口 `src/index.ts` 域组合）。

| 域 | 文件（行数） | 目标 |
| --- | --- | --- |
| 视图 | `src/views/TerminalView.vue`（764，编排层） | `terminal/TerminalView.vue` |
| store | `src/stores/terminalBuffer.ts`（698，Channel/事件/ack 状态机） | `terminal/store.ts` |
| composables | `terminal/*` 8 文件（1639）+ `useTerminalBuffer`（316）+ `useTerminalScroll`（942）+ `useTuiCompat`（311）+ `useMockTerminal`（231）+ `writeCoalescer`（284）+ `useViewportPanGuard`（121） | `terminal/composables/**` |
| store（输入助手） | `src/stores/inputAssistant.ts`（503） | `terminal/inputAssistant.ts` |
| 组件 | `Terminal*.vue` 6 个（3085）+ `TaskPickerModal`（437）+ `ShortcutConfigModal`（1048）+ `ShortcutHelpModal` + `ConfirmDialog`（136）+ `Toggle` | `terminal/components/**` |
| utils | `terminal*.ts` 6 个 + `nextPaintFrame` + `reconnectCountdown`（+ `clipboard` 自持副本） | `terminal/utils/**` |
| 样式/字体 | `styles/terminal.css`（514）+ `terminal-font.css`（31）+ 字体 woff2（1.05MB）+ LICENSE | `terminal/styles/**` + `terminal/assets/**` |
| config | `terminalThemes`（349）+ `terminalOnboardingSteps`（95）+ `agentPresets`（146） | `terminal/config/**` |
| 文案 | `mobile.terminal.*` 76 键 ×2 + 输入栏/通用键（见 §2.7） | `terminal/i18n.ts` |
| 测试 | 宿主 `src/__tests__` 终端域 15 文件 + 1 夹具 | `terminal/__tests__/**`（宿主 vitest include 扩展） |

**顺带处置（依赖迁走后的硬约束）**：`InputAssistant.vue`（悬浮球，**零消费者死组件**）+ 其独占子组件 `SettingsModal.vue` / `ShortcutPanel.vue` → **删除**（不迁；不删则 import `@/stores/inputAssistant` 编译红）。

### 1.3 宿主设施依赖矩阵（迁移后替代）

| 宿主设施 | 依赖方 | 插件侧替代 |
| --- | --- | --- |
| `@tauri-apps/api` Channel/event/invoke | store（Channel/emit）、TerminalInputBar（2 命令） | **宿主新增 `mobileApi.openTerminalStream`（D-15b）** + `context.events`（listen）+ `context.commands.execute` + `context.storage` |
| `@/stores/terminalBuffer` | 4 文件 | 随迁（同域） |
| `@/stores/inputAssistant` | 4 文件 + ShortcutConfigModal | 随迁（同域；持久化见 §2.6） |
| `@/composables/useMobileConnection` | TerminalView、useTerminalInput | `getMobileApi()`（activeSessions/sessionConfigs/isConnected ✔ 已暴露） |
| `@/composables/useHttpApi`（resize/send input） | useTerminalResize、useTuiCompat | `getMobileApi().httpRequest`（task/api.ts 范式） |
| `@/composables/usePresetTasks` | TerminalView、useTerminalInput、TaskPickerModal | `getPresetTasks()`（共享模块 ✔ 已暴露） |
| `@/composables/useTheme`（明暗） | useTerminalDisplay、TerminalSettingsModal | **`mobileApi` 新增主题只读面（D-15b 同批）** |
| `@/composables/useMobileSettings`（vibrate） | TerminalInputBar | 同上（宿主设置只读面） |
| `@/locales` | useTerminalScroll/useTerminalSubscription | `context.i18n.t` + 插件 i18n（§2.7） |
| `@/plugin/components/PluginTerminalBar` | TerminalHeader | **宿主壳 provide + SDK 消费**（§2.1） |
| `FileSidebar`（1000，宿主 CodeExplorer 共用） | TerminalView | **宿主壳 provide 组件引用**（不迁，§2.1） |
| `inject('safeArea')` | TerminalView、TerminalInputBar | App.vue 已 provide（插件组件经宿主树渲染 ⇒ inject 链天然连通） |
| `@/utils/clipboard` | useTerminalScroll | 插件自持副本（navigator.clipboard + execCommand fallback） |
| `terminalMetrics` 网格预算 | SessionsView（启动会话预热） | **临时桥 `terminal-session.default-grid` 命令**（§2.6，票 13 收口） |

### 1.4 宿主反向依赖（迁移引出的收窄点）

| 调用点 | 语义 | 处置 |
| --- | --- | --- |
| `useMobileConnection.ts:206/416/636/779` | 断线/重置 → `markAllUnsubscribed` | 移插件（§2.6 事件面） |
| `useMobileConnection.ts:343-346/360-361/370-371` | 会话 running/stopped/removed → `markSessionRunning/markSessionStopped/clearBuffer` | 移插件（同上） |
| `SessionsView.vue:264/284` | `mobile-terminal` 导航 | 路由不变，零改动 |
| `SessionsView.vue:372-375` | 启动会话默认网格预算（读终端字号/字间距 + 字体预热） | 临时桥命令（§2.6） |
| `SessionsView.vue:225 + mock 卡片` | DEV mock 会话入口 | `MOCK_SESSION_ID` 常量下沉 SDK（单一事实源）；SessionsView 保留开关显示 |
| `InputAssistant.vue` / `SettingsModal.vue` / `ShortcutPanel.vue` | 死组件（零消费者） | 删除（§1.2） |

---

## 2. 设计

### 2.1 挂载形态（D-15a 推荐案）

```text
宿主路由 /mobile/terminal/:id（保留，零 URL 变更）
  └─ TerminalView.vue（薄壳，~80 行）
       ├─ route.params.id → :session-id
       ├─ 渲染 registry.terminalView 组件（插件注册；未激活时显性报错 + 触发激活）
       ├─ provide 宿主机制（插件经 SDK 消费）：
       │    · 'bedcodeHostComponents' → { FileSidebar, PluginTerminalBar }
       │    · safeArea / safeAreaReady（App.vue 既有 provide，插件树天然可 inject）
       └─ 插件组件 props: { sessionId: string }
```

- **新扩展点** `context.ui.registerTerminalView(component)`：registry 新增 `terminalView` 槽（Disposable 回收，`clearPlugin` 前缀清理），SDK `UIRegistry` 同步；权限位沿用 `ui:input`（工具栏/输入域，不新增词汇）。
- **不选**插件动态路由（`/mobile/plugins/{id}/{route}` 无参数、URL 形状变更、2 处导航调用点与深链全改）。
- 插件组件在宿主 Vue 树内渲染 ⇒ i18n/router/pinia/inject/safeArea/响应式天然连通，`mobile-api` 共享面照旧。
- 宿主壳行为：`HIDE_BOTTOM_NAV_ROUTES`（MobileLayout）不变；直达深链时经 `pluginLoader.activate(pluginId)`（照 router 守卫 pluginRoute 模式）。

### 2.2 帧通道（D-15b 推荐案）

- **宿主 SDK 新增 `mobileApi.openTerminalStream(sessionId, onBytes)`**：宿主内部 `new Channel<ArrayBuffer>()` + `terminal_page_subscribe`（既有传输机制，不新增 Rust 面），返回 `{ dispose }`（内部 `terminal_page_unsubscribe`）。插件 store 只消费「按序字节回调 + dispose」，**不碰 Tauri API**。
- 同批扩展 `mobileApi`（均为宿主注入的只读/机制面，非业务）：
  - `theme: Ref<'light'|'dark'>`（useTheme 等价物，宿主注入）
  - `mobileSettings: Ref<{ vibrate: boolean; ... }>`（宿主 settings 只读投影）
  - `onSessionEvent(handler)`：宿主封装 `ws_sync_session_*` / `ws_unexpected_disconnect` 等事件（**白名单封装，插件不裸听宿主内部事件名**），驱动 §2.6 的 store 联动。
  - `mockSessionId: string | null`（DEV mock 判定；`MOCK_SESSION_ID` 常量下沉 SDK `constants`）。
- 状态/重锚事件（`plugin:com.bedcode.terminal-session:terminal-state|terminal-resync`）继续由插件前端经 `context.events` 接收（插件 WASM 自 emit，既有范式）。
- **保留**（不动的确定面）：`terminal_stream_gateway.rs`、`host-terminal-stream.forward-output`、`host-connection.primary-target`、`terminal:output` 权限位、Tauri 命令 `terminal_page_subscribe/unsubscribe`（传输机制）。

### 2.3 插件命令面（前端内部调用）

宿主 `useMobileCommands.ts` 的 9 个终端封装（subscribe/unsubscribe/unsubscribe-all/remove/send-input/ack-rendered/get-state + page 两命令）中：

- 7 个 `plugin_invoke` 封装 → 迁移后直接 `context.commands.execute('terminal-session.*')`（同插件）；
- 2 个 `terminal_page_subscribe/unsubscribe` → 收敛进 `openTerminalStream`（§2.2）；
- 宿主 `useMobileCommands.ts` 删除终端函数族（`plugin_invoke` 调插件命令的 7 个 wrapper），`initMobileEventListeners` 保留（宿主自身消费）。

### 2.4 迁移目录结构（域重组，非逐文件平移）

```text
plugins/terminal-session/src/
├── index.ts                    # 域组合：activateTerminalDomain + activateTaskDomain + activateAuthDomain(已有 rust 域)
├── terminal/                   # 终端消费 UI 域（本票）
│   ├── activate.ts             # 注册（terminalView / toolbar 数据 / i18n / 样式注入）+ dispose
│   ├── TerminalView.vue        # 编排层（props.sessionId）
│   ├── store.ts                # terminalBuffer store（去 Channel/Tauri 直连）
│   ├── inputAssistant.ts       # 输入助手 store（持久化：localStorage 键不变 + 插件 storage）
│   ├── i18n.ts                 # 终端域文案（zh-CN/en）
│   ├── composables/  components/  utils/  config/  styles/  assets/
│   └── __tests__/              # 随迁测试（宿主 vitest include 覆盖）
└── task/                       # 任务域（票 16，不动）
```

- 迁移即**域重组**：目录按职责归位（不保留历史命名漂移）；样式中 CSS 变量 `--mobile-*` / `--terminal-*` 属宿主 token，继续引用（跨域读 token 是既有事实）。
- 字体：woff2 随插件（vite `new URL(..., import.meta.url)` 资产）+ 运行时动态注入 `@font-face`（CSS 由 `?inline` import 后 document.head 注入，相对 URL 不可用——必须绝对 URL）。
- xterm 依赖：**插件自打包**（`@xterm/xterm` + 4 addons + `marked`，对齐桌面 `wasm-apps/terminal-session` 先例；不改共享模块表，避免三处同步与版本锁步）。
- `agentPresets`：随迁（唯一消费者 inputAssistant store + TerminalInputBar）；`terminalThemes` 的 `settings.appearance.*` label 键 → 插件 i18n 复制。

### 2.5 依赖替换矩阵（迁移时逐文件执行）

| 原依赖 | 替换 |
| --- | --- |
| `new Channel()` + `terminalPageSubscribe` | `mobileApi.openTerminalStream` |
| `listen('plugin:...:terminal-state/resync')` | `context.events.on(...)` |
| `emit('terminal_output_activity')` | **删除**（链退役，§2.9） |
| `invoke('get_all_db_settings_mobile'/'set_db_setting_mobile')`（custom_commands） | `context.storage`（+ 一次性迁移 §2.6） |
| `terminalSubscribe/...`（7 wrapper） | `context.commands.execute('terminal-session.*')` |
| `useMobileConnection()` | `getMobileApi()` |
| `useHttpApi`（resize/send input） | `getMobileApi().httpRequest` |
| `useTheme` / `useMobileSettings` | `mobileApi.theme` / `mobileApi.mobileSettings` |
| `usePresetTasks` | `getPresetTasks()` |
| `@/locales` / `useI18n` + `mobile.terminal.*` | `context.i18n.t('terminal.*')` |
| `@/plugin/components/PluginTerminalBar` | inject `bedcodeHostComponents`（§2.1） |
| `@/utils/clipboard` | 自持副本 |
| `@/utils/frontendLogger` | 插件 logger（`context.logger` 或同构副本；热路径保持克制） |

### 2.6 宿主残留处置

1. **store 联动**（§1.4）：`useMobileConnection` 删除 7 处 terminal store 调用；插件侧改由 `mobileApi.onSessionEvent` 驱动（断线 → markAllUnsubscribed；running/stopped/removed → 对应 mark）。**行为等价性**以现有 `useMobileConnection` 条件逻辑为准（running 仅当未跟踪/已停止才复位；断线保 handler 不注销）。
2. **默认网格预算**：新增插件命令 `terminal-session.default-grid`（返回 `{cols, rows}`，内部 = `ensureTerminalFontLoaded` + `computeDeviceDefaultGridSize`，字号取插件设置）；`SessionsView.handleStartSession` 改经 `pluginInvoke` 调用；**票 13 会话控制迁插件后收口删除该桥**。
3. **设置持久化与迁移**：
   - localStorage 键（`terminal_shortcut_stats` / `input_assistant_settings` / `terminal_shortcut_config` 等）：宿 webview 与插件同源同 localStorage ⇒ **键不变、零迁移**。
   - 宿主 settings DB 两键（`custom_commands` / `agent_type_overrides`）：迁插件 `context.storage`；宿主新增 `terminal_settings_migration.rs`（照 `peer_migration.rs` 先例：setup 幂等读旧键 → 写插件存储 → 删旧键）。旧读路径删除（fail-visible①）。
4. **死组件**：`InputAssistant.vue` / `SettingsModal.vue` / `ShortcutPanel.vue` 删除。
5. **SessionsView**：mock 判定改 SDK 常量（`MOCK_SESSION_ID`）；`mobile.terminal.preparing` 文案保留宿主 i18n；`useMockTerminal` 收缩为宿主 DEV 开关判定（渲染面随插件）。

### 2.7 i18n 处置

- **插件文案**：`mobile.terminal.*` 76 键 ×2 + `mobile.terminalHelp.*` + 输入栏键（`mobile.input.*` 6 键）+ 通用键副本（`common.button.{copy,cancel,confirm}`、`mobile.connection.connectFailed`、`mobile.toolbox.sendFailed`、`mobile.session.mockName`、`desktop.terminal.title`、`mobile.shortcutConfig.title`、`settings.appearance.followSystem` 等）→ `terminal/i18n.ts`（zh-CN/en 同步，经 `context.i18n.registerMessages` 自动前缀）。
- **宿主键删除**：逐键审计消费者后删除纯插件键（真源搬迁①：旧读路径删除）；宿主仍消费的键（如 `mobile.terminal.preparing` SessionsView）保留。
- 门禁：i18n 双语同步 + `i18nPrefixDiscipline.test.ts`。

### 2.8 测试与构建

- 终端域测试 15 文件随迁 `terminal/__tests__/**`；宿主 `vitest.config.ts` include 加 `plugins/terminal-session/src/__tests__/**/*.test.ts`（复用 happy-dom + setup；插件测试经相对路径 import 插件源码，`@` alias 仍指宿主 src 不适用于插件源码——插件源码内部一律相对路径）。
- 插件工程：`vite build`（前端）+ `node scripts/build.js --rust-only`（wasm32 真门禁）+ 宿主 `pnpm run plugins:build -- --plugin com.bedcode.terminal-session`（产物刷新，含 manifest 重生成）。
- 变异自检：新增锁按 §2.9 执行（旁路 → 转红 → 还原 → git diff 复核）。

### 2.9 阶段 B：host-terminal / terminal-hooks 退役（ABI 16→17）

**WIT**（`packages/plugin-sdk-mobile/rust/wit/bedcode.wit`）：删 `host-terminal`（40-42）+ `terminal-hooks`（403-406）+ world 两行（443/461）；abi 注释追加 v17 条目（破坏性：import/export 删除，旧产物实例化期点名缺失 interface = fail-visible②）。

**SDK**：删 `host/terminal.rs` + `host/mod.rs`（29/46/159/180）+ `wasm_host.rs`（16-24/194-200）+ `lib.rs:51` + `wasm.rs`（42-43/105-110/138-143/332-342，顺手修 356 行陈旧「当前 v9」）+ `types.rs`（LifecycleContribution 收窄 216-219/232-233/246-247）+ `types.ts`（TS 契约）+ **`TerminalAPI` 整面退役**（sendInput/onOutput，`context.ts:122-140` + `permission.ts` 映射）——`onOutput` 是悬挂监听（§1.1），按「承诺兑现不了即退役」口径处置。

**宿主**：删 `host_impl/terminal.rs` + `host_impl.rs` 两行注册 + `component.rs`（114-118/411/673-689/834-843 + 测试 1644-1646/1683-1689）+ `plugin/types.rs`（TerminalInput/TerminalOutput 变体 + name/tauri_event_name/to_payload 分支）+ `router/event.rs`（392-424 TerminalOutput listener）+ `lib.rs:157` 注册 + `manager.rs` 注释同步 + 前端 `context.ts`（354-363 LifecycleAPI 两条）。

**权限位**：`terminal:input` 退役（SDK `permission.rs:7/40/64` + `PERMISSION_API_MAP`）；**manifest-gen 清理推导线**（`bin/manifest-gen.js:24/26/33` 的 `terminal:input` 规则 + 201 的 `on_terminal_input` handler 推导）；前端权限词汇（`src/plugin/permission.ts:9-10`、`shell/permissions.ts`、locales `mobile.plugin.perm.terminalInput`）。`terminal:output` 保留（host-terminal-stream 门）。

**全插件重编译**：terminal-session / file-transfer / ai-chatbox + `plugin-component-test` 夹具（ABI 硬编码 76-81 → 17）+ `src-tauri/resources/plugins/mobile/**` 产物刷新。

**新锁** `retired_mobile_host_terminal_hooks_lock.rs`（4 例 + 变异自检）：WIT 零 host-terminal/terminal-hooks/world 行 · SDK/宿主零 HostTerminal/terminal_hooks 接线 · 前端零 LifecycleAPI terminal 两条 + 零 `terminal:input` 词汇 · 反向钉保留面（host-terminal-stream/forward-output/terminal_stream_gateway/primary-target/terminal:output 权限在）。

---

## 3. 裁决点（待用户拍板）

| # | 裁决 | 推荐（★） | 备选 | 影响 |
| --- | --- | --- | --- | --- |
| **D-15a** | 终端页挂载形态 | ★ 宿主 `/mobile/terminal/:id` 保留**薄壳** + 新扩展点 `registerTerminalView` 插件注册主视图（URL/导航/深链零变更） | ① 插件动态路由（URL 形状与 2 处导航调用点全改）② 插件自挂载全屏面板（脱离路由/返回语义） | 路由表、导航调用点、深链兼容 |
| **D-15b** | 帧通道归属 | ★ 宿主 SDK `mobileApi.openTerminalStream`（Channel 由宿主创建，字节流 + dispose 交插件；同批注入 theme/mobileSettings/onSessionEvent） | ① 帧管道（store 半份）留宿主、插件只渲染 ② 插件经裸 `@tauri-apps/api`（违反插件边界，不建议） | store 迁移完整度、SDK 契约面 |
| **D-15c** | 输入助手/终端设置一族 | ★ 随终端域**整体迁插件**（store + 终端设置持久化迁插件；`custom_commands`/`agent_type_overrides` 两 DB 键一次性迁移；死组件删除） | ① 设置留宿主、插件经窄接口读写（双真源风险） ② 只迁 UI 不迁设置（插件依赖宿主 store 形状） | 迁移范围、宿主设置页联动 |
| **D-15d** | `terminal:input` 权限位 + SDK `TerminalAPI` | ★ 随 host-terminal **整面退役**（含 manifest-gen 宽推导线清理——否则插件源码 `.terminal` 子串会自动加回） | ① 保留词汇与非门控 TerminalAPI（留死位/悬挂面） | ABI 17 面、权限词典、manifest-gen |

未列入裁决（按 spec 既有口径/最小改动执行，如与预期不符请一并指出）：
- xterm 自打包（对齐桌面 wasm-app 先例）、字体随插件、共享模块表不动；
- 测试随迁 + 宿主 vitest include 扩展（不新建插件 vitest 工程）；
- 宿主 i18n 纯插件键删除、宿主残留消费者键保留；
- A→B 两阶段连续实施（同票内），B 完成后全插件重编译。

---

## 4. 实施记录（2026-10-08）

### 裁决落定（用户，2026-10-08）

- **D-15a**：**插件内部全部迁移**——终端 UI（含页面编排）整体入插件，**宿主的旧前端壳将被新壳（`/mobile/shell`）替换**，本票不为旧壳新增长期机制，只保留最小薄壳与宿主机制 provide。
- D-15b/D-15c/D-15d 按推荐案：`mobileApi.openTerminalStream` / 设置一族随迁 / `terminal:input` + SDK `TerminalAPI` 随 host-terminal 整面退役（后者属阶段 B）。

### 阶段 A · 终端 UI 域下沉（已完成）

**宿主新增机制面（薄、无业务语义）**：

1. `src/plugin/terminal-stream.ts`（新）：`openTerminalStream(sessionId, onBytes)` —— 宿主内部 `new Channel` + `terminal_page_subscribe` 登记，向插件交「按序字节回调 + dispose」（页通道传输机制，非业务）。
2. `src/plugin/host-events.ts`（新）：`subscribeHostSessionEvents` —— 白名单事件（`ws_disconnected` / `ws_unexpected_disconnect` / `ws_sync_session_status_changed|stopped|removed`）+ **`isConnected` 翻转**（覆盖手动断开/切换设备前置清理路径）→ `MobileHostSessionEvent`（插件不裸听宿主内部事件名）。
3. `src/plugin/registry.ts` + `context.ts` + `permission.ts`（`ui:input`）：新扩展点 **`registerTerminalView`**（终端主视图单实例槽，宿主 `/mobile/terminal/:id` 薄壳与宿主壳运行面共同消费）。
4. `mobileApi` 扩展（SDK `MobileHostApi` + 宿主注入 + dev-shell mock）：`openTerminalStream` / `isDark`（`useTheme` 新导出 `useIsDark`）/ `mobileSettings` / `onSessionEvent` / `mockSessionId`（DEV）/ `loadActiveSessions` / `loadSessionConfigs` / `hasLoadedConfigs` / `isLoadingConfigs`。
5. SDK：`MOCK_SESSION_ID` 常量下沉（`src/constants.ts`，宿主与插件单一事实源）；`TerminalViewContribution` / `TerminalStreamHandle` / `MobileHostSessionEvent` 类型；`UIRegistry.registerTerminalView`。

**插件侧（`plugins/terminal-session/src/terminal/**`，47 文件 / 约 9.6k 行）**：

- 域组织：`TerminalView.vue`（编排层，props.sessionId 优先、缺省回落 `mobileApi.activeSessionId`，宿主壳渲染）· `store.ts`（terminalBuffer：命令面经 `context.commands`、事件经 `context.events`、页通道经 `mobileApi.openTerminalStream`——**页面代际号**保证异步登记完成时旧代立即注销）· `inputAssistant.ts`（持久化迁插件 storage）· `composables/`（13）· `components/`（13）· `utils/`（9）· `config/`（3）· `styles/`（terminal.css / markdown-body.css）· `assets/`（help md ×4）。
- `host.ts`（新）：域宿主接入点（`t` / `invokeTerminal` / `storage` / `events` / `logger`（变参含 `log`）/ `toast`；未初始化走降级实现，纯逻辑模块可独立加载）。
- `api.ts`（新）：域 HTTP 面（`httpResizeSession` / `httpSendSessionInput`，经 `mobileApi.httpRequest`）。
- `activate.ts`（新）：注入域服务 + 注册域文案（`i18n.ts`，脚本自宿主 locales 精确提取 **127 键 ×2**）+ `registerTerminalView` + 全域样式注入（xterm.css / terminal.css / markdown-body.css，`?inline` 运行时挂载）+ **会话/连接生命周期 → 订阅状态联动**（语义与迁移前 `useMobileConnection` 内 4 处逐条一致）。
- `i18n.ts`（新，脚本生成）：键映射 `terminal.* ← mobile.terminal.*` · `theme.* ← settings.appearance.*` · `common.*` / `connection` / `session` / `toolbox` / `presetTask` / `taskPicker` / `input` / `shortcutConfig` / `shortcutHelp` 同名子树。
- 依赖：`@xterm/*` 5 包 + `marked`（自打包，对齐桌面 wasm-app 先例）+ `pinia`（devDep，类型解析与宿主同版本；运行时仍走共享实例）。
- 宿主组件经 `inject('bedcodeHostComponents')` 消费：`FileSidebar` / `PluginTerminalBar`（宿主壳 provide）；`FileExplorer`（TaskEditDialog 内部）同款（代码浏览域不随迁）。

**宿主收窄与切换**：

- `src/views/TerminalView.vue` → **薄壳**（~80 行：路由 id → props、宿主机制 provide、插件激活、渲染 `registry.terminalView`）。
- `useMobileConnection.ts`：删 7 处终端 buffer 联动（迁插件 activate 的事件桥）。
- `useMobileCommands.ts`：删 7 个终端协议 wrapper + `TerminalLinkState`；**保留** `terminal_page_subscribe/unsubscribe`（宿主传输机制）与 `TERMINAL_PLUGIN_ID`（票 14 认证编排共用）。
- `SessionsView.vue`：`prepareSession` → 插件命令转发（`pluginInvoke('terminal-session.subscribe')`，fire-and-forget）；mock 入口改 `@/utils/mockSession`（新）；启动会话网格预算随终端域迁出（见偏差）。`CodeViewerSettingsModal` 的 `resolveThemeLabel` 改 `@/utils/themeLabel`（新，宿主版）。
- **删除**（迁走后零引用）：宿主 terminal 域 34 文件 + 3 个零消费者死组件（`InputAssistant.vue` / `SettingsModal.vue` / `ShortcutPanel.vue`）；**保留** `Toggle.vue` / `ConfirmDialog.vue`（宿主多页在用）与 `clipboard.ts` / `terminal-font.css` / 字体 woff2（app 级资源）。
- 宿主 locale 键：**暂未删除**（见偏差）。

**测试与产物**：

- 终端域测试 21 文件随迁 `terminal/__tests__/**`（mock 面适配：`vi.mock('../host')` 命令分发 + SDK `getMobileApi` 替身 + `testKit.ts` 注入器；`terminal_output_activity` 契约随链退役删除）。
- 宿主 `vitest.config.ts` include 扩展（插件测试目录由宿主统一驱动）；宿主旧测试与 `integration/fixtures/terminalInputBarBlurHost.vue` 删除。插件 `tsconfig.json` 的 `exclude` 补 `src/**/__tests__/**`（与宿主口径对齐：测试目录不进 `vue-tsc`，由 vitest 运行时检查）。
- 插件产物刷新：`pnpm run plugins:build -- --plugin com.bedcode.terminal-session`（wasm 522,366 B / index.js 911,023 B / manifest 重生成，已复制 `src-tauri/resources/plugins/mobile/`）。

### 阶段 B · host-terminal / terminal-hooks 退役（已完成，2026-10-08）

按 §2.9 清单逐项执行（WIT / SDK / 宿主 / 权限位 / manifest-gen / 夹具 / 插件 / 锁）：

**WIT（`bedcode-mobile/packages/plugin-sdk-mobile/rust/wit/bedcode.wit`）**：

- 删 `interface host-terminal`（send）+ `interface terminal-hooks`（on-terminal-input / on-terminal-output）+ world 的 `import host-terminal;` / `export terminal-hooks;` 两行
- abi 注释追加 v17 条目（破坏性：import/export 删除，≤v16 产物实例化期点名缺失 interface = fail-visible ②；`terminal:output` 保留）

**SDK（`bedcode-mobile/packages/plugin-sdk-mobile/`）**：

- Rust：`ABI_VERSION` 16→17（v17 演进注释 + 契约测试同步）；删 `host/terminal.rs`（HostTerminal trait）与 `terminal.rs`（TerminalHandler trait）整文件；`host/mod.rs` 删模块声明 / re-export / `HostApi` 约束；`wasm_host.rs` 删 import 与 impl；`lib.rs` 删 `pub mod terminal` / re-export；`wasm.rs` 删 `WasmPlugin.on_terminal_input/on_terminal_output` 默认方法 + `wasm_entry!` 的 terminal-hooks Guest impl + 对应测试，顺手修陈旧注释「当前 v9」、6 组→5 组；`types.rs` 的 `LifecycleContribution` 删 on_terminal_input/output、`TerminalContribution` 删 inputHandlers/outputParsers；`permission.rs` 删 `PERMISSION_TERMINAL_INPUT` 常量 + `VALID_PERMISSIONS` 条目 + API_MAP 行（`terminal:output` 映射收窄为 `terminal-stream.forwardOutput`）+ 测试样本换保留词汇；`traits.rs` 删 `BedcodePlugin.terminal_handlers` 扩展点（native 插件面同族，零消费者，票 §2.9 清单外的同批删面——WIT 契约删除后是死码）；`context.rs` 测试样本换 `mdns`；`tests/{lifecyclecontribution,pluginmanifest,contributions}.rs` 样本同步
- TS：`types.ts` 删 `TerminalAPI` 接口 / `LifecycleAPI.onTerminal*` 两条 / `TerminalContribution` handlers 字段 / `LifecycleContribution.onTerminal*` / `PluginContext.terminal`；`index.ts` 删 `TerminalAPI` 导出；README 删 `context.terminal` 行
- dev-shell：`mock-context.ts` 删 terminal mock 对象 + lifecycle 两条 + context 组装字段；`mock/session.ts` 删 `plugin:lifecycle:terminalInput/terminalOutput` 发射（`terminal:output` 事件保留，供 `openTerminalStream` mock）；`MockTerminalView.vue` / `locales.ts` / `README.md` 注释文案同步

**宿主（`bedcode-mobile/src-tauri/`）**：

- 删 `plugin/wasm_runtime/host_impl/terminal.rs`（terminal_send 权限门 + HTTP 逻辑层）整文件；`host_impl.rs` 删模块声明与 re-export
- `component.rs`：删 `host_terminal::Host` impl（留退役记账注释）+ `add_to_linker` 行 + `on_terminal_input/on_terminal_output` 导出调用方法 + `call_lifecycle_event` 的 TerminalInput/TerminalOutput 分支 + 测试两处
- `plugin/types.rs`：`PluginLifecycleEvent` 删 TerminalInput/TerminalOutput 变体 + name/tauri_event_name/to_payload 三分支（留退役记账注释）
- `router/event.rs`：删 `terminal_output_activity` listener 整段（`init_terminal_output_listener`）；`lib.rs` 删注册块
- `manager.rs`：dispatch 注释同步；审批闸门测试样本 `terminal:input` → `terminal:output`
- `wasm_runtime.rs`：模块注释同步

**权限位与推导线**：

- manifest-gen（`bin/manifest-gen.js`）：删 FRONTEND 三条（`\.terminal\.sendInput` / `\.terminal\.onOutput` / `\.terminal\b` 宽规则）+ RUST `\bterminal_send\b` + `extractTerminalHandlers` 函数及调用（头注释同步）
- 前端：`plugin/permission.ts` 删 `terminal:input` 行、`terminal:output` 收窄 `terminal-stream.forwardOutput`；`shell/permissions.ts` 删映射与分组条目；`shell/types.ts` / `plugin/sessionCommands.ts` 注释同步；locales zh-CN/en 删 `mobile.plugin.perm.terminalInput`（terminalOutput 说明对齐窄转发语义）；`views/PluginView.vue` 删 `PERMISSION_META.terminal:input` 条目（新锁首跑转红逮到的漏网点）
- 插件：`plugins/terminal-session/plugin.json` 删 `"terminal:input"`；`rust/src/session.rs` 注释同步
- 夹具：`plugin-component-test` 删 `TerminalHooksGuest` import + impl（留记账注释），ABI 硬编码 16→17

**新锁** `retired_mobile_host_terminal_hooks_lock.rs`（4 例 + 变异自检 3/3）：

1. `retired_host_terminal_interfaces_are_absent_from_wit`——WIT 零 host-terminal / terminal-hooks（接口行 + world 行；带尾随空格 needle 防 `host-terminal-stream` 误伤）
2. `retired_host_terminal_rust_wiring_is_absent`——SDK + 宿主两侧 Rust 零 `HostTerminal`（trait/impl/尾随逗号 import 形态）/ `TerminalHandler` / `terminal_handlers(` / `terminal_send(` / `host_terminal::send` / `PERMISSION_TERMINAL_INPUT` / `"terminal:input"` 字面量（带边界 needle 防 `HostTerminalStream` / `host_terminal_stream` 误伤；scan 跳过 `//`、`/*`、`*` 三种注释行——记账注释不算回接）
3. `frontend_has_no_retired_terminal_api_or_permission`——前端零 `TerminalAPI` / `terminal:input` / `terminal.sendInput|onInput|onOutput` / `onTerminalInput|onTerminalOutput` / `plugin:lifecycle:terminalInput`（排除 `__tests__`：测试是锁的消费者不是回接面）
4. `mobile_terminal_stream_retained_face_stays`——反向钉保留面：WIT `host-terminal-stream` 接口 + `forward-output` + `primary-target` 原语、SDK `PERMISSION_TERMINAL_OUTPUT`、宿主 `terminal_stream_gateway.forward_output` + `host_impl/terminal_stream.rs` 权限门

**变异自检 3/3**（注入 → 转红 → 还原 → 复绿 + git diff 复核无残留）：WIT 注入 `interface host-terminal` → 锁 1 红；宿主 `approval.rs` 注入 `PERMISSION_TERMINAL_INPUT` 常量 → 锁 2 红（双 needle 命中）；前端 `mockSession.ts` 注入 `'terminal:input'` + `TerminalAPI` → 锁 3 红（双命中）。第 4 例保留面锁与票 12 锁同款形态（该形态已被票 12/13/14 多次变异验证）。

**产物**：terminal-session（wasm 组件 519,601 B）+ file-transfer（720,797 B）+ ai-chatbox（542,031 B）三插件全部重编译刷新至 ABI 17 并复制 `src-tauri/resources/plugins/mobile/`；wasm 内嵌 manifest 经二进制 grep 核验——含 `terminal:output` / `terminal-session.subscribe`（正向对照命中），零 `terminal:input`。

按 §2.9 清单执行：WIT 删面（host-terminal / terminal-hooks / world 两行）+ ABI **16→17** + SDK 删面（`HostTerminal` / hooks / `LifecycleContribution` 收窄 / `TerminalAPI` 整面退役）+ 宿主删面（`host_impl/terminal.rs`、component 接线、`PluginLifecycleEvent::TerminalInput/TerminalOutput`、`router/event.rs` 的 `terminal_output_activity` listener、`context.ts` LifecycleAPI 两条）+ `terminal:input` 权限位退役（含 manifest-gen 宽推导线清理）+ 全插件重编译 + 新锁 `retired_mobile_host_terminal_hooks_lock.rs`。**排期理由**：阶段 A 已使该退役面「零消费者」成立（行为零损失）；WIT/SDK/宿主 Rust 为单点文件，避开并行会话（票 13 会话控制迁插件正在途）的运行窗口。

---

## 5. 门禁（阶段 A）

| 项 | 结果 |
| --- | --- |
| 宿主 `vue-tsc --noEmit` | **0 新增错误**（仅 `EgressSettingsView.vue` 6 处并行会话在途基线） |
| 插件 `vue-tsc -p tsconfig.json` | **0 error** |
| 插件 `vite build` | ✓ 911.03 kB（gzip 245.5 kB） |
| 根 `pnpm exec eslint .` | **0 error**（113 warnings，均为存量/迁移带入的 attributes-order 等；我新增文件单独复验 0） |
| 插件测试（`vitest run plugins/terminal-session/src/terminal/__tests__`） | **20 文件 / 257 用例全绿** |
| 宿主测试（分目录批次） | stores/composables/config/utils 11 文件 134 全绿；components/views/shell/services/scripts/fixtures 16 文件 175 全绿；integration/plugin/plugins 除 **6 个失败 = 并行会话（票 13 会话控制迁插件）在途基线**（`sessionCommands` 经 `plugin_invoke` 的 mock 缺口：`start/stop/remove` + `sendHttpInput` 拿 undefined），其余全绿 |
| 插件真门禁（wasm32 + componentize + manifest + 产物复制） | ✓（见上） |
| 宿主 Rust | **零改动**（本阶段不动 `src-tauri/src/**`；`cargo test` 未跑，理由：无 Rust 面变更） |

### 门禁（阶段 B）

| 项 | 结果 |
| --- | --- |
| SDK crate `cargo test --features wasm` | **76 用例全绿**（61 lib + 6 集成 + 4 内嵌 + 3 traits 等） |
| 插件 native `cargo test`（terminal-session/rust） | **50/50 绿** |
| 宿主 `cargo test --lib` | **314 通过 / 6 失败 = `egress.rs` 票 20 在途基线（同数同款）**，0 新增 |
| 宿主 `cargo test --tests --no-fail-fast` | **全部集成目标绿**（build_manifest_smoke / http_auth_flow 17 / http_proxy_flow 7 / ws_client_domain_lock 2 / mock_plugin_ws_fixture 14 / session_http_flow / ws_protocol_integration / 全部退役锁回归 22 用例——含新锁 4/4 与阶段 A 前锁回归） |
| 新锁变异自检 | **3/3**（WIT / Rust 接线 / 前端词汇注入均转红，还原后复绿，git diff 复核无残留） |
| 插件 wasm32 真门禁 | terminal-session ✓（组件 519,601 B）；产物刷新 + file-transfer / ai-chatbox 同批重编译（720,797 / 542,031 B） |
| wasm 内嵌 manifest 核验 | 二进制 grep：含 `terminal:output` / `terminal-session.subscribe` 正向命中，零 `terminal:input` |
| 宿主前端 `vue-tsc --noEmit` | **0 新增错误**（仅 `EgressSettingsView.vue` 6 处并行会话在途基线，与阶段 A 同款） |
| 前端全量 vitest | **66 文件 / 720 用例全绿**（`pluginContextHttp.test.ts` 3 例 TerminalAPI 用例改写为退役面断言；`useMdnsDiscovery` 1 例复跑通过 = flaky） |
| 根 `pnpm exec eslint .` | **0 error**（110 warnings，均为存量） |
| `cargo fmt` / `clippy` | fmt：`--check` 噪音为存量（SDK 多文件 CRLF + 历史风格，纪律禁整 crate fmt），本票新增文件对齐现有锁文件风格；clippy 转后台执行（改动面无新增 warning 预期） |
| 未跑 | 真机三流往返（留票 21）；`cross-end-tests`（无跨端 wire 变更——本票纯移动端插件契约删面） |

---

## 6. 风险与偏差

| 风险 | 吸收 |
| --- | --- |
| 9.6k 行迁移引入行为回归（订阅/重锚/ack/键盘避让/滚动） | 测试随迁全绿为门禁；store/订阅状态机不改逻辑只换依赖；真机三流往返留票 21 |
| 插件产物膨胀（xterm + 字体 + 终端 UI ≈ 1.5MB+） | 单文件 index.js + 独立 assets；本地加载无网络成本；超限则评估 xterm 入共享表（另立） |
| manifest-gen 权限误推导（`.terminal` 子串） | D-15d 同批清理宽规则；构建后核对插件权限集零 `terminal:input` |
| 宿主事件面（`onSessionEvent`）成为新的隐式契约 | 白名单封装 + 文档登记（code-map + 插件开发清单）；不裸暴露事件名 |
| SessionsView 预热/网格预算桥临时性 | 票 13 会话控制迁插件后收口删除；本票在票文档点名 |
| 并行会话冲突（单点文件 lib.rs / registry / types.ts / plugin.json / WIT） | 开工前 `git status` + `find -newermt`；写前重读、写后复查（票 12/14 教训） |

### 阶段 A 实施偏差（如实记录，均已在代码注释同步）

| 偏差 | 内容 | 理由 / 收口 |
| --- | --- | --- |
| 字体文件留宿主 | `terminal-font.css`（@font-face）+ woff2 仍由宿主 `main.ts` 全局引入；插件只持字体族名（`FONT_FAMILY`）与就绪等待逻辑 | 字体是 app 级共享资源；`?inline` 注入后 CSS 内相对 `url()` 失效、`?url` 在插件 asset 协议下的解析需额外机制。旧壳退役时随「字体声明归属」一并重估 |
| 会话预热不等待就绪 | `SessionsView` 的 `prepareSession`（等待 subscribed ≤8s）→ 插件命令 fire-and-forget（订阅已发起、链路异步建立；终端页挂载 fresh subscribe + 重试兜底） | 宿主已无从观察插件链路状态；行为损失为「跳转可能少省一次握手」，无功能回退 |
| 启动会话网格预算回退 | 「按屏幕预算初始网格」随终端域迁出，`startSession` 不再带 size（主机缺省尺寸起步，终端页挂载后 fit 校准） | 预算依赖终端域设置与字体测量；票 13 会话控制迁插件后在插件内闭环（主机首帧回绕窗口重现，真机留观） |
| 宿主 locale 纯插件键未删 | `mobile.terminal.*` 等 127 键在宿主 locales 成零消费者死键 | locale 文件有并行会话在途改动（撞车风险）+ 删除涉面广；插件已是文案真源，死键无行为影响——收口随票 21 文档收尾 |
| `Toggle` / `ConfirmDialog` / `RepeatableToggle` 双份 | 宿主保留（多页在用）+ 插件自持副本 | 通用 UI 小件；跨工程共享组件无既有机制，双份优于引入新共享面 |
