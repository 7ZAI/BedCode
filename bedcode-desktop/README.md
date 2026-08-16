<div align="center">

<img src="public/favicon.svg" width="96" alt="BedCode Desktop logo">

# BedCode Desktop

**BedCode 的桌面端主机** — 在 Windows 上运行多个 Agent CLI / 终端会话（Claude Code、opencode、pi 等），供手机在同一 WiFi 下远程接管。

[![Version](https://img.shields.io/badge/version-2.0.0-blue.svg)](https://github.com/7ZAI/BedCode)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](../LICENSE)
[![Tauri](https://img.shields.io/badge/Tauri-2.0-orange.svg)](https://v2.tauri.app/)

[English](README_en.md) | 简体中文

</div>

本仓库是 BedCode 单仓库中的桌面端项目（Tauri 2.0 + Vue 3 + Rust）。移动端项目见 [`bedcode-mobile/`](../bedcode-mobile/)，整体介绍见根目录 [README](../README.md)。

## 功能特性

- **会话管理** — 同时运行多个 Agent CLI / 终端会话，SQLite 持久化会话配置，xterm.js 实时输出预览
- **PTY 终端** — 原生伪终端运行任意命令行程序（含 TUI 应用），支持 WSL2 发行版
- **设备配对与发现** — 二维码 + 6 位验证码安全配对，mDNS 服务广播供移动端自动发现
- **HTTP + WebSocket 服务器** — Actix Web 实现终端双向流与 REST API（插件 hooks、文件服务），支持高级网络配置（worker 线程、Keep-Alive、超时、帧大小限制）与独立指标仪表盘
- **插件系统** — WASM（wasmtime 沙箱，WASM Component Model）+ cdylib 动态加载，权限控制、hooks 集成、会话 ID 绑定
- **系统托盘** — 后台常驻与快捷操作
- **自动更新** — Tauri updater（正式版由 GitHub Actions 签名发布）
- **国际化** — vue-i18n（zh-CN / en）完整支持

## 技术栈

| 分类 | 技术 |
|------|------|
| 框架 | Tauri 2.0（Windows），Node.js + Rust |
| 前端 | Vue 3 + TypeScript + Vite + TailwindCSS |
| 状态 | Pinia + vue-router |
| 后端 | Rust（Tokio）、Actix Web 4 + tokio-tungstenite |
| 数据库 | SQLite（rusqlite） |
| 终端 | @xterm/xterm + addon-fit / web-links / webgl |
| 认证 | JWT（HS256）、设备指纹、6 位配对码 / QR 令牌 |
| 插件 | wasmtime（WASM 组件运行时）+ cdylib 动态加载 |
| 其他 | shiki、ECharts、qrcode、vue-i18n@9、tracing 日志 |

## 目录结构

```
bedcode-desktop/
├── src/                    # Vue 3 前端
│   ├── components/         # UI 组件（TitleBar、Sidebar、TerminalPreview 等）
│   ├── composables/        # 业务逻辑（useServer、usePairing、usePluginManager 等）
│   ├── stores/             # Pinia store（session、device、settings、wsl 等）
│   ├── views/              # 页面（终端窗口、会话配置、服务器、插件、设备等）
│   ├── plugin/             # 前端插件加载器与权限映射
│   └── locales/            # i18n（zh-CN / en）
├── src-tauri/              # Rust 后端
│   ├── src/
│   │   ├── pty/            # 伪终端与进程管理
│   │   ├── server/         # Actix Web HTTP + WebSocket 服务器
│   │   ├── session/        # 会话模型与生命周期
│   │   ├── db/             # SQLite 持久化
│   │   ├── plugin/         # 插件宿主：wasmtime 运行时 + cdylib 加载
│   │   ├── mdns/           # mDNS 服务广播
│   │   ├── events/         # 事件总线与 WebSocket 广播
│   │   └── commands/       # Tauri commands
├── packages/
│   └── plugin-sdk-desktop/ # 插件开发 SDK（TS + Rust），见其 README
├── plugins/                # 官方插件（ai-chatbox / auto-task / file-transfer / scheduler）
├── scripts/                # 开发构建脚本（含 README）
└── docs/                   # 项目文档（code-map.md 等）
```

## 快速开始

### 环境要求

- [Node.js](https://nodejs.org/) >= 18、[Rust](https://www.rust-lang.org/tools/install) >= 1.70
- [Tauri 2.0 CLI](https://v2.tauri.app/start/prerequisites/) 及平台相关依赖
- 已安装并配置 Agent CLI（如 [Claude Code](https://claude.ai/code)、opencode、pi）

### 安装与运行

```bash
npm install

# 开发模式（自动构建插件 + 启动 Tauri dev）
npm run tauri:dev

# 生产构建（tauri-build.js 自动解析 updater 签名密钥，
# 未配置密钥时自动禁用升级包生成，本地构建无需私钥）
npm run tauri:build
```

### 测试

```bash
npm run test:run            # 前端单元测试（vitest run，注意不要用 npm run test 的 watch 模式）
npx playwright test         # E2E 测试
cd src-tauri && cargo test  # Rust 测试
```

### 其他常用脚本

| 命令 | 说明 |
|------|------|
| `npm run build` / `build:fast` | 前端类型检查 + 构建 / 仅构建 |
| `npm run plugins:build` | 构建全部官方插件（wasm + 前端产物） |
| `npm run plugins:dev` | 插件开发热重载 |
| `npm run lint` / `format` | ESLint / Prettier |
| `npm run target:size` | 检查 `src-tauri/target` 目录大小（超过 15GB 建议 `target:clean`） |

## 插件系统

桌面端插件基于 **wasmtime 运行时（WASM Component Model）**：插件由 Rust / TypeScript 编译为 WASM 组件，在宿主内沙箱加载运行，同时兼容 cdylib 动态库插件。插件可观察和扩展宿主会话行为：

- **WASM 沙箱运行时** — 资源受限、内存隔离，插件崩溃不影响宿主
- **动态加载** — 扫描 `plugins/desktop/{plugin-id}/plugin.json` 运行时加载，无需重编译宿主
- **宿主 API 桥接** — 版本化 API 访问宿主功能（发送输入、读取输出、会话信息），统一权限校验
- **权限控制** — 插件声明所需权限，宿主强制执行访问边界
- **Hooks 集成** — 会话启动自动配置项目级 hooks，经 HTTP API 推送任务状态（idle / in_progress / asking / completed / interrupted）
- **会话 ID 绑定** — PTY 注入 `BEDCODE_SESSION_ID`，绑定 Agent CLI 会话与 BedCode 会话

### 官方插件

| 插件 | 版本 | 说明 |
|------|------|------|
| **AI Chatbox** | 1.0.0-beta | AI 大模型对话：接入任意 OpenAI 兼容供应商，流式对话、多会话管理 |
| **Auto Task** | 1.0.0-beta | Agent 任务队列与自动授权：同步任务状态、队列调度、预设任务、定时任务 |
| **File Transfer** | 1.0.0-beta | 内网文件传输：对端发现、远程目录浏览、多任务并发传输（断点续传 / 失败重试） |
| **Scheduler** | 1.0.0-beta | 通用调度框架：cron 表达式触发 shell 脚本 / 内联命令，执行记录可审计 |

开发自己的插件：使用 [`@binblink/plugin-sdk-desktop`](packages/plugin-sdk-desktop/README.md)（TS SDK + Rust `bedcode-plugin-api` crate），完整指南见 [plugin-dev-desktop.md](plugin-dev-desktop.md)。

## 相关文档

- 根仓库 [README](../README.md) — 项目总览与安全模型
- [plugin-dev-desktop.md](plugin-dev-desktop.md) — 桌面端插件开发指南
- [docs/code-map.md](docs/code-map.md) — 代码结构索引（含 scripts 说明）
- [scripts/README.md](scripts/README.md) — 构建脚本说明

## 许可证

MIT - 详见 [LICENSE](../LICENSE)。
