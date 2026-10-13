# BedCode Mobile Code Map

本文档作为移动端项目代码探索的索引入口。**只记录目录层级与模块职责划分，不索引具体代码文件**——定位到目标目录后，再用 `ls` / `grep` 等工具在该目录内查找具体文件。

---

## 使用指引

**当用户命令包含以下动作时，请先阅读本文档：**

- 探索代码 / 查看代码 / 了解代码结构
- 查找文件 / 定位模块 / 寻找某个功能
- 理解架构 / 分析项目组成
- 修改某模块前需要了解上下文

**阅读流程：**

1. 先浏览 Project Structure 了解目录布局与各目录职责
2. 根据 Core Modules 深入理解核心模块的划分与协作
3. 用 Quick Navigation 按功能定位到目标目录
4. 进入目标目录后用 `ls` / `rg` 查找具体文件（文件命名遵循 AGENTS.md 规范）

---

## Project Structure

```
bedcode-mobile/                       # 移动端项目 (Tauri 2.0 + Vue 3)
├── scripts/                          # 构建/开发脚本：dev-run.js（dev 编排与 adb fd0 预检自愈）、
│                                     #   android-dev-log.js（logcat 落盘 + 按天清理 + 控制台保色）、
│                                     #   lib/dev-log-router.js（dev-log 行缓冲与双写：raw 控制台 /
│                                     #   clean 落盘与过滤判定）、插件构建
│                                     #   （plugin-build.js）、产物大小检查（check-target-size.js）、
│                                     #   adb-fd0-shim.sh（adb client fd0 bug 自愈 shim）
├── packages/                         # 共享包（供移动端插件开发与测试使用）
│   ├── plugin-sdk-mobile/            # 移动端插件开发 SDK：Rust + TS 双侧、dev-shell 调试壳、
│   │                                 #   插件模板（template/）、UI 子路径导出
│   └── plugin-component-test/        # 测试用 WASM 插件 crate，供宿主测试套件做连通性验证
├── wasm-apps/                      # wasm 应用源码目录（每个 app 独立 package：plugin.json 元数据 +
│                                     #   rust/ WASM 后端 + src/ TS 前端 + vite.config.ts 独立构建）
│   ├── ai-chatbox/                   # AI Chatbox 插件：多供应商 OpenAI 兼容客户端
│   ├── file-transfer/                # 文件传输插件：基于对等网络的在线对端发现与共享目录浏览、
│                                     #   多选批量传输、接收策略、SAF 自选保存位置与历史记录
│                                     #   **rust/ 已退役（双端单产物，2026-10-13）**：wasm 产物源在
│                                     #   桌面端 `bedcode-desktop/wasm-apps/file-transfer/rust/`（同一
│                                     #   crate 双端加载）；本目录只剩前端 + plugin.json，构建经
│                                     #   package.json 的 `bedcode.sharedRustManifest` 指向共享 crate
│                                     #   （`scripts/plugin-build.js` 先构建共享 rust 再走前端）；
│                                     #   业务实现（台账归约 / 判据 / 注册表 / 设置 / 会话表）在共享核
│                                     #   `packages/bedcode-file-transfer-core`（ADR 0044），平台差异
│                                     #   经共享 crate 的 `platform.rs` 探测 + `adapters_mobile.rs`
│                                     #   端口包表达（防漂移锁 `wiring_lock.rs` 钉住「核内判据不得
│                                     #   回流 / 移动端产物源不得重建 / 共享 crate 只链桌面 SDK」）
│                                     #   （OCR 插件已随 feature/ocr-plugin 隔离，本分支无）
│   └── terminal-session/             # 远程终端控制端内置 app（与桌面同名不同职责，ADR 0018/C8，
│                                     #   票 12/14/15/16）：终端消费 UI 前端（票 15 迁入：
│                                     #   TerminalView 编排 / 订阅 store / 输入助手 / 快捷键 /
│                                     #   新手引导 / 样式 / 文案）+ 订阅协议与认证编排（rust）+
│                                     #   任务域前端（原 auto-task 并入：队列面板 / 工具箱两页签）
├── src/                              # Vue 3 前端（扁平化结构）
│   ├── components/                   # UI 组件：设备卡片、文件浏览/查看、移动布局/导航/状态栏、
│   │                                 #   滑动容器、配对、任务弹窗、图标（终端组件随票 15 迁
│   │                                 #   wasm-apps/terminal-session/src/terminal/components/）
│   ├── composables/                  # 业务逻辑 composable：连接管理、HTTP API、文件树、代码高亮、
│   │                                 #   mDNS 发现/广播、预设任务、系统通知、前台服务、边到边显示、
│   │                                 #   屏幕方向、更新检查、设备数据擦除（useClearAllData，票
│   │                                 #   2026-10-10：设备级动作，凭据与宿主连接态不可由插件触碰，
│   │                                 #   入口在壳设置屏危险区）等（终端内核域随票 15 迁
│   │                                 #   wasm-apps/terminal-session/src/terminal/composables/）
│   ├── stores/                       # Pinia 全局状态：代码查看器、设置、i18n
│   │                                 #   （输入助手 / 终端缓冲随票 15 迁插件）
│   ├── views/                        # 页面：代码浏览器、设备、mDNS 发现、插件、扫码、会话、
│   │                                 #   设置（views/settings/ 下按领域拆分子页：外观/连接/认证/
│   │                                 #   通知/关于）、终端（薄壳，票 15：渲染插件注册的
│   │                                 #   registry.terminalView）、工具箱
│   ├── plugin/                       # 前端插件系统：加载器、注册表、权限、上下文、事件、命令、
│   │                                 #   共享模块运行时、对话框宿主、终端流 / 会话事件机制面（票 15）；
│   │                                 #   components/ 下为插件 UI 宿主（导航页签/设置页/终端工具栏/
│   │                                 #   视图宿主；任务队列面板已随票 16 归
│   │                                 #   wasm-apps/terminal-session/src/task/）
│   ├── utils/                        # 工具函数（剪贴板、mock 会话开关、主题 label 等）
│   ├── config/                       # 配置（启动闪屏等；终端主题随票 15 迁插件）
│   ├── services/                     # 跨端复用服务（linkCrypto.ts 链路加密客户端）
│   ├── assets/                       # 静态资源（终端字体 woff2 等 app 级资源；帮助文档随票 15 迁插件）
│   ├── styles/                       # 样式：mobile.css 全局 + markdown-body + terminal-font（字体声明）
│   ├── locales/                      # 国际化（zh-CN / en，各含 common / desktop / mobile / settings / shell）
│   ├── router/                       # 路由
│   ├── shell/                        # 移动端前端宿主壳（WASM 应用运行平台，路由 /mobile/shell）——
│   │                                 #   **移动端前端新功能 / 重构的默认落点**（AGENTS.md §6「移动端
│   │                                 #   前端：优先对接宿主壳」；旧 src/{components,views,composables,stores}
│   │                                 #   只接受缺陷修复）。分层 types → registry → adapters →
│   │                                 #   composables → components；adapters/pluginAppSource.ts 是壳与
│   │                                 #   插件系统唯一耦合点（换 wasm-app 真源只换它）；应用内页面只留
│   │                                 #   挂载位（ShellPlaceholderSurface / ShellDefaultSlotCard 兜底），
│   │                                 #   待 wasm-app 注册 surface/slot 顶替。壳内两层自足面（旧件副本，
│   │                                 #   契约逐字一致，迁移映射见 ui/index.ts 头注）：
│   │                                 #   · components/ui/ —— 公共组件库（Button/Toggle/Modal/
│   │                                 #     ConfirmDialog/PromptDialog/LoadingDialog/CollapseSection/
│   │                                 #     QuickActionButton/LetterAvatar）
│   │                                 #   · composables/ —— 平台机制副本（useToast/usePlatform/
│   │                                 #     useOrientation/useSwipeTabs/useViewportPanGuard）
│   │                                 #   落地说明见
│   │                                 #   .scratch/2026-10-07-mobile-wasm-platform/host-shell-spec.md
│   │                                 #   与同目录 ui-mechanism-port.md
│   └── __tests__/                    # 前端测试（composables/stores/utils/config/plugin/shell 子目录 +
│                                     #   fixtures 测试数据 + integration 集成测试；终端域测试随
│                                     #   票 15 迁 wasm-apps/terminal-session/src/terminal/__tests__/，
│                                     #   由本 vitest 配置统一驱动）
└── src-tauri/                        # Rust 后端（Tokio 异步）
    └── src/                          # 模块按领域扁平组织，每领域配同名入口文件（auth.rs、commands.rs 等）
        ├── auth/                     # 认证模块：认证管理器（HTTP 认证：配对提交 / QR /
        │                             #   JWT reauth / 生物挑战应答）+ 认证状态（票 14 起
        │                             #   不再含本地配对码面，见 Core Modules 连接段）
        ├── commands/                 # Tauri 命令层：Android 特定、认证、连接、mDNS、
        │                             #   移动端特有（Quick Actions/Settings）、会话、终端
        │                             #   （Session Config 内存存储面 2026-10-11 退役：只读无写，
        │                             #   恒返回空）
        ├── connection/               # 远程连接模块（核心模块，详见 Core Modules）
        ├── enums/                    # 枚举类型：认证、控制、插件、会话、特殊键、总结
        ├── file_service/             # 文件服务：SAF 目录树读取（saf_tree.rs）
        ├── handler/                  # WS 消息处理器：认证、系统、插件事件路由（plugin_event）、默认
        ├── mdns/                     # mDNS 服务发现与广播（局域网设备互发现）
        ├── model/                    # 数据模型：API DTO、WebSocket 消息
        ├── peer_net.rs               # 对等网络接入：节点身份初始化（与 DeviceIdentity 分离）、
        │                             #   节点/发现守护装配（Android 启动守护前先经 Kotlin
        │                             #   MulticastLockPlugin 申请多播锁，不持锁则单侧不可见）、
        │                             #   首连确认闸门事件桥接
        ├── peer_receive.rs           # 接收侧：询问回执表 + 接收事件桥 + 策略闸门
        │                             #   （票 07 起无接收任务表；落点缺省 app 私有下载目录，
        │                             #   MediaLanding 提升进 MediaStore）
        ├── peer_remote.rs            # 远端浏览/拉取：共享目录列目录 + 多文件拉取编排（只读，
        │                             #   票 10 起零前端命令面）
        ├── peer_transfer.rs          # 发送侧：会话句柄表 + 引擎事件桥（票 06 起无任务状态机）
        ├── peer_migration.rs         # 旧对等网络数据 → file-transfer 插件存储键一次性幂等迁移
        ├── plugin/                   # 插件系统（WASM 组件沙箱架构，核心模块，详见「插件核心模块引导」）
        ├── router/                   # 消息路由：路由上下文、事件、注册表、主实现
        ├── system/                   # 系统模块：共享命令、配置管理、常量（按领域分组）、
        │                             #   统一错误类型、Panic 捕获、JSON 设置持久化
        ├── session.rs                # 远程会话管理
        ├── state.rs                  # 全局状态管理（单例管理器 + Token 存储）
        ├── lib.rs                    # 库入口（模块声明 + peer_* 模块的 Tauri 命令直接注册；
        │                             #   注意 setup 内 init 顺序敏感）
        └── main.rs                   # 二进制入口
```

