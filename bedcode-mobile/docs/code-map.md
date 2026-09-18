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
│                                     #   android-dev-log.js（logcat 落盘 + 按天清理）、插件构建
│                                     #   （plugin-build.js）、产物大小检查（check-target-size.js）、
│                                     #   adb-fd0-shim.sh（adb client fd0 bug 自愈 shim）
├── packages/                         # 共享包（供移动端插件开发与测试使用）
│   ├── plugin-sdk-mobile/            # 移动端插件开发 SDK：Rust + TS 双侧、dev-shell 调试壳、
│   │                                 #   插件模板（template/）、UI 子路径导出
│   └── plugin-component-test/        # 测试用 WASM 插件 crate，供宿主测试套件做连通性验证
├── plugins/                          # 插件源码目录（每个插件独立 package：plugin.json 元数据 +
│                                     #   rust/ WASM 后端 + src/ TS 前端 + vite.config.ts 独立构建）
│   ├── ai-chatbox/                   # AI Chatbox 插件：多供应商 OpenAI 兼容客户端
│   ├── auto-task/                    # Auto Task 插件：任务队列 UI 面板（后端逻辑在桌面端同名插件）
│   └── file-transfer/                # 文件传输插件：基于对等网络的在线对端发现与共享目录浏览、
│                                     #   多选批量传输、接收策略、SAF 自选保存位置与历史记录
│                                     #   （OCR 插件已随 feature/ocr-plugin 隔离，本分支无）
├── src/                              # Vue 3 前端（扁平化结构）
│   ├── components/                   # UI 组件：设备卡片、文件浏览/查看、输入助手/输入栏、
│   │                                 #   移动布局/导航/状态栏、滑动容器、配对、任务弹窗、
│   │                                 #   终端输入栏/头部/设置/确认弹窗、快捷键面板、图标
│   ├── composables/                  # 业务逻辑 composable：连接管理、HTTP API、文件树、代码高亮、
│   │                                 #   终端缓冲/滚动、mDNS 发现/广播、预设任务、系统通知、
│   │                                 #   前台服务、边到边显示、屏幕方向、更新检查等；
│   │                                 #   terminal/ 下为终端内核域（模板内核上下文 + 渲染器/resize/
│   │                                 #   键盘避让/订阅，TerminalView 拆分产物，经 terminalKernel
│   │                                 #   交换实例与回调）
│   ├── stores/                       # Pinia 全局状态：代码查看器、输入助手、设置、终端缓冲、i18n
│   ├── views/                        # 页面：代码浏览器、设备、mDNS 发现、插件、扫码、会话、
│   │                                 #   设置（views/settings/ 下按领域拆分子页：外观/连接/认证/
│   │                                 #   通知/关于）、终端、工具箱
│   ├── plugin/                       # 前端插件系统：加载器、注册表、权限、上下文、事件、命令、
│   │                                 #   共享模块运行时、对话框宿主；components/ 下为插件 UI 宿主，
│   │                                 #   auto-task/ 为 Auto Task 任务队列面板
│   ├── utils/                        # 工具函数（剪贴板等）
│   ├── config/                       # 配置（终端主题定义）
│   ├── services/                     # 跨端复用服务（linkCrypto.ts 链路加密客户端）
│   ├── assets/                       # 静态资源：快捷键/终端帮助文档（zh-CN/en markdown）
│   ├── styles/                       # 样式：mobile.css 全局 + terminal.css 终端/xterm/滚动条
│   ├── locales/                      # 国际化（zh-CN / en，各含 common / desktop / mobile / settings）
│   ├── router/                       # 路由
│   └── __tests__/                    # 前端测试（composables/stores/utils/config/plugin 子目录 +
│                                     #   fixtures 测试数据 + integration 集成测试）
└── src-tauri/                        # Rust 后端（Tokio 异步）
    └── src/                          # 模块按领域扁平组织，每领域配同名入口文件（auth.rs、commands.rs 等）
        ├── auth/                     # 认证模块：认证状态管理、配对流程
        ├── commands/                 # Tauri 命令层：Android 特定、认证、连接、mDNS、
        │                             #   移动端特有（Quick Actions/Settings/Session Config）、会话、终端
        ├── connection/               # 远程连接模块（核心模块，详见 Core Modules）
        ├── enums/                    # 枚举类型：认证、控制、插件、会话、特殊键、总结、同步
        ├── file_service/             # 文件服务：SAF 目录树读取（saf_tree.rs）
        ├── handler/                  # WS 消息处理器：认证、同步、系统、终端
        ├── mdns/                     # mDNS 服务发现与广播（局域网设备互发现）
        ├── model/                    # 数据模型：API DTO、WebSocket 消息
        ├── peer_net.rs               # 对等网络接入：节点身份初始化（与 DeviceIdentity 分离）、
        │                             #   节点/发现守护装配（Android 启动守护前先经 Kotlin
        │                             #   MulticastLockPlugin 申请多播锁，不持锁则单侧不可见）、
        │                             #   首连确认闸门事件桥接
        ├── peer_receive.rs           # 接收侧：询问应答回流、接收任务登记、接收策略设置
        │                             #   （落点恒为 app 私有下载目录，MediaLanding 提升进 MediaStore）
        ├── peer_remote.rs            # 远端浏览/拉取：共享目录列目录 + 多文件拉取编排（只读）
        ├── peer_transfer.rs          # 发送侧：扇出发送编排、进度节流行转、终态历史持久化
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
> （ForegroundService/ForegroundServicePlugin、TaskNotificationPlugin/Manager、SafPicker/SafTransfer、
> BiometricKey、AllFilesAccess、DownloadsDir、FileDelete、DeviceInfo、MulticastLock、
> StatusBarStyle（App 主题 → 系统栏图标外观同步）、PluginAssetExtractor 等）
> 及 AndroidManifest、res/xml 配置。
> 改动 Kotlin 后必须跑 `./gradlew :app:compileUniversalDebugKotlin` 验证（见 AGENTS.md）。gen/android 重建后需恢复清单见 AGENTS.md。

