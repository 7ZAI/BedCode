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
bedcode/                              # 仓库根目录
├── bedcode-desktop/                  # 桌面端项目 (Tauri 2.0 + Vue 3)
│   ├── src/                          # Vue 3 frontend
│   │   ├── modules/
│   │   │   ├── desktop/              # 桌面端 UI 模块
│   │   │   │   ├── components/       # 桌面端组件
│   │   │   │   │   ├── DesktopLayout.vue
│   │   │   │   │   ├── SessionCard.vue
│   │   │   │   │   ├── SessionForm.vue
│   │   │   │   │   ├── SessionItem.vue
│   │   │   │   │   ├── Sidebar.vue
│   │   │   │   │   ├── TerminalPreview.vue
│   │   │   │   │   └── TitleBar.vue
│   │   │   │   ├── composables/      # 桌面端业务逻辑
│   │   │   │   │   ├── model.ts
│   │   │   │   │   ├── useConnectedDevices.ts
│   │   │   │   │   ├── useDesktopCommands.ts
│   │   │   │   │   ├── useGlobalTerminal.ts
│   │   │   │   │   ├── useNetwork.ts
│   │   │   │   │   ├── usePairing.ts
│   │   │   │   │   ├── usePluginManager.ts
│   │   │   │   │   ├── usePtyOutput.ts
│   │   │   │   │   └── useWsl.ts
│   │   │   │   ├── stores/           # 桌面端 Pinia stores
│   │   │   │   │   └── wsl.ts
│   │   │   │   └── views/            # 桌面端页面
│   │   │   │       ├── DevicesView.vue
│   │   │   │       ├── PluginConfigView.vue
│   │   │   │       ├── PluginsView.vue
│   │   │   │       ├── SessionManagerView.vue
│   │   │   │       ├── SessionsConfigView.vue
│   │   │   │       ├── SettingsView.vue
│   │   │   │       └── TerminalWindowView.vue
│   │   │   └── shared/               # 共享 UI 模块
│   │   │       ├── components/       # 共享组件
│   │   │       │   ├── Button.vue
│   │   │       │   ├── EmptyState.vue
│   │   │       │   ├── Input.vue
│   │   │       │   ├── Modal.vue
│   │   │       │   ├── NotificationBadge.vue
│   │   │       │   ├── Select.vue
│   │   │       │   ├── Skeleton.vue
│   │   │       │   ├── Spinner.vue
│   │   │       │   ├── SplashLoading.vue
│   │   │       │   ├── Toast.vue
│   │   │       │   ├── Toggle.vue
│   │   │       │   ├── Tooltip.vue
│   │   │       │   └── index.ts
│   │   │       ├── composables/      # 共享业务逻辑
│   │   │       │   ├── model.ts
│   │   │       │   ├── useAnsiRenderer.ts
│   │   │       │   ├── useFontSize.ts
│   │   │       │   ├── useGlobalNotifications.ts
│   │   │       │   ├── useKeyboardShortcuts.ts
│   │   │       │   ├── usePlatform.ts
│   │   │       │   ├── useQrCode.ts
│   │   │       │   ├── useRunTime.ts
│   │   │       │   ├── useSessionStatusListener.ts
│   │   │       │   ├── useSessionWindows.ts
│   │   │       │   ├── useTauri.ts
│   │   │       │   ├── useTheme.ts
│   │   │       │   └── useToast.ts
│   │   │       ├── plugin/           # 前端插件系统
│   │   │       │   ├── index.ts      # 插件系统入口
│   │   │       │   ├── commands.ts   # 插件命令注册
│   │   │       │   ├── context.ts    # 插件上下文
│   │   │       │   ├── events.ts     # 插件事件
│   │   │       │   ├── loader.ts     # 插件加载器
│   │   │       │   ├── permission.ts # 插件权限管理
│   │   │       │   ├── registry.ts   # 插件注册表
│   │   │       │   ├── types.ts      # 插件类型定义
│   │   │       │   └── components/   # 插件 UI 组件
│   │   │       │       ├── PluginCommandPalette.vue
│   │   │       │       ├── PluginStatusBar.vue
│   │   │       │       ├── PluginTerminalToolbar.vue
│   │   │       │       ├── PluginTitleBarItems.vue
│   │   │       │       └── PluginViewHost.vue
│   │   │       ├── stores/           # Pinia 全局状态
│   │   │       │   ├── device.ts
│   │   │       │   ├── i18n.ts
│   │   │       │   ├── inputAssistant.ts
│   │   │       │   ├── quickAction.ts
│   │   │       │   ├── session.ts
│   │   │       │   └── settings.ts
│   │   │       ├── utils/            # 共享工具函数
│   │   │       │   └── invoke.ts
│   │   │       └── views/            # 共享页面
│   │   │           └── LoadingView.vue
│   │   ├── plugins/                  # 前端插件目录
│   │   │   └── com.bedcode.ai-chatbox/  # AI Chatbox 插件
│   │   │       ├── index.ts          # 插件入口
│   │   │       ├── plugin.json       # 插件元数据
│   │   │       ├── types.ts          # 类型定义
│   │   │       ├── vite.config.ts    # 插件独立构建配置
│   │   │       ├── components/       # 插件组件
│   │   │       │   ├── ChatInput.vue
│   │   │       │   ├── ChatMessage.vue
│   │   │       │   ├── ChatView.vue
│   │   │       │   ├── PromptOptimizeDialog.vue
│   │   │       │   └── ProviderManager.vue
│   │   │       ├── composables/      # 插件业务逻辑
│   │   │       │   ├── useAiChat.ts
│   │   │       │   ├── useAiConfig.ts
│   │   │       │   └── usePromptOptimizer.ts
│   │   │       └── services/         # 插件服务
│   │   │           └── openaiClient.ts
│   │   ├── __tests__/                # 前端测试
│   │   ├── locales/                  # 国际化
│   │   ├── router/                   # 路由
│   │   ├── App.vue
│   │   └── main.ts
│   │
│   └── src-tauri/
│       └── src/
│           ├── shared/               # 共享模块 (desktop + mobile)
│           │   ├── auth/             # 设备配对与认证
│           │   │   ├── pairing.rs    # 配对流程
│           │   │   └── storage.rs    # 认证存储 (已移除 jwt.rs, qr_token.rs → desktop/auth/)
│           │   ├── db/               # SQLite 数据库 (仅桌面端)
│           │   │   ├── database.rs   # 数据库连接
│           │   │   ├── models.rs     # 数据模型
│           │   │   └── operations.rs # CRUD 操作
│           │   ├── enums/            # 共享枚举类型
│           │   │   ├── auth.rs       # 认证状态枚举
│           │   │   ├── control.rs    # 控制消息枚举
│           │   │   ├── plugin.rs     # 插件相关枚举
│           │   │   ├── session.rs    # 会话状态枚举
│           │   │   ├── special_key.rs# 特殊键枚举
│           │   │   ├── sumary.rs     # 总结类型枚举
│           │   │   └── sync.rs       # 同步消息枚举
│           │   ├── event/            # 事件系统
│           │   │   ├── events.rs     # 事件定义
│           │   │   └── handler.rs    # 事件处理器
│           │   ├── model/            # 共享数据模型
│           │   │   ├── api_dto.rs    # API 数据传输对象
│           │   │   └── message.rs    # WebSocket 消息
│           │   ├── system/           # 系统工具
│           │   │   ├── commands.rs   # 共享 Tauri commands
│           │   │   ├── config.rs     # 配置管理
│           │   │   ├── error.rs      # 统一错误类型
│           │   │   ├── error_boundary.rs # Panic 捕获
│           │   │   └── process.rs    # 进程管理工具
│           │   └── utils.rs          # 共享工具函数
│           │
│           ├── desktop/              # 桌面端模块
│           │   ├── app_context.rs    # 全局应用上下文 (DI 容器)
│           │   ├── auth/             # 桌面端认证
│           │   │   ├── jwt.rs        # JWT 生成与验证
│           │   │   └── qr_token.rs   # QR 码令牌管理
│           │   ├── commands/         # 桌面端 Tauri commands（按领域拆分）
│           │   │   ├── devices.rs
│           │   │   ├── plugin.rs     # 插件命令
│           │   │   ├── pty_input.rs
│           │   │   ├── qr.rs
│           │   │   ├── quick_actions.rs
│           │   │   ├── session.rs
│           │   │   ├── session_config.rs
│           │   │   ├── settings.rs
│           │   │   └── wsl.rs
│           │   ├── enums/            # 桌面端枚举
│           │   │   ├── pty_status.rs # PTY 状态
│           │   │   └── shell.rs      # Shell 类型
│           │   ├── events/           # 事件处理
│           │   │   ├── sync_event.rs     # 同步事件
│           │   │   └── sync_handler.rs   # 同步事件处理
│           │   ├── model/            # 桌面端数据模型
│           │   │   ├── pty_output.rs # PTY 输出
│           │   │   └── session_event.rs # 会话事件
│           │   ├── notify.rs         # 通知服务
│           │   ├── parser/           # 解析器 (后端状态检测)
│           │   │   ├── ansi.rs       # ANSI 解析
│           │   │   ├── markdown.rs   # Markdown 解析
│           │   │   ├── service.rs    # 解析服务
│           │   │   └── types.rs      # 解析类型
│           │   ├── plugin/           # 插件系统
│           │   │   ├── api_bridge.rs # 插件 API 桥接
│           │   │   ├── host.rs       # 插件宿主
│           │   │   ├── loader.rs     # 插件加载器
│           │   │   ├── manager.rs    # 插件管理器（任务状态 + 会话映射 + 自动授权模式）
│           │   │   ├── permission.rs # 插件权限
│           │   │   ├── registry.rs   # 插件注册表
│           │   │   ├── setup.rs      # 全局 hooks 自动配置
│           │   │   ├── storage.rs    # 插件存储
│           │   │   └── types.rs      # 插件类型定义
│           │   ├── pty/              # PTY 进程管理
│           │   │   ├── command.rs    # 命令构建
│           │   │   ├── pty_process.rs # 进程管理
│           │   │   ├── pty_reader.rs # 输出读取
│           │   │   ├── pty_handler.rs # PTY 处理器
│           │   │   ├── pty_output_listener.rs # 输出监听
│           │   │   ├── frontend_output_handler.rs # 前端输出
│           │   │   └── wsl.rs        # WSL 支持
│           │   ├── server/           # Actix Web HTTP + WS 服务器
│           │   │   ├── controllers/  # HTTP REST 控制器
│           │   │   │   ├── auth_controller.rs
│           │   │   │   ├── session_controller.rs
│           │   │   │   ├── config_controller.rs
│           │   │   │   ├── file_controller.rs
│           │   │   │   ├── git_controller.rs
│           │   │   │   └── plugin_controller.rs
│           │   │   ├── dtos/         # 请求/响应 DTO
│           │   │   │   ├── auth_dto.rs
│           │   │   │   ├── session_dto.rs
│           │   │   │   ├── config_dto.rs
│           │   │   │   ├── git_dto.rs
│           │   │   │   └── plugin_dto.rs
│           │   │   ├── middleware/    # Actix 中间件
│           │   │   │   ├── jwt_auth.rs
│           │   │   │   └── cors.rs
│           │   │   ├── ws/           # WebSocket 终端
│           │   │   │   ├── terminal_ws.rs  # WS actor
│           │   │   │   ├── session.rs      # WS 会话状态
│           │   │   │   └── registry.rs     # WS 连接注册表
│           │   │   ├── services/     # 业务服务
│           │   │   │   ├── auth_service.rs
│           │   │   │   ├── pairing_service.rs
│           │   │   │   ├── session_config.rs
│           │   │   │   ├── session_control.rs
│           │   │   │   ├── session_sub.rs
│           │   │   │   └── terminal_service.rs
│           │   │   ├── app.rs        # Actix 路由配置和服务器启动
│           │   │   ├── client_info.rs
│           │   │   ├── connection_types.rs
│           │   │   ├── ipc.rs        # IPC 通信
│           │   │   ├── message.rs    # 服务器消息
│           │   │   ├── metrics.rs    # 服务器指标
│           │   │   └── port_checker.rs # 端口检查
│           │   ├── session/          # 会话管理
│           │   │   ├── session_manager.rs # 会话管理器
│           │   │   ├── session_config.rs  # 会话配置 CRUD
│           │   │   ├── event_bus.rs       # 统一事件广播
│           │   │   ├── session_components.rs # 内部组件（注册表/命名/映射/检测）
│           │   │   ├── session_output.rs   # 输出管理（缓存/队列/订阅/全局）
│           │   │   └── storage.rs         # 会话存储
│           │   ├── traits/           # 需要多态的 trait
│           │   │   ├── pty_handler.rs
│           │   │   ├── pty_output_handler.rs
│           │   │   └── pty_output_listener.rs
│           │   ├── event_forwarder.rs # 事件转发
│           │   └── websocket_manager.rs # 服务器管理器（仅 port/initialized 状态）
│           │
│           ├── lib.rs
│           └── main.rs
│
├── bedcode-mobile/                   # 移动端项目 (Tauri 2.0 + Vue 3)
│   ├── src/                          # Vue 3 frontend
│   │   ├── modules/
│   │   │   ├── mobile/               # 移动端 UI 模块
│   │   │   │   ├── components/       # 移动端组件
│   │   │   │   │   ├── AutoExecuteBar.vue
│   │   │   │   │   ├── BottomSheet.vue
│   │   │   │   │   ├── CodeViewerSettingsModal.vue
│   │   │   │   │   ├── DeviceCard.vue
│   │   │   │   │   ├── FileSidebar.vue
│   │   │   │   │   ├── FileTreeItem.vue
│   │   │   │   │   ├── FileViewerModal.vue
│   │   │   │   │   ├── icons/        # 图标组件
│   │   │   │   │   │   ├── FileIcon.vue
│   │   │   │   │   │   ├── FolderClosedIcon.vue
│   │   │   │   │   │   └── FolderOpenIcon.vue
│   │   │   │   │   ├── InputAssistant.vue
│   │   │   │   │   ├── InputBar.vue
│   │   │   │   │   ├── MobileLayout.vue
│   │   │   │   │   ├── MobileNav.vue
│   │   │   │   │   ├── MobileStatusBar.vue
│   │   │   │   │   ├── MobileSwipeContainer.vue
│   │   │   │   │   ├── PairingInput.vue
│   │   │   │   │   ├── PresetTaskCard.vue
│   │   │   │   │   ├── QuickActionButton.vue
│   │   │   │   │   ├── SessionCard.vue
│   │   │   │   │   ├── SessionConfigCard.vue
│   │   │   │   │   ├── SettingsModal.vue
│   │   │   │   │   ├── ShortcutConfigModal.vue
│   │   │   │   │   ├── ShortcutPanel.vue
│   │   │   │   │   ├── TaskPickerModal.vue
│   │   │   │   │   └── TerminalInputBar.vue
│   │   │   │   ├── composables/      # 移动端业务逻辑
│   │   │   │   │   ├── model.ts
│   │   │   │   │   ├── useAndroidFeatures.ts
│   │   │   │   │   ├── useAutoExecutor.ts
│   │   │   │   │   ├── useCodeHighlight.ts
│   │   │   │   │   ├── useEdgeToEdge.ts
│   │   │   │   │   ├── useFileTree.ts
│   │   │   │   │   ├── useForegroundService.ts
│   │   │   │   │   ├── useHttpApi.ts
│   │   │   │   │   ├── useMobileCommands.ts
│   │   │   │   │   ├── useMobileConnection.ts
│   │   │   │   │   ├── useOrientation.ts
│   │   │   │   │   ├── usePresetTasks.ts
│   │   │   │   │   ├── useTaskExecutionState.ts
│   │   │   │   │   ├── useTaskNotification.ts
│   │   │   │   │   └── useTerminalOutput.ts
│   │   │   │   └── views/            # 移动端页面
│   │   │   │       ├── CodeExplorerView.vue
│   │   │   │       ├── DevicesView.vue
│   │   │   │       ├── ScanView.vue
│   │   │   │       ├── SessionsView.vue
│   │   │   │       ├── SettingsView.vue
│   │   │   │       ├── TerminalView.vue
│   │   │   │       └── ToolboxView.vue
│   │   │   └── shared/               # 共享 UI 模块（精简版）
│   │   │       ├── components/       # 共享组件
│   │   │       │   ├── Button.vue
│   │   │       │   ├── Modal.vue
│   │   │       │   ├── Toast.vue
│   │   │       │   ├── Toggle.vue
│   │   │       │   └── index.ts
│   │   │       ├── composables/      # 共享业务逻辑
│   │   │       │   ├── model.ts
│   │   │       │   ├── useFontSize.ts
│   │   │       │   ├── usePlatform.ts
│   │   │       │   ├── useRunTime.ts
│   │   │       │   ├── useTheme.ts
│   │   │       │   └── useToast.ts
│   │   │       ├── stores/           # Pinia 全局状态
│   │   │       │   ├── codeViewer.ts
│   │   │       │   ├── i18n.ts
│   │   │       │   ├── inputAssistant.ts
│   │   │       │   └── settings.ts
│   │   │       └── utils/            # 共享工具函数
│   │   │           └── clipboard.ts
│   │   ├── locales/                  # 国际化
│   │   ├── router/                   # 路由
│   │   ├── App.vue
│   │   └── main.ts
│   │
│   └── src-tauri/
│       └── src/
│           ├── shared/               # 共享模块 (移动端精简版)
│           │   ├── auth/             # 设备配对与认证
│           │   │   └── pairing.rs    # 配对流程
│           │   ├── enums/            # 共享枚举类型
│           │   │   ├── auth.rs
│           │   │   ├── control.rs
│           │   │   ├── plugin.rs
│           │   │   ├── session.rs
│           │   │   ├── special_key.rs
│           │   │   ├── sumary.rs
│           │   │   └── sync.rs
│           │   ├── model/            # 共享数据模型
│           │   │   └── message.rs    # WebSocket 消息
│           │   ├── models.rs         # API DTO 等数据模型
│           │   └── system/           # 系统工具
│           │       ├── commands.rs   # 共享 Tauri commands
│           │       ├── config.rs     # 配置管理
│           │       ├── error.rs      # 统一错误类型
│           │       └── error_boundary.rs # Panic 捕获
│           │
│           ├── mobile/               # 移动端模块
│           │   ├── commands/         # 移动端 Tauri commands
│           │   │   ├── android.rs    # Android 特定命令
│           │   │   ├── auth.rs       # 认证命令
│           │   │   ├── connection.rs # 连接命令
│           │   │   ├── mobile_commands.rs # 移动端特有命令 (Quick Actions, Settings, Session Config)
│           │   │   ├── session.rs    # 会话命令
│           │   │   ├── terminal.rs   # 终端命令
│           │   │   └── token.rs      # Token 命令
│           │   ├── handler/          # 消息处理器
│           │   │   ├── auth.rs       # 认证处理
│           │   │   ├── sync.rs       # 同步处理
│           │   │   ├── system.rs     # 系统处理
│           │   │   └── terminal.rs   # 终端处理
│           │   ├── managers.rs       # 移动端管理器集合
│           │   ├── remote/           # 远程连接模块
│           │   │   ├── connection.rs # 连接管理
│           │   │   ├── pairing_service.rs # 配对服务
│           │   │   └── request.rs    # 请求发送
│           │   ├── router/           # 路由模块
│           │   │   ├── context.rs    # 路由上下文
│           │   │   ├── event.rs      # 路由事件
│           │   │   ├── registry.rs   # 路由注册
│           │   │   └── router.rs     # 路由主实现
│           │   ├── system/           # 移动端系统模块
│           │   │   └── settings.rs   # 设置管理 (JSON 文件存储)
│           │   ├── websocket_client/ # WebSocket 客户端
│           │   │   ├── ws_client.rs  # 客户端主实现
│           │   │   ├── connection.rs # 连接管理
│           │   │   ├── heartbeat.rs  # 心跳机制
│           │   │   ├── reconnect.rs  # 重连逻辑
│           │   │   ├── io.rs         # I/O 操作
│           │   │   ├── lifecycle.rs  # 生命周期管理
│           │   │   ├── request_response.rs # 请求响应
│           │   │   ├── router.rs     # 客户端路由
│           │   │   ├── default_handler.rs # 默认消息处理
│           │   │   ├── codec.rs      # 消息编解码
│           │   │   └── traits.rs     # WebSocket traits
│           │   ├── auth.rs           # 移动端认证状态
│           │   ├── global.rs         # 全局状态
│           │   └── session.rs        # 会话管理
│           │
│           ├── lib.rs
│           └── main.rs
│
├── docs/                             # 文档目录
│   ├── code-map.md                   # 代码地图 (本文件)
│   ├── commands.md                   # Tauri commands 文档
│   ├── plugin-system.md              # 插件系统文档
│   ├── plugin-system-defects.md      # 插件系统缺陷记录
│   ├── i18n-progress.md              # 国际化进度
│   ├── release-workflow.md           # 发布流程
│   ├── knowledge/                    # 知识库
│   │   └── build-process.md          # 构建流程
│   └── superpowers/                  # Superpowers 技能文档
│       ├── plans/                    # 实现计划
│       └── specs/                    # 设计规格
│
├── .github/                          # GitHub Actions CI/CD
├── CLAUDE.md                         # Claude Code 项目指引
├── CHANGELOG.md                      # 变更日志
├── LICENSE                           # 开源协议
├── README.md                         # 英文说明
└── README_zh.md                      # 中文说明
```

---

## Module Overview

### 项目结构

BedCode 采用多项目仓库结构，桌面端和移动端各自独立：

| 项目 | 路径 | 说明 |
|------|------|------|
| Desktop | `bedcode-desktop/` | 桌面端：Tauri 2.0 + Vue 3，作为主机运行 PTY 和 WebSocket 服务器 |
| Mobile | `bedcode-mobile/` | 移动端：Tauri 2.0 + Vue 3，作为远程终端连接桌面端 |

### Frontend (Vue 3)

| 模块 | 路径 | 职责 |
|------|------|------|
| Desktop | `bedcode-desktop/src/modules/desktop/` | 桌面端 UI：会话管理、设备列表、终端预览、插件管理 |
| Mobile | `bedcode-mobile/src/modules/mobile/` | 移动端 UI：终端、工具箱、文件浏览、配对流程、自动执行 |
| Shared (Desktop) | `bedcode-desktop/src/modules/shared/` | 桌面端共享组件、业务逻辑、Pinia stores、插件系统 |
| Shared (Mobile) | `bedcode-mobile/src/modules/shared/` | 移动端共享组件、业务逻辑、Pinia stores（精简版） |

### Backend (Rust)

| 模块 | 路径 | 职责 |
|------|------|------|
| Shared (Desktop) | `bedcode-desktop/src-tauri/src/shared/` | 桌面端共享代码：认证、数据库、枚举、事件、模型、系统工具 |
| Shared (Mobile) | `bedcode-mobile/src-tauri/src/shared/` | 移动端共享代码：认证、枚举、模型、系统工具（精简版，无 db/event 子模块） |
| Desktop | `bedcode-desktop/src-tauri/src/desktop/` | 桌面端专属：PTY、WebSocket 服务器、会话管理、插件系统 |
| Mobile | `bedcode-mobile/src-tauri/src/mobile/` | 移动端专属：WebSocket 客户端、配对、连接、路由 |

### Desktop Shared 模块详解

| 子模块 | 职责 |
|--------|------|
| `auth/` | 配对流程、认证存储 (JWT/QR Token 已迁移至 desktop/auth/) |
| `db/` | SQLite 数据库连接与 CRUD 操作（仅桌面端） |
| `enums/` | 认证状态、控制消息、插件、会话状态、特殊键、同步消息、总结类型 |
| `event/` | Tauri 事件系统封装 |
| `model/` | WebSocket 消息、API DTO |
| `system/` | 错误处理、配置管理、Panic 捕获、进程管理、共享 Tauri commands |
| `utils.rs` | 共享工具函数 |

### Mobile Shared 模块详解

| 子模块 | 职责 |
|--------|------|
| `auth/` | 配对流程 |
| `enums/` | 认证状态、控制消息、插件、会话状态、特殊键、同步消息、总结类型 |
| `model/` | WebSocket 消息（message.rs） |
| `models.rs` | API DTO 等数据模型（单文件） |
| `system/` | 错误处理、配置管理、Panic 捕获、共享 Tauri commands |

### Desktop 模块详解

| 子模块 | 职责 |
|--------|------|
| `commands/` | Tauri 命令：按领域拆分（devices, plugin, pty_input, qr, quick_actions, session, session_config, settings, wsl） |
| `app_context.rs` | 全局应用上下文 (DI 容器)，统一管理所有全局单例 || `auth/` | JWT 认证、QR 码令牌管理 |
| `pty/` | PTY 进程生命周期、输出读取、WSL 支持 |
| `server/` | WebSocket 服务器：路由、控制器、服务层、端口检查、IPC、指标 |
| `session/` | 会话管理：配置、状态、输出缓存、统一输出队列 |
| `events/` | 同步事件、同步事件处理 |
| `parser/` | 后端状态检测（ANSI/Markdown 解析，前端渲染用前端 composables） |
| `plugin/` | 插件系统：宿主、加载器、管理器、权限、注册表、存储、API 桥接、全局 hooks 自动配置 |
| `notify.rs` | 通知服务 |
| `traits/` | 需要多态的 trait（PtyHandler, PtyOutputHandler, PtyOutputListener） |

### Desktop 前端插件系统详解

| 文件 | 职责 |
|------|------|
| `shared/plugin/index.ts` | 插件系统入口 |
| `shared/plugin/loader.ts` | 插件加载器 |
| `shared/plugin/registry.ts` | 插件注册表 |
| `shared/plugin/permission.ts` | 插件权限管理 |
| `shared/plugin/context.ts` | 插件上下文 |
| `shared/plugin/events.ts` | 插件事件 |
| `shared/plugin/commands.ts` | 插件命令注册 |
| `shared/plugin/types.ts` | 插件类型定义 |
| `shared/plugin/components/PluginCommandPalette.vue` | 插件命令面板 |
| `shared/plugin/components/PluginStatusBar.vue` | 插件状态栏 |
| `shared/plugin/components/PluginTerminalToolbar.vue` | 插件终端工具栏 |
| `shared/plugin/components/PluginTitleBarItems.vue` | 插件标题栏项 |
| `shared/plugin/components/PluginViewHost.vue` | 插件视图宿主 |
| `plugins/com.bedcode.ai-chatbox/` | AI Chatbox 插件（OpenAI 兼容客户端） |

### Mobile 前端模块详解

| 文件 | 职责 |
|------|------|
| `composables/useMobileConnection.ts` | 连接管理：初始化、连接/断开、认证、会话操作 |
| `composables/useMobileCommands.ts` | Tauri 命令封装：WebSocket 连接、认证、会话控制、终端输入 |
| `composables/useHttpApi.ts` | HTTP API 调用：文件树浏览、插件模式设置、终端输入 |
| `composables/useAutoExecutor.ts` | 自动执行状态机：按 sessionId 隔离的任务自动执行引擎 |
| `composables/usePresetTasks.ts` | 预设任务管理 |
| `composables/useTaskExecutionState.ts` | 任务执行状态管理 |
| `composables/useTaskNotification.ts` | 任务通知管理 |
| `composables/useFileTree.ts` | 文件树数据管理 |
| `composables/useCodeHighlight.ts` | 代码语法高亮 |
| `composables/useForegroundService.ts` | Android 前台服务管理 |
| `composables/useAndroidFeatures.ts` | Android 平台特定功能 |
| `composables/useEdgeToEdge.ts` | 边到边显示模式 |
| `composables/useOrientation.ts` | 屏幕方向处理 |
| `composables/useTerminalOutput.ts` | 终端输出订阅管理（全局单一 ws_output 监听 + sessionId 分发） |
| `composables/model.ts` | 移动端类型定义 |
| `views/TerminalView.vue` | 终端页面 |
| `views/CodeExplorerView.vue` | 代码浏览器页面 |
| `views/DevicesView.vue` | 设备列表页面 |
| `views/ToolboxView.vue` | 工具箱页面 |
| `views/SessionsView.vue` | 会话列表页面 |
| `components/AutoExecuteBar.vue` | 自动执行状态条 |
| `components/TaskPickerModal.vue` | 任务选择弹窗 |
| `components/PresetTaskCard.vue` | 预设任务卡片 |
| `components/SessionConfigCard.vue` | 会话配置卡片 |
| `components/CodeViewerSettingsModal.vue` | 代码查看器设置弹窗 |
| `components/ShortcutConfigModal.vue` | 快捷键配置弹窗 |
| `components/FileSidebar.vue` | 文件侧边栏 |
| `components/FileTreeItem.vue` | 文件树项 |
| `components/FileViewerModal.vue` | 文件查看弹窗 |
| `components/icons/` | 文件/文件夹图标组件 |
| `components/MobileLayout.vue` | 移动端布局容器 |

### Mobile 后端模块详解 (Rust)

| 子模块/文件 | 职责 |
|--------|------|
| `commands/` | Tauri 命令：Android、认证、连接、移动端特有、会话、终端、Token |
| `handler/` | 消息处理：认证、同步、系统、终端 |
| `managers.rs` | 移动端管理器集合 |
| `remote/` | 远程连接：连接管理、配对服务、请求发送 |
| `router/` | 路由：上下文、事件、注册、主实现 |
| `system/` | 移动端系统模块：设置管理 (JSON 文件存储) |
| `websocket_client/` | WebSocket 客户端 |
| `auth.rs` | 移动端认证状态管理 |
| `global.rs` | 全局状态管理 |
| `session.rs` | 远程会话管理 |

---

## Quick Navigation

### 按功能查找

| 功能 | 关键文件 |
|------|----------|
| PTY 进程管理 | `bedcode-desktop/src-tauri/src/desktop/pty/pty_process.rs`, `desktop/pty/pty_handler.rs` |
| 会话管理 | `bedcode-desktop/src-tauri/src/desktop/session/session_manager.rs` |
| WebSocket 服务器 | `bedcode-desktop/src-tauri/src/desktop/websocket_manager.rs`, `desktop/server/app.rs` |
| WebSocket 客户端 | `bedcode-mobile/src-tauri/src/mobile/websocket_client/ws_client.rs` |
| 消息路由 | `bedcode-mobile/src-tauri/src/mobile/router/router.rs` |
| 设备认证 | `bedcode-desktop/src-tauri/src/desktop/auth/jwt.rs`, `shared/auth/pairing.rs` |
| 终端输入 | `bedcode-desktop/src-tauri/src/desktop/server/services/terminal_service.rs` |
| 输出缓存 | `bedcode-desktop/src-tauri/src/desktop/session/session_output.rs` |
| ANSI 解析 | `bedcode-desktop/src-tauri/src/desktop/parser/ansi.rs` |
| 数据库操作 | `bedcode-desktop/src-tauri/src/shared/db/operations.rs` |
| 移动端连接管理 | `bedcode-mobile/src-tauri/src/mobile/remote/connection.rs`, `mobile/composables/useMobileConnection.ts` |
| 移动端终端输出 | `bedcode-mobile/src/modules/mobile/composables/useTerminalOutput.ts` |
| 移动端路由 | `bedcode-mobile/src-tauri/src/mobile/router/router.rs` |
| 同步事件 | `bedcode-desktop/src-tauri/src/desktop/events/sync_event.rs` |
| 端口检查 | `bedcode-desktop/src-tauri/src/desktop/server/port_checker.rs` |
| IPC 通信 | `bedcode-desktop/src-tauri/src/desktop/server/ipc.rs` |
| 服务器指标 | `bedcode-desktop/src-tauri/src/desktop/server/metrics.rs` |
| Git 控制器 | `bedcode-desktop/src-tauri/src/desktop/server/controllers/git_controller.rs` |
| 应用上下文 | `bedcode-desktop/src-tauri/src/desktop/app_context.rs` |
| QR 码令牌 | `bedcode-desktop/src-tauri/src/desktop/auth/qr_token.rs` |
| 通知服务 | `bedcode-desktop/src-tauri/src/desktop/notify.rs` |
| 文件浏览 | `bedcode-mobile/src/modules/mobile/composables/useHttpApi.ts` |
| 移动端终端输入 | `bedcode-mobile/src/modules/mobile/composables/useHttpApi.ts`（httpSendSessionInput） |
| 移动端设置 | `bedcode-mobile/src-tauri/src/mobile/system/settings.rs` |
| HTTP API | `bedcode-desktop/src-tauri/src/shared/model/api_dto.rs` |
| 插件系统 (Rust) | `bedcode-desktop/src-tauri/src/desktop/plugin/manager.rs`, `desktop/plugin/setup.rs` |
| 插件系统 (前端) | `bedcode-desktop/src/modules/shared/plugin/`, `desktop/composables/usePluginManager.ts` |
| 插件 API Crate | `bedcode-desktop/src-tauri/bedcode-plugin-api/` — 共享 trait、类型、权限定义 |
| 插件 API 桥接 | `bedcode-desktop/src-tauri/src/desktop/plugin/api_bridge.rs` |
| 插件宿主 | `bedcode-desktop/src-tauri/src/desktop/plugin/host.rs` |
| 插件加载器 | `bedcode-desktop/src-tauri/src/desktop/plugin/loader.rs` |
| 插件权限 | `bedcode-desktop/src-tauri/src/desktop/plugin/permission.rs` (re-export from api crate) |
| 插件注册表 | `bedcode-desktop/src-tauri/src/desktop/plugin/registry.rs` |
| 插件存储 | `bedcode-desktop/src-tauri/src/desktop/plugin/storage.rs` |
| AI Chatbox 插件 (Rust) | `bedcode-desktop/src-tauri/plugins/ai-chatbox/` — BedcodePlugin trait 实现 |
| AI Chatbox 插件 (TS) | `bedcode-desktop/src/plugins/com.bedcode.ai-chatbox/` |
| 自动执行引擎 | `bedcode-mobile/src/modules/mobile/composables/useAutoExecutor.ts` |
| 预设任务 | `bedcode-mobile/src/modules/mobile/composables/usePresetTasks.ts` |
| 任务执行状态 | `bedcode-mobile/src/modules/mobile/composables/useTaskExecutionState.ts` |
| 任务通知 | `bedcode-mobile/src/modules/mobile/composables/useTaskNotification.ts` |
| 代码浏览器 | `bedcode-mobile/src/modules/mobile/views/CodeExplorerView.vue` |
| 代码查看器设置 | `bedcode-mobile/src/modules/shared/stores/codeViewer.ts` |
| 进程管理工具 | `bedcode-desktop/src-tauri/src/shared/system/process.rs` |
| 共享工具函数 | `bedcode-desktop/src-tauri/src/shared/utils.rs` |

### 自动化任务执行机制

BedCode 通过 Claude Code 自定义插件 + HTTP API + WebSocket 事件链路实现移动端远程自动执行多个任务。

#### 整体架构

```
Claude Code Hook (Python)
    ↓ HTTP POST
