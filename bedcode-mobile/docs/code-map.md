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
├── plugins/                          # 插件源码目录（每个插件独立 package：plugin.json 元数据 +
│                                     #   rust/ WASM 后端 + src/ TS 前端 + vite.config.ts 独立构建）
│   ├── ai-chatbox/                   # AI Chatbox 插件：多供应商 OpenAI 兼容客户端
│   ├── file-transfer/                # 文件传输插件：基于对等网络的在线对端发现与共享目录浏览、
│                                     #   多选批量传输、接收策略、SAF 自选保存位置与历史记录
│                                     #   （OCR 插件已随 feature/ocr-plugin 隔离，本分支无）
│   └── terminal-session/             # 远程终端控制端内置 app（与桌面同名不同职责，ADR 0018/C8，
│                                     #   票 12/14/16）：终端订阅消费 + 认证/配对编排（rust）+
│                                     #   任务域前端（原 auto-task 并入：队列面板 / 工具箱两页签）
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
│   │                                 #   共享模块运行时、对话框宿主；components/ 下为插件 UI 宿主
│   │                                 #   （导航页签/设置页/终端工具栏/视图宿主；任务队列面板
│   │                                 #   已随票 16 归 plugins/terminal-session/src/task/）
│   ├── utils/                        # 工具函数（剪贴板等）
│   ├── config/                       # 配置（终端主题定义）
│   ├── services/                     # 跨端复用服务（linkCrypto.ts 链路加密客户端）
│   ├── assets/                       # 静态资源：快捷键/终端帮助文档（zh-CN/en markdown）
│   ├── styles/                       # 样式：mobile.css 全局 + terminal.css 终端/xterm/滚动条
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
│                                     #   fixtures 测试数据 + integration 集成测试）
└── src-tauri/                        # Rust 后端（Tokio 异步）
    └── src/                          # 模块按领域扁平组织，每领域配同名入口文件（auth.rs、commands.rs 等）
        ├── auth/                     # 认证模块：认证管理器（HTTP 认证：配对提交 / QR /
        │                             #   JWT reauth / 生物挑战应答）+ 认证状态（票 14 起
        │                             #   不再含本地配对码面，见 Core Modules 连接段）
        ├── commands/                 # Tauri 命令层：Android 特定、认证、连接、mDNS、
        │                             #   移动端特有（Quick Actions/Settings/Session Config）、会话、终端
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
> （ForegroundService/ForegroundServicePlugin、TaskNotificationPlugin/Manager、SafPicker/SafTransfer、
> BiometricKey、AllFilesAccess、DownloadsDir、FileDelete、DeviceInfo、MulticastLock、
> StatusBarStyle（App 主题 → 系统栏图标外观同步）、PluginAssetExtractor 等）
> 及 AndroidManifest、res/xml 配置。
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

### 终端链路（协议客户端在插件 · 窄转发在宿主，票 12）— `plugins/terminal-session/` + `src-tauri/src/terminal_stream_gateway.rs`

桌面端 PTY 输出的移动端消费链路（2026-09-12 迁入 Rust 取代前端直连 WS；2026-09-26 票 05
对齐桌面插件 `ws_terminal.rs` 新协议；**2026-10-08 票 12 协议客户端整体迁入 wasm app
`com.bedcode.terminal-session`**（移动版，与桌面同名不同职责——远程终端控制端，C8），
宿主 `terminal_link.rs`（1,363 行）与 `enums/special_key.rs` 退役，回接由
`tests/retired_mobile_terminal_link_lock.rs` 拦截）：

- **插件侧（`plugins/terminal-session/rust/src/`，协议事实面）**：`link.rs` 状态机
  （fresh subscribe 门控 / cursor / session_missing 三振 / pending_resync）、`protocol.rs`
  帧构造与纯函数（wire 形状真源 = 桌面 `ws_terminal.rs`，零变化）、`keys.rs` 按键翻译
  （自宿主等价移植，桌面同款先例）、`commands.rs` 命令面
  （`terminal-session.{subscribe,unsubscribe,unsubscribe-all,remove,send-input,ack-rendered,get-state}`，
  前端经 `plugin_invoke` 调用）。连接经 host-websocket（`jwt-auth` 宿主代发首消息认证帧，
  token 零过境插件；`auto-reconnect` 宿主退避重建 + `ws:open` 携带 `reconnectedFrom` 回接
  新句柄 → 插件重订阅）。状态事件 `plugin:com.bedcode.terminal-session:terminal-state` /
  `:terminal-resync`（载荷与退役前逐字段一致）