---

## Core Modules（核心模块）

### 远程连接 — `src-tauri/src/connection/`

移动端作为远程终端与桌面端通信的核心：

- **ws_client / ws_connection**：WebSocket 客户端主实现与连接管理
- **heartbeat / reconnect**：心跳保活、断线重连
- **codec / request / request_response**：消息编解码、请求发送与请求-响应关联
- **manager / lifecycle**：连接管理器、生命周期管理
- **pairing_service**：配对服务（与 `auth/pairing.rs` 协作）
- **client_router / default_handler / traits**：客户端消息路由与处理 trait

### 终端链路（TB v3 字节连续 + 两段订阅）— `src-tauri/src/terminal_link.rs` + `stores/terminalBuffer.ts`

桌面端 PTY 输出的移动端消费链路（2026-09-12 迁入 Rust 取代前端直连 WS；2026-09-17 拆两段订阅 + 两段背压）：

- **两段订阅（生命周期彼此独立）**：
  - **段1 会话级**（Rust ↔ 桌面端）：会话 WS 连接成功后 `terminal_subscribe`，会话停止/设备断开
    `terminal_unsubscribe`；背压水位 `acked` 锚定 Rust 缓存游标（收帧即消化）
  - **段2 页面级**（前端 ↔ Rust）：进入终端页 `terminal_page_subscribe(sessionId, channel)`、退出
    `terminal_page_unsubscribe`（与 `set_mode realtime/batch` 配对）；帧经**页面级 Tauri Channel**
    投递（TB v3 二进制 Raw，一条消息可含多帧；状态/重锚仍走事件），背压水位 `frontend_rendered`
    锚定前端渲染游标——未渲染窗口越高位水停推，**ack 推进即补投**一批（≤256KB，节奏由消费端掌控）
- **terminal_link.rs（Rust 后端持有，真源 = Rust 缓存）**：每会话一个 tokio-tungstenite WS、JWT 认证、
  TB v3 帧解析（start_offset 8LE + len 4LE）、会话级字节缓存（16MB LRU + `contiguous_runs` 按洞切分）、
  ack 水位 + 节流回发（含空闲轮询兜底）、退避重连（保留游标 from_offset 重订阅）、双速 mode、一次性历史
  （缓存优先，HTTP `/api/sessions/{id}/history` 回退增量拉取）；帧出口 = 页面 Channel（`encode_data_frame`），
  状态事件 `terminal-state`/`terminal-resync`；
  命令 `terminal_subscribe/unsubscribe` · `terminal_page_subscribe/unsubscribe` ·
  `remove/send_input/set_mode/ack_rendered/get_history/get_state`
- **前端**：`stores/terminalBuffer.ts`（Rust 命令驱动 + 事件消费 + lastRenderedOffset 游标/去重/缺口
  重拼接/截断清屏/跨帧裁剪；历史拼接完成后才消费实时帧）+ `useTerminalBuffer.ts`（写队列 rAF 合并 + 背压 ack）
