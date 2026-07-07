<div align="center">

# BedCode

**本地远程终端 — 用手机控制桌面**

[![Version](https://img.shields.io/badge/version-1.1.11-blue.svg)](https://github.com/7ZAI/BedCode)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Tauri](https://img.shields.io/badge/Tauri-2.0-orange.svg)](https://v2.tauri.app/)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20Android-lightgrey.svg)](https://github.com/7ZAI/BedCode)

[English](README.md) | 简体中文

</div>

---

BedCode 是一个跨平台本地远程终端应用，让你可以通过移动设备在同一个本地局域网中远程控制桌面终端。桌面端 (Tauri + Vue 3) 作为主机运行终端会话，手机则成为带有优化触控界面的远程终端。它兼容任何终端程序——从系统 Shell 到 Claude Code 等交互式 CLI 工具均可使用。

目前使用场景：如应用名所述躺床上编程；居家环境下当你需要编程同时并行处理其他事务的时候，如：上厕所、做饭、带孩子、睡觉等。

> 目前仅适配桌面端和移动端在同一 WiFi 下的场景，未来会预留互联网接口或内网穿透协议。

## 功能特性

### 桌面端（主机）
- **会话管理** - 创建、配置和管理多个终端会话，SQLite 持久化存储
- **终端预览** - 基于 xterm.js 的实时终端输出预览
- **设备配对** - 二维码 + 6位数字验证码安全配对
- **插件系统** - cdylib 动态加载插件架构，支持宿主 API 桥接、权限控制和持久化存储
- **HTTP + WebSocket 服务器** - 基于 Actix Web 的 HTTP API + WebSocket 终端通信，支持高级网络配置（worker 线程、Keep-Alive、超时、帧大小限制等）
- **服务器管理** - 独立服务器视图，状态监控、指标仪表盘、网络配置编辑器
- **系统托盘** - 托盘快捷操作
- **WSL2 支持** - 在 Windows Subsystem for Linux 中运行会话，支持发行版选择
- **mDNS 发现** - mDNS 服务广播，供移动端发现设备

### 移动端（远程控制）
- **设备发现与配对** - 基于 mDNS 的设备发现、扫描二维码或输入配对码连接
- **终端输出** - 增强模式（解析 ANSI/Markdown）与原始模式切换
- **智能输入栏** - 特殊按键（Tab、Ctrl+C、Esc、方向键）、输入助手、快捷键配置
- **代码浏览器** - 浏览项目文件、语法高亮查看代码、Git diff 渲染
- **预设任务** - 预配置任务卡片，支持类型标签、编辑对话框、一键执行
- **工具箱** - 快捷操作面板，支持自定义命令
- **任务通知** - 按会话的任务状态通知
- **自动重连** - 意外断开后自动重新连接
- **前台服务** - 后台保持连接存活并保持屏幕唤醒（Android）
- **边到边显示** - 现代化全屏移动端体验

### 安全性
- 基于 JWT 的会话认证（HS256，7 天有效期）
- QR 令牌一次性使用，支持可配置 TTL
- 插件 Token 用于 Claude Code hooks 认证
- 配对码 60 秒后自动过期
- 连接时验证设备指纹

> **注意：** 端到端加密（X25519 密钥交换 + AES-GCM）尚在计划中，当前 WebSocket 通信未加密（ws://）。详见[路线图](#路线图)。

### 国际化
- 基于 vue-i18n 的完整 i18n 支持（zh-CN / en）
- 设置页面语言切换器，偏好持久化
- 错误码映射系统，实现本地化错误消息

## 架构

```
┌──────────────────────────────────┐                ┌──────────────────────────────────┐
│          桌面端应用                │                │          移动端应用                │
│        (Tauri + Vue 3)            │                │        (Tauri + Vue 3)            │
│                                    │                │                                    │
│  ┌────────────┐  ┌────────────┐   │                │  ┌────────────┐  ┌────────────┐   │
│  │ PTY 管理器 │  │ WS 服务器  │   │   WebSocket    │  │ WS 客户端  │  │ 代码浏览器 │   │
│  │ (Claude)   │  │ (Actix)    │◄──├───────────────►├─►│            │  │            │   │
│  └────────────┘  └────────────┘   │   + HTTP API   │  └────────────┘  └────────────┘   │
│  ┌────────────┐  ┌────────────┐   │                │  ┌────────────┐  ┌────────────┐   │
│  │ 插件管理器 │  │ HTTP API   │   │                │  │ 预设任务   │  │ 触控界面   │   │
│  │ (cdylib)   │  │ (Actix)    │   │                │  │            │  │            │   │
│  └────────────┘  └────────────┘   │                │  └────────────┘  └────────────┘   │
└──────────────────────────────────┘                └──────────────────────────────────┘
```

项目采用 **Monorepo 独立平台项目** 架构：

| 层级 | 前端 (Vue 3) | 后端 (Rust) |
|------|-------------|-------------|
| **桌面端** | 会话管理器、终端预览、服务器视图、插件配置、侧边栏 | PTY、Actix Web 服务器、会话管理、cdylib 插件系统、mDNS 广告 |
| **移动端** | 终端视图、代码浏览器、预设任务、工具箱、设备发现 | WS 客户端、HTTP 客户端、远程连接、路由、mDNS 发现 |

## 技术栈

| 分类 | 技术 |
|------|------|
| 框架 | Tauri 2.0 |
| 前端 | Vue 3 + TypeScript |
| 样式 | TailwindCSS |
| 状态管理 | Pinia |
| 后端 | Rust (Tokio 异步运行时) |
| HTTP 服务器 | Actix Web 4 |
| 数据库 | SQLite (rusqlite) |
| 通信 | WebSocket + HTTP REST API |
| 终端 | @xterm/xterm + @xterm/addon-fit + @xterm/addon-web-links + @xterm/addon-webgl |
| 国际化 | vue-i18n@9 |
| 测试 | Vitest、Rust test |

## 快速开始

### 环境要求

- [Node.js](https://nodejs.org/) >= 18
- [Rust](https://www.rust-lang.org/tools/install) >= 1.70
- [Tauri 2.0 CLI](https://v2.tauri.app/start/prerequisites/) 及平台相关依赖
- 已安装并配置 [Claude Code CLI](https://claude.ai/code)（可选，用于 Claude Code 会话）

### 安装依赖

```bash
# 桌面端
cd bedcode-desktop
npm install

# 移动端
cd bedcode-mobile
npm install

# Rust 依赖由 Cargo 自动获取
```

### 开发

```bash
# 启动桌面端开发模式
cd bedcode-desktop
npm run tauri:dev

# 启动移动端开发模式
cd bedcode-mobile
npm run tauri:android:dev
```

### 构建

```bash
# 构建桌面端应用
cd bedcode-desktop
npm run tauri:build

# 构建 Android APK（Release）
cd bedcode-mobile
npm run tauri:android:build

# 构建 Android APK（Debug 快速构建）
cd bedcode-mobile
npm run tauri:android:build:fast
```

### 测试

```bash
# 前端单元测试
npm run test

# Rust 测试
cargo test
```

### 代码检查与格式化

```bash
# 代码检查
npm run lint

# 格式化
npm run format
```

## 配置

BedCode 使用 `config.properties` 文件（作为 Tauri 资源打包）进行运行时配置。文件支持注释，按分类组织：

### 网络

| 键名 | 默认值 | 说明 |
|------|--------|------|
| `network.port` | `8765` | WebSocket 服务器端口 |
| `network.auto_start` | `true` | 应用启动时自动开启服务器 |
| `network.prevent_sleep` | `true` | 服务器运行时阻止系统休眠 |
| `network.workers` | `0` | Actix Web worker 线程数（0 = CPU 核心数） |
| `network.keep_alive_secs` | `5` | HTTP Keep-Alive 超时秒数（0 = 禁用） |
| `network.client_request_timeout_secs` | `5` | 客户端请求头读取超时秒数 |
| `network.client_disconnect_timeout_secs` | `3` | 客户端断开连接等待超时秒数 |
| `network.max_connections` | `256` | 每 worker 最大并发连接数 |
| `network.backlog` | `2048` | TCP 半连接队列上限 |
| `network.tcp_nodelay` | `true` | 启用 TCP_NODELAY（禁用 Nagle 算法） |
| `network.shutdown_timeout_secs` | `30` | 优雅停机超时秒数 |
| `network.ws_max_frame_size_kb` | `64` | WebSocket 单帧最大大小（KB） |
| `network.ws_max_message_size_mb` | `16` | WebSocket 单消息最大大小（MB，可跨多帧） |

### 会话

| 键名 | 默认值 | 说明 |
|------|--------|------|
| `session.default_environment` | `windows` | 默认执行环境（windows / wsl2） |
| `session.default_wsl_distro` | *(空)* | 默认 WSL 发行版（仅 wsl2 环境有效，空 = 默认发行版） |
| `session.default_working_dir` | *(空)* | 默认工作目录（空 = 用户主目录） |
| `session.default_command` | *(空)* | 默认启动命令（空 = 系统 Shell） |
| `session.session_timeout` | `3600` | 会话超时时间（秒）- 无活动自动关闭 |

### 界面

| 键名 | 默认值 | 说明 |
|------|--------|------|
| `ui.theme` | `system` | 主题（system/light/dark） |
| `ui.terminal_font_size` | `12` | 终端字体大小 |
| `ui.terminal_font_family` | `Consolas` | 终端字体名称 |
| `ui.terminal_theme` | `dracula` | 终端配色主题名 |
| `ui.show_preview` | `true` | 是否显示终端预览 |

### 终端

| 键名 | 默认值 | 说明 |
|------|--------|------|
| `terminal.default_cols` | `120` | 默认终端列数 |
| `terminal.default_rows` | `40` | 默认终端行数 |
| `terminal.flush_interval_ms` | `50` | 输出缓冲刷新间隔（毫秒） |
| `terminal.max_buffer_size` | `65536` | 最大输出缓冲大小（字节） |
| `terminal.read_buffer_size` | `4096` | PTY 读取缓冲区大小（字节） |

### Channel 容量

| 键名 | 默认值 | 说明 |
|------|--------|------|
| `channels.output_broadcast_capacity` | `2048` | PTY 输出事件广播容量 |
| `channels.status_broadcast_capacity` | `64` | 会话状态变更广播容量 |
| `channels.restart_broadcast_capacity` | `64` | 会话重启事件广播容量 |
| `channels.event_broadcast_capacity` | `256` | 统一事件广播容量 |
| `channels.pty_subscription_capacity` | `1024` | PTY 订阅广播容量 |
| `channels.global_queue_capacity` | `50000` | 全局输出队列容量（供移动端回放） |
| `channels.ws_event_capacity` | `1024` | WebSocket 事件广播容量 |
| `channels.lifecycle_capacity` | `16` | 生命周期事件广播容量 |

### 插件

| 键名 | 默认值 | 说明 |
|------|--------|------|
| `plugin.token` | *(空)* | HTTP API 认证 token - 插件推送任务状态时需携带此 token（空 = 跳过验证，开发模式） |

## 工作原理

1. **启动桌面端** - 在电脑上启动 BedCode，自动开启 Actix Web 服务器（HTTP + WebSocket）和 mDNS 发现服务
2. **配对手机** - 在手机上打开 BedCode，通过 mDNS 发现桌面端，扫描二维码或输入 6 位配对码
3. **远程控制** - 配对成功后，选择会话即可从手机发送命令
4. **实时输出** - 终端输出实时推送到手机，支持 ANSI 渲染
5. **浏览代码** - 使用代码浏览器浏览项目文件，查看带语法高亮的代码和 diff
6. **预设任务** - 配置常用任务为预设卡片，一键执行

## 插件系统

BedCode 桌面端采用基于 cdylib 的动态插件系统：

- **动态加载** - 插件编译为 `.cdylib` 共享库，运行时动态加载
- **宿主 API 桥接** - 插件通过版本化 API 桥接访问宿主功能（发送输入、读取输出、会话信息）
- **权限控制** - 每个插件声明所需权限，宿主强制执行访问边界
- **持久化存储** - 插件可通过宿主提供的存储接口存取键值数据
- **自动配置** - 会话启动时自动配置项目级 Claude Code hooks
- **任务状态追踪** - Claude Code hooks 通过 HTTP API 推送任务状态（idle/in_progress/asking/completed/interrupted）到桌面端
- **会话 ID 绑定** - PTY 会话注入 `BEDCODE_SESSION_ID` 环境变量，绑定 Claude Code 会话与 BedCode 会话

```
Claude Code Hook (Python)
    ↓ HTTP POST
Rust HTTP API (plugin_controller)
    ↓ DesktopSyncEvent
SyncEventHandler → WebSocket broadcast
    ↓ ws_sync_task_status_changed
Mobile Tauri Event → 预设任务 / UI
    ↓ sendInput / HTTP API
Claude Code (PTY)
```

## 项目结构

```
BedCode/
├── bedcode-desktop/               # 桌面端应用 (Tauri + Vue 3)
│   ├── src/                       # Vue 3 前端
│   │   ├── components/            # UI 组件
│   │   ├── composables/           # 业务逻辑 composables
│   │   ├── stores/                # Pinia 状态管理
│   │   ├── views/                 # 页面视图
│   │   ├── locales/               # i18n 翻译文件（zh-CN / en）
│   │   └── plugins/               # 前端插件加载器
│   ├── src-tauri/
│   │   ├── src/
│   │   │   ├── commands/          # Tauri invoke 命令
│   │   │   ├── db/                # SQLite 数据库层
│   │   │   ├── enums/             # 枚举类型
│   │   │   ├── events/            # 全局事件系统
│   │   │   ├── mdns/              # mDNS 服务广播
│   │   │   ├── plugin/            # cdylib 插件系统
│   │   │   ├── pty/               # PTY 管理（Windows + WSL2）
│   │   │   ├── server/            # Actix Web HTTP/WS 服务器
│   │   │   ├── session/           # 会话管理
│   │   │   ├── system/            # 配置、错误处理、应用上下文
│   │   │   └── utils/             # 认证（JWT、配对）、解析器（ANSI、Markdown）
│   │   └── resources/
│   │       └── config.properties  # 运行时配置
│   └── docs/
│       └── code-map.md            # 桌面端模块索引
│
├── bedcode-mobile/                # 移动端应用 (Tauri + Vue 3)
│   ├── src/                       # Vue 3 前端
│   │   ├── components/            # UI 组件
│   │   ├── composables/           # 业务逻辑 composables
│   │   ├── stores/                # Pinia 状态管理
│   │   ├── views/                 # 页面视图
│   │   └── locales/               # i18n 翻译文件（zh-CN / en）
│   ├── src-tauri/
│   │   └── src/
│   │       ├── auth/              # 认证（管理器、配对）
│   │       ├── commands/          # Tauri invoke 命令
│   │       ├── connection/        # 远程连接（WS 客户端、心跳、重连）
│   │       ├── enums/             # 枚举类型
│   │       ├── handler/           # 消息处理器
│   │       ├── mdns/              # mDNS 服务发现
│   │       ├── model/             # 数据模型
│   │       ├── plugin/            # Android 插件桥接
│   │       ├── router/            # 消息路由
│   │       ├── system/            # 配置、错误处理、设置
│   │       ├── session.rs         # 远程会话管理
│   │       └── state.rs           # 全局状态
│   └── docs/
│       └── code-map.md            # 移动端模块索引
│
├── docs/                          # 共享文档
└── .github/                       # CI/CD 工作流
```

完整模块索引请参阅 [bedcode-desktop/docs/code-map.md](bedcode-desktop/docs/code-map.md) 和 [bedcode-mobile/docs/code-map.md](bedcode-mobile/docs/code-map.md)。

## 路线图

- [x] Claude Code hooks 插件系统与 cdylib 动态加载
- [x] 移动端文件浏览器和代码查看器（含 diff 渲染）
- [x] 多语言支持 (i18n: zh-CN / en)
- [x] 预设任务卡片，支持一键执行
- [x] Actix Web 服务器高级网络配置
- [x] 服务器管理视图与指标仪表盘
- [ ] 端到端加密（X25519 + AES-GCM）
- [ ] Linux 桌面端支持
- [ ] 互联网连接接口预留
- [ ] FCM 推送通知
- [ ] 终端历史虚拟滚动

## 参与贡献

欢迎贡献！请随时提交 Pull Request。

1. Fork 本仓库
2. 创建功能分支 (`git checkout -b feat/my-feature`)
3. 提交更改 (`git commit -m 'feat: add my feature'`)
4. 推送到分支 (`git push origin feat/my-feature`)
5. 发起 Pull Request

## 许可证

本项目基于 MIT 许可证开源 - 详见 [LICENSE](LICENSE) 文件。