> **Android 原生层**：`src-tauri/gen/android/app/src/main/java/com/bedcode/mobile/` 下有大量自定义 Kotlin 插件
> （ForegroundService/ForegroundServicePlugin、TaskNotificationPlugin/Manager（v18 起为插件面
> `host-notify` 域桥：showPluginNotification 震动/声音分控 + pluginVibrate / pluginPlaySound）、
> SafPicker/SafTransfer、BiometricKey、AllFilesAccess、DownloadsDir、FileDelete、DeviceInfo、
> MulticastLock、StatusBarStyle（App 主题 → 系统栏图标外观同步）、PluginAssetExtractor 等）
> 及 AndroidManifest、res/xml 配置；改 Kotlin 后同步 `src-tauri/android-backup/app-java/` 恢复副本。
> 改动 Kotlin 后必须跑 `./gradlew :app:compileUniversalDebugKotlin` 验证（见 AGENTS.md）。gen/android 重建后需恢复清单见 AGENTS.md。

---

## Core Modules（核心模块）

### 远程连接 — `src-tauri/src/connection/`

移动端作为远程终端与桌面端通信的核心：

- **ws_client / ws_connection**：WebSocket 客户端主实现与连接管理（WS 帧级链路加密已随桌面端
  插件端点帧加密退役——`TrafficChannel::WsPlugin => false`，帧永不加解密；HTTP 信封加密保留）
- **event_ws**：`session-control` 插件端点常驻事件通道（票 03：极简认证帧 `{"type":"auth","token"}` +
  事件帧 `{"type":"event",…}` → `MobileEvent` → 前端 `ws_sync_*`；就绪发射 `ws_event_channel_ready`，
  前端据此 HTTP 对账补齐重连期间事件缺口——事件不重放）
- **heartbeat / reconnect**：心跳保活、断线重连——**退避下限 1000ms**
  （`system/constants/reconnect.rs::MIN_RECONNECT_DELAY_MS`，钳在 `calculate_delay` 里而非配置里）
  + **同因熔断**（连续 5 次同因失败即放弃，原因变化重置，`reset` 清熔断态）
- **关闭码与自愈边界**（M1，ADR 0031 配套）：`ws_client` 保留 Close 帧的
  **code**（`ServerClosed { code, reason }`；未携带按 1006 处理）——曾被丢弃时
  「认证失败」与「网络掉线」不可区分。`system/constants/connection.rs::
  WS_AUTH_FATAL_CLOSE_CODES = [4001, 4003]` + `is_auth_fatal_close_code` 判**致命**：
  跳过 supervisor 自愈、事件带 `fatal: true`、前端只发一次「需重新配对」toast
  （`ws_unexpected_disconnect` 不走 `handleUnexpectedDisconnect`）。
  桌面端此刻已 fail-closed 拒绝（无认证中心 / 设备被撤销），重连不可能成功
  （背景与桌面端裁决面见 `docs/knowledge/mobile-desktop-auth.md` §4.0）
- **codec / request / request_response**：消息编解码、请求发送与请求-响应关联
- **manager / lifecycle**：连接管理器、生命周期管理
- **client_router / default_handler / traits**：客户端消息路由与处理 trait
- ~~**pairing_service**：配对服务~~ —— **票 14 已随 `auth/pairing.rs` 一并退役**：移动端
  不是配对码颁发方（配对码由桌面端生成、本端只提交），本地生成 / 持有 / 校验 / 清码与
  pending 设备登记是 WS 握手时代遗留且零消费者，回接由
  `tests/retired_mobile_local_pairing_code_face_lock.rs` 拦截
- **认证命令面（票 14 阶段 B 收窄）**：配对 / QR / 生物挑战的**流程编排**已迁
  `com.bedcode.terminal-session` 插件 auth 域（WIT `host-auth` 5 原语，权限位 `auth`
  fail-closed，**凭据零过境**——JWT 由宿主 `apply_auth_success` 落地 global token + 凭据表）。
  宿主命令面只剩引擎事实 / 凭据面：`ws_authenticate`（重启 / 重连 JWT 换新）·
  `ws_get_auth_credentials`（前端持久化镜像窄读口）· 生物凭证绑定 / 解绑 / 状态（C4：
  Keystore 私钥与公钥注册留宿主）。流程事件（`ws_pairing_request` / `ws_pairing_verified` /
  `ws_paired` / `ws_auth_failed`）由插件经 host-events 广播，事件名与载荷与退役前逐字一致。
  回接由 `tests/retired_mobile_auth_orchestration_command_face_lock.rs` 拦截

### 终端链路（协议客户端与消费 UI 皆在插件 · 窄转发在宿主，票 12 + 票 15 阶段 A）— `wasm-apps/terminal-session/` + `src-tauri/src/plugin/terminal_stream.rs`

桌面端 PTY 输出的移动端消费链路（2026-09-12 迁入 Rust 取代前端直连 WS；2026-09-26 票 05
对齐桌面插件 `ws_terminal.rs` 新协议；**2026-10-08 票 12 协议客户端整体迁入 wasm app
`com.bedcode.terminal-session`**（移动版，与桌面同名不同职责——远程终端控制端，C8），
宿主 `terminal_link.rs`（1,363 行）与 `enums/special_key.rs` 退役，回接由
`tests/retired_mobile_terminal_link_lock.rs` 拦截）：

- **插件侧（`wasm-apps/terminal-session/rust/src/`，协议事实面）**：`link.rs` 状态机
  （fresh subscribe 门控 / cursor / session_missing 三振 / pending_resync）、`protocol.rs`
  帧构造与纯函数（wire 形状真源 = 桌面 `ws_terminal.rs`，零变化）、`keys.rs` 按键翻译
  （自宿主等价移植，桌面同款先例）、`commands.rs` 命令面
  （`terminal-session.{subscribe,unsubscribe,unsubscribe-all,remove,send-input,ack-rendered,get-state}`，
  前端经 `plugin_invoke` 调用）。连接经 host-websocket（`jwt-auth` 宿主代发首消息认证帧，
  token 零过境插件；`auto-reconnect` 宿主退避重建 + `ws:open` 携带 `reconnectedFrom` 回接
  新句柄 → 插件重订阅）。状态事件 `plugin:com.bedcode.terminal-session:terminal-state` /
  `:terminal-resync`（载荷与退役前逐字段一致）
- **宿主侧（零解析窄转发，ADR 0022 四类薄壳④）**：`plugin/terminal_stream.rs` 同处两段——
  页面 Channel 表 + 转发（`terminal_page_subscribe/unsubscribe` 命令是 Tauri 传输机制登记面；
  该表票 05 前住 wasm-core `terminal_stream_gateway.rs`，随域迁入宿主）；WIT
  `host-terminal-stream.forward-output`（同文件，权限 `terminal:output`）把插件交来的裸字节
  按 session-id 推给前端（`InvokeResponseBody::Raw`，全程零 JSON——C3）；
  `plugin/connection.rs` 的 `host-connection.primary-target`（主连接目标事实，票 13 复用）
- **协议（真源 = 桌面 `ws_terminal.rs`，wire 零变化）**：`/ws/plugin/com.bedcode.terminal-session/terminal`
  端点；握手 `auth{token}`（宿主代发）；订阅 `subscribe{sessionId,mode:live}`；输入可打印文本
  `input{data:UTF-8}`、控制字符/特殊键 binary（插件 keys 翻译）；流控 ack `ack{offset=<本地已渲染
  字节数>}`（64KB 阈值节流在插件）；输出 = **裸字节**；重锚 `ring_resync`（唯一重锚信号）；
  停止 `session_stopped`（尾帧在前，插件显式 close 取消宿主自动重连）；`error{message}` 分类——
  含「会话不存在」主动断开借宿主退避循环重试，超 3 次停止
- **插件会话控制域（票 13，`rust/src/session.rs`）**：list / start / stop / remove / input(HTTP)
  五命令（`terminal-session.{list-sessions,start-session,stop-session,remove-session,send-http-input}`）
  经 host-http（`jwtAuth` 宿主代注 Bearer，token 零过境插件——C4）直连桌面 `/api/sessions*`；
  宿主 `session/http.rs` + `SessionManager` + 三命令（零消费者）退役，`ws_disconnect` 的「停活跃会话」
  死分支同删，回接由 `tests/retired_mobile_session_control_face_lock.rs` 拦截（原 `session/http.rs`
  三个结构锁随迁：旧信封命令名 / WS 加密零残留 / 控制面无信封引用）。resize 不在此域
  （终端 UI 经 `mobileApi.httpRequest` 直发，票 15 口径）
