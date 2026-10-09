# 2026-10-09 移动端前端宿主迁移 + 三 wasm-app 独立页面化 — spec v0.1

> 状态：规划中 · 用户指令（2026-10-09）：「移动端前端当前的宿主迁移进入
> terminal-session wasm-app 中当作一个整体的页面；文件传输和 aichatbox 是
> 有独立页面的应用，不再作为插件嵌入到之前的宿主中；然后用新设计的界面
> 替换旧的前端宿主，接入三个 wasm-app。」
> 代码：`bedcode-mobile/`（`src/shell/**` = 新设计界面；`src/views/**` +
> `MobileLayout` = 旧前端宿主；`wasm-apps/*/` = 三个 wasm-app）

---

## 1. 指令解析（目标架构）

```
App 入口（新设计界面 = src/shell，唯一前端宿主）
  ├── Home（快捷卡片 + 应用网格）/ Apps / AppDetail / Permissions / Settings / Switcher
  └── 三个 wasm-app 运行面（registerSurface，独立页面）：
       ├── com.bedcode.terminal-session —— 整体页面 = 当前宿主主流程
       │     （设备发现/配对 → 会话列表/管理 → 终端）
       ├── com.bedcode.file-transfer   —— 独立页面（现有三段式主视图）
       └── com.bedcode.ai-chatbox      —— 独立页面（现有 ChatView）
旧前端宿主（MobileSwipeContainer 四页 + /mobile/** views + 插件嵌入面）退役
```

与既有下沉系列（票 12–16：终端链路 / 会话控制 / 认证编排 / 终端 UI / 任务域
逐一迁入 terminal-session）同一条路线：**宿主前端剩余的业务 UI（设备发现 +
会话管理 + 连接编排）最后一批下沉到 terminal-session 插件前端**，宿主只剩
新壳（平台机制）+ 引擎事实面（mobileApi 投影 / WIT 原语）。

## 2. 现状盘点（已核对代码）

### 2.1 新设计界面（壳）已就绪
- `src/shell/**`（路由 `/mobile/shell`）：Home / Apps / AppDetail / AppRun /
  Permissions / Settings / Switcher 全实现；`ShellAppSurface` 渲染优先级
  ① registerSurface（wasm-app 正路）→ ② pluginAppSource 延迟解析 →
  ③ 预留位空态。目前三 app 都走 ②（terminal→terminalView、ft→toolbox、
  ai→navTab），① 无人注册。
- 壳入口：设置 → 系统 → 「WASM 应用平台（宿主壳）」；默认入口仍是旧宿主（`/` → MobileSwipeContainer）。

### 2.2 三个 wasm-app 前端现状
| app | 现注册（旧宿主嵌入面） | 页面组件 | 数据通道 |
| --- | --- | --- | --- |
| terminal-session | `ui.registerTerminalView`（终端主视图单槽）+ task 域（票 16 自挂载） | `terminal/TerminalView.vue` | mobileApi（activeSessions / isConnected / onSessionEvent / openTerminalStream / httpRequest）+ context.commands（会话 5 命令 + auth 编排命令，票 13/14）+ host-events |
| file-transfer | `ui.registerToolboxPage` + `registerRoute` + `registerSettingsSection` | `components/FileTransferView.vue`（三段式） | context.commands（host-peer 域）+ host-mdns browse |
| ai-chatbox | `ui.registerNavTab`（order 150 会话右侧） | `components/ChatView.vue` | context.commands + mobileApi.httpRequest |

### 2.3 旧宿主主流程（要下沉的内容）
- `MobileSwipeContainer.vue`：四页 DevicesView / SessionsView / ToolboxView /
  SettingsView + 插件 navTab 页。
- `views/DevicesView.vue`（1191 行）：mDNS 发现（`useMdnsDiscovery` →
  `mdns_start_discovery` / `mdns_get_discovered_services` + 事件
  `mdns_service_found|resolved|removed`）+ 配对（QR / 配对码 / 生物）+
  `useMobileConnection`（设备清单、连接状态）+ `wsGetBiometricKeyStatus` /
  `wsAuthenticateWithQr`。
- `views/SessionsView.vue`（539 行）：会话列表 / 起停删（票 13 已走插件
  `sessionCommands`）+ 会话状态（useMobileConnection）+ 直发输入。
