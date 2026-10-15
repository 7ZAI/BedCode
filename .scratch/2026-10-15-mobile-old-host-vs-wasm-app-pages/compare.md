# 移动端「旧宿主连接页/会话页」 vs 「现 wasm-app bedcode」 对照

## 0. 取版本方式（git）

| 角色 | 提交 | 说明 |
| --- | --- | --- |
| 旧宿主（业务页未下沉） | `407e35e81`（2026-10-14，revert 到旧宿主 `92222e8b6`） | 旧宿主四页容器 + `views/DevicesView.vue`（1191 行）/ `views/SessionsView.vue`（539 行）在场 |
| 首次下沉 | `e92cc40a3`（2026-10-09） | 删 `DevicesView/SessionsView/TerminalView/ToolboxView/PluginView/PresetTasksView`，UI 迁 wasm-apps |
| 全量下沉收口 | `675c660b2`（2026-10-10） | 底部导航 / 设置页下沉，壳只认 `registerSurface` |
| 当前形态 | `b84767916`（2026-10-14）→ dev HEAD | 壳 `/mobile/shell` 单入口；业务页在 `wasm-apps/bedcode/src/` |

抽取命令（旧文件已存本目录 `old/`）：

```bash
git show 407e35e81:bedcode-mobile/src/views/DevicesView.vue
git show 407e35e81:bedcode-mobile/src/views/SessionsView.vue
```

现文件：
`bedcode-mobile/wasm-apps/bedcode/src/host/components/{DevicesSection,SessionsSection,QrScanner}.vue`、
`.../host/useHostPage.ts`、`.../app/AppRoot.vue`。

**重要前提**：`407e35e81` 时后端编排（auth 域 `host-auth`、会话命令面 `terminal-session.*`、终端链路）
已迁插件，**连接/认证的传输实现两版一致**。差异集中在「页面层形态 + 谁持有编排/状态 + 若干行为取舍」。

---

## 1. 容器与页面骨架

| 维度 | 旧宿主 | 现 wasm-app |
| --- | --- | --- |
| 入口 | 路由 `/mobile/devices`，在 `MobileSwipeContainer`（4 页 keep-alive 横滑）内 | 壳 `/mobile/shell` → 应用运行屏 → `AppRoot.vue`（插件自持页签容器） |
| 页头 | 每页自带 `page-header`：连接页「连接配对」+ 状态小字 + 雷达按钮；会话页「会话」+ 刷新 | 应用级头部一行：标题「远程终端」+ 连接状态 chip + 刷新；**两页都没有自己的标题/页头** |
| 底部导航 | 宿主 `MobileNav`（连接/会话/工具箱/设置） | 应用内 `NavBar`（同样四页签，active 顶部指示条） |
| 页签装载 | keep-alive 常驻 | 首次访问后挂载并常驻（`visited` 集合），切页 CSS 停靠过渡 + 横滑 |
| 终端 | 路由跳 `/mobile/terminal/:id`（独立页 + `LoadingDialog` 就绪等待） | `AppRoot` 内整页沉浸（`v-if="terminalSession"`，隐藏头部与导航） |

---

## 2. 连接页：样式 / 布局差异