- **插件前端（票 15 阶段 A：终端消费 UI 域整体迁入，`wasm-apps/terminal-session/src/terminal/`，
  47 文件 / 约 9.6k 行）**：`TerminalView.vue`（页面编排；宿主壳经 `registry.terminalView` 渲染，
  `props.sessionId` 优先、缺省回落 `mobileApi.activeSessionId`）+ `store.ts`（订阅状态机 / 本地
  字节游标 / 背压 ack / `ring_resync` 唯一重锚；命令经 `context.commands`、事件经 `context.events`、
  页通道经 `mobileApi.openTerminalStream`，页面代际号保证异步登记过期即注销）+ `composables/`
  （渲染器 / resize / 订阅重试与历史门控 / 键盘避让 / 触摸滚动 / TUI 兼容等）+ `components/`
  （头部工具栏 / 输入栏 / 设置 / 快捷键 / 任务选择与编辑 / 引导 / 帮助）+ `inputAssistant.ts`
  （字号字间距主题、快捷键、自定义命令；持久化在插件 storage）+ `activate.ts`（域文案注册 /
  主视图注册 / 样式注入 / 会话-连接-订阅联动）。测试在 `terminal/__tests__/**`（宿主 vitest
  统一驱动，20 文件 / 257 用例）
- **宿主侧（票 15 阶段 A 收窄后）**：`views/TerminalView.vue` 薄壳（路由 id → props + 机制
  provide + 插件激活）；机制面 `plugin/terminal-stream.ts`（`mobileApi.openTerminalStream` 页面
  字节通道）/ `plugin/host-events.ts`（`mobileApi.onSessionEvent` 白名单事件投影）/
  `registry.registerTerminalView`（新扩展点，`ui:input` 门）；`useMobileConnection` 的终端
  buffer 联动与 `useMobileCommands` 的 `terminal*` 协议 wrapper 已删除，仅存
  `terminal_page_subscribe/unsubscribe`（Tauri 传输登记面）
- 协议与架构细节：`.scratch/2026-09-26-mobile-desktop-adaptation/spec.md` §3.3 +
  `.scratch/2026-10-07-mobile-wasm-core-refactor/ticket-12-terminal-link-downsink.md` +
  `ticket-15-terminal-ui-downsink.md`（阶段 B：host-terminal / terminal-hooks 退役待实施）

### 宿主 WS 出站连接域 — `host-websocket`（客户端子集，ABI v14 起 · 票 11 · 引擎抽根 ADR 0043）

为票 12（终端订阅协议客户端迁插件）铺路的通用出站连接原语（ADR 0041；终端链路消费方详见
下一节）：

- **客户端 5 函数**（connect / send-text / send-binary / close / is-connected）：**引擎机制全部
  在通用能力 crate `packages/bedcode-ws-client-engine`**（句柄表 / reader-writer 双任务 / 心跳 /
  退避重连 / 帧信封 / 停用回收；零 WIT / 零 SDK / 零平台形态，平台差异面经端口
  `WsClientPorts` 注入）；移动端只剩薄适配器
  `bedcode-mobile/src-tauri/src/plugin/host_domains/ws.rs`（票 05 绑定归位迁入；5 原语转发 +
  `MobileWsClientPorts`：权限门读 manifest / 总线投递 / 宿主运行时任务 / jwt 代发 token /
  全局退避钳制），SDK trait 与帧信封解析在 `packages/plugin-sdk-mobile/rust/src/host/ws.rs`
- **服务端域不跟演**：桌面 15 函数里的服务端域 9 函数与 `connection-context` 不存在于移动端
  （ADR 0018/0041）；边界锁 `src-tauri/tests/mobile_host_websocket_client_domain_lock.rs`
  锁 WIT 服务端函数名与 `ws:server` 权限词汇
- **事件与帧走属主私有总线 topic**：`<plugin-id>:ws:open|error|close`（JSON 状态事件）+
  `<plugin-id>:ws:message`（二进制帧信封：kind + handle 长度 + handle + 原始字节，零 JSON
  编解码）；订阅须在 activate 期完成，且 **manifest 须同时声明 `ws:client` + `bus` 两个权限位**
- **不做 wss；心跳与退避重连为引擎参数**：`connect` 仅接受 `ws://`、同步阻塞至握手完成
  （timeout 上限 5s）；`heartbeatSecs` / `autoReconnect` 为 config 可选面（断线按宿主退避策略
  重建 + 新句柄 `ws:open` 携带 `reconnectedFrom`，取消 = 对句柄 close 或停用回收）；
  订阅协议 / 重订阅编排归插件
- 真实 WASM 组件全链路集成测试：`src-tauri/tests/ws_client_e2e.rs` 的
  `ws_client_domain_full_loop_with_real_component`（票 05 自内核 `component.rs` tests 迁入：
  component-test 插件 `ws-client` feature：connect → send-text → 对端回帧 → close 事件闭环）

### 插件核心模块引导

移动端插件系统与桌面端同架构（wasmtime 组件沙箱），并有移动端特有能力。做插件相关改动时按层定位：

**插件机制整核（单一 wasm-core，仓库根 crate）— `packages/bedcode-wasm-core/`：**
双端共用单一 crate（ADR 0045 D1；移动 fork crate 已随票 06 批次 04 退役，目录删除）。
移动形态依赖形态（`src-tauri/Cargo.toml`）：**直连**单 crate
`bedcode-wasm-core = { path = "../../packages/bedcode-wasm-core", default-features = false }`
（单形态 2026-10-13 起无 common/bare 之分；移动不开 `desktop-assembly` 桌面专属装配面，
dev 依赖加 `test-support`；ABI 规格由本宿主构造 `WasmRuntime` 时经 `AbiPolicy` 注入）。
统一 wasm-core（`runtime-single-form` 收口后）分三层：

- **机制核 + 统一壳**（bus / config / db / monitor / permission / security / storage /
  `manager::runtime::engine` 组件引擎唯一一份 + `runtime.rs` / `runtime/component.rs`
  唯一运行时壳——**双端同一份源码**，移动经同一壳装配）；
- **移动宿主自报域**（`plugin/host_domains/`，单形态收口后只留**端差异 4 域** http /
  peer / mdns / ws；双端全等 8 域 storage / log / config / plugin-database / events /
  fs / bus / platform 已删移动副本、改由内核 `core_binding` 单份实现——权限门收敛
  统一 `PermissionManager`（激活时授予进统一词表）、插件私有库布局统一内核
  `plugins/<plugin_id>/plugin.db`）：linker = 内核 core 交集绑定 + **宿主自报能力模块**
  （`inventory`）+ WASI p2/p3；`command` / `lifecycle` / `manifest` 走内核 core 交集
  绑定（`manager/runtime/core_binding.rs`，绑契约真源 `wit/core.wit`）、
  `events.*` 走宿主注入的 `GuestEventDispatcher`（导出探测名由内核 `contract.rs` 单点派生，
  版本随 wit 包上 wire，ADR 0051）。移动独有 5 域
  （notify / fs_mobile / terminal_stream / connection / auth）整面住宿主——
  **2026-10-13 契约归位后**：`notify` 本体是共享域契约 `host-notify`（真源
  `packages/bedcode-notify-engine/wit/notify.wit`，实现仍在本端 `plugin/notify.rs`），
  移动扩展 4 函数为 `host-notify-mobile`（cap-mobile）；
- **端口与测试基建**：`host_api/ports.rs`（`HostEnginePorts`，宿主引擎调用入口）+
  `test_support`（夹具构建器 + MockPorts + 组件实例转发面 + `setup_headless_runtime`，
  `any(test, feature = "test-support")` 门控）。`host_api/sql_guard.rs` 与
  `terminal_stream_gateway.rs` 均已退役/迁出（票 05）。

**防回接**：`src-tauri/tests/retired_mobile_wasm_core_fork_lock.rs`（直连 / 禁别名键 /
路径字形 / 标识符 / **第二份织体目录** 五判据）+ `src-tauri/tests/mobile_wasm_core_purity_lock.rs`
（移动独有域不回流 / core 域绑定与实现不回流 / 共享核在场 / 壳行数上限 /
移动独有机制与退役垫片不回流 / 退役引擎不复活 / **端词汇全集** 七判据；见锁索引）；
WIT 生成物由仓库根 `node scripts/compose-wit.mjs mobile --check`
（CI `wit-drift-lock.yml` + husky）盯住，手改生成物即红。

**Rust 宿主侧 — `src-tauri/src/plugin/`：**

- **manager / loader / registry / storage**：生命周期管理（加载/激活/停用/状态持久化）、
  APK assets 内置插件解压 + app_data_dir 扫描、注册表、存储
- **bindings.rs**：宿主自带 `bindgen!`（移动 WIT）+ `expect_host_module!` 白名单双向校验
  —— 解孤儿规则 E0117、给 linker 提供宿主侧装配点（ADR 0047 D5，与桌面同形装配面）
- **core 12 域 adapter**（票 05 绑定归位落点 `host_domains/`，14 文件 /
  ~2.5k 行）：`mod.rs`（模块描述符 `mobile-host-core` + 12 interface 注册 + 停用回收聚合）
  + 每域「域逻辑 + `impl Host`」同处（`log` / `storage` / `db` / `event` / `http` / `fs` /
  `config` / `bus` / `peer` / `mdns` / `platform` / `ws` / `support`）。域逻辑是共享核的
  薄转发（`bedcode-host-api-core` / `bedcode-server-http` / `bedcode-server-peer-net` /
  `bedcode-ws-client-engine` / `bedcode-discovery-engine`）；**权限门读实例级
  `granted_permissions`**（桌面 `core_binding` 的 `PermissionManager` 词表不含移动词汇，
  不能共用那份实现）；插件私有库按移动布局 `plugins/<id 去点划>.db`（非内核
  `plugins/<plugin_id>/plugin.db`）