- `views/TerminalView.vue`（74 行）：薄壳 → 插件 terminalView（票 15 已下沉，无需再动）。
- `composables/useMobileConnection.ts`（1225 行）：连接态 + ws_sync_* 事件
  + 会话/配置投影 + 设备清单真源（host 前端内存态）。
- `composables/useMobileCommands.ts`（509 行）：ws_* 命令封装。
- `composables/useMdnsDiscovery.ts`（135 行）。

### 2.4 插件可用的通道盘点（迁移数据面的落点）
| 旧宿主数据面 | 插件通道 | 缺口 |
| --- | --- | --- |
| 会话列表 / 起停删 / 直发输入 | ✅ context.commands（票 13 五命令）+ mobileApi.activeSessions / loadActiveSessions | 无 |
| 终端输出 / 输入 / resize | ✅ terminalView + mobileApi.openTerminalStream（票 15） | 无 |
| 配对 / 认证编排 | ✅ plugin auth.rs + host-auth 5 原语 + host-events 四事件（票 14） | 无 |
| 认证态对账 | ✅ host-auth.has-credentials + mobileApi（isConnected 投影） | 无 |
| 连接生命周期事件（ws_reconnecting / ws_reconnected / ws_unexpected_disconnect / ws_reauth_rejected / ws_reconnect_failed / ws_event_channel_ready） | ⚠️ mobileApi.onSessionEvent 只覆盖 disconnected / session_* | **需扩容投影**（host 前端包装，引擎事实，零 WIT） |
| mDNS 主机发现（_bedcode._tcp） | ⚠️ host-mdns.browse 存在（file-transfer 先例），但 terminal-session manifest 无 `mdns` 权限位、rust 无 browse 命令 | **需补**：manifest 权限 + rust 命令（plugin crate 内，零宿主 WIT 变更）或 mobileApi 投影原始 mDNS 事实 |
| 生物凭证状态（ws_get_biometric_key_status） | ❌ 无 | 需 mobileApi 投影（host 前端包装既有窄读命令，C4 凭据零过境——只投状态不投材料） |
| 设备清单（paired / last-seen） | ❌ host 前端内存态 | 按票 09 先例：真源下沉插件前端（mobileApi 原始发现事实 + 插件自建状态机），宿主不持有派生视图 |
| 设置写面（set_db_setting / keep_screen_awake / set_screen_orientation / set_auto_reconnect） | ⚠️ mobileApi.mobileSettings 只读投影已有；写面在宿主命令 | 写面决策见 §5 D5 |
| 凭据镜像（ws_get_token / ws_set_token / ws_clear_token） | 🚫 **红线**：凭据零过境（C4），不得投影给插件；宿主前端 localStorage 镜像逻辑留宿主（壳/连接域），不进插件 | 无（保持现状） |

### 2.5 前端零资源访问 / 壳约束锁（红线，不能碰）
- `eslint.config.js` `frontend-no-resource-access` 覆盖 `bedcode-mobile/src/**`；
  插件前端（`wasm-apps/*/src`）在锁外，但**插件前端不得直连宿主 Tauri 命令**——
  迁移一律走 mobileApi / context.commands / context.events / host-* WIT。
- 壳 L1–L7 锁（`src/__tests__/shell/shellConstraintLocks.test.ts`）继续有效；
  新增壳面须登记锁索引。
- mobileApi 扩容只加**引擎事实**投影，禁止业务形状（B1–B6 零命中自检）。

## 3. 归属判定（AGENTS §5.1 三问）
- 设备发现 / 会话管理 / 终端 UI = 产品语义 → 归 terminal-session（B 命中）✅
- mDNS 浏览 / 连接状态 / 生物凭证状态 = 引擎事实 → 宿主 mobileApi / WIT 原语 ✅
- 设备清单派生视图（paired / last-seen 归约）→ 插件前端（票 09 先例）✅
- 新壳 = 平台（注册表 / 挂载 / 权限闸门 / 设置）→ 宿主 ✅

## 4. 分阶段实施

### 阶段 A（纯前端，零 WIT / 零 ABI）— 壳默认 + 三 app surface 接线
- **A1 壳默认入口**：`/` 路由指向壳（ShellView 挂到 `/` 或 redirect），
  旧 `/mobile/**` 路由暂保留（过渡期从壳设置进入）。
