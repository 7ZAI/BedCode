# BedCode Code Map

本文档作为项目代码探索的索引入口，记录完整的目录结构和各模块职责。

---

## 使用指引

**当用户命令包含以下动作时，请先阅读本文档：**

- 探索代码 / 查看代码 / 了解代码结构
- 查找文件 / 定位模块 / 寻找某个功能
- 理解架构 / 分析项目组成
- 修改某模块前需要了解上下文

**阅读流程：**

1. 先浏览 Project Structure 了解整体布局
2. 根据 Module Overview 确定目标模块所属区域
3. 使用 Quick Navigation 按功能或类型快速定位关键文件

---

## Project Structure

```
bedcode/
├── src/                          # Vue 3 frontend
│   ├── modules/
│   │   ├── desktop/              # 桌面端 UI 模块
│   │   │   ├── components/       # 桌面端组件
│   │   │   │   ├── SessionCard.vue
│   │   │   │   ├── SessionForm.vue
│   │   │   │   ├── SessionItem.vue
│   │   │   │   ├── Sidebar.vue
│   │   │   │   ├── TerminalPreview.vue
│   │   │   │   └── TitleBar.vue
│   │   │   ├── composables/      # 桌面端业务逻辑
│   │   │   │   ├── model.ts
│   │   │   │   ├── useConnectedDevices.ts
│   │   │   │   ├── useDesktopCommands.ts
│   │   │   │   ├── useGlobalTerminal.ts
│   │   │   │   ├── useNetwork.ts
│   │   │   │   ├── usePairing.ts
│   │   │   │   ├── usePtyOutput.ts
│   │   │   │   └── useWsl.ts
│   │   │   └── views/            # 桌面端页面
│   │   │       ├── DevicesView.vue
│   │   │       ├── SessionManagerView.vue
│   │   │       ├── SessionsConfigView.vue
│   │   │       ├── SettingsView.vue
│   │   │       └── TerminalWindowView.vue
│   │   ├── mobile/               # 移动端 UI 模块
│   │   │   ├── components/       # 移动端组件
│   │   │   │   ├── BottomSheet.vue
│   │   │   │   ├── DeviceCard.vue
│   │   │   │   ├── InputAssistant.vue
│   │   │   │   ├── InputBar.vue
│   │   │   │   ├── MobileNav.vue
│   │   │   │   ├── MobileStatusBar.vue
│   │   │   │   ├── MobileSwipeContainer.vue
│   │   │   │   ├── PairingInput.vue
│   │   │   │   ├── QuickActionButton.vue
│   │   │   │   ├── SessionCard.vue
│   │   │   │   ├── SettingsModal.vue
│   │   │   │   ├── ShortcutPanel.vue
│   │   │   │   └── TerminalInputBar.vue
│   │   │   ├── composables/      # 移动端业务逻辑
│   │   │   │   ├── model.ts
│   │   │   │   ├── useAndroidFeatures.ts
│   │   │   │   ├── useBackgroundMonitor.ts
│   │   │   │   ├── useEdgeToEdge.ts
│   │   │   │   ├── useMobileCommands.ts
│   │   │   │   ├── useMobileConnection.ts
│   │   │   │   └── useOrientation.ts
│   │   │   └── views/            # 移动端页面
│   │   │       ├── DevicesView.vue
│   │   │       ├── QuickActionsView.vue
│   │   │       ├── ScanView.vue
│   │   │       ├── SessionsView.vue
│   │   │       ├── SettingsView.vue
│   │   │       └── TerminalView.vue
│   │   └── shared/               # 共享 UI 模块
│   │       ├── components/       # 共享组件
│   │       │   ├── Button.vue
│   │       │   ├── EmptyState.vue
│   │       │   ├── Input.vue
│   │       │   ├── Modal.vue
│   │       │   ├── NotificationBadge.vue
│   │       │   ├── Select.vue
│   │       │   ├── Skeleton.vue
│   │       │   ├── Spinner.vue
│   │       │   ├── SplashLoading.vue
│   │       │   ├── Toast.vue
│   │       │   ├── Toggle.vue
│   │       │   ├── Tooltip.vue
│   │       │   └── index.ts
│   │       ├── composables/      # 共享业务逻辑
│   │       │   ├── model.ts
│   │       │   ├── useAnsiRenderer.ts
│   │       │   ├── useErrorHandler.ts
│   │       │   ├── useGlobalNotifications.ts
│   │       │   ├── useKeyboardShortcuts.ts
│   │       │   ├── useOutputBuffer.ts
│   │       │   ├── useOutputParser.ts
│   │       │   ├── usePlatform.ts
│   │       │   ├── usePluginSession.ts
│   │       │   ├── useQrCode.ts
│   │       │   ├── useRunTime.ts
│   │       │   ├── useSessionStatusListener.ts
│   │       │   ├── useSessionWindows.ts
│   │       │   ├── useTauri.ts
│   │       │   └── useToast.ts
│   │       ├── stores/           # Pinia 全局状态
│   │       │   ├── device.ts
│   │       │   ├── inputAssistant.ts
│   │       │   ├── quickAction.ts
│   │       │   ├── session.ts
│   │       │   └── settings.ts
│   │       ├── utils/            # 共享工具函数
│   │       │   └── invoke.ts
│   │       └── views/            # 共享页面
│   │           └── LoadingView.vue
│   ├── __tests__/                # 前端测试
│   └── App.vue
│
├── src-tauri/
│   └── src/
│       ├── shared/               # 共享模块 (desktop + mobile)
│       │   ├── auth/             # 设备配对与认证
│       │   │   ├── jwt.rs        # JWT 生成与验证
│       │   │   ├── pairing.rs    # 配对流程
│       │   │   ├── qr_token.rs   # QR 码令牌
│       │   │   └── storage.rs    # 认证存储
│       │   ├── db/               # SQLite 数据库
│       │   │   ├── database.rs   # 数据库连接
│       │   │   ├── models.rs     # 数据模型
│       │   │   └── operations.rs # CRUD 操作
│       │   ├── enums/            # 共享枚举类型
│       │   │   ├── auth.rs       # 认证状态枚举
│       │   │   ├── control.rs    # 控制消息枚举
│       │   │   ├── session.rs    # 会话状态枚举
│       │   │   ├── special_key.rs# 特殊键枚举
│       │   │   ├── sumary.rs     # 总结类型枚举
│       │   │   └── sync.rs       # 同步消息枚举
│       │   ├── event/            # 事件系统
│       │   │   ├── events.rs     # 事件定义
│       │   │   └── handler.rs    # 事件处理器
│       │   ├── model/            # 共享数据模型
│       │   │   ├── action.rs     # 快捷操作
│       │   │   ├── config.rs     # 配置模型
│       │   │   ├── device.rs     # 设备信息
│       │   │   ├── message.rs    # WebSocket 消息
│       │   │   ├── notification.rs # 通知模型
│       │   │   ├── parser.rs     # 解析器模型
│       │   │   ├── session.rs    # 会话信息
│       │   │   └ setting.rs     # 设置模型
│       │   ├── notify/           # 通知服务
│       │   │   ├── service.rs    # 通知服务实现
│       │   │   └ types.rs       # 通知类型定义
│       │   ├── parser/           # 解析器 (ANSI、Markdown)
│       │   │   ├── ansi.rs       # ANSI 解析
│       │   │   ├── markdown.rs   # Markdown 解析
│       │   │   ├── service.rs    # 解析服务
│       │   │   └ types.rs       # 解析类型
│       │   ├── system/           # 系统工具
│       │   │   ├── commands.rs   # 共享 Tauri commands
│       │   │   ├── config.rs     # 配置管理
│       │   │   ├── error.rs      # 统一错误类型
│       │   │   ├── error_boundary.rs # Panic 捕获
│       │   │   └ settings.rs    # 设置管理
│       │   └ websocket/        # WebSocket 模块
│       │       ├── client/       # WebSocket 客户端 (移动端)
│       │       │   ├── ws_client.rs  # 客户端主实现
│       │       │   ├── connection.rs # 连接管理
│       │       │   ├── heartbeat.rs  # 心跳机制
│       │       │   ├── reconnect.rs  # 重连逻辑
│       │       │   ├── io.rs         # I/O 操作
│       │       │   ├── lifecycle.rs  # 生命周期管理
│       │       │   ├── request_response.rs # 请求响应
│       │       │   ├── router.rs     # 客户端路由
│       │       │   └ default_handler.rs # 默认消息处理
│       │       ├── codec.rs      # 消息编解码
│       │       └ traits.rs     # WebSocket traits
│       │
│       ├── desktop/              # 桌面端模块
│       │   ├── commands.rs       # 桌面端 Tauri commands
│       │   ├── enums/            # 桌面端枚举
│       │   │   ├── pty_status.rs # PTY 状态
│       │   │   └ shell.rs      # Shell 类型
│       │   ├── events/           # 事件处理
│       │   │   ├── sync_event.rs     # 同步事件
│       │   │   ├── sync_handler.rs   # 同步事件处理
│       │   │   └ ws_server_handler.rs # WebSocket 服务器事件处理
│       │   ├── model/            # 桌面端数据模型
│       │   │   ├── pty_output.rs # PTY 输出
│       │   │   └ session_event.rs # 会话事件
│       │   ├── plugin/           # 插件系统
│       │   │   ├── jsonl.rs      # JSONL 插件
│       │   │   └ manager.rs    # 插件管理器
│       │   ├── pty/              # PTY 进程管理
│       │   │   ├── command.rs    # 命令构建
│       │   │   ├── pty_process.rs # 进程管理
│       │   │   ├── pty_reader.rs # 输出读取
│       │   │   ├── pty_handler.rs # PTY 处理器
│       │   │   ├── pty_output_listener.rs # 输出监听
│       │   │   ├── frontend_output_handler.rs # 前端输出
│       │   │   ├── subscription.rs # 输出订阅
│       │   │   ├── pty_subscription_handler.rs # 订阅处理
│       │   │   ├── tmux.rs       # Tmux 支持
│       │   │   └ wsl.rs        # WSL 支持
│       │   ├── server/           # Actix Web HTTP + WS 服务器
│       │   │   ├── controllers/  # HTTP REST 控制器
│       │   │   │   ├── auth_controller.rs
│       │   │   │   ├── session_controller.rs
│       │   │   │   ├── config_controller.rs
│       │   │   │   └ file_controller.rs
│       │   │   ├── dtos/         # 请求/响应 DTO
│       │   │   │   ├── common.rs
│       │   │   │   ├── auth_dto.rs
│       │   │   │   ├── session_dto.rs
│       │   │   │   └ config_dto.rs
│       │   │   ├── middleware/    # Actix 中间件
│       │   │   │   ├── jwt_auth.rs
│       │   │   │   └ cors.rs
│       │   │   ├── ws/           # WebSocket 终端
│       │   │   │   ├── terminal_ws.rs  # WS actor
│       │   │   │   └ session.rs       # WS 会话状态
│       │   │   ├── services/     # 业务服务
│       │   │   │   ├── auth_service.rs
│       │   │   │   ├── pairing_service.rs
│       │   │   │   ├── session_config.rs
│       │   │   │   ├── session_control.rs
│       │   │   │   ├── session_sub.rs
│       │   │   │   └ terminal_service.rs
│       │   │   ├── app.rs        # Actix 路由配置和服务器启动
│       │   │   ├── client_info.rs
│       │   │   ├── connection_types.rs
│       │   │   ├── message.rs        # 服务器消息
│       │   │   └ port_checker.rs    # 端口检查
│       │   ├── session/          # 会话管理
│       │   │   ├── session_manager.rs # 会话管理器
│       │   │   ├── config_mapper.rs # 配置映射
│       │   │   ├── event_bus.rs  # 事件总线
│       │   │   ├── naming_service.rs # 命名服务
│       │   │   ├── output_cache.rs # 输出缓存
│       │   │   ├── pty_registry.rs # PTY 注册表
│       │   │   ├── session_config.rs # 会话配置
│       │   │   ├── session_info.rs # 会话信息
│       │   │   ├── session_output_manager.rs # 会话输出管理
│       │   │   ├── global_output_manager.rs # 全局输出管理
│       │   │   ├── unified_output_queue.rs # 统一输出队列
│       │   │   ├── status_detector.rs # 状态检测
│       │   │   └ storage.rs    # 会话存储
│       │   ├── traits/           # 桌面端 traits
│       │   │   ├── config_mapper.rs
│       │   │   ├── naming_service.rs
│       │   │   ├── output_cache.rs
│       │   │   ├── pty_handler.rs
│       │   │   ├── pty_output_handler.rs
│       │   │   ├── pty_output_listener.rs
│       │   │   ├── pty_registry.rs
│       │   │   ├── session_event_bus.rs
│       │   │   └ session_info_registry.rs
│       │   ├── event_forwarder.rs # 事件转发
│       │   └ websocket_manager.rs # 服务器管理器 (Actix Web)
│       │
│       ├── mobile/               # 移动端模块
│       │   ├── commands/         # 移动端 Tauri commands
│       │   │   ├── android.rs    # Android 特定命令
│       │   │   ├── auth.rs       # 认证命令
│       │   │   ├── connection.rs # 连接命令
│       │   │   ├── session.rs    # 会话命令
│       │   │   ├── terminal.rs   # 终端命令
│       │   │   └ token.rs      # Token 命令
│       │   ├── handler/          # 消息处理器
│       │   │   ├── auth.rs       # 认证处理
│       │   │   ├── sync.rs       # 同步处理
│       │   │   ├── system.rs     # 系统处理
│       │   │   └ terminal.rs    # 终端处理
│       │   ├── remote/           # 远程连接模块
│       │   │   ├── connection.rs # 连接管理
│       │   │   ├── output_receiver.rs # 输出接收
│       │   │   ├── pairing_service.rs # 配对服务
│       │   │   └ request.rs    # 请求发送
│       │   ├── router/           # 路由模块
│       │   │   ├── context.rs    # 路由上下文
│       │   │   ├── event.rs      # 路由事件
│       │   │   ├── registry.rs   # 路由注册
│       │   │   └ router.rs     # 路由主实现
│       │   ├── auth.rs           # 移动端认证状态
│       │   ├── events.rs         # 移动端事件
│       │   ├── global.rs         # 全局状态
│       │   ├── session.rs        # 会话管理
│       │   └ storage.rs        # 本地存储
│       │
│       ├── lib.rs
│       └── main.rs
│
├── docs/                         # 文档目录
│   ├── android-setup.md          # Android 构建指南
│   ├── commands.md               # Tauri commands 文档
│   ├── bug-report.md             # Bug 报告模板
│   ├── testing.md                # 测试指南
│   ├── mobile-desktop-connection-issues.md # 连接问题排查
│   ├── code-map.md               # 代码地图 (本文件)
│   ├── superpowers/              # Superpowers 技能文档
│   ├── implementation-plans/     # 实现计划
│   └── knowledge/                # 知识库
│
├── src-tauri/tests/              # Rust 集成测试 (空目录)
└── src-tauri/bedcode.db          # SQLite 数据库文件
```