- **guest_events.rs**：移动 `events` 导出派发器（`on-bus-message` / `on-auth-success` /
  `on-disconnect` / `on-session-*`）——内核只留事件循环机制，调哪个导出由宿主绑定实现
  （ADR 0022「机制留内核、契约面上移」）；生产在 `PluginManager::init_wasm_runtime_with` 注入
- **移动独有域 adapter**（票 04 落点，自带 `impl Host` + 域逻辑 + android 桥）：
  `notify.rs`（共享 `host-notify` 1 原语 + 移动扩展 `host-notify-mobile` 4 原语一模块
  两 impl，权限位 `notify` 同门）/ `terminal_stream.rs`（host-terminal-stream 1）/
  `fs_mobile.rs`（host-fs-mobile 2，SAF 与媒体落盘）/ `connection.rs`
  （host-connection.primary-target 1）/ `auth.rs`（host-auth 5）
- **端口接线**：`host_ports.rs`（`HostEnginePorts` 宿主引擎端口实现，含 egress_check /
  global_token 等）/ `peer_ports.rs`（`PeerPorts`，票 03）
- **`wasm_host` 垫片已退役**（票 05）：它只 re-export 内核 `host_api::sql_guard`
  （单条 `column_to_json`）且两侧零消费者——域实现直连共享核。HTTP 执行半部早已上移
  能力域 crate `bedcode-server-http`
- **downloader**：插件远程下载 + SHA256 校验 + 安装到 app_data_dir
- **approval / validation**：权限审批与内容钉扎、插件身份校验（目录名与 manifest id 一致性，防冒名）
- **saf_io / saf_path**：SAF 存储访问抽象（`SafIo` trait 主 seam，Kotlin `SafTransferPlugin` 实现）
  与 SAF Uri → 真实路径解析（文件传输 SAF 化改造核心接口）
- **message_bus / types / commands**：插件间消息总线、类型定义、Tauri 命令
  （`fs_auth` 宿主自实现已随 2026-10-11 裁决退役：fs 授权统一走内核
  `security::fs_auth::FsAuthChecker` 判定链）