| 区块 | 旧 `DevicesView.vue` | 现 `DevicesSection.vue` |
| --- | --- | --- |
| 整体结构 | 页头 + 可滚动内容区（多点互斥分支：已连接 / 扫码 / mDNS 发现 / 历史+扫码结果）+ **底部常驻双 CTA** | 单列流式（扫码入口 → mDNS 行 → 发现列表 → 扫码结果卡 → 历史 → 连接中 → 错误 → 已认证卡 → 配对卡 → 断开），无底部 CTA |
| 扫码入口 | 底部「二维码连接」按钮；相机态时该按钮切红为「停止扫描」（**退出相机的唯一入口**） | 顶部按钮「扫码连接」；相机态**整块替换内容区**，`QrScanner` 正常态无返回/关闭（仅错误态有「返回」）——退出依赖错误态或扫码成功 |
| 手动连接 | `BottomSheet` 弹层（标题 + 占位 + loading），**端口缺省回落设置 `defaultPort`（8765）** | 与「扫描设备」同排的输入框 + 「连接」按钮；**必须写 `地址:端口`**，`parseAddress` 不合法直接 toast（设置项 `defaultPort` 在此不生效） |
| mDNS 发现 | 独立互斥面板：吸顶头部（标题 + 数量徽章 + 停止/重新扫描 + ×）、spinner 扫描态、**雷达扫射空态动画**、设备行带「连接/已连接」badge | 常驻区块：一行「扫描设备/停止扫描」按钮 + 「正在扫描局域网…」小字 + 发现列表；**无雷达动画、无数量徽章、无关闭面板、无吸顶、无已连接 badge** |
| 发现列表项 | `group-card`/`group-row` 行：设备 chip + 名称 + 地址 + 状态 badge | 圆角卡片按钮：名称 + `地址:端口`（无 chip、无状态 badge） |
| 扫码结果 | 历史下方同级别区块，标题「扫描结果」+ × 关闭；卡片名 **硬编码 `Desktop`**，点击整卡即连接 | 卡片含 `host:port` + 两个按钮「连接 / 关闭」；不再硬编码设备名 |
| 连接历史 | 标题 + 数量 + 「清空」（**确认弹窗**）；每项有名称/地址/**相对时间（刚刚/x 分钟前/x 小时前/x 天前/日期）**+ 单项 × 删除；列表 TransitionGroup 进出动画 | 标题 + 「清空」（**无确认**）；每项仅名称 + 地址；**无相对时间、无单项删除**（`removeFromConnectionHistory` 已投影但 UI 未用；`hub.lastConnected` 键成了死键） |
| 连接中 | **全屏 `LoadingDialog` 遮罩**「正在连接 {name}…」（12s 兜底） | 行内卡片：脉冲点 + 「正在连接 {name}…」+ **取消连接**按钮 |
| 连接失败 | 内容区顶部 `group-card` 错误行 + toast（`classifyConnectionError` 分类文案） | 行内一行 warning 文案 + toast（`host/errors.ts` 分类表，同一套语义） |
| 已连接/已认证 | 设备卡：`device-icon` + 名称 + 地址 + **同行右侧「断开」红按钮**；无会话入口 | 卡内名称 + 「已认证」badge + **「会话」按钮（切到会话页签）**；生物识别登录/绑定/解绑按钮；**「断开」是页面底部独立按钮** |
| 等待认证 | 无此形态（认证走弹窗，配对口在弹窗里） | **行内认证卡**：名称 + 「等待认证」badge + 提示文案 + 生物登录（已绑定才显示）+「请求配对」→ 行内配对码输入 + 验证；配对错误行内显示 |
| 断开 | `Modal` 二次确认 | 直接断开，无确认 |
| 视觉实现 | 宿主全局类（`page-header` / `group-card` / `group-row` / `chip-*` / `status-badge`）+ `mobile.css` token | 组件内 Tailwind 工具类 + 内联 `var(--mobile-*)` token（无 scoped 样式块） |
| 动效 | `fade` / `config-list` TransitionGroup / `prefers-reduced-motion` 降级 | 基本无过渡（仅 `QrScanner` 扫描线 2s 动画） |

---

## 3. 会话页：样式 / 布局差异

| 区块 | 旧 `SessionsView.vue` | 现 `SessionsSection.vue` |
| --- | --- | --- |
| 未连接空态 | 大图标 + 文案 + **「二维码连接」按钮跳连接页** | 虚线框 + 「未连接桌面端」+ 提示（无跳转按钮，靠底部导航） |
| 区块头 | `section-head` 统一 44px 触控语言：「会话配置」（**可折叠**，chevron + 刷新按钮）与「运行中的会话」两个同高区块头 | 两行标题（「进行中的会话」+ 刷新按钮；「会话配置」）+ 无折叠 |
| 会话配置卡 | `SessionConfigCard`：环境 chip（wsl2/普通）+ 名称 + **「启动」实底按钮**；下挂**工程目录（可点击进文件浏览）**与**启动命令**两行元信息 | 一行：名称 + 「启动 / 启动中…」按钮；**无环境、无工程目录、无启动命令、无进文件浏览入口** |
| 运行中会话卡 | `SessionCard`：状态 chip 图标 + 名称 + **运行时长计时（useRunTime）** + **任务状态标签** + 状态 badge + 停止/删除图标按钮 | 卡片：名称 + 状态文案 + 「打开终端 / 停止（仅 running 态）/ 删除」按钮；**无时长、无任务状态、无状态徽章/图标** |
| 加载态 | 3 张 skeleton 卡 + 「已加载」标志 | 无 skeleton（直接列表/空态） |
| 启动会话 | 走宿主 composable（含 **maxOpenTerminals 上限校验**），成功后**留在会话页**（toast + 列表刷新） | 走插件命令 `terminal-session.start-session`（**无上限校验**），toast 成功后**立即 emit 打开终端** |
| 打开终端 | 设 `activeSessionId` + `terminal-session.subscribe`（**整段回放**）预热 → 路由跳转；`LoadingDialog`「正在准备终端...」 | `terminal-session.prewarm`（**只建连、不发 subscribe**，避免注定丢弃的整段重放）→ `AppRoot` 切整页终端 |
| 停止 / 删除 | `Modal` 二次确认（含 loading 态）+ 成功后本地状态更新（停止改 stopped、删除剔除） | 直接调用，无确认（成功与否靠 toast） |
| DEV Mock 会话 | `isMockSessionDev` 开关 + Mock 行 | 无（改用 `mobileApi.mockSessionId`，终端页侧处理） |
| 滚动条 | `session-scroll` 局部 6px 压暗 | 未定制 |

---

## 4. 连接流程（行为序列）差异

### 4.1 旧（`DevicesView.startConnection`，显式状态机）

```
点击历史/发现/手动 → pendingDevice=device
  connection.connect: wsIsConnected→wsDisconnect（清残留）→ httpProbe(3s) → wsConnect（前端 12s 兜底 / Rust 10s）
  skipPairing（历史/发现=true；手动=false）→ connection.authenticate()（存量 JWT 换新）
  失败或手动 → wsGetBiometricKeyStatus()
        preferredAuthMethod==='biometric' 且已绑定 → 弹 BiometricAuthDialog 并自动跑指纹
        否则 → startPairingFlow()（15s 超时 + 全屏遮罩）→ PairingInput 弹窗输入配对码
  弹窗可互切（生物 ↔ 配对码）；**关闭弹窗 = 断开连接**
  成功 → addToConnectionHistory + loadSessionConfigs / loadActiveSessions
扫码 → connect → **轮询 isConnected（200ms × 最多 10s）** → wsAuthenticateWithQr → saveCredentials → add history
```

### 4.2 现（`useHostPage.connectDevice` + 派生状态）

```
点击历史/发现/手动 → connectDevice(target, {silentAuth 默认 true})
  api.connectDevice = 宿主 connection.connect（同样的清残留 + httpProbe + 12s 兜底）
  silentAuth!==false 且未认证 → api.authenticateWithStoredToken()（= connection.authenticate）
认证入口由状态推导：needsAuth = !isPaired && status ∈ {connected, pairing}（或 error+pairingError）
  「请求配对」→ 插件命令 request-pairing → pairingStage='requested' → 行内输入 → verify-pairing-code
  生物登录按钮仅在 deviceSupported && hasKey 时出现（**不读 preferredAuthMethod、不自动触发**）
扫码 → QrScanner 校验载荷 → 结果卡「连接」→ connectDevice(silentAuth:false) → authenticate-with-qr
连接中可「取消连接」（api.cancelConnection）
```

### 4.3 逐条差异

1. **编排归属**：旧把「选哪种认证 / 何时弹窗 / 何时加历史」写在宿主 `DevicesView`（含 `pendingDevice`）；
   现写在插件页 `useHostPage`，宿主只投引擎事实（§5.1 越线形态已消除）。
2. **首选认证方式设置失效**：`mobile.preferredAuthMethod` 仍在应用设置页，但新流程不再读取 → 生物登录从「自动弹」变为「手动点」。
3. **默认端口设置失效**：`mobile.defaultPort` 仍在设置页，手动连接不再回退缺省端口。
4. **连接历史写入缺失**：`addToConnectionHistory` 当前全仓**只有定义与导出、无调用点**（旧由 DevicesView 在认证/配对/扫码成功后调用）⇒ 新连接页的历史区在干净安装下恒为空，仅能显示/清空旧版遗留的 localStorage 数据。
5. **去掉了两个轮询/兜底分支**：QR 的「等 isConnected 10s 轮询」与配对的 15s 超时竞速不再存在；改由 `connectDevice` 的 await + 引擎状态事件收敛（更简单，但也少了旧版对「连上但状态未翻转」的兜底路径）。
6. **取消/关闭语义变化**：旧「关闭弹窗 = 断开」；现无弹窗，认证卡常驻、由底部「断开」显式断开（错误态 + pairingError 时认证卡仍在，可重试）。
7. **上限/配额**：旧 `loadActiveSessions` **与** `startSession` 都按 `maxOpenTerminals` 判（超出丢弃 + toast / 拒绝启动）；
   现列表同步仍由宿主 composable 判（丢弃 + toast），但**启动走插件命令绕过了上限校验**。
8. **打开终端预热**：`subscribe`（整段回放，预热期无人接收）→ `prewarm`（只建连），并新增 `terminal-resync{reason}` 区分重放/环淘汰。
9. **页面生命周期**：旧 `onActivated` 强制重读历史 + `keepResults` 续扫，`onDeactivated/onUnmounted` 停 mDNS；
   现页面常驻，无「切走停扫 / 切回续扫」逻辑——切页签后 mDNS 仍会继续扫。
10. **连接成功后的联动**：旧 `watch(isConnected)` 收起扫码/发现面板并触发会话加载；现无此联动（面板由本页按钮控制，会话页自行刷新）。

---

## 5. 未随下沉迁移的旧能力（备忘）

- 旧 `ScanPanel` 的「我的二维码」信息弹窗（本机平台 + 默认端口）——`QrScanner` 注释明示未迁（缺平台投影）。
- 会话配置的工程目录/启动命令展示与「进文件浏览」（`mobile-files` 路由仍由宿主保留）。
- 会话运行时长计时器（`useRunTime`，随旧组件退役）、任务状态标签。
- `SessionListItem.vue` / `DeviceCard.vue` / `InputBar.vue`（旧宿主版本）中的部分形态未在新页复刻。