Rust HTTP API (plugin_controller.rs)
    ↓ DesktopSyncEvent
SyncEventHandler → WebSocket broadcast
    ↓ ws_sync_task_status_changed / ws_sync_session_mode_changed
Mobile Tauri Event → useAutoExecutor (状态机)
    ↓ sendInput / HTTP API
Claude Code (PTY)
```

#### 链路详解

**1. Claude Code 插件** (`scripts/bedcode-plugin/`)

Hook 脚本 (`scripts/bedcode_hook.py`) 注册 4 个事件：

| Hook 事件 | 触发时机 | 处理逻辑 |
|-----------|---------|---------|
| SessionStart | Claude Code 会话启动 | 推送 `idle` 状态到桌面端 |
| PreToolUse | Claude Code 调用工具前 | 查询会话模式：自动模式→auto-approve；手动模式→仍推送 asking 状态但不做 auto-approve |
| Stop | Claude Code 停止响应 | 解析任务状态（completed/in_progress/asking/interrupted）并推送 |
| SubagentStop | 子代理停止 | 同 Stop，解析并推送状态 |

**2. HTTP API** (`desktop/server/controllers/plugin_controller.rs`)

| 路由 | 方法 | 用途 |
|------|------|------|
| `/plugin/task-status` | POST | 接收插件推送的任务状态变更（含 bedcode_session_id 映射） |
| `/plugin/session-mode` | POST | 移动端设置会话自动/手动模式 |
| `/plugin/session-mode` | GET | Python PreToolUse hook 查询会话模式 |

**3. PluginManager** (`desktop/plugin/manager.rs`)

内存存储三个 HashMap：
- `task_states: HashMap<bedcode_session_id, TaskStateEntry>` — 任务状态 + reason + questions
- `auto_modes: HashMap<bedcode_session_id, bool>` — 会话级自动授权模式
- `session_id_map: HashMap<claude_session_id, bedcode_session_id>` — Claude Code ↔ BedCode 会话 ID 映射

每次更新都通过 `DesktopSyncEvent` 广播到所有 WebSocket 客户端。

**4. 移动端自动执行引擎** (`composables/useAutoExecutor.ts`)

按 sessionId 隔离的状态机，核心逻辑：

| 收到状态 | 自动模式行为 | 手动模式行为 |
|---------|-------------|-------------|
| `idle` | 如果有待执行任务则 `startNext()` | 不处理 |
| `in_progress` | 标记当前任务 running | 不处理 |
| `asking` | `handleAsking()` 更新 UI（Python hook 已自动回答） | 不处理（用户在 Claude Code 原生界面操作） |
| `completed` | 标记完成 → `/clear` → 等下次 idle 开始下一个 | 不处理 |
| `interrupted` | 发送"继续"利用上下文从中断点恢复，最多 3 次，超过则标记 failed，开始下一个 | 不处理 |

**5. 模式切换流程**

移动端通过 HTTP 请求切换模式，不经过 PTY：
```
Mobile → POST /api/plugin/session-mode (JWT 认证) → PluginManager 内存更新
    → DesktopSyncEvent::SessionModeChanged → WebSocket broadcast
    → Mobile 收到 ws_sync_session_mode_changed → 同步 UI 状态