- **android_plugins/**：Android 原生插件 Rust 注册桥（前台服务、SAF 选择/写入、生物识别、
  全部文件访问、下载目录、文件删除、组播锁、通知、设备信息、插件资产解压 → 对应 Kotlin 端 Plugin，
  Kotlin 文件位置见 Project Structure 下方的「Android 原生层」说明）。
  设备唯一 ID 的 `device.rs` 注册桥 2026-10-11 退役：对应 `DeviceIdPlugin.kt` 在 `gen/android` 中并不存在，
  且注册函数从未进 `lib.rs` 的 plugin 装配——属「看起来有、实际既不注册也不被调」的假桥）

**前端插件系统 — `src/plugin/`：** 加载器、注册表、权限、上下文、事件、命令、共享模块运行时
（`__BEDCODE_SHARED__`）、对话框宿主、终端流 / 会话事件机制面（`terminal-stream.ts` /
`host-events.ts`，票 15）；`components/` 下为插件 UI 宿主（导航页签/设置页/终端工具栏/视图宿主）。
扩展点收口：`ui.registerToolboxPage` / `registerNavTab` / `registerTerminalToolbarItem` /
`registerSettingsSection` / `registerRoute` + **`registerTerminalView`（票 15：终端主视图单实例
槽，`/mobile/terminal/:id` 薄壳与宿主壳运行面共同消费，`ui:input` 门）**。
任务队列面板不在宿主侧——随票 16 并入 `wasm-apps/terminal-session/src/task/`（插件前端经 createApp 自挂载）。

**插件开发 SDK — `packages/plugin-sdk-mobile/`：** Rust + TS 双侧 SDK；相比桌面端额外封装移动端专属能力
（SAF 存储访问、对话框/系统通知、动态路由、生命周期钩子、dev-shell 演示数据协议）；含插件模板
（`template/`）、脚手架（`bin/`）、调试壳（`dev-shell/`）、共享 UI 子路径导出（`./ui`）。
调试壳内置**与宿主壳同构的 mini 壳**（`dev-shell/src/shell/**`：types → registry → composables →
components/screens，数据源 `adapters/devAppSource.ts` 把「内置应用 + 被调试插件」投影成壳应用），
与宿主 `src/shell/**` 的契约漂移由 `dev-shell/__tests__/shell/contractDrift.test.ts` 钉住；
**视觉层**（设计 token / Tailwind 主题）走单一真源：monorepo 内调试壳直接 import 宿主
`src/styles/mobile.css`（`dev-shell/vite.config.ts` 的 `@bedcode/mobile-styles` 别名，npm 包内
回落到自带副本），副本与宿主的一致性由 `dev-shell/__tests__/visualParity.test.ts` 钉住——插件 UI
几乎只靠 `var(--mobile-*)` 上色，token 副本分叉会让预览呈现与真机不同的配色；
模拟终端等内置页已降为**内置应用**（`dev-shell/src/apps/mock-terminal/`，与被调试插件同一条
加载与渲染路径），不再有壳自带的调试页签。
完整开发指南见仓库根 `bedcode-mobile/plugin-dev-mobile.md`。

**端 WIT 生成物（票 03；分片面 2026-10-13 契约归位，ADR 0052）：** `packages/plugin-sdk-mobile/rust/wit/` =
`core.wit`（共享**机制必需面**真源 `packages/bedcode-wasm-core/wit/core.wit`：8 import + 4 export）+ 各 cap 分片：
移动扩展 `cap-mobile.wit`（真源 `rust/wit-src/cap-mobile.wit`：`host-notify-mobile` / `host-fs-mobile` /
`host-terminal-stream` / `host-connection-target` / `host-auth-client` / 移动 `events`）+
能力域共享分片 `cap-mdns` / `cap-peer` / `cap-http` / `cap-ws` / `cap-notify`（真源在对应
`packages/bedcode-*/wit/*.wit`，与桌面消费同一份）+ `bedcode.wit`（package +
`world plugin { include core; include cap-…; }`），由根 `scripts/compose-wit.mjs` 按
`packages/plugin-sdk-mobile/compose.json` 拼装（`--check` 漂移锁）。契约包当前 `bedcode:plugin@0.2.0`
（minor bump 随通知/目录选择功能级统一与 core 收窄，ADR 0051 D4 / ADR 0052 D5）。
移动端 bindgen 在**宿主**：`bedcode-mobile/src-tauri/src/plugin/bindings.rs` 的 path 指
`../packages/plugin-sdk-mobile/rust/wit` 目录（push_dir 加载合成 package）——票 05 前该 bindgen
住内核 `manager/runtime/component.rs`，随绑定归位迁出（内核零 WIT 绑定）。

**票 04 交集切片收拢（ABI 19→20）：** 共享核心 core.wit 收拢 `host-websocket`（客户端 5 =
交集完整）/ `host-http`（fetch）/ `host-fs`（交集 6）；本端独有 SAF/媒体下载目录
`save-to-document` / `write-media-downloads` 拆入新接口 `host-fs-mobile`（fork
`component.rs` 拆分 `impl host_fs::Host` → `impl host_fs_mobile::Host`，add_to_linker 同行）；
旧产物（v19）import `host-fs.write-media-downloads` 等在实例化期被拒，
`stale_artifact_rebuild_hint` 点名按 v20 SDK 重建。票 05 起 `host-fs-mobile` 的
`impl Host` 在 `src-tauri/src/plugin/fs_mobile.rs`（宿主自报 `mobile-fs` 模块）。

**内置 wasm 应用源码 — `wasm-apps/*/`：** 每个 app 独立 package：`plugin.json` 元数据 + `rust/` WASM 后端 +
`src/` TS 前端 + `vite.config.ts` 独立构建。改插件后需重新构建并同步产物到打包资源。

**典型任务入口：** 新增宿主能力 → `src-tauri/src/plugin/host_domains/`（core 域）或
`src-tauri/src/plugin/<domain>.rs`（移动独有域）+ SDK `rust/src/host/` 对应域 trait；
新增 Android 原生能力 → Kotlin Plugin（gen/android）+ `android_plugins/` 注册桥；
插件 UI/存储问题 → 先分清前端（`src/plugin/`）还是 Rust 侧（`commands.rs` / `manager.rs`
  以及 `plugin/host_domains/` 对应域）。

### 对等网络 — 宿主适配层 `src-tauri/src/peer_*.rs` + 共享能力域 `packages/bedcode-server-peer-net`

与桌面端镜像的跨设备可信直连底座，与终端链路（`_bedcode._tcp`）完全独立。

**2026-10-10 能力域集中化（`.scratch/2026-10-10-peer-capability-consolidation/`）**：
引擎控制面（节点装配 / 生命周期 / 发送-接收-远端控制面 / 引擎事件桥）真源在**双端共享
能力域 crate** `packages/bedcode-server-peer-net`（`default = []` 纯引擎形态，
`desktop-assembly` 才装桌面 WIT 绑定）；移动 `peer_*.rs` 只剩**适配层**（端口 + 平台缝 +
装配 + 委托）。防回接锁 `tests/peer_engine_consolidation_lock.rs`（三面判据）。

- **底座 crate**：`packages/peer-net`（identity / cert / transport / trust_store / discovery / transfer）
  与 `packages/link-crypto`，双端共享；能力域 = `packages/bedcode-server-peer-net`
- **peer_net.rs（适配层）**：5 端口实现（bus / event_sink / paths / system_info / mdns——
  mdns 直接复用 `bedcode-discovery-engine::engine` 共享守护三函数）+ `PeerPlatformSeam`
  平台缝（SAF 共享访问 / MediaStore 落点提升 / 内置 `"下载"` 条目）+ `peer_ctx` 装配 +
  平台预热（设备名轮询 / 下载目录缓存）+ 生命周期薄壳（`FILE_TRANSFER_PLUGIN_ID` 归本层）；
  `NodeIdentity` 首启纯随机生成、与 `DeviceIdentity` 刻意分离（不做 ANDROID_ID 派生）；
  Android 多播锁随共享守护常驻（setup init hook）
- **插件入口（真入口）— WIT `host-peer`**：同桌面端**逐字一致的 19 函数**（`core.wit`
  的 `host-peer` 为双端全等接口：dial-peer / close / send-files / cancel 族 / 应答族 /
  共享根 / 浏览 / 拉取 / 节点启停 / active-transfers / collect-outgoing），宿主实现在
  `src-tauri/src/plugin/host_domains/peer.rs`（票 05 归位；薄转发到能力域 crate，ADR 0022 v3）；
  插件侧经 `HostPeer` trait 调用（`wasm-apps/file-transfer/rust/src/peer.rs`）。
  **对齐方向**：能力域 crate 的 19 个 `plugin_binding::peer_*` 域函数已是 WIT-无关
  （`&Arc<dyn PeerPorts>`），移动可直接复用（补移动 `PeerPorts` + 薄转发）——见
  `.scratch/2026-10-10-peer-capability-consolidation/`
- **命令面（注册于 `lib.rs`）：票 09 + 票 10 后为零**——`peer_net` / `peer_transfer` /
  `peer_receive` / `peer_remote` 四个模块**零 Tauri 注册**。票 09 清掉发现与连接编排面
  （设备列表查询 / 缓存解析版拨号 / 共享目录注册表 CRUD / 节点启停 / 首连应答 /
  信任管理六命令），票 10 清掉传输调度面与远端浏览面（`cancel_peer_transfer` /
  `pause_peer_transfer` / `resume_peer_transfer` / `peer_pick_files`、
  `get_peer_receive_settings` / `set_peer_transfer_encryption` /
  `set_peer_transfer_concurrency`、`list_peer_shared_roots` / `browse_peer_directory` /
  `pull_peer_files`；`set_peer_receive_policy` 去注册但保留引擎原语）。真入口只有两条：
  ① 插件 activate-deactivate 外壳（`ensure_node_started` / `stop_node_for_plugin`）
  ② WIT 原语——`host-peer`（拨号 / 发送 / 暂停恢复 / 应答 / 策略 / 落点 / 共享根镜像 /
  浏览 / 拉取 / 节点启停）与 `host-platform` 选源。原语函数可见性收为 `pub(crate)`
  （编译期双保险：`invoke_handler!` 注回即 **E0433**）。防回接锁
  `src-tauri/tests/retired_mobile_peer_transfer_command_face_lock.rs`
- **设备列表投影（票 09 收口）**：宿主**不持有**设备列表派生视图——真源是插件前端
  `composables/deviceState.ts` 状态机（host-mdns 自建 browse 的属主定向
  `<owner>::mdns:found|lost` 事件驱动——双端共享 lib spec M3 统一属主命名空间
  终态 + last-seen 快照经
  `device_bridge` 落 host-storage）；拨号寻址显式（`dial-peer(endpoint)`，
  引擎不内藏 node-id → 地址解析表）。宿主 `DiscoveryCache` 只作**引擎事实**
  （入站连接展示名、首连确认落库元数据、endpoint 拨号的展示名兜底）。旧快照 topic
  `peer:devices` 双向退役（宿主无发布者、插件不再订阅），防回接锁
  `src-tauri/tests/retired_mobile_peer_discovery_projection_lock.rs`
- **传输设置字段（票 10 收口）**：`PeerTransferSettings` 只剩两个写入面——策略/超时
  （`set-receive-policy`）与接收落点（`set-download-dir`），加密开关与拉取并发上限的
  写入侧随命令面退役：加密由插件随 `send-files` 载荷逐项下发（宿主兜底分支恒 false），
  拉取并发读值退化为「磁盘既有值或缺省 3」（拉取编排本身仍是宿主 B2 遗留，见票 07/08 §5.2）
- **数据面**：`peer_remote.rs` 浏览/拉取（线协议「单连接单请求」）；`peer_transfer.rs` 发送侧
  **句柄表 + 引擎事件桥**（票 06：一次调用即发一会话，扇出的并发节流归插件侧闸门）；
  `peer_receive.rs` **询问回执表 + 策略闸门**（ask/always_accept/always_deny + 询问超时 +
  落点）+ 接收事件桥（票 07：接收任务表已下沉插件）
- **`peer_events.rs`**：双方向共用的引擎事实→插件总线事件翻译单点（载荷构造 + 节流窗口 +
  直推；`peer:transfer-event` / `peer:receive-event` 两条 topic）
- **事件范式**：发送方向 `peer:transfer-event`、接收方向 `peer:receive-event` 引擎原始事件
  直推插件总线（progress 150ms 节流；kind 集见 WIT `host-peer` 接口文档），**旧快照 topic
  `peer-transfer-changed` / `peer-receive-changed` 均已退役**（票 06 / 票 07）
- **业务归属**：传输 UI 与业务逻辑在 file-transfer 插件（**双端单产物**：wasm 编排层在桌面端
  `bedcode-desktop/wasm-apps/file-transfer/rust/src/peer.rs`，2026-10-13 起移动端 `rust/` 退役、
  同一份产物双端加载；`usePeerDevices` / `useConsent` 等前端经 `HostPeer` trait 调宿主原语）；
  双方向任务真源 = 插件 `transfer_store::reduce_event` 事件归约（票 06 发送 / 票 07 接收）；
  `peer_migration.rs` 把引擎侧旧数据幂等迁入插件存储键
- **回放与节流判据单点**（票 08，判据在共享核 `packages/bedcode-file-transfer-core/src/transfer.rs`
  纯函数，桌面端产物 crate 的 `transfer_store.rs` 为纯转出）：
  `retry_source`（终态 + 有 `retry_meta` 才可重试，三类拒绝各有文案）· `send_slot_open`
  （发送闸门，下限 1）· `push_pull_intent` / `take_pull_intent`（拉取意图队列，封顶
  `PULL_INTENT_CAP`，**入队先于 `peer-pull-files`** —— 否则 `pull-started` 事件早于入队、
  `retry_meta` 永挂不上）。编排层（插件 `peer.rs`）相应次序：`retry` 判据与闸门前置到
  调引擎之前（send 方向无引擎建行事件，先发后校验会铸出永不入店的孤儿会话并占死槽位）；
  排队发送批派发失败按 `MAX_SEND_ATTEMPTS` 重排队、用尽后落带凭证的终态行（不再静默丢
  用户意图）；`node-stopped` 摘除陈旧 session 句柄、保留 endpoint memo 供重拨；插件停用清空
  进程内意图（排队发送批 + 待挂载拉取凭证）

### 前端终端链路 — `src/composables/`（含 `terminal/`）+ `src/stores/`

- **views/TerminalView.vue**：编排层——只留 Vue 生命周期接线（`onMounted` / `onUnmounted` /
  `watch`）+ 模板 ref/computed 与跨一步的模板事件薄封装
- **composables/terminal/**（TerminalView 拆分产物，范式参考桌面端 `composables/terminal/`，
  共享实例与回调经 `terminalKernel` 上下文交换）。按**大颗粒主题**分组（2026-09-18 二次拆分）：
  - **useTerminalDisplay**（终端显示）：xterm 实例装配与销毁（构造选项/addon/ResizeObserver/
    首帧 fit 收敛/DPR 监听）、内置 CJK 等宽字体就绪等待、主题解析与网格重排、
    清屏 / 手动刷新 / 合成层强制重绘、选择操作栏定位
  - **useTerminalInput**（终端输入）：输入栏回传（文本 / 执行 / 特殊键）、预设任务发送与执行、
    命令面板预设识别（config_id 反查）、侧栏「插入引用」填充
  - **useTerminalPanels**（功能栏）：标题栏 / 侧边栏 / 弹窗开关状态、工具栏动作分发、新手引导
  - **useTerminalRenderer**：网格测量与构造期预估、DPR 感知 fit（列 ±1 漂移钳制）、WebGL 可选加载与
    context-loss 恢复、字符图集预热（仅 WebGL 生效）、DPR 变化监听
  - **useTerminalResize**：PTY 尺寸串行队列 + 服务端正统渲染端裁决（覆盖确认弹窗）
  - **useTerminalKeyboardAvoidance**：visualViewport + 插件 safeAreaChanged 双通道键盘检测、
    **lift 语义避让**（`.movable-area` 整体 `translateY(-键盘高)`，网格尺寸不变、零重排；
    旧的「根容器高度收缩 = resize 语义」已注释保留在文件末）、页面 pan 守卫
  - **useTerminalSubscription**：订阅失败重试 + 历史渲染就绪门控（加载遮罩放行三信号）
- **terminalBuffer store + useTerminalBuffer**：Rust 命令驱动（订阅/输入/ack），帧消费 =
  页面 Channel **裸字节**按序渲染 + 本地计数（lastRenderedOffset，仅 ack 水位）；
  重锚 `terminal-resync` 清屏归零（`hasRenderedContent` 门控提示一次）；停止帧后字节丢弃；
  无缺口判定/去重/裁剪（缺口号不再误报）；`subscribed`（phase=live）为渲染与输入门控
  （无独立 history 阶段）
- **useTerminalScroll**：触摸滚动（含惯性）、自定义滚动条、长按选择模式
- **utils/terminal***：resize 触发策略（`resolveGridResize` 列漂移钳制）、分层防抖、网格测量、滚动历史行数
- **终端字体（`src/styles/terminal-font.css` + `src/assets/fonts/`）**：随包内置 **Sarasa Mono SC 子集**
  （更纱黑体，OFL-1.1；拉丁 0.5em / CJK 1em=2 格 / 制表符 0.5em），根治「系统等宽给拉丁 advance +
  比例 CJK 回退 1em」造成的行尾凹凸与 TUI 背景盒出界；子集生成器 `scripts/build-terminal-font.mjs`
  （10635 码位 / 1.05MB woff2，许可证随字体入库）。就绪时序由 `terminalMetrics.ensureTerminalFontLoaded()`
  把关（首次测量前 await，3s 超时回退 fallback）——字体栈 `FONT_FAMILY` 是唯一真源
- **useMobileConnection / useMobileCommands / useHttpApi**：连接初始化与事件同步、Tauri 命令封装、HTTP API
  （文件树、resize、任务队列）；**会话控制（列表 / 起停删 / 直发输入）经 `plugin/sessionCommands.ts` 走
  `com.bedcode.terminal-session` 插件命令面（票 13，宿主 HTTP 代理路径仅余 resize）**

> 历史：终端 WS 曾由前端 `useTerminalSocket.ts` 直连桌面（TB v2 seq 语义），2026-09-12 已迁入 Rust
>（`src-tauri/src/terminal_link.rs`）并升级 TB v3 字节偏移——useTerminalSocket.ts 已删除；2026-09-18
> `TerminalView.vue` 按域拆分为 `composables/terminal/`（同批修复键盘避让连带 ±1 列漂移触发整缓冲重排）；
> 同日夜段2 修背压死锁（ack 驱动补投，优化文档 §17）+ 输出帧改页面 Channel（§18，删除 `terminal-frame` 事件）；
> 2026-09-18 二次拆分按大颗粒主题重排为 显示 / 输入 / 功能栏 三域（原先过细的
> 主题/动作/实例/选择栏/预设域并入这三域），组件退化为纯编排层

### 自动化任务执行机制（移动端视角）

通过 WebSocket 事件接收桌面端推送的任务状态变更，驱动任务执行与通知：

```
Desktop 插件任务域 → SDK SyncEvent → 宿主 HostSyncEvent 薄适配 → WebSocket broadcast (sync_data)
    ↓ ws_sync_task_status_changed / ws_sync_session_mode_changed
