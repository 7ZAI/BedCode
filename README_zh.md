<div align="center">

# BedCode

**用手机远程控制桌面上的 Claude Code**

[![Version](https://img.shields.io/badge/version-0.1.0-blue.svg)](https://github.com/7ZAI/BedCode)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Tauri](https://img.shields.io/badge/Tauri-2.0-orange.svg)](https://v2.tauri.app/)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20Android-lightgrey.svg)](https://github.com/7ZAI/BedCode)

[English](README.md) | 简体中文

</div>

---

BedCode 是一个跨平台应用，让你可以通过移动设备在同一个本地局域网中远程控制 [Claude Code](https://claude.ai/code)。桌面端 (Tauri + Vue 3) 作为主机运行终端会话，手机则成为带有优化触控界面的远程终端。

虽然项目最初的目的是作为 Claude Code 的远程控制应用，但是目前应用同样实现了远程终端的效果。

目前使用场景：如应用名所述躺床上编程；居家环境下当你需要编程同时并行处理其他事务的时候，如：上厕所、做饭、带孩子、睡觉等。

> 目前仅仅适配桌面端和移动端在同一个 WiFi 下的场景。

未来会预留联通互联网的接口或内网穿透的协议（需要服务器）。

## 功能特性

### 桌面端（主机）
- **会话管理** - 创建、配置和管理多个 Claude Code 会话
- **终端预览** - 基于 xterm.js 的实时终端输出预览
- **设备配对** - 二维码 + 6位数字验证码安全配对
- **插件系统** - 自动配置 Claude Code hooks、任务状态追踪、自动授权模式
- **HTTP + WebSocket 服务器** - 基于 Actix Web 的 HTTP API + WebSocket 终端通信
- **系统托盘** - 托盘快捷操作
- **WSL2 支持** - 在 Windows Subsystem for Linux 中运行会话

### 移动端（远程控制）
- **设备发现与配对** - 扫描二维码或输入配对码连接
- **终端输出** - 增强模式（解析 ANSI/Markdown）与原始模式切换
- **智能输入栏** - 特殊按键（Tab、Ctrl+C、Esc、方向键）、输入助手、快捷键配置
- **自动执行引擎** - 任务队列式自动执行，支持自动/手动模式切换
- **代码浏览器** - 浏览项目文件、语法高亮查看代码、Git diff 渲染
- **预设任务** - 预配置任务卡片，支持类型标签和操作菜单
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
│  │ PTY 管理器 │  │ WS 服务器  │   │   WebSocket    │  │ WS 客户端  │  │ 自动执行   │   │
│  │ (Claude)   │  │ (Actix)    │◄──├───────────────►├─►│            │  │ 引擎       │   │
│  └────────────┘  └────────────┘   │   + HTTP API   │  └────────────┘  └────────────┘   │
│  ┌────────────┐  ┌────────────┐   │                │  ┌────────────┐  ┌────────────┐   │
│  │ 插件管理器 │  │ HTTP API   │   │                │  │ 代码浏览器 │  │ 触控界面   │   │
│  │ (Hooks)    │  │ (Actix)    │   │                │  │            │  │            │   │
│  └────────────┘  └────────────┘   │                │  └────────────┘  └────────────┘   │
└──────────────────────────────────┘                └──────────────────────────────────┘
```

项目采用 **共享 + 平台特定** 的分层架构：

| 层级 | 前端 (Vue 3) | 后端 (Rust) |
|------|-------------|-------------|
| **共享层** | 组件、composables、stores、i18n、工具函数 | 认证、数据库、WebSocket、解析器、模型、错误处理 |
| **桌面端** | 会话管理器、终端预览、侧边栏 | PTY、Actix Web 服务器、会话管理、插件系统 |
| **移动端** | 终端视图、代码浏览器、自动执行、工具箱 | WS 客户端、HTTP 客户端、远程连接、路由 |

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
| 终端 | xterm.js |
| 国际化 | vue-i18n@9 |
| 测试 | Vitest、Playwright、Rust test |

## 快速开始

### 环境要求

- [Node.js](https://nodejs.org/) >= 18
- [Rust](https://www.rust-lang.org/tools/install) >= 1.70
- [Tauri 2.0 CLI](https://v2.tauri.app/start/prerequisites/) 及平台相关依赖
- 已安装并配置 [Claude Code CLI](https://claude.ai/code)

### 安装依赖

```bash
# 安装前端依赖
npm install

# Rust 依赖由 Cargo 自动获取
```

### 开发

```bash
# 启动桌面端开发模式
npm run tauri:dev
```

### 构建

```bash
# 构建桌面端应用
npm run tauri:build

# 构建 Android APK
npm run tauri:android:build
```

### 测试

```bash
# 前端单元测试
npm run test

# 前端测试覆盖率
npm run test:coverage

# E2E 测试
npm run test:e2e

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

BedCode 使用 `config.json` 文件（作为 Tauri 资源打包）进行运行时配置：

| 分类 | 键名 | 默认值 | 说明 |
|------|------|--------|------|
| 网络 | `network.port` | `8765` | WebSocket 服务器端口 |
| 网络 | `network.heartbeat_interval_secs` | `30` | 心跳间隔 |
| 会话 | `session.default_command` | `"claude"` | 默认终端命令 |
| 界面 | `ui.theme` | `"system"` | 主题（system/light/dark） |
| 界面 | `ui.language` | `"zh-CN"` | 语言（zh-CN/en） |
| 终端 | `terminal.default_cols` | `120` | 默认终端列数 |
| 终端 | `terminal.flush_interval_ms` | `30` | 输出刷新间隔 |

## 工作原理

1. **启动桌面端** - 在电脑上启动 BedCode，自动开启 Actix Web 服务器（HTTP + WebSocket）和 mDNS 发现服务
2. **配对手机** - 在手机上打开 BedCode，扫描二维码或输入 6 位配对码
3. **远程控制** - 配对成功后，选择会话即可从手机发送命令
4. **实时输出** - 终端输出实时推送到手机，支持 ANSI 渲染
5. **自动执行任务** - 在手机上排队多个任务，自动执行引擎会在 Claude Code 空闲时逐个发送
6. **浏览代码** - 使用代码浏览器浏览项目文件，查看带语法高亮的代码和 diff

## 插件系统

BedCode 通过基于 hooks 的插件系统与 Claude Code 集成：

- **自动配置** - 会话启动时自动配置项目级 Claude Code hooks
- **任务状态追踪** - Claude Code hooks 通过 HTTP API 推送任务状态（idle/in_progress/asking/completed/interrupted）到桌面端
- **自动授权模式** - 自动模式下，Claude Code 的工具使用权限自动批准；手动模式下，用户直接操作 Claude Code
- **会话 ID 绑定** - PTY 会话注入 `BEDCODE_SESSION_ID` 环境变量，绑定 Claude Code 会话与 BedCode 会话

```
Claude Code Hook (Python)
    ↓ HTTP POST
Rust HTTP API (plugin_controller)
    ↓ DesktopSyncEvent
SyncEventHandler → WebSocket broadcast
    ↓ ws_sync_task_status_changed
Mobile Tauri Event → 自动执行引擎 (状态机)
    ↓ sendInput / HTTP API
Claude Code (PTY)
```

## 项目结构

```
bedcode/
├── src/                          # Vue 3 前端
│   ├── modules/
│   │   ├── desktop/              # 桌面端 UI（会话、终端、设备）
│   │   ├── mobile/               # 移动端 UI（终端、代码浏览器、工具箱、配对）
│   │   └── shared/               # 共享组件、stores、composables、i18n
│   └── locales/                  # i18n 翻译文件（zh-CN / en）
├── src-tauri/
│   └── src/
│       ├── shared/               # 共享 Rust 模块（认证、数据库、枚举、模型、系统）
│       ├── desktop/              # 桌面端专属（PTY、Actix 服务器、会话管理、插件）
│       └── mobile/               # 移动端专属（WS 客户端、HTTP 客户端、路由、远程连接）
├── docs/                         # 文档
└── e2e/                          # E2E 测试
```

完整模块索引请参阅 [docs/code-map.md](docs/code-map.md)。

## 路线图

- [x] Claude Code hooks 插件系统与自动执行
- [x] 移动端文件浏览器和代码查看器（含 diff 渲染）
- [x] 多语言支持 (i18n: zh-CN / en)
- [x] 自动执行任务引擎（自动/手动模式）
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