```

`POST /api/plugin/session-mode` 支持双认证：Python hook 用 plugin token，移动端用 JWT（`useHttpApi` 自动注入 `Authorization: Bearer <token>` header）。

同时 Python PreToolUse hook 每次被触发时通过 `GET /api/plugin/session-mode` 查询当前模式，
自动模式时返回 `permissionDecision: "allow"` + AskUserQuestion 自动选择推荐项。

**6. 会话 ID 绑定机制**

Claude Code 和 BedCode 各自有独立的 session ID 体系，同一 cwd 下可运行多个 Claude Code 实例，
因此不能通过目录绑定。BedCode 通过进程环境变量实现绑定：

```
BedCode 桌面端启动 PTY 会话
  ↓ pty_process.rs: start()
  ↓ cmd.env("BEDCODE_SESSION_ID", &self.id)
  ↓
Shell 进程继承环境变量 → Claude Code 子进程继承
  ↓
SessionStart hook 触发
  ↓ bedcode_hook.py 读取 os.environ["BEDCODE_SESSION_ID"]
  ↓
POST /plugin/task-status
  ↓ { session_id: "claude-xxx", bedcode_session_id: "pty-uuid", ... }
  ↓
PluginManager.register_session_mapping("claude-xxx" → "pty-uuid")
  ↓