- 协议与架构细节：`docs/knowledge/pty-output-pipeline.md`、`.scratch/mobile-ws-rust/spec.md`

### 插件核心模块引导

移动端插件系统与桌面端同架构（wasmtime 组件沙箱），并有移动端特有能力。做插件相关改动时按层定位：

**Rust 宿主侧 — `src-tauri/src/plugin/`：**

- **manager / loader / registry / storage**：生命周期管理（加载/激活/停用/状态持久化）、
  APK assets 内置插件解压 + app_data_dir 扫描、注册表、存储
- **wasm_runtime + wasm_runtime/host_impl/**：wasmtime Engine/Store/Instance 管理（`component.rs`
  为 WASM 组件模式接线）；宿主能力实现按功能域拆分于 host_impl/
  （storage/db/fs/http/mdns/terminal/event/bus/config/notify/peer/support/platform）
- **wasm_host**：WASM Host Function 通用工具函数（SQL 表名前缀校验、数据库列类型转换、
  HTTP 代理执行），从原 `host_context.rs` 迁移的核心逻辑
- **downloader**：插件远程下载 + SHA256 校验 + 安装到 app_data_dir
- **approval / validation**：权限审批与内容钉扎、插件身份校验（目录名与 manifest id 一致性，防冒名）
- **saf_io / saf_path**：SAF 存储访问抽象（`SafIo` trait 主 seam，Kotlin `SafTransferPlugin` 实现）
  与 SAF Uri → 真实路径解析（文件传输 SAF 化改造核心接口）
- **message_bus / fs_auth / types / commands**：插件间消息总线、文件系统校验、类型定义、Tauri 命令
- **android_plugins/**：Android 原生插件 Rust 注册桥（前台服务、SAF 选择/写入、生物识别、
  全部文件访问、下载目录、文件删除、组播锁、通知、设备信息、插件资产解压 → 对应 Kotlin 端 Plugin，
  Kotlin 文件位置见 Project Structure 下方的「Android 原生层」说明）

**前端插件系统 — `src/plugin/`：** 加载器、注册表、权限、上下文、事件、命令、共享模块运行时
（`__BEDCODE_SHARED__`）、对话框宿主；`components/` 下为插件 UI 宿主（导航页签/设置页/终端工具栏/视图宿主），
`auto-task/` 为 Auto Task 任务队列面板。

**插件开发 SDK — `packages/plugin-sdk-mobile/`：** Rust + TS 双侧 SDK；相比桌面端额外封装移动端专属能力
（SAF 存储访问、对话框/系统通知、动态路由、生命周期钩子、dev-shell 演示数据协议）；含插件模板
（`template/`）、脚手架（`bin/`）、调试壳（`dev-shell/`）、共享 UI 子路径导出（`./ui`）。
完整开发指南见仓库根 `bedcode-mobile/plugin-dev-mobile.md`。

**内置插件源码 — `plugins/*/`：** 每个插件独立 package：`plugin.json` 元数据 + `rust/` WASM 后端 +
`src/` TS 前端 + `vite.config.ts` 独立构建。改插件后需重新构建并同步产物到打包资源。

**典型任务入口：** 新增宿主能力 → `wasm_runtime/host_impl/` + SDK `rust/src/host/` 对应域 trait；
新增 Android 原生能力 → Kotlin Plugin（gen/android）+ `android_plugins/` 注册桥；
插件 UI/存储问题 → 先分清前端（`src/plugin/`）还是 Rust 侧（`commands.rs` / `manager.rs`
  对应的 host_impl 域）。

### 对等网络 — `src-tauri/src/peer_*.rs` + `packages/peer-net`

与桌面端镜像的跨设备可信直连底座，与终端链路（`_bedcode._tcp`）完全独立：

- **底座 crate**：`packages/peer-net`（identity / cert / transport / trust_store / discovery / transfer）
  与 `packages/link-crypto`，双端共享
- **peer_net.rs**：`NodeIdentity` 首启纯随机生成、与 `DeviceIdentity` / `device_identity.json`
  刻意分离（不做 ANDROID_ID 等设备标识派生）；setup 阶段自动启动节点 + 发现守护；
  **Android 专属**启动守护前经 Kotlin MulticastLockPlugin 申请多播锁、关停时释放（不持锁则 mDNS
  响应收不到，表现为单侧可见）；首连确认闸门经 `peer-consent-requested` 事件桥接前端确认框
- **插件入口（真入口）— WIT `host-peer`**：同桌面端的 13 原语（见桌面 code-map 对等网络节），
  宿主实现在 `plugin/wasm_runtime/host_impl/peer.rs`（ADR 0022 v3）；插件侧经 `HostPeer` trait
  调用（`plugins/file-transfer/rust/src/peer.rs`）
- **命令面（注册于 `lib.rs`，移动端全量保留）**：节点/发现（`start/stop_peer_node`、
  `list_discovered_peers`、`dial_peer`、`disconnect_peer`）、首连确认与信任管理
  （`respond_peer_consent` / `list_trusted_peers` / `revoke_trusted_peer`）、共享目录注册表
  （`list_shared_directories` / `add_shared_directory_saf` / `remove_shared_directory`）、
  发送侧（`send_files_to_peer`、`cancel_peer_transfer`、`retry_peer_transfer`、
  `list_peer_transfers`、`clear_peer_transfer_history`、`peer_pick_files/folder`）、
  接收侧（`list_peer_receiving`、`respond_peer_transfer`、`cancel_peer_receiving`、
  `get_peer_receive_settings`、`set_peer_receive_policy`、`set_peer_transfer_encryption`、
  `clear_peer_receiving_history`）、远端浏览（`list_peer_shared_roots`、
  `browse_peer_directory`、`pull_peer_files`）
- **数据面**：`peer_remote.rs` 浏览/拉取（线协议「单连接单请求」）；`peer_transfer.rs` 发送扇出
  （「群发」仅是前端编排概念，每个接收方独立 batch_id）；`peer_receive.rs` 接收策略
  （ask/always_accept/always_deny）+ 询问超时（无落点设置）
- **事件范式**：`peer-transfer-changed` / `peer-receive-changed` 全量推送，前端按 batchId 合并双源列表
- **业务归属**：传输 UI 与业务逻辑在 file-transfer 插件（`rust/src/peer.rs`、`usePeerDevices`、
  `useConsent` 等，经 `HostPeer` trait 调宿主原语）；`peer_migration.rs` 把引擎侧旧数据幂等迁入插件存储键

### 前端终端链路 — `src/composables/`（含 `terminal/`）+ `src/stores/`

- **views/TerminalView.vue**：编排层——只留 Vue 生命周期接线（`onMounted` / `onUnmounted` /
  `watch`）+ 模板 ref/computed 与跨一步的模板事件薄封装
- **composables/terminal/**（TerminalView 拆分产物，范式参考桌面端 `composables/terminal/`，
  共享实例与回调经 `terminalKernel` 上下文交换）。按**大颗粒主题**分组（2026-09-18 二次拆分）：
  - **useTerminalDisplay**（终端显示）：xterm 实例装配与销毁（构造选项/addon/ResizeObserver/
    首帧 fit 收敛/DPR 监听）、主题解析与网格重排、清屏 / 手动刷新 / 合成层强制重绘、
    选择操作栏定位
  - **useTerminalInput**（终端输入）：输入栏回传（文本 / 执行 / 特殊键）、预设任务发送与执行、
    命令面板预设识别（config_id 反查）、侧栏「插入引用」填充
  - **useTerminalPanels**（功能栏）：标题栏 / 侧边栏 / 弹窗开关状态、工具栏动作分发、新手引导
  - **useTerminalRenderer**：网格测量与构造期预估、DPR 感知 fit（列 ±1 漂移钳制）、WebGL 可选加载与
    context-loss 恢复、字符图集预热（仅 WebGL 生效）、DPR 变化监听
  - **useTerminalResize**：PTY 尺寸串行队列 + 服务端正统渲染端裁决（覆盖确认弹窗）
  - **useTerminalKeyboardAvoidance**：visualViewport + 插件 safeAreaChanged 双通道键盘检测、
    根容器高度收缩避让、页面 pan 守卫
  - **useTerminalSubscription**：订阅失败重试 + 历史渲染就绪门控（加载遮罩放行三信号）
- **terminalBuffer store + useTerminalBuffer**：Rust 命令驱动（订阅/模式/输入/ack/历史），帧消费 =
  页面 Channel（`onChannelMessage` 按 16B 头逐帧解析 TB v3，与 `handleFrame` 共用投递路径）+ 状态事件
  `terminal-state`/`terminal-resync`；lastRenderedOffset 字节游标 + 跨帧裁剪 + 缺口重拼接 + 截断清屏；
  历史拼接（terminalGetHistory）完成才消费实时帧；双速模式（进页 realtime / 离页 batch）
- **useTerminalScroll**：触摸滚动（含惯性）、自定义滚动条、长按选择模式
- **utils/terminal***：resize 触发策略（`resolveGridResize` 列漂移钳制）、分层防抖、网格测量、滚动历史行数
- **useMobileConnection / useMobileCommands / useHttpApi**：连接初始化与事件同步、Tauri 命令封装（含
  `terminal_*`）、HTTP API（文件树、会话模式、任务队列）

> 历史：终端 WS 曾由前端 `useTerminalSocket.ts` 直连桌面（TB v2 seq 语义），2026-09-12 已迁入 Rust
>（`src-tauri/src/terminal_link.rs`）并升级 TB v3 字节偏移——useTerminalSocket.ts 已删除；2026-09-18
> `TerminalView.vue` 按域拆分为 `composables/terminal/`（同批修复键盘避让连带 ±1 列漂移触发整缓冲重排）；
> 同日夜段2 修背压死锁（ack 驱动补投，优化文档 §17）+ 输出帧改页面 Channel（§18，删除 `terminal-frame` 事件）；
> 2026-09-18 二次拆分按大颗粒主题重排为 显示 / 输入 / 功能栏 三域（原先过细的
> 主题/动作/实例/选择栏/预设域并入这三域），组件退化为纯编排层

### 自动化任务执行机制（移动端视角）

通过 WebSocket 事件接收桌面端推送的任务状态变更，驱动任务执行与通知：

```
Desktop PluginManager → DesktopSyncEvent → WebSocket broadcast
    ↓ ws_sync_task_status_changed / ws_sync_session_mode_changed