- **宿主侧（零解析窄转发，ADR 0022 四类薄壳④）**：`terminal_stream_gateway.rs` 页面
  Channel 表 + 转发（`terminal_page_subscribe/unsubscribe` 命令是 Tauri 传输机制登记面）；
  WIT `host-terminal-stream.forward-output`（`host_impl/terminal_stream.rs`，权限
  `terminal:output`）把插件交来的裸字节按 session-id 推给前端（`InvokeResponseBody::Raw`，
  全程零 JSON——C3）；`host_impl/connection.rs` 的 `host-connection.primary-target`
  （主连接目标事实，票 13 复用）
- **协议（真源 = 桌面 `ws_terminal.rs`，wire 零变化）**：`/ws/plugin/com.bedcode.terminal-session/terminal`
  端点；握手 `auth{token}`（宿主代发）；订阅 `subscribe{sessionId,mode:live}`；输入可打印文本
  `input{data:UTF-8}`、控制字符/特殊键 binary（插件 keys 翻译）；流控 ack `ack{offset=<本地已渲染
  字节数>}`（64KB 阈值节流在插件）；输出 = **裸字节**；重锚 `ring_resync`（唯一重锚信号）；
  停止 `session_stopped`（尾帧在前，插件显式 close 取消宿主自动重连）；`error{message}` 分类——
  含「会话不存在」主动断开借宿主退避循环重试，超 3 次停止
- **前端**：`stores/terminalBuffer.ts`（事件监听 = 插件命名空间事件名；命令封装在
  `useMobileCommands.ts` 的 `terminal*` 函数族，内部 `plugin_invoke`）+ `useTerminalBuffer.ts`
  （写队列 rAF 合并 + 背压 ack）；`ring_resync` 唯一重锚（清屏 + 归零）
- 协议与架构细节：`.scratch/2026-09-26-mobile-desktop-adaptation/spec.md` §3.3 +
  `.scratch/2026-10-07-mobile-wasm-core-refactor/ticket-12-terminal-link-downsink.md`

### 宿主 WS 出站连接域 — `host-websocket`（客户端子集，ABI v14 · 票 11）

为票 12（终端订阅协议客户端迁插件）铺路的通用出站连接原语（ADR 0041；终端链路消费方详见
下一节）：

- **客户端 5 函数**（connect / send-text / send-binary / close / is-connected）：宿主实现
  `src-tauri/src/plugin/wasm_runtime/host_impl/ws.rs`（句柄表 + reader/writer 任务 + 属主仲裁 +
  停用 purge 回收），SDK trait 与帧信封解析在 `packages/plugin-sdk-mobile/rust/src/host/ws.rs`
- **服务端域不跟演**：桌面 15 函数里的服务端域 9 函数与 `connection-context` 不存在于移动端
  （ADR 0018/0041）；边界锁 `src-tauri/tests/mobile_host_websocket_client_domain_lock.rs`
  锁 WIT 服务端函数名与 `ws:server` 权限词汇
- **事件与帧走属主私有总线 topic**：`<plugin-id>:ws:open|error|close`（JSON 状态事件）+
  `<plugin-id>:ws:message`（二进制帧信封：kind + handle 长度 + handle + 原始字节，零 JSON
  编解码）；订阅须在 activate 期完成，且 **manifest 须同时声明 `ws:client` + `bus` 两个权限位**
- **不做 wss / 重连编排**：`connect` 仅接受 `ws://`、同步阻塞至握手完成（timeout 上限 5s）；
  退避重连 / 心跳 / 订阅协议编排归插件（票 12 迁 `terminal_link` 时自带）
- 真实 WASM 组件全链路集成测试：`component.rs` tests 的 `ws_client_domain_full_loop_with_real_component`
  （component-test 插件 `ws-client` feature：connect → send-text → 对端回帧 → close 事件闭环）

### 插件核心模块引导

移动端插件系统与桌面端同架构（wasmtime 组件沙箱），并有移动端特有能力。做插件相关改动时按层定位：

**Rust 宿主侧 — `src-tauri/src/plugin/`：**

- **manager / loader / registry / storage**：生命周期管理（加载/激活/停用/状态持久化）、
  APK assets 内置插件解压 + app_data_dir 扫描、注册表、存储