后续所有状态推送和模式查询通过映射关联
```

**无竞态风险**：每个 PTY 进程有独立环境变量空间，多会话互不影响：

```
PTY Session A (PID 1000) → BEDCODE_SESSION_ID=uuid-aaa
PTY Session B (PID 1001) → BEDCODE_SESSION_ID=uuid-bbb
```

**映射使用场景**：

| 场景 | 输入 | 解析 | 查询 key |
|------|------|------|----------|
| task-status 推送 | `claude_session_id` + `bedcode_session_id` | 有 bedcode_sid 时直接用它 | `bedcode_session_id` |
| session-mode 查询 (GET) | `claude_session_id` | `resolve_session_id()` 查映射 | 解析后的 `bedcode_session_id` |
| session-mode 设置 (POST) | `bedcode_session_id`（移动端已知） | 无需解析 | `bedcode_session_id` |

**7. 全局 Hooks 自动配置** (`desktop/plugin/setup.rs`)

应用启动时自动完成以下配置，对用户完全无感：

1. 校验/生成 plugin token
2. 将 `bedcode_hook.py` 复制到 `~/.claude/` 目录
3. 在全局 `~/.claude/settings.json` 中注入 hooks 配置（不覆盖已有配置）
4. 注入 `BEDCODE_PORT` 和 `BEDCODE_TOKEN` 环境变量到 hook 命令
5. 验证 hooks 配置是否生效

合并策略：保留用户已有的非 BedCode hooks 和其他顶层字段（如 `permissions`、`env`），
只替换/更新 BedCode 相关的 hook 条目（识别标准：command 字段包含 `bedcode_hook.py`）。

#### 关键文件索引

| 层 | 文件 | 职责 |
|----|------|------|
| Plugin | `scripts/bedcode_hook.py` | Hook 脚本：状态推送 + 模式查询 + auto-approve + session ID 映射 |
| Rust Setup | `desktop/plugin/setup.rs` | 全局 hooks 自动配置（~/.claude/settings.json 注入） |
| Rust HTTP | `desktop/server/controllers/plugin_controller.rs` | HTTP API 路由处理（含 session ID 解析） |
| Rust DTO | `desktop/server/dtos/plugin_dto.rs` | 请求类型（含 bedcode_session_id 字段） |
| Rust Core | `desktop/plugin/manager.rs` | 任务状态 + 会话映射 + 自动授权模式内存存储、事件广播 |
| Rust PTY | `desktop/pty/pty_process.rs` | PTY 启动时注入 BEDCODE_SESSION_ID 环境变量 |
| Rust Event | `desktop/events/sync_event.rs` | DesktopSyncEvent 定义 |
| Rust Handler | `desktop/events/sync_handler.rs` | 事件→WebSocket 消息转换 |
| Rust Forward | `mobile/router/event.rs` | WebSocket→Tauri 前端事件转发 |
| Mobile Cmd | `mobile/composables/useMobileCommands.ts` | 事件监听注册 |
| Mobile Conn | `mobile/composables/useMobileConnection.ts` | 同步事件回调处理 |
| Mobile HTTP | `mobile/composables/useHttpApi.ts` | HTTP API 封装（含 httpSetSessionMode） |
| Mobile Engine | `mobile/composables/useAutoExecutor.ts` | 自动执行状态机 |
| Mobile Output | `mobile/composables/useTerminalOutput.ts` | 终端输出订阅管理（全局 ws_output 监听 + sessionId 分发） |
| Mobile UI | `mobile/components/AutoExecuteBar.vue` | 自动执行状态条 |
| Mobile UI | `mobile/components/TaskPickerModal.vue` | 任务选择弹窗 |
| Mobile View | `mobile/views/TerminalView.vue` | 终端视图（整合 AutoExecutor） |

### 按类型查找

| 类型 | 路径模式 |
|------|----------|
| Tauri Commands | `*/commands.rs`, `*/commands/*.rs` |
| 错误处理 | `shared/system/error.rs` |
| 数据模型 | `*/model/*.rs`, `*/model.rs`, `*/models.rs` |
| 枚举类型 | `*/enums/*.rs` |
| Traits | `*/traits/*.rs`, `*/traits.rs` |
| 消息处理器 | `*/handler/*.rs`, `*/handlers/*.rs` |
| 业务服务 | `desktop/server/services/*.rs` |
| Pinia Stores | `src/modules/shared/stores/*.ts` |
| Composables | `src/modules/*/composables/*.ts` |
| DTO | `desktop/server/dtos/*.rs`, `shared/model/api_dto.rs` |
| 前端插件 | `src/modules/shared/plugin/`, `src/plugins/` |
| Rust 插件 | `desktop/plugin/*.rs` |

---

## Desktop 与 Mobile Shared 差异

桌面端和移动端的 `shared/` 模块存在差异：

| 子模块 | Desktop | Mobile | 说明 |
|--------|---------|--------|------|
| `auth/` | ✅ pairing.rs | ✅ pairing.rs | 相同 |
| `db/` | ✅ database/models/operations | ❌ | 数据库仅桌面端使用 |
| `enums/` | ✅ 含 plugin.rs | ✅ 含 plugin.rs | 相同 |
| `event/` | ✅ events.rs + handler.rs | ❌ | 事件系统仅桌面端使用 |
| `model/` | ✅ api_dto.rs + message.rs | ✅ message.rs | 移动端只有 message.rs，DTO 用 models.rs |
| `models.rs` | ❌ | ✅ | 移动端单文件 DTO |
| `system/` | ✅ 含 process.rs | ✅ 无 process.rs | 桌面端多进程管理工具 |
| `utils.rs` | ✅ | ❌ | 桌面端共享工具函数 |

---

## 最近更新

- 2026-07-01: 移动端 Android 构建修复 — 安装 @xterm/xterm + addon-fit + addon-web-links + addon-webgl 依赖；重建 gen/android（包名 com.bedcode.app → com.bedcode.mobile）；恢复 ForegroundService/TaskNotification 自定义 Kotlin 文件和 ic_notification.xml；配置 Gradle 代理和本地缓存；移动端 shared 精简（移除 useBackgroundMonitor、useErrorHandler、useGlobalNotifications、useKeyboardShortcuts、useOutputBuffer、useOutputParser、useQrCode、useSessionStatusListener、useSessionWindows、useTauri、device/quickAction/session stores、invoke.ts、LoadingView.vue、EmptyState/Input/NotificationBadge/Select/Skeleton/Spinner/SplashLoading/Tooltip 组件）；新增 useTerminalOutput.ts；移动端 shared/model 新增 message.rs；桌面端 server 新增 git_controller.rs、git_dto.rs、ipc.rs、metrics.rs
- 2026-06-30: 多项目仓库重构 — 从单一 `bedcode/` 拆分为 `bedcode-desktop/` + `bedcode-mobile/` 独立项目；桌面端新增前端插件系统（shared/plugin/）、Rust 插件扩展（api_bridge, host, loader, permission, registry, storage, types）、AI Chatbox 插件（com.bedcode.ai-chatbox）、PluginConfigView/PluginsView、usePluginManager、shared/enums/plugin.rs、shared/system/process.rs、shared/utils.rs、stores/i18n.ts、stores/wsl.ts；移动端新增 useAutoExecutor/usePresetTasks/useTaskExecutionState/useTaskNotification、CodeExplorerView、CodeViewerSettingsModal/ShortcutConfigModal/SessionConfigCard/PresetTaskCard、stores/codeViewer.ts；移动端 shared 精简（移除 db/event 子模块，model/ → models.rs，移除 remote/http_client.rs 和 commands/http.rs）；桌面端 dtos 移除 common.rs
- 2026-06-26: Hooks 全局化 + 会话 ID 绑定 — hooks 配置从项目级 `.claude/settings.json` 改为全局 `~/.claude/settings.json`；hook 脚本从 `${CLAUDE_PROJECT_DIR}/scripts/` 改为 `~/.claude/bedcode_hook.py`（启动时自动复制）；新增 `BEDCODE_SESSION_ID` 环境变量注入实现 Claude Code session 与 BedCode PTY session 绑定；PluginManager 新增 `session_id_map` 映射和 `resolve_session_id()` 解析；去掉 Stop/SubagentStop 的 prompt hook（避免终端可见输出）；HTTP 路由修正为 `/plugin/*`（不含 `/api` 前缀）；新增 `desktop/plugin/setup.rs` 和 `plugin_controller.rs` 到目录树和索引
- 2026-06-23: 自动化任务执行机制文档 — 新增"自动化任务执行机制"章节，记录 Plugin→HTTP→Rust→WebSocket→Mobile 完整链路；模式切换改为 HTTP 直接修改桌面端内存，移除 PTY 输入拦截 /bedcode 命令；手动模式下 PreToolUse 仍推送 asking 状态同步
- 2026-06-19: 大幅重构更新 — WebSocket 客户端从 shared 迁移至 mobile/websocket_client/，JWT/QR Token 从 shared 迁移至 desktop/auth/，notify/parser 从 shared 迁移至 desktop/，新增 app_context (DI 容器)、mobile/system (设置管理)、mobile/commands/http (HTTP API)、mobile/commands/mobile_commands (移动端特有命令)、mobile/remote/http_client (文件浏览)，前端新增文件浏览组件 (FileSidebar/FileTreeItem/FileViewerModal/icons/)、ToolboxView 替代 QuickActionsView、新增 useCodeHighlight/useFileTree/useForegroundService/useHttpApi/useFontSize/useTheme
- 2026-06-10: 同步项目当前结构，新增 events/、remote/、router/ 等目录，更新 stores 位置