---

## Module Overview

### Frontend (Vue 3)

| 模块 | 路径 | 职责 |
|------|------|------|
| Desktop | `src/modules/desktop/` | 桌面端 UI：会话管理、设备列表、终端预览 |
| Mobile | `src/modules/mobile/` | 移动端 UI：终端、快捷操作、配对流程 |
| Shared | `src/modules/shared/` | 共享组件、业务逻辑、Pinia stores、工具函数 |

### Backend (Rust)

| 模块 | 路径 | 职责 |
|------|------|------|
| Shared | `src-tauri/src/shared/` | 桌面端与移动端共享代码 |
| Desktop | `src-tauri/src/desktop/` | 桌面端专属：PTY、WebSocket 服务器、会话管理 |
| Mobile | `src-tauri/src/mobile/` | 移动端专属：WebSocket 客户端、配对、连接 |

### Shared 模块详解

| 子模块 | 职责 |
|--------|------|
| `auth/` | JWT 认证、配对流程、QR 码生成 |
| `db/` | SQLite 数据库连接与 CRUD 操作 |
| `enums/` | 认证状态、控制消息、会话状态、特殊键、同步消息、总结类型 |
| `event/` | Tauri 事件系统封装 |
| `model/` | WebSocket 消息、设备信息、会话配置、通知、解析器模型 |
| `notify/` | 系统通知服务 |
| `parser/` | ANSI 转义序列解析、Markdown 解析 |
| `system/` | 错误处理、配置管理、Panic 捕获 |
| `websocket/` | WebSocket 客户端与服务器实现 |

