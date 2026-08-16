<div align="center">

<img src="bedcode-desktop/public/favicon.svg" width="96" alt="BedCode logo">

# BedCode

[![Version](https://img.shields.io/badge/version-2.0.0-blue.svg)](https://github.com/7ZAI/BedCode)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Tauri](https://img.shields.io/badge/Tauri-2.0-orange.svg)](https://v2.tauri.app/)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20Android-lightgrey.svg)](https://github.com/7ZAI/BedCode)

[English](README_en.md) | 简体中文

</div>

BedCode 是一个局域网远程终端应用：桌面端作为主机运行终端会话（Claude Code、opencode 等 Agent CLI），手机变成带优化触控界面的远程终端，在同一 WiFi 下随时接管你的终端。任意命令行程序（含 TUI 应用）都可以在桌面端启动、从手机远程操作。

> 使用场景：如应用名所述，躺床上编程；或在家务、带孩子、睡觉的同时并行处理编程任务。

> [!NOTE]
> 当前面向桌面端与移动端同一 WiFi 的场景；未来预留互联网连接接口 / 内网穿透协议（需服务器）。

## 功能特性

### 桌面端（主机）

- **会话管理** — 多个 Agent CLI / 终端会话，SQLite 持久化，xterm.js 实时输出预览
- **设备配对** — 二维码 + 6 位验证码安全配对，mDNS 服务广播供移动端自动发现
- **HTTP + WebSocket 服务器** — Actix Web，支持高级网络配置（worker 线程、Keep-Alive、超时、帧大小限制等），独立服务器管理视图与指标仪表盘
- **插件系统** — WASM（wasmtime 沙箱），宿主 API 桥接、权限控制、hooks 集成
- **WSL2 支持** — 在 Windows Subsystem for Linux 中运行会话，可指定发行版
- **系统托盘** — 快捷操作

### 移动端（远程控制）

- **设备发现与配对** — mDNS 发现、扫码或输入配对码连接
- **终端输出** — 增强模式（ANSI / Markdown 解析）与原始模式切换
- **智能输入栏** — 特殊按键（Tab、Ctrl+C、Esc、方向键）、输入助手、快捷键配置
- **代码浏览器** — 项目文件浏览、语法高亮、Git diff 渲染
- **预设任务** — 任务卡片，类型标签、编辑、一键执行
- **工具箱** — 快捷操作面板，自定义命令
- **任务通知** — 按会话任务状态推送通知，前台服务保活并保持屏幕唤醒（Android）
- **自动重连** — 意外断开自动重连，边到边全屏显示

### 安全

- **配对** — 6 位配对码（60 秒过期）、一次性 QR 令牌（可配置 TTL）
- **生物认证** — 移动端生物凭证绑定公钥，挑战-应答验签后签发会话凭证（防重放）
- **JWT 会话认证**（HS256，7 天）+ 设备指纹验证；插件 Token 用于 Agent CLI hooks 认证

> [!WARNING]
> 端到端加密工具库（X25519 ECDH + AES-256-GCM）已实现，但 WebSocket / 文件传输接入仍在进行中，当前终端通信仍为明文（`ws://`），请在可信局域网内使用。

### 国际化

vue-i18n 完整支持（zh-CN / en），设置页语言切换并持久化；错误码映射系统提供本地化错误消息。

## 架构

Monorepo 双独立项目，各自包含 `src/`（前端）+ `src-tauri/`（Rust 后端）：

```mermaid
flowchart LR
    subgraph DESKTOP["桌面端（主机）· Tauri 2.0"]
        direction TB
        D_UI["Vue 3 前端<br/>会话管理 · 终端预览 · 服务器视图 · 插件配置"]
        D_CORE["Rust 宿主<br/>Tauri 命令 / 事件桥"]
        D_PTY["PTY 进程管理<br/>命令启动 · WSL · 输出分发"]
        D_SESS["会话管理器<br/>生命周期 · 事件总线"]
        D_SRV["Actix Web 服务器<br/>HTTP REST + WebSocket"]
        D_PLG["插件宿主<br/>wasmtime 沙箱 · 权限 · API 桥接"]
        D_FS["文件服务<br/>目录挂载 · 传输引擎"]
        D_MDNS["mDNS 服务广播"]
        D_DB[("SQLite")]
        D_UI <--> D_CORE
        D_CORE --- D_PTY & D_SESS & D_SRV & D_PLG & D_FS & D_MDNS
        D_SESS --- D_DB
        D_PLG --- D_SRV
    end

    subgraph MOBILE["移动端（远程终端）· Tauri 2.0 / Android"]
        direction TB
        M_UI["Vue 3 前端<br/>终端视图 · 代码浏览器 · 工具箱 · 预设任务"]
        M_CORE["Rust 宿主<br/>Tauri 命令 / 事件桥"]
        M_WS["WS 客户端<br/>心跳 · 重连 · 请求-响应"]
        M_RT["消息路由<br/>终端 / 同步 / 文件处理器"]
        M_AUTH["认证<br/>配对 · JWT · 生物凭证"]
        M_PLG["插件宿主<br/>wasmtime 沙箱"]
        M_FS["文件服务<br/>SAF · 传输引擎"]
        M_MDNS["mDNS 发现"]
        M_UI <--> M_CORE
        M_CORE --- M_WS & M_RT & M_AUTH & M_PLG & M_FS & M_MDNS
    end

    D_SRV <--> M_WS
    D_FS <--> M_FS
    D_MDNS <--> M_MDNS
```

| 端 | 前端 (Vue 3) | 后端 (Rust) |
|----|-------------|-------------|
| **桌面端** | 会话管理器、终端预览、服务器视图、插件配置 | PTY、Actix Web（HTTP + WS）、会话管理、WASM 插件系统、mDNS 广播 |
| **移动端** | 终端视图、代码浏览器、预设任务、工具箱、设备发现 | WS/HTTP 客户端、远程连接与路由、文件服务、mDNS 发现 |

通信：**WebSocket**（终端双向流）+ **HTTP REST API**（插件 hooks、文件服务）。

## 技术栈

| 分类 | 技术 |
|------|------|
| 框架 | Tauri 2.0（桌面端 Windows / 移动端 Android） |
| 前端 | Vue 3 + TypeScript + Vite |
| 样式 | TailwindCSS，状态管理 Pinia + vue-router |
| 后端 | Rust（Tokio 异步运行时），Actix Web 4 + tokio-tungstenite |
| 数据库 | SQLite（rusqlite） |
| 终端 | @xterm/xterm + addon-fit / web-links / webgl |
| 认证 | JWT（jsonwebtoken HS256）、ECDSA 生物凭证（p256）、设备指纹 |
| 加密 | X25519 ECDH + AES-256-GCM（HKDF 派生）、ChaCha20-Poly1305、RSA-OAEP/PSS |
| 设备发现 | mDNS（mdns-sd） |
| 插件系统 | wasmtime（WASM 组件运行时） |
| 其他 | shiki（代码高亮）、ECharts（指标仪表盘）、qrcode / html5-qrcode、vue-i18n@9、tracing 日志 |

## 支持平台

| 平台 | 桌面端 | 移动端 |
|------|:---:|:---:|
| Windows | ✔ | — |
| Android | — | ✔ |
| macOS / Linux | 预留 | — |
| iOS | — | 预留 |

当前聚焦 **Windows（桌面端）+ Android（移动端）** 双平台，两端核心能力（终端会话、文件服务、插件系统）均已跑通。跨平台适配工程量较大（系统权限模型、打包分发、平台集成），精力有限暂未覆盖。

> 依托 Tauri 2.0 架构（Rust 后端 + Web 前端），跨平台的可能性与便利性天然保留：核心业务逻辑与 UI 均为跨平台技术，未来扩展 macOS / Linux / iOS 时无需重写业务代码，主要工作是平台适配层（打包、权限、系统 API 对接）。

## 快速开始

### 环境要求

- [Node.js](https://nodejs.org/) >= 18、[Rust](https://www.rust-lang.org/tools/install) >= 1.94（wasmtime 47 MSRV）
- [Tauri 2.0 CLI](https://v2.tauri.app/start/prerequisites/) 及平台相关依赖
- 已安装并配置 Agent CLI（如 [Claude Code](https://claude.ai/code)）

### 安装与运行

```bash
# 安装依赖
cd bedcode-desktop && npm install
cd bedcode-mobile && npm install

# 开发
cd bedcode-desktop && npm run tauri:dev         # 桌面端
cd bedcode-mobile && npm run tauri:android:dev  # 移动端（查看 Android 日志：tauri:android:dev:log）

# 构建
cd bedcode-desktop && npm run tauri:build
cd bedcode-mobile && npm run tauri:android:build

# 测试
cd bedcode-desktop && npm run test:run          # 前端（vitest run）
cd bedcode-desktop/src-tauri && cargo test      # Rust
```

## 插件系统

桌面端插件基于 **wasmtime 运行时（WASM Component Model）**：插件由 Rust / TypeScript 编译为 WASM 组件，在宿主内沙箱加载运行。插件可观察和扩展宿主会话行为：

- **WASM 沙箱运行时** — 资源受限、内存隔离，插件崩溃不影响宿主
- **动态加载** — 扫描 `plugins/desktop/{plugin-id}/plugin.json` 运行时加载，无需重编译宿主
- **宿主 API 桥接** — 版本化 API 访问宿主功能（发送输入、读取输出、会话信息），统一权限校验
- **权限控制** — 插件声明所需权限，宿主强制执行访问边界
- **Hooks 集成** — 会话启动自动配置项目级 hooks，经 HTTP API 推送任务状态（idle / in_progress / asking / completed / interrupted）
- **会话 ID 绑定** — PTY 注入 `BEDCODE_SESSION_ID`，绑定 Agent CLI 会话与 BedCode 会话

```
Agent CLI Hook (Python)
    ↓ HTTP POST
Rust HTTP API (plugin_controller)
    ↓ DesktopSyncEvent
SyncEventHandler → WebSocket broadcast
    ↓ ws_sync_task_status_changed
Mobile Tauri Event → 预设任务 / UI
    ↓ sendInput / HTTP API
Agent CLI (PTY)
```

### 官方插件

| 插件 | 版本 | 说明 |
|------|------|------|
| **AI Chatbox** | 1.0.0-beta | AI 大模型对话：接入任意 OpenAI 兼容供应商（OpenAI / Anthropic / DeepSeek / 通义千问），流式对话、多会话管理、JSONL 对话日志落盘 |
| **Auto Task** | 1.0.0-beta | Agent 任务队列与自动授权：同步 Claude Code / pi / opencode / Codex 任务状态，任务队列调度、预设任务、定时任务与历史统计；agent 请求授权时自动放行 |
| **File Transfer** | 1.0.0-beta | 内网文件传输：在线对端发现与切换、远程目录浏览、多任务并发传输（暂停 / 恢复 / 断点续传 / 失败重试），支持本地目录挂载供对端访问 |

### 插件开发 SDK

- **`@bedcode/plugin-sdk-desktop`** / **`@bedcode/plugin-sdk-mobile`**（npm，MIT）— 主 API、Vite 插件（`./vite`）、共享 UI 组件（`./ui`）、类型定义（`./types`）等子路径导出
- **脚手架 CLI** — `bedcode-plugin-desktop`（移动端 `bedcode-plugin`）：`create` 生成插件工程、`dev` 浏览器 HMR 开发环境、`build` 构建、`manifest` 自动填充声明、`validate` 校验、`doctor` 环境自检
- **开发文档** — `bedcode-desktop/plugin-dev-desktop.md`（桌面端）与 `bedcode-mobile/plugin-dev-mobile.md`（移动端）

### 未来展望

- **本地工具热插拔** — 插件机制已支持运行时加载、启用、停用与卸载：把常用本地工具（终端增强、代码分析、文件处理、快捷命令等）封装为插件，按需安装、即插即用，宿主无需重编译
- **WASI 演进红利** — 插件运行于 WASM Component Model + wasmtime 沙箱之上，随着 WASI（WebAssembly System Interface）标准化推进（文件系统、网络、时钟、进程等系统能力），插件可在安全沙箱内获得接近原生的系统能力，且跨宿主可移植——同一插件可运行于任何 WASI 兼容环境
- **个人预言** — 随着 AI 的发展，未来人人都可能构建一套各自领域的本地化应用：用自然语言描述需求，云 vibe coding 生成插件、云构建编译、云算力托管，最终装进宿主即插即用。这套「宿主 + 插件」架构让工具的生产从少数开发者手中解放出来——人人都能直面自己的需求，工具为个人而生

## 贡献指南

欢迎贡献！随时提交 Pull Request：

1. Fork 本仓库
2. 创建功能分支（`git checkout -b feat/my-feature`）
3. 提交更改（`git commit -m 'feat: ...'`）
4. 推送分支并发起 PR

## 许可证

MIT - 详见 [LICENSE](LICENSE)。