Mobile Tauri Event → useMobileCommands 监听 → useMobileConnection 更新会话状态
    ↓ useNotification 系统通知
Mobile UI（会话卡片 / AutoTaskPanelHost 任务队列）
    ↓ sendInput / HTTP API（task-queue 操作）
Desktop PTY → Claude Code
```

模式切换走 HTTP（不经过 PTY）：`POST /api/plugin/com.bedcode.auto-task/session-mode`（JWT 认证）→ 广播回同步 UI。

涉及目录：`src/composables/`（useMobileConnection/useNotification/useHttpApi/usePresetTasks）、
`src/components/`（TaskPickerModal/TaskEditDialog）、`src/plugin/auto-task/`。

---

## Quick Navigation

### 按功能查找（定位到目录）

| 功能 | 目录 |
|------|------|
| WebSocket 客户端 / 心跳 / 重连 | `src-tauri/src/connection/` |
| 终端链路（Rust 持有：TB v3 / 缓存 / ack / 重连） | `src-tauri/src/terminal_link.rs` |
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
| Auto Task 插件面板 | `src/plugin/auto-task/` |
| 文件传输 (SAF) | `plugins/file-transfer/` + `src-tauri/src/plugin/`（saf_io/saf_path）+ `src-tauri/src/file_service/` |
| 对等网络（节点/信任/发现） | `src-tauri/src/peer_net.rs` |
| 对等传输（发送/接收/远端浏览） | `src-tauri/src/peer_transfer.rs`、`peer_receive.rs`、`peer_remote.rs` |
| 对等网络底座 crate | `../packages/peer-net`、`../packages/link-crypto` |
| 链路加密（HTTP 信封 + WS 帧） | `src/services/linkCrypto.ts`、`src/composables/`（useLinkEncryption） |
| 多播锁（mDNS 前置） | `src-tauri/src/plugin/android_plugins/multicast_lock.rs` + Kotlin `MulticastLockPlugin` |
| SAF Uri → 路径解析 / SAF 读写抽象 | `src-tauri/src/plugin/saf_path.rs`、`saf_io.rs`（主 seam） |
| 插件审批 / 身份校验 | `src-tauri/src/plugin/approval.rs`、`validation.rs` |
| 设置子页面 (外观/连接/认证/通知) | `src/views/settings/` |
| 帮助文档资源 (快捷键/终端) | `src/assets/` |
| Android 前台服务 / 平台特性 | `src/composables/`（useForegroundService/useAndroidFeatures/useEdgeToEdge） |
| Android 原生插件注册 | `src-tauri/src/plugin/android_plugins/` |
| 插件系统 (Rust) | `src-tauri/src/plugin/` |
| 插件系统 (前端) | `src/plugin/`、`src/views/`（PluginView） |
| 插件开发 SDK | `packages/plugin-sdk-mobile/`（开发指南：仓库根 `plugin-dev-mobile.md`） |
| 插件源码 | `plugins/*/` |
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
| 插件源码 | `plugins/*/` |

---