- **A2 插件前端 → 壳 surface 通道**：共享运行时暴露壳注册表
  （`__BEDCODE_SHARED__.shell`），SDK 增 `ui.registerSurface`（或 shell 域），
  插件前端可注册运行面；pluginAppSource.resolveSurface 优先级 ① 前置。
- **A3 file-transfer / ai-chatbox 独立页面化**：停旧宿主嵌入
  （registerToolboxPage / registerNavTab / registerRoute），改注册 surface
  （现有视图组件原样复用）；i18n / 权限 / 停用回收对齐。
- **A4 terminal-session 整体页**：插件前端建「宿主页」
  （设备 → 会话 → 终端三段流转，复用现有 TerminalView 与 task 域），
  迁移 DevicesView / SessionsView / 连接编排：
  - 会话 / 终端 / 认证：走既有通道（无缺口）
  - 连接生命周期事件：mobileApi 扩容投影（host 前端包装既有 `ws_*` 命令
    与 `ws_*` Tauri 事件，只投引擎事实）
  - mDNS：优先 mobileApi 原始发现事实投影（最小改动）；终态可落
    host-mdns browse（票 09 先例）——阶段内二选一，见 §5 D6
  - 设备清单状态机：插件前端自建（消费上述原始事实）
- **A5 壳面接线收尾**：Home 快捷卡片（registerSlot 贡献：会话数 / 传输进度 /
  引用句——按 prototype 契约）、胶囊项、i18n 双语、测试（壳锁 +
  插件测试）、eslint。

### 阶段 B（退役）— 旧前端宿主退役（待阶段 A 验证后）
- B1 旧 `/mobile/**` views + MobileSwipeContainer / MobileNav 插件嵌入面
  退役删除；防回接锁新增（旧宿主 UI 面不回接：路由 / 视图符号 / 命令字面量）。
- B2 仅旧宿主在用的宿主命令面退役（mdns_* / ws_set_token / ws_clear_token 等），
  先核对插件面无回接；真源搬迁 fail-visible 三形态。
- B3 锁索引 / code-map / CHANGELOG 双语 / 文档同步。

### 阶段 C（收尾）
- 全量 vitest（端目录）+ eslint 0 error；Rust 侧若有改动按 crate 根 cargo test；
  lens_diagnostics 无 blocker；手工验证清单（真机核验壳默认入口 + 三 app 流转）。

## 5. 待裁决问题
- **D1 terminal-session 整体页范围**：设备/会话/终端（当前宿主核心内容）进
  terminal-session。`ToolboxView` 工具页（CodeExplorer 文件浏览器等）与
  `SettingsView`（壳已有设置屏）是否也随迁 / 保留？建议：设置留壳；文件浏览器
  暂走旧路由过渡，后续按归属裁决。
- **D2 旧宿主退役时机**：阶段 A 完成、壳默认入口验证后再退役（B 阶段），
  过渡期旧页面仍可从壳设置进入；还是 A 完成后立即退役？
- **D3 file-transfer / ai-chatbox 停旧嵌入面时机**：与旧宿主同批（B 阶段）还是
  阶段 A 直接停（A3）？（A3 直接停会让旧宿主工具箱页少两页——旧宿主过渡期
  是否接受）
- **D4 移动端 manifest / 命令面**：terminal-session 增加 `mdns` 权限位 +
  browse 命令（plugin crate 内，不动宿主 WIT）——是否接受？
- **D5 设置写面**：`set_db_setting` / `keep_screen_awake` / `set_screen_orientation`
  / `set_auto_reconnect` 是宿主引擎设置。迁入插件后写面怎么走？
  (a) mobileApi 只读投影 + 写面仍留宿主命令由壳设置页调用（推荐）
  (b) 新 WIT 原语（ABI bump）——不建议，阶段内不动 ABI
- **D6 mDNS 通道二选一**：(a) mobileApi 原始发现事实投影（最小改动，推荐
  阶段 A）(b) host-mdns browse（票 09 终态先例，plugin rust 命令 + 权限位，
  阶段 A4 内一并做）。可先 (a) 后迁 (b)，或直接 (b)。

## 6. 验证
- 阶段 A：壳默认入口可进三 app；terminal-session 整体页完成 设备→会话→终端
  全链路（真机/浏览器核验 + vitest）；eslint 0 error；i18n 双语同步。
- 阶段 B：全量 vitest + cargo test（涉及 Rust 改动面）+ 防回接锁实测。