### Desktop 模块详解

| 子模块 | 职责 |
|--------|------|
| `pty/` | PTY 进程生命周期、输出读取、Tmux/WSL 支持 |
| `server/` | WebSocket 服务器：路由、处理器、服务层、端口检查 |
| `session/` | 会话管理：配置、状态、输出缓存、统一输出队列 |
| `events/` | 同步事件、WebSocket 服务器事件处理 |
| `plugin/` | 插件系统：JSONL 日志等 |
| `traits/` | 桌面端抽象接口 |

### Mobile 前端模块详解

| 文件 | 职责 |
|------|------|
| `composables/useMobileConnection.ts` | 连接管理：初始化、连接/断开、认证、会话操作 |
| `composables/useMobileCommands.ts` | Tauri 命令封装：WebSocket 连接、认证、会话控制、终端输入 |
| `composables/useAndroidFeatures.ts` | Android 平台特定功能 |
| `composables/useBackgroundMonitor.ts` | 后台监控 |
| `composables/useEdgeToEdge.ts` | 边到边显示模式 |
| `composables/useOrientation.ts` | 屏幕方向处理 |
| `composables/model.ts` | 移动端类型定义 |
| `views/TerminalView.vue` | 终端页面 |
| `views/DevicesView.vue` | 设备列表页面 |
| `views/QuickActionsView.vue` | 快捷操作页面 |