Mobile Tauri Event → useMobileCommands 监听 → useMobileConnection 更新会话状态
    ↓ useNotification 系统通知
Mobile UI（会话卡片 / AutoTaskPanelHost 任务队列）
    ↓ sendInput / HTTP API（task-queue 操作）
Desktop PTY → Claude Code
```

模式切换走 HTTP（不经过 PTY）：`POST /api/plugin/com.bedcode.terminal-session/session-mode`（JWT 认证）→ 广播回同步 UI。

涉及目录：`src/composables/`（useMobileConnection/useNotification/useHttpApi/usePresetTasks）、
`src/components/`（TaskPickerModal/TaskEditDialog）、`wasm-apps/terminal-session/src/task/`（票 16 起任务面板在插件工程内）。

---

## Quick Navigation

### 按功能查找（定位到目录）

| 功能 | 目录 |
|------|------|
| WebSocket 客户端 / 心跳 / 重连 | `src-tauri/src/connection/` |
| 终端链路（协议客户端在插件 `com.bedcode.terminal-session`；宿主窄转发面 `plugin/terminal_stream.rs`） | `wasm-apps/terminal-session/` + `src-tauri/src/plugin/terminal_stream.rs` |
| 消息路由 | `src-tauri/src/router/` |
| WS 消息处理器 | `src-tauri/src/handler/` |
| 认证 / 配对 | `src-tauri/src/auth/` |
| 连接管理 (前端) | `src/composables/`（useMobileConnection/useMobileConnection 相关） |
| 终端缓冲 / 触摸滚动 | `src/stores/terminalBuffer.ts 所在 stores/` + `src/composables/`（终端链路前端侧） |
| 终端样式 / 主题 | `src/styles/`、`src/config/` |
| 文件浏览 / 代码查看 | `src/composables/`（useFileTree/useCodeHighlight/useHttpApi）、`src/views/`（CodeExplorerView） |
| mDNS 发现与广播 | `src-tauri/src/mdns/` + `src/composables/`（useMdnsDiscovery/useMdnsAdvertiser） |
| Tauri 命令 | `src-tauri/src/commands/`、`src-tauri/src/system/`（共享命令） |
| 移动端设置持久化 | `src-tauri/src/system/`（settings） |
| 预设任务 / 任务弹窗 | `src/composables/`（usePresetTasks）、`src/components/`（Task* 弹窗） |
| 任务通知 | `src/composables/`（useNotification） |
| 任务队列面板（任务域，票 16 并入） | `wasm-apps/terminal-session/src/task/` |
| 文件传输 (SAF) | `wasm-apps/file-transfer/`（前端 + manifest；**wasm 产物源在桌面端同一 crate**，双端单产物 2026-10-13）+ `src-tauri/src/plugin/`（saf_io/saf_path）+ `src-tauri/src/file_service/` |
| file-transfer 桌面兼容桩（单产物 import 闭合） | `src-tauri/src/plugin/desktop_compat.rs`（`host-api-call` / `host-platform-desktop` 全函数 fail-visible `Err`；bindgen 桌面 wit `world desktop-compat`） |
| 对等网络（节点/信任/发现） | `src-tauri/src/peer_net.rs` |
| 对等传输（发送/接收/远端浏览） | `src-tauri/src/peer_transfer.rs`、`peer_receive.rs`、`peer_remote.rs` |
| 对等网络底座 crate | `../packages/peer-net`、`../packages/link-crypto` |
| 链路加密（HTTP 信封；**WS 帧级已随桌面端插件端点加密退役**） | `src/services/linkCrypto.ts`、`src/composables/`（useLinkEncryption） |
| 多播锁（mDNS 前置） | `src-tauri/src/plugin/android_plugins/multicast_lock.rs` + Kotlin `MulticastLockPlugin` |
| SAF Uri → 路径解析 / SAF 读写抽象 | `src-tauri/src/plugin/saf_path.rs`、`saf_io.rs`（主 seam） |
| 插件审批 / 身份校验 | `src-tauri/src/plugin/approval.rs`、`validation.rs` |
| 设置子页面 (外观/连接/认证/通知) | `src/views/settings/` |
| 帮助文档资源 (快捷键/终端) | `src/assets/` |
| Android 前台服务 / 平台特性 | `src/composables/`（useForegroundService/useAndroidFeatures/useEdgeToEdge） |
| Android 原生插件注册 | `src-tauri/src/plugin/android_plugins/` |
| 插件系统 (Rust) | `src-tauri/src/plugin/` |
| 插件系统 (前端) | `src/plugin/`、`src/views/`（PluginView） |
| **移动端前端默认落点（宿主壳）** | `src/shell/`——公共组件库 `components/ui/`、平台机制副本 `composables/`（规则见 AGENTS.md §6） |
| 插件开发 SDK | `packages/plugin-sdk-mobile/`（开发指南：仓库根 `plugin-dev-mobile.md`） |
| 插件源码 | `wasm-apps/*/` |
| 系统常量 / 错误类型 | `src-tauri/src/system/`（constants/ 按领域分组） |
| 全局状态 (Rust) | `src-tauri/src/state.rs` |
| 国际化 | `src/locales/` |
| 更新检查 | `src/composables/`（useUpdateChecker） |

### 按类型查找

| 类型 | 路径模式 |
|------|----------|
| Tauri Commands | `src-tauri/src/commands/*.rs`, `src-tauri/src/system/commands.rs` |
| 错误处理 | `src-tauri/src/system/error.rs` |
| 系统常量 | `src-tauri/src/system/constants/*.rs` |
| 数据模型 | `src-tauri/src/model/*.rs` |
| 枚举类型 | `src-tauri/src/enums/*.rs` |
| 消息处理器 | `src-tauri/src/handler/*.rs` |
| Pinia Stores | `src/stores/*.ts` |
| Composables | `src/composables/*.ts` |
| WebSocket 客户端 | `src-tauri/src/connection/*.rs` |
| 对等网络 | `src-tauri/src/peer_*.rs`（命令注册于 `lib.rs`） |
| 路由 | `src-tauri/src/router/*.rs` |
| 认证 | `src-tauri/src/auth/*.rs` |
| mDNS | `src-tauri/src/mdns/*.rs` |
| 插件系统 (Rust) | `src-tauri/src/plugin/*.rs` |
| 插件系统 (前端) | `src/plugin/` |
| 插件源码 | `wasm-apps/*/` |

---

### 防回接锁索引（改宿主 / 改前端壳前扫一眼）

与桌面端同源纪律（AGENTS.md §5.1 / §6 / §8）：越线回接会直接测红。锁名 → 所在文件：

- **前端零资源访问**（三层封锁，移动端形态与桌面**不同**，勿照抄桌面结论）：
  - 源码静态锁（含 ESLint ignores 之外的二次门禁）→ 根 `eslint.config.js` 的
    `bedcode/frontend-no-resource-access`（`frontendSourceGlobs` 含 `bedcode-mobile/src/**`，
    即**壳层已被覆盖**）。两点须留意：① `ignores` 里有 `**/__tests__/**`，所以
    **测试文件不受此锁约束**，改窄 glob 时壳的**测试期**门禁只剩下面的 L1；② 改窄 glob 会让壳
    悄悄失去约束，故 L1 独立复刻了一份源码扫描
  - 运行期 ACL → `src-tauri/capabilities/mobile.json`（权限白名单里**没有** http / fs / shell /
    updater / opener 类插件；新增即等于开能力面）
  - 引擎级 CSP → `src-tauri/tauri.conf.json` 的 `security.csp`（生产 `connect-src: 'none'`
    硬封网络）。**注意口径差异**：`security.devCsp` 把 `connect-src` 放宽为
    `'self' ipc: ws: wss: http: https:`（dev HMR 需要），所以 **dev 下 CSP 不构成网络封锁**，
    dev 期的实际边界只有源码静态锁 + ACL + Rust 权限闸门
- **壳层约束锁 L1–L7**（测试期门禁，`pnpm run test:run` 必跑；扫描跳过注释）→
  `src/__tests__/shell/shellConstraintLocks.test.ts`：L1 前端零资源访问（网络/文件/导航原语）·
  L2 不经裸 invoke · L3 平台授权弹窗（`FsAuthDialog` / `EgressConsentDialog`）在 `App.vue`
  恰好挂载 1 处 · L4 无静默 catch · L5 模板区文案走 i18n（CJK 区 `一-鿿`）· L6 无硬编码颜色
  （`styles/shell.css` 为 token 落点，已排除）· **L7 壳内自足**（壳内不得 import
  `@/components|@/composables|@/views`：新界面缺件先复制进 `src/shell/**`，确需桥接在
  `BRIDGE_ALLOWLIST` 登记并写理由；同锁正面钉住复制面文件在场，防「删文件绕过依赖锁」）。
  新增壳锁**同步登记本索引**
- **壳注册桥与插件域机制锁**（票 2026-10-09，测试期门禁，`pnpm run test:run` 必跑）：
  - 插件上下文 → 壳注册表（运行面 appId 代填 / `PluginViewHost` 补 `provide('pluginContext')` /
    dispose 入 `_disposables` / 运行面免 `ui:*` 权限门而旧嵌入面仍受门 / slot·capsule·settings 三面）
    → `src/__tests__/plugin/pluginContextShell.test.ts`
  - 运行面解析优先级（① 应用自带 surface → ② 数据源延迟解析 → ③ 空态；组件按引用透传）
    → `src/__tests__/shell/useShellAppsSurface.test.ts`
  - 壳设置门类齐全（连接 / 认证 / 出站 / 外观 / 权限 / 通知 / 关于七项在场；新增三项跳既有设置页路由，
    既有项口径不漂移；设置项文案双语在场）—— **旧宿主退役的前置**（壳不覆盖全部设置门类就不许删旧设置）
    → `src/__tests__/shell/shellSettingsScreen.test.ts`
  - 连接生命周期事件白名单投影（6 个 `ws_*` 事件 / 载荷映射 / dispose 幂等 / 非 Tauri 降级 / 在途注销）
    → `src/__tests__/plugin/connectionEvents.test.ts`
  - 插件域机制与登记（错误分类表 + `ensureCommandOk` + 机制键双语在场 / 二维码载荷解析 /
    **宿主页两区块真实挂载**（防「模板绑定未声明」这类只会在渲染期暴露的缺陷）/ 宿主页域激活回收 /
    任务域壳内任务页注册）→ `wasm-apps/terminal-session/src/{host,task}/__tests__/**`
  - 应用壳域（底部导航页签状态机 + 嵌套横滑区仲裁 + 运行面注册唯一性，票 2026-10-10 A）
    → `wasm-apps/terminal-session/src/app/__tests__/**`
  - 业务设置域（定义表 ↔ 归一函数 ↔ KV 桥读写；越界值夹回区间、非法值回落默认、写失败不更新本地值）
    → `wasm-apps/terminal-session/src/settings/__tests__/**`
- **旧宿主 UI 退役面不回接（票 2026-10-09 阶段 B）** → `src/__tests__/shell/retiredHostUIRetirementLocks.test.ts`：
  R1 退役路由名（`mobile-devices` / `mobile-sessions` / `mobile-terminal` / `mobile-toolbox` /
  `mobile-preset-tasks` / `mobile-plugins` / `mobile-home-alt`，带引号字面量锁定，不误伤
  `--mobile-terminal-bg` 类 CSS 变量）不得在 `src/` 出现；R2 退役视图 / 组件符号
  （`MobileSwipeContainer` / `MobileNav` / `MobileLayout` / `MobileStatusBar` / `DevicesView` /
  `SessionsView` / `ToolboxView` / `PluginView` / `SettingsView` / `PresetTasksView` /
  `TerminalView` / `registerSettingsSection`，词边界锁定，不误伤服役面 `PluginViewHost` /
  `EgressSettingsView` / `registerTerminalView`）不得在 `src/` 出现；R3 正面钉壳等价物在场
  （`ShellView` / `ShellHost` / `ShellTabbar` / `ShellSettingsScreen` + 路由 `mobile-shell`），
  防「删新面绕过退役锁」。扫描跳注释，覆盖整个 `src/`（含测试文件）
  - **票 2026-10-10 C1/C2/C4/D1 追加（ADR 0046）**（同一文件新增四把锁）：
    R4 旧嵌入扩展点（`registerToolboxPage` / `registerNavTab` / `registerTerminalToolbarItem` /
    `registerTerminalView`）与其宿主 UI 组件（`PluginNavTabHost` / `PluginSettingsHost` /
    `PluginTerminalBar`）不得在 `src/` 出现；运行面解析（`pluginAppSource.resolveSurface`）
    不得再有 `toolboxViews` / `navTabs` / `terminalView` 回退链；
    R4b 已下沉的 `mobile-settings-notifications` 路由 / `NotificationSettingsView` 不得回接；
    R5 已删宿主孤儿（`DeviceCard` / `SessionListItem` / `useAndroidFeatures` / `useRunTime`）
    不得重新出现在 `src/` 且文件确已不在盘。
    白名单仅 `src/plugin/context.ts`（fail-visible 抛错桩）+ 两个钉死其行为的测试文件，
    逐条登记不做通配
- 发送编排退役面（票 06）→ `src-tauri/tests/retired_mobile_send_orchestration_lock.rs`（`retired_mobile_send_orchestration_is_not_reintroduced`）
- 接收编排退役面（票 07）→ `src-tauri/tests/retired_mobile_receive_orchestration_lock.rs`（`retired_mobile_receive_orchestration_is_not_reintroduced`）
- 传输 / 接收 / 远端浏览调度与设置命令面退役（票 10）→ `src-tauri/tests/retired_mobile_peer_transfer_command_face_lock.rs`（`retired_peer_transfer_settings_command_face_is_not_reintroduced` + `peer_transfer_scheduling_entrypoints_stay_engine_only`；同文件另钉 peer 宿主模块不得带 `#[tauri::command]` / 不得进 `invoke_handler`）
- 发现投影 / 设备列表投影 + 宿主 peer 命令面退役（票 09）→ `src-tauri/tests/retired_mobile_peer_discovery_projection_lock.rs`（两条：`retired_mobile_peer_discovery_projection_is_not_reintroduced` / `retired_host_peer_command_face_is_not_reintroduced`）
- 终端订阅协议客户端退役（票 12）→ `src-tauri/tests/retired_mobile_terminal_link_lock.rs`（符号缺失 / 命令未注册 / 前端无退役命令字面量 三条 + `mobile_terminal_retained_face_stays` 正面钉住保留面）
- host-terminal / terminal-hooks 整面退役（票 15 阶段 B：终端 UI 域随阶段 A 迁插件前端后，WIT import
  `host-terminal` 与导出 `terminal-hooks`、SDK `HostTerminal` / `TerminalHandler` / TS `TerminalAPI`、
  宿主 `PluginLifecycleEvent::TerminalInput/TerminalOutput`、`terminal:input` 权限位整层删除，
  mobile ABI 16 → 17）→ `src-tauri/tests/retired_mobile_host_terminal_hooks_lock.rs`
  （WIT 缺席 / SDK+宿主 Rust 接线缺席 / 前端退役词汇缺席（排除 `__tests__`）三条 +
  `mobile_terminal_stream_retained_face_stays` 反向钉住：`host-terminal-stream.forward-output` +
  `terminal:output` 权限位 + `plugin/terminal_stream.rs` 窄转发 + `primary-target` 在场）
- auto-task 独立插件整体退役（票 16：并入 `com.bedcode.terminal-session`，任务域前端在
  `wasm-apps/terminal-session/src/task/`）→ `src-tauri/tests/retired_mobile_auto_task_plugin_lock.rs`
  （旧 id / 视图 id / 命令 id 前缀扫描 + `merged_task_domain_and_plugin_retirement_stay` 反向断言：
  插件目录与打包资源目录不得复活、manifest 权限并集与换 id 后的扩展点必须在场）
- 本地配对码编排面退役（票 14 阶段 A，本端不是配对码颁发方）→ `src-tauri/tests/retired_mobile_local_pairing_code_face_lock.rs`（同上三形态 + `mobile_auth_engine_face_stays`）
- 认证编排命令面退役（票 14 阶段 B：配对 / QR / 生物挑战编排迁 `com.bedcode.terminal-session`
  插件 auth 域，宿主只余引擎事实面）→ `src-tauri/tests/retired_mobile_auth_orchestration_command_face_lock.rs`
  （宿主零退役编排符号含 3 个流程事件 helper / `invoke_handler!` 零 5 注册项 / 前端零退役命令字面量 +
  `auth_engine_projection_and_plugin_domain_stays` 反向钉住：引擎方法 + `plugin/auth.rs` 投影 +
  窄读命令 `ws_get_auth_credentials` + 插件编排域 + manifest `auth` 位）
- 会话控制客户端退役（票 13：`session/http.rs` + `SessionManager` + 三命令迁插件 session 域，
  `ws_disconnect` 停活跃会话死分支同删）→ `src-tauri/tests/retired_mobile_session_control_face_lock.rs`
  （退役符号 12 needle / `invoke_handler!` 零 3 注册项 / 前端零退役字面量 / 新面在场四条 +
  原 `session/http.rs` 三结构锁随迁：旧信封命令名 / WS 帧级加密零残留 / 控制面无信封引用；
  变异自检 4/4：符号回接 → 锁1，stub+注册 → 锁1+2，前端字面量 → 锁3+4，删 `network:http` → 锁4）
- 插件 KV 真源 = 主库 `plugin_storage` 表（票 05b 真源搬迁）→ `src-tauri/tests/plugin_storage_db_backed_lock.rs`（`plugin_storage_is_db_backed_not_file_backed`）
- `host-websocket` 客户端子集域边界（票 11：移动 WIT 不得长出 ws **服务端**域 / 不得定义 ws 服务端权限）→ `src-tauri/tests/mobile_host_websocket_client_domain_lock.rs`
- egress 三档策略架构不变量（票 20：档位→动作映射单点 `StrategyStep::of` / 写入面 `parse_wire` 单点 / 安全义务符号在场）→ `src-tauri/tests/egress_tier_mapping_single_point_lock.rs`（3 例 + 变异自检 3/3）
- **移动 fork crate 退役面**（票 06 批次 04 / 票 07 批次 02；**票 05 泛化为「禁任何第二份 wasm-core 织体」；票 06 补第 5 判据**）：fork 目录不得复活 / `bedcode-wasm-core-mobile` 只能当依赖键别名（不得作包名）/ 移动构建面不得出现 fork 形态路径（`"../packages/bedcode-wasm-core"`）/ `src-tauri` **直连**仓库根 crate（`bedcode-wasm-core` + 单形态 feature，禁 rename 别名键）/ 移动 `src` + `tests` 源码不得出现 `bedcode_wasm_core_mobile` 标识符 / **`packages/bedcode-wasm-core` 之外不得有任何 `bedcode-wasm-core*` 目录**（第 5 判据盯「换个新名字重新长一份」——票 04/05 把双端绑定迁出内核后「复制一份内核自带绑定」是最省事也最危险的动作）→ `src-tauri/tests/retired_mobile_wasm_core_fork_lock.rs`（5 判据；变异自检：票 05 为 ⑤ 标识符注入 1/1 + 构建级即红；票 06 为第 5 判据注入 1/1）
- **wasm-core 纯净性防回接**（票 2026-10-10 `wasm-core-mobile-purity` 票 06-①；**票 2026-10-11/12/13 `wasm-core-single-form` 票 05 按绑定归位终态重写、票 06 补判据 7**；ADR 0047 D2/D4/D5 + ADR 0049）：① 移动宿主实现的域不得回流（含共享 `host-notify` 的移动实现 + 扩展 `host-notify-mobile`；5 域 `Host` impl / 域文件零命中，正向钉住宿主 adapter 在场）② core 12 域的绑定与实现不得回流（内核唯一合法落点 = `manager/runtime/core_binding.rs`；移动壳禁 `bindgen!` 与 `bedcode::plugin::` 路径，正向钉住宿主 `bindings.rs` / `guest_events.rs` / `host_domains/` 在场）③ 共享实现核必须在场（`server-http::plugin_binding::egress` · `host-api-core::{fs,storage,bus}`）且宿主 adapter 真的转发到它们 ④ 移动壳行数上限（单文件 600 / 合计 800）⑤ 移动独有机制与退役垫片不回流（终端网关住宿主 `plugin/terminal_stream.rs`；零消费者 `sql_guard` 垫片退役）⑥ 已退役的 `host_api/http_engine.rs` 不得复活 ⑦ **端词汇全集**：7a 内核代码行零 cap 分片 world 名（注释放行——模块文档必须说明「cap 组合不在内核」）· 7b 声明位零端名（豁免走 `DECLARED_NAME_EXEMPTIONS` 登记表，每条须写理由）· 7c 移动端 SDK 词汇封顶在唯一已知落点 `manager/runtime/bare/component.rs`（双 SDK 收敛遗留项的看门狗，见 §遗留声明）· 7d 正向锚点（`core_binding.rs` 确含 `bindgen!` 且指向 `wit/core.wit`，防 7a–7c 空转）→ `src-tauri/tests/mobile_wasm_core_purity_lock.rs`（7 判据；变异自检：票 05 判据 1–6 **4/4**，票 06 判据 7 的 7a/7b/7c **3/3**）。**WASI 不在判据内**——用户裁决 2026-10-12 定 WASI 装配面住内核、双端默认全开（`manager/runtime/wasi.rs` + `CoreConfig::wasi`），它是共享机制不是形态差异轴。**改 wasm-core `runtime/{common,bare}*` 或移动 `src/plugin/{host_domains/**,bindings,guest_events}.rs` 前先看它**
- **契约归属边界三把锁（ADR 0052 · domain-owned-contracts 票 05，跨端扫描）**：① core 机制纯净——`core.wit` interface 集必须 == 8 import + 4 export 白名单（业务能力回流即点名）；② 域契约组合——双端 caps 值对照域契约房 / 端 wit-src，**移动 caps 禁端扩展键与 `pty`**，每个 cap 真源声明 `world cap-<key>`；③ 能力域端中立——域 crate 绑定面必须 `#[cfg(feature = "common-host")]` 门控、域分片 interface 仅限共享白名单、生产代码零端扩展词汇 → `bedcode-desktop/src-tauri/tests/domain_contract_structure_locks.rs`（变异自检 3/3）
- **对等网络能力域集中化**（2026-10-10，ADR 0048）：移动宿主不得重新持有引擎控制面（节点装配 / 生命周期闸门 / 会话句柄表 / 载荷构造 / 接收设置落盘）→ `src-tauri/tests/peer_engine_consolidation_lock.rs`（三面判据：退役面缺席 / 适配面在场 / 共享依赖在场；变异自检 1/1）
- **mDNS 广播死面退役**（2026-10-10，ADR 0048）：`MdnsAdvertiser` / 广播命令 / 前端死 composable / 自建 `ServiceDaemon::new` 不得回到移动宿主与前端 → `src-tauri/tests/retired_mobile_mdns_advertise_lock.rs`（退役面缺席 / browse 薄壳在场 / 共享守护复用锚点；变异自检 1/1）
- **移动 WIT 分片拼装组合**（票 03/07；**版本面 2026-10-13 改判据**，ADR 0051）：`compose.json` 的 `package` 包名 == 版本真源 `contracts/bedcode-plugin-package.rs` 的 `PACKAGE_NAME` / 移动 WIT 源文件 package 行 == `package <name>@<version>;` / 生成物 `bedcode.wit` include 集 == `core` + 各 caps 键 / `worlds` 列表在生成物目录有声明 → `src-tauri/tests/mobile_wit_composition_lock.rs`（变异自检 1/1）。整数 ABI 与 `interface abi` 已退役，锁内不再有 `abi.version` 断言。生成物逐字漂移面另有仓库根 `node scripts/compose-wit.mjs --all --check`（CI `wit-drift-lock.yml` + husky pre-commit）
- 票 19 Part A/B 的 A1–A4 契约锁与 fork 对称结构锁（`bedcode-mobile/packages/bedcode-wasm-core/tests/{sdk_wit_contract_locks,fork_boundary_lock}.rs`）**随移动 fork crate 一并退役**（票 06 批次 04，目录删除 = 旧读路径消失）：A1 的结构/计数面由 `mobile_wit_composition_lock.rs` 承接，A2/A3/A4 的语义面由「移动 SDK 为唯一真源 + `stale_artifact_rebuild_hint` 旧产物点名 + 双端 ABI 锁步（ADR 0019）」承接；重建需求见 `.scratch/2026-10-09-wasm-core-single-crate/ticket-07-locks-and-docs-closeout.md` §5 遗留欠账
- **Tauri 命令面注册完备性**（2026-10-11）：`#[tauri::command]` 只贴标签，真正成为命令靠 `lib.rs` 的 `generate_handler!` 注册；漏注册能过 `cargo build`、能被 `pub` 导出，只在真机 `invoke` 时回 "command not found" → `src-tauri/tests/command_registration_completeness_lock.rs`（声明 ⇄ 注册双向比对 + 判据防空跑；变异自检 2/2）。**新增 `#[tauri::command]` 时必看它**——本锁已因此钉出过一次真实漏注册（`plugin_approve`，前端 `src/plugin/commands.ts` 有对应 `invoke`，装插件后卡 NeedsApproval 的用户会看到「命令不存在」而非审批弹窗）
- **出站 HTTP 面 egress 闸门覆盖**（2026-10-11）：egress 是唯一外网出口裁决面，其安全前提是**覆盖全部出站路径** → `src-tauri/tests/outbound_egress_gate_coverage_lock.rs`（两条：构造 `reqwest` 客户端的文件必须能追到 egress 引用；设了 `.redirect(...)` 的必须用 `egress::redirect_policy()` 逐跳复检。豁免仅 `auth/http.rs` 一条——认证通道目标绑定已配对桌面端，非任意 URL；变异自检 1/1）。**新增出站路径 / 改动 `plugin/downloader.rs` 前必看它**——本锁已钉住一次真实旁路：`plugin_download` 命令的 `zip_url` 由前端传入且自建 `reqwest::Client`，是当时唯一不过闸门的出站面（下载即执行，放行未授权目标 = 远端代码在设备上落地）
- **接收侧编排退役面**（票 07，含 2026-10-11 扩充）：除编排构件的结构面扫描外，另钉「`src/peer_receive/tests/` 不得复活」——该目录曾是一对**永不编译**的单测（能力域集中化把符号搬进 `bedcode-server-peer-net` 后 `#[cfg(test)] mod tests` 声明被删，`tests/*.rs` 不被 rustc 自动发现），读代码的人以为有覆盖、实际零覆盖 → 同 `retired_mobile_receive_orchestration_lock.rs`

> 迁移 / 抽包类任务的规格与逐票记录在 `.scratch/2026-10-07-mobile-wasm-core-refactor/` 与
> `.scratch/2026-10-07-capability-crates-to-root-packages/`（`.scratch` 只在 uat/master 之外分支入库）。

---