- **wasm_runtime + wasm_runtime/host_impl/**：wasmtime Engine/Store/Instance 管理（`component.rs`
  为 WASM 组件模式接线）；宿主能力实现按功能域拆分于 host_impl/
  （storage/db/fs/http/mdns/terminal/event/bus/config/notify/peer/support/platform/ws）
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
（`__BEDCODE_SHARED__`）、对话框宿主；`components/` 下为插件 UI 宿主（导航页签/设置页/终端工具栏/视图宿主）。
任务队列面板不在宿主侧——随票 16 并入 `plugins/terminal-session/src/task/`（插件前端经 createApp 自挂载）。

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
  `mdns:found.<id>` / `mdns:lost.<id>` 事件驱动 + last-seen 快照经
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
- **业务归属**：传输 UI 与业务逻辑在 file-transfer 插件（`rust/src/peer.rs`、`usePeerDevices`、
  `useConsent` 等，经 `HostPeer` trait 调宿主原语）；双方向任务真源 = 插件
  `transfer_store::reduce_event` 事件归约（票 06 发送 / 票 07 接收）；
  `peer_migration.rs` 把引擎侧旧数据幂等迁入插件存储键
- **回放与节流判据单点**（票 08，均在 `plugins/file-transfer/rust/src/transfer_store.rs` 纯函数）：
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
`src/components/`（TaskPickerModal/TaskEditDialog）、`plugins/terminal-session/src/task/`（票 16 起任务面板在插件工程内）。

---

## Quick Navigation

### 按功能查找（定位到目录）

| 功能 | 目录 |
|------|------|
| WebSocket 客户端 / 心跳 / 重连 | `src-tauri/src/connection/` |
| 终端链路（协议客户端在插件 `com.bedcode.terminal-session`；宿主窄转发面 `terminal_stream_gateway.rs`） | `plugins/terminal-session/` + `src-tauri/src/terminal_stream_gateway.rs` |
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
| 任务队列面板（任务域，票 16 并入） | `plugins/terminal-session/src/task/` |
| 文件传输 (SAF) | `plugins/file-transfer/` + `src-tauri/src/plugin/`（saf_io/saf_path）+ `src-tauri/src/file_service/` |
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
- 发送编排退役面（票 06）→ `src-tauri/tests/retired_mobile_send_orchestration_lock.rs`（`retired_mobile_send_orchestration_is_not_reintroduced`）
- 接收编排退役面（票 07）→ `src-tauri/tests/retired_mobile_receive_orchestration_lock.rs`（`retired_mobile_receive_orchestration_is_not_reintroduced`）
- 传输 / 接收 / 远端浏览调度与设置命令面退役（票 10）→ `src-tauri/tests/retired_mobile_peer_transfer_command_face_lock.rs`（`retired_peer_transfer_settings_command_face_is_not_reintroduced` + `peer_transfer_scheduling_entrypoints_stay_engine_only`；同文件另钉 peer 宿主模块不得带 `#[tauri::command]` / 不得进 `invoke_handler`）
- 发现投影 / 设备列表投影 + 宿主 peer 命令面退役（票 09）→ `src-tauri/tests/retired_mobile_peer_discovery_projection_lock.rs`（两条：`retired_mobile_peer_discovery_projection_is_not_reintroduced` / `retired_host_peer_command_face_is_not_reintroduced`）
- 终端订阅协议客户端退役（票 12）→ `src-tauri/tests/retired_mobile_terminal_link_lock.rs`（符号缺失 / 命令未注册 / 前端无退役命令字面量 三条 + `mobile_terminal_retained_face_stays` 正面钉住保留面）
- auto-task 独立插件整体退役（票 16：并入 `com.bedcode.terminal-session`，任务域前端在
  `plugins/terminal-session/src/task/`）→ `src-tauri/tests/retired_mobile_auto_task_plugin_lock.rs`
  （旧 id / 视图 id / 命令 id 前缀扫描 + `merged_task_domain_and_plugin_retirement_stay` 反向断言：
  插件目录与打包资源目录不得复活、manifest 权限并集与换 id 后的扩展点必须在场）
- 本地配对码编排面退役（票 14 阶段 A，本端不是配对码颁发方）→ `src-tauri/tests/retired_mobile_local_pairing_code_face_lock.rs`（同上三形态 + `mobile_auth_engine_face_stays`）
- 认证编排命令面退役（票 14 阶段 B：配对 / QR / 生物挑战编排迁 `com.bedcode.terminal-session`
  插件 auth 域，宿主只余引擎事实面）→ `src-tauri/tests/retired_mobile_auth_orchestration_command_face_lock.rs`
  （宿主零退役编排符号含 3 个流程事件 helper / `invoke_handler!` 零 5 注册项 / 前端零退役命令字面量 +
  `auth_engine_projection_and_plugin_domain_stays` 反向钉住：引擎方法 + `host_impl/auth.rs` 投影 +
  窄读命令 `ws_get_auth_credentials` + 插件编排域 + manifest `auth` 位）
- 插件 KV 真源 = 主库 `plugin_storage` 表（票 05b 真源搬迁）→ `src-tauri/tests/plugin_storage_db_backed_lock.rs`（`plugin_storage_is_db_backed_not_file_backed`）
- `host-websocket` 客户端子集域边界（票 11：移动 WIT 不得长出 ws **服务端**域 / 不得定义 ws 服务端权限）→ `src-tauri/tests/mobile_host_websocket_client_domain_lock.rs`

> 迁移 / 抽包类任务的规格与逐票记录在 `.scratch/2026-10-07-mobile-wasm-core-refactor/` 与
> `.scratch/2026-10-07-capability-crates-to-root-packages/`（`.scratch` 只在 uat/master 之外分支入库）。

---