### Mobile 后端模块详解 (Rust)

| 子模块/文件 | 职责 |
|--------|------|
| `commands/` | Tauri 命令：Android、认证、连接、会话、终端、Token |
| `handler/` | 消息处理：认证、同步、系统、终端 |
| `remote/` | 远程连接：连接管理、输出接收、配对服务、请求发送 |
| `router/` | 路由：上下文、事件、注册、主实现 |
| `auth.rs` | 移动端认证状态管理 |
| `events.rs` | 移动端事件定义 |
| `global.rs` | 全局状态管理 |
| `session.rs` | 远程会话管理 |
| `storage.rs` | 本地存储 |

---

## Quick Navigation

### 按功能查找

| 功能 | 关键文件 |
|------|----------|
| PTY 进程管理 | `desktop/pty/pty_process.rs`, `desktop/pty/pty_handler.rs` |
| 会话管理 | `desktop/session/session_manager.rs` |
| WebSocket 服务器 | `shared/websocket/server/ws_server.rs` |
| WebSocket 客户端 | `shared/websocket/client/ws_client.rs` |
| 消息路由 | `desktop/server/router/business_router.rs` |
| 设备认证 | `shared/auth/jwt.rs`, `shared/auth/pairing.rs` |
| 终端输入 | `desktop/server/services/terminal_service.rs` |
| 输出缓存 | `desktop/session/output_cache.rs` |
| ANSI 解析 | `shared/parser/ansi.rs` |
| 数据库操作 | `shared/db/operations.rs` |
| 移动端连接管理 | `mobile/remote/connection.rs`, `mobile/composables/useMobileConnection.ts` |
| 移动端路由 | `mobile/router/router.rs` |
| 同步事件 | `desktop/events/sync_event.rs` |
| 端口检查 | `desktop/server/port_checker.rs` |
| 统一输出队列 | `desktop/session/unified_output_queue.rs` |

### 按类型查找

| 类型 | 路径模式 |
|------|----------|
| Tauri Commands | `*/commands.rs`, `*/commands/*.rs` |
| 错误处理 | `shared/system/error.rs` |
| 数据模型 | `*/model/*.rs`, `*/model.rs` |
| 枚举类型 | `*/enums/*.rs` |
| Traits | `*/traits/*.rs` |
| 消息处理器 | `*/handler/*.rs`, `*/handlers/*.rs` |
| 业务服务 | `desktop/server/services/*.rs` |
| Pinia Stores | `src/modules/shared/stores/*.ts` |
| Composables | `src/modules/*/composables/*.ts` |

---

## 最近更新

- 2026-06-10: 同步项目当前结构，新增 events/、remote/、router/ 等目录，更新 stores 位置