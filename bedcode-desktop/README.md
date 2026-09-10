<div align="center">

<img src="public/favicon.svg" width="96" alt="BedCode Desktop logo">

# BedCode Desktop

**BedCode 的桌面端主机** — 在 Windows / Linux 上运行多个 Agent CLI / 终端会话（Claude Code、pi、opencode、Codex 等），供手机在同一 WiFi 下远程接管。

[![Version](https://img.shields.io/badge/version-2.1.0-blue.svg)](https://github.com/7ZAI/BedCode)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](../LICENSE)
[![Tauri](https://img.shields.io/badge/Tauri-2.0-orange.svg)](https://v2.tauri.app/)
[![Wasmtime](https://img.shields.io/badge/wasmtime-47-%232F6FED.svg)](https://wasmtime.dev/)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20Linux-lightgrey.svg)](https://github.com/7ZAI/BedCode)

[English](README_en.md) | 简体中文

</div>

本仓库是 BedCode 单仓库中的桌面端项目（Tauri 2.0 + Vue 3 + Rust）。移动端项目见 [`bedcode-mobile/`](../bedcode-mobile/)，整体介绍见根目录 [README](../README.md)。

## 功能特性

- **会话管理** — 同时运行多个 Agent CLI / 终端会话，SQLite 持久化会话配置，xterm.js 实时输出预览
- **PTY 终端** — 原生伪终端运行任意命令行程序（含 TUI 应用），支持 WSL2 发行版，启动时注入 `BEDCODE_SESSION_ID` 绑定 Agent 会话
- **设备配对与发现** — 二维码 + 6 位验证码安全配对，mDNS 服务广播供移动端自动发现，生物凭证绑定公钥
- **HTTP + WebSocket 服务器** — Actix Web 实现终端双向流与 REST API（插件 hooks、文件服务），支持高级网络配置（worker 线程、Keep-Alive、超时、帧大小限制）与独立指标仪表盘
- **链路加密** — X25519 身份密钥 + HKDF 方向分离派生，HTTP 信封协议与 WS 双 ECDH 握手帧加密（opt-in，设置页开关）
- **对等网络（P2P）** — 节点身份（Ed25519）+ 自签证书「指纹即身份」、TLS 1.3 mTLS 直连、信任存储与首连确认闸门、专用 `_bedcode-peer` mDNS 发现；承载共享目录浏览与批量断点续传
- **插件系统** — WASM（wasmtime 沙箱，WASM Component Model）+ cdylib 动态加载，权限控制、hooks 集成、会话 ID 绑定
- **系统托盘** — 后台常驻与快捷操作
- **自动更新** — Tauri updater（正式版由 GitHub Actions 签名发布）
- **国际化** — vue-i18n（zh-CN / en）完整支持

## 技术栈

| 分类   | 技术                                                        |
| ------ | ----------------------------------------------------------- |
| 框架   | Tauri 2.0（Windows / Linux），Node.js + Rust               |
| 前端   | Vue 3 + TypeScript + Vite + TailwindCSS                     |
| 状态   | Pinia + vue-router                                          |
| 后端   | Rust（Tokio）、Actix Web 4 + tokio-tungstenite              |
| 数据库 | SQLite（rusqlite）                                          |
| 终端   | @xterm/xterm + addon-fit / unicode11 / web-links / webgl    |
| 认证   | JWT（HS256）、ECDSA 生物凭证（p256）、设备指纹、6 位配对码 |
| 加密   | X25519 ECDH + AES-256-GCM（HKDF）、ChaCha20-Poly1305、RSA-OAEP/PSS |
| 插件   | wasmtime 47（WASM 组件运行时）+ cdylib 动态加载             |
| 其他   | shiki、ECharts、qrcode、vue-i18n@9、tracing 日志            |

## 目录结构

```
bedcode-desktop/
├── src/                    # Vue 3 前端（扁平化）
│   ├── components/         # UI 组件（TitleBar、Sidebar、TerminalPreview、设备/会话/插件等）
│   ├── composables/        # 业务逻辑（useServer、usePairing、usePluginManager、useWsl 等）
│   ├── stores/             # Pinia store（session、device、settings、wsl、inputAssistant 等）
│   ├── views/              # 页面（终端窗口、会话配置、服务器、插件列表/详情/配置、设备、设置等）
│   ├── plugin/             # 前端插件系统：加载器、注册表、权限、共享模块运行时
│   ├── utils/  router/     # Tauri invoke 封装 / 路由
│   ├── dev/                # 开发调试资源（终端 mock、PTY 输出 dump）
│   ├── locales/            # i18n（zh-CN / en）
│   └── __tests__/          # 前端测试
├── src-tauri/              # Rust 后端
│   ├── resources/          # 打包资源（config.properties + 内置插件构建产物）
│   └── src/                # 按领域扁平组织，每领域配同名入口文件
│       ├── commands/       # Tauri invoke 命令层（devices、mdns、plugin、pty_input、qr 等）
│       ├── pty/            # 伪终端与进程管理（输出读取/缓存、WSL 支持）
│       ├── server/         # Actix Web HTTP + WS：controllers、dtos、middleware、ws、
│       │                   #   filter（流量过滤器链）、link_crypto（链路报文加密）、metrics、
│       │                   #   supervisor（服务器生命周期）
│       ├── session/        # 会话模型与生命周期、输出管理（缓存/队列/订阅回放）、输入扩展点
│       ├── db/             # SQLite 持久化
│       ├── plugin/         # 插件宿主：wasmtime 运行时（host_impl 按功能域拆分）+ cdylib 加载、
│       │                   #   权限审批、消息总线、fs_auth 三层校验
│       ├── mdns/           # mDNS 服务广播
│       ├── events/         # 全局事件系统：AppEvent + 前端转发 + WebSocket 同步广播
│       ├── system/         # 应用上下文 (DI)、配置、错误类型、生命周期钩子、常量分组
│       ├── utils/          # auth（JWT/配对/QR Token）、crypto、parser（ANSI/Markdown）
│       ├── enums/          # 枚举类型（认证、控制、插件、PTY 状态、会话、特殊键等）
│       ├── peer_net.rs     # 对等网络接入（节点身份、信任闸门、节点生命周期）
│       ├── peer_receive.rs # 入站连接处理与事件桥接
│       ├── peer_remote.rs  # 远程节点访问（共享目录浏览等）
│       ├── peer_transfer.rs# 共享目录批量断点续传
│       └── peer_migration.rs# 旧文件服务 → 对等网络迁移
├── packages/
│   ├── plugin-sdk-desktop/ # 插件开发 SDK（TS + Rust，含 dev-shell、模板、脚手架 CLI）
│   ├── plugin-test/        # 覆盖全部宿主调用路径的测试用 WASM 插件
│   ├── plugin-sdk-test/    # SDK 接口测试用插件
│   └── plugin-wasi-test/   # WASI preopen 测试插件（wasm32-wasip2）
├── plugins/                # 官方插件（ai-chatbox / auto-task / file-transfer）
├── e2e/                    # E2E 测试（wdio）
├── scripts/                # 开发构建脚本（含 README）
└── docs/                   # 项目文档（code-map.md、linux-build.md 等）
```

## 快速开始

### 环境要求

- [Node.js](https://nodejs.org/) ≥ 18、[Rust](https://www.rust-lang.org/tools/install) ≥ 1.94（wasmtime 47 MSRV）
- [Tauri 2.0 CLI](https://v2.tauri.app/start/prerequisites/) 及平台依赖
  （Linux 系统依赖见 [docs/linux-build.md](docs/linux-build.md)）
- 已安装并配置 Agent CLI（如 [Claude Code](https://claude.ai/code)、pi、opencode、Codex）

### 安装与运行

```bash
pnpm install

# 开发模式（自动构建插件 + 启动 Tauri dev）
pnpm run tauri:dev

# 生产构建（tauri-build.js 自动解析 updater 签名密钥，
# 未配置密钥时自动禁用升级包生成，本地构建无需私钥；产物：Windows NSIS + Linux DEB）
pnpm run tauri:build
```

### 测试

```bash
pnpm run test:run                  # 前端单元测试（vitest run，注意不要用 pnpm run test 的 watch 模式）
pnpm run test:e2e                  # E2E 测试（wdio）
cd src-tauri && cargo test         # Rust 测试
```

### 其他常用脚本

| 命令                            | 说明                                                              |
| ------------------------------- | ----------------------------------------------------------------- |
| `pnpm run build` / `build:fast` | 前端类型检查 + 构建 / 仅构建                                      |
| `pnpm run plugins:build`        | 构建全部官方插件（wasm + 前端产物）                               |
| `pnpm run plugins:dev`          | 插件开发热重载                                                    |
| `pnpm run lint` / `format`      | ESLint / Prettier                                                 |
| `pnpm run target:size`          | 检查 `src-tauri/target` 目录大小（超过 15GB 建议 `target:clean`） |

## 插件系统

桌面端插件基于 **wasmtime 47 运行时（WASM Component Model）**：插件由 Rust / TypeScript 编译为 WASM 组件，在宿主内沙箱加载运行，同时兼容 cdylib 动态库插件。插件可观察和扩展宿主会话行为：

- **WASM 沙箱运行时** — 资源受限、内存隔离，插件崩溃不影响宿主
- **动态加载** — 运行时扫描 `plugins/{plugin-id}/plugin.json` 加载，无需重编译宿主
- **宿主能力桥接** — 经 wit ABI 与 `host-*` 原语访问引擎能力（进程/网络/存储/安全/通信），按功能域实现于 `host_impl/`，统一权限校验
- **插件互调** — 对外可调 API 须在 manifest `api` 字段声明，经 JSON-RPC 2.0 跨插件调用；消息总线按 topic 发布/订阅
- **权限控制** — 插件声明所需权限，前端快速失败 + 宿主最终仲裁；文件系统走三层校验（路径白名单 → 插件白名单 → 弹窗授权）
- **Hooks 集成** — 会话启动自动配置项目级 hooks，经 HTTP API 推送任务状态（idle / in_progress / asking / completed / interrupted）
- **会话 ID 绑定** — PTY 注入 `BEDCODE_SESSION_ID`，绑定 Agent CLI 会话与 BedCode 会话
- **生命周期/输入扩展点** — 插件经 `SessionLifecycleListener`、`SessionInputListener` 介入会话创建与输入提交

### 官方插件

| 插件              | 版本       | 说明                                                                        |
| ----------------- | ---------- | --------------------------------------------------------------------------- |
| **AI Chatbox**    | 1.0.0-beta | AI 大模型对话：接入任意 OpenAI 兼容供应商（OpenAI / Anthropic / DeepSeek / 通义千问），流式对话、多会话管理、JSONL 对话日志落盘 |
| **Auto Task**     | 1.0.0-beta | Agent 任务队列与自动授权：适配 Claude Code / pi / opencode / Codex，任务状态同步、队列调度、预设任务、定时任务、自动应答 |
| **File Transfer** | 1.0.0-beta | 内网文件传输（基于对等网络直连）：在线对端发现与切换、远程目录浏览、多任务并发传输（暂停 / 恢复 / 断点续传 / 失败重试）、本地目录挂载供对端访问 |

开发自己的插件：使用 [`@binblink/bedcode-plugin-sdk-desktop`](packages/plugin-sdk-desktop/README.md)（TS SDK + Rust `bedcode-plugin-api` crate），完整指南见 [plugin-dev-desktop.md](plugin-dev-desktop.md)。

## 相关文档

- 根仓库 [README](../README.md) — 项目总览与安全模型
- [plugin-dev-desktop.md](plugin-dev-desktop.md) — 桌面端插件开发指南
- [docs/code-map.md](docs/code-map.md) — 代码结构索引（含 scripts 说明）
- [docs/linux-build.md](docs/linux-build.md) — Linux 构建依赖与交叉编译
- [scripts/README.md](scripts/README.md) — 构建脚本说明

## 许可证

MIT - 详见 [LICENSE](../LICENSE)。
