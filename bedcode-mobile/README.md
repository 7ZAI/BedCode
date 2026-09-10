<div align="center">

# BedCode Mobile

**BedCode 的移动端远程终端** — 手机变成带优化触控界面的远程终端，在同一 WiFi 下随时接管桌面端运行的 Agent CLI / 终端会话（Claude Code、pi、opencode、Codex 等）。躺床上也能编程。

[![Version](https://img.shields.io/badge/version-2.1.0-blue.svg)](https://github.com/7ZAI/BedCode)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](../LICENSE)
[![Tauri](https://img.shields.io/badge/Tauri-2.0-orange.svg)](https://v2.tauri.app/)
[![Wasmtime](https://img.shields.io/badge/wasmtime-47-%232F6FED.svg)](https://wasmtime.dev/)
[![Platform](https://img.shields.io/badge/platform-Android-lightgrey.svg)](https://github.com/7ZAI/BedCode)

[English](README_en.md) | 简体中文

</div>

本仓库是 BedCode 单仓库中的移动端项目（Tauri 2.0 + Vue 3 + Rust，Android）。桌面端主机项目见 [`bedcode-desktop/`](../bedcode-desktop/)，整体介绍见根目录 [README](../README.md)。

## 功能特性

- **设备发现与配对** — mDNS 自动发现桌面端，扫码或输入配对码安全连接，连接历史记录一键重连
- **终端输出** — 增强模式（ANSI / Markdown 解析）与原始模式切换，TUI 兼容滚动
- **智能输入栏** — 特殊按键（Tab、Ctrl+C、Esc、方向键）、输入助手、快捷键配置
- **代码浏览器** — 远程项目文件浏览、语法高亮（shiki）、Git diff 渲染与分支切换
- **预设任务** — 任务卡片，类型标签、编辑、一键执行
- **工具箱** — 快捷操作面板，自定义命令
- **任务通知** — 按会话任务状态推送系统通知，前台服务保活并保持屏幕唤醒
- **自动重连** — 意外断开自动重连，边到边全屏显示（含刘海 / 手势条安全区适配）
- **生物认证** — 生物凭证绑定公钥，挑战-应答验签后签发会话凭证（防重放）
- **链路加密** — 与桌面端配对的 X25519 + HKDF 加密链路（HTTP 信封 + WS 帧加密，opt-in）
- **对等网络（P2P）** — 节点身份（Ed25519）+ 自签证书、TLS 1.3 mTLS 直连、信任存储与首连确认闸门、专用 `_bedcode-peer` mDNS 发现；承载共享目录浏览与批量断点续传
- **插件系统** — 与桌面端一致的插件架构，支持 SAF 存储访问、动态路由、系统返回键等移动端专属能力
- **国际化** — vue-i18n（zh-CN / en）完整支持

## 技术栈

| 分类 | 技术                                                            |
| ---- | --------------------------------------------------------------- |
| 框架 | Tauri 2.0（Android），Node.js + Rust                            |
| 前端 | Vue 3 + TypeScript + Vite + TailwindCSS                         |
| 状态 | Pinia + vue-router                                              |
| 后端 | Rust（Tokio）、tokio-tungstenite（WS 客户端）                    |
| 终端 | @xterm/xterm + addon-fit / unicode11 / web-links / webgl        |
| 认证 | JWT（HS256）、ECDSA 生物凭证（p256）、设备指纹                  |
| 加密 | X25519 ECDH + AES-256-GCM（HKDF）、ChaCha20-Poly1305            |
| 文件 | SAF（Storage Access Framework）目录树遍历与中转复制             |
| 插件 | wasmtime 47（WASM 组件运行时）                                  |
| 其他 | shiki、html5-qrcode、marked、vue-i18n@9、tracing 日志（logcat） |

## 目录结构

```
bedcode-mobile/
├── src/                    # Vue 3 前端（扁平化）
│   ├── components/         # UI 组件（TerminalView、MobileNav、InputBar、设备卡片、文件浏览等）
│   ├── composables/        # 业务逻辑（useMobileConnection、usePresetTasks、useFileTree、
│   │                       #   useTerminalBuffer、useEdgeToEdge、useForegroundService 等）
│   ├── stores/             # Pinia store（settings、terminalBuffer、codeViewer、inputAssistant 等）
│   ├── views/              # 页面（终端、代码浏览器、设备、mDNS 发现、扫码、会话、工具箱、预设任务、
│   │                       #   插件；views/settings/ 按领域拆分子页：外观/连接/认证/通知/关于）
│   ├── plugin/             # 前端插件系统：加载器、注册表、权限、共享模块运行时、对话框宿主
│   ├── services/           # 跨端复用服务（链路加密客户端）
│   ├── config/  styles/    # 终端主题定义 / 全局样式（mobile.css + terminal.css）
│   ├── assets/  utils/  router/  locales/  # 静态资源 / 工具 / 路由 / i18n（zh-CN / en）
│   └── __tests__/          # 前端测试（含 fixtures 与 integration）
├── src-tauri/              # Rust 后端
│   ├── resources/          # 打包资源（config.json + 内置插件产物）
│   ├── src/                # 按领域扁平组织，每领域配同名入口文件
│   │   ├── connection/     # WebSocket 客户端：编解码、请求-响应关联、心跳、重连、消息路由
│   │   ├── handler/        # WS 消息处理器（认证、同步、系统、终端）
│   │   ├── router/         # 消息路由（路由上下文、事件、注册表）
│   │   ├── auth/           # 认证状态与配对流程
│   │   ├── file_service/   # 文件服务：SAF 目录树读取
│   │   ├── plugin/         # 插件宿主：wasmtime 运行时（host_impl 按功能域拆分）、
│   │   │                   #   APK assets 解压、downloader、saf_io、android_plugins 原生桥
│   │   ├── mdns/           # mDNS 服务发现与广播
│   │   ├── model/  enums/  # 数据模型（API DTO、WS 消息）/ 枚举类型
│   │   ├── system/         # 共享命令、配置管理、常量分组、统一错误类型、JSON 设置持久化
│   │   ├── session.rs      # 远程会话管理
│   │   ├── state.rs        # 全局状态管理（单例管理器 + Token 存储）
│   │   ├── peer_net.rs     # 对等网络接入（节点身份、信任闸门、节点生命周期）
│   │   ├── peer_receive.rs # 入站连接处理与事件桥接
│   │   ├── peer_remote.rs  # 远程节点访问（共享目录浏览等）
│   │   ├── peer_transfer.rs# 共享目录批量断点续传
│   │   └── peer_migration.rs# 旧文件服务 → 对等网络迁移
│   └── gen/android/        # Android 工程（含自定义 Kotlin 插件：前台服务、SAF、生物识别、
│                           #   下载目录、全部文件访问、组播锁、通知等）
├── packages/
│   ├── plugin-sdk-mobile/  # 插件开发 SDK（TS + Rust，含 dev-shell 与模板）
│   └── plugin-component-test/ # 测试用 WASM 插件 crate
├── plugins/                # 官方插件（ai-chatbox / auto-task / file-transfer）
├── scripts/                # 开发构建脚本（Android dev 日志落盘、插件构建等）
└── docs/                   # 项目文档（code-map.md 等）
```

## 快速开始

### 环境要求

- [Node.js](https://nodejs.org/) ≥ 18、[Rust](https://www.rust-lang.org/tools/install) ≥ 1.94（wasmtime 47 MSRV）
- [Tauri 2.0 CLI](https://v2.tauri.app/start/prerequisites/) 及 Android SDK / NDK 环境
- 一台电脑运行 [BedCode Desktop](../bedcode-desktop/) 作为主机

### 安装与运行

```bash
pnpm install

# 开发模式：构建并安装到 Android 设备（真机或模拟器）
pnpm run tauri:android:dev

# 开发模式 + 电脑端日志落盘（logcat 同时写入 .dev-logs/，便于排查）
pnpm run tauri:android:dev:log

# 生产构建（aarch64）
pnpm run tauri:android:build

# 快速 debug 构建（跳过类型检查）/ 模拟器构建
pnpm run tauri:android:build:fast
pnpm run tauri:android:build:emulator
```

> [!NOTE]
> `gen/android` 重建后需恢复自定义 Kotlin 文件（前台服务、SAF、生物识别、下载目录等插件）与 AndroidManifest.xml、签名密钥等，详见根仓库 [AGENTS.md](../AGENTS.md) 的 Android 一节。

### 测试

```bash
pnpm run test:run                  # 前端单元测试（vitest run，注意不要用 pnpm run test 的 watch 模式）
cd src-tauri && cargo test         # Rust 测试
```

> 修改 `src-tauri/gen/android/` 下自定义 Kotlin 插件后，必须额外运行
> `./gradlew :app:compileUniversalDebugKotlin`（在 `src-tauri/gen/android/` 目录）验证编译，
> `cargo test` / 前端测试无法覆盖 Kotlin 代码。

### 其他常用脚本

| 命令                                | 说明                                                              |
| ----------------------------------- | ----------------------------------------------------------------- |
| `pnpm run build` / `build:fast`     | 前端类型检查 + 构建 / 仅构建                                      |
| `pnpm run tauri:android:build:fast` | 跳过类型检查的 debug 快速构建                                     |
| `pnpm run tauri:android:build:emulator` | 模拟器（x86_64）debug 构建                                    |
| `pnpm run plugins:build`            | 构建官方插件（WASM 产物）                                         |
| `pnpm run build:all`                | 先构建插件再构建前端                                              |
| `pnpm run target:size`              | 检查 `src-tauri/target` 目录大小（超过 15GB 建议 `target:clean`） |

## 插件系统

移动端插件与桌面端共享同一套插件架构（wasmtime 47、WASM Component Model、权限控制），并额外封装移动端专属能力：**SAF 存储访问**、对话框 / 系统通知、**动态路由**、生命周期钩子、Android 系统返回键接管，以及 dev-shell 演示数据协议（浏览器 HMR 开发环境）。宿主能力实现按功能域拆分于 `host_impl/`（storage/db/fs/http/terminal/event/bus/config/notify/peer/support），内置插件从 APK assets 解压到 app_data_dir 后扫描加载，支持远程下载 + SHA256 校验安装。

开发自己的插件：使用 [`@binblink/bedcode-plugin-sdk-mobile`](packages/plugin-sdk-mobile/README.md)（TS SDK + Rust `bedcode-plugin-api-mobile` crate），完整指南见 [plugin-dev-mobile.md](plugin-dev-mobile.md)。

### 官方插件

| 插件              | 版本       | 说明                                                                            |
| ----------------- | ---------- | ------------------------------------------------------------------------------- |
| **AI Chatbox**    | 1.0.0-beta | AI 大模型对话：接入任意 OpenAI 兼容供应商（OpenAI / Anthropic / DeepSeek / 通义千问），流式对话、多会话管理、JSONL 对话日志落盘 |
| **Auto Task**     | 1.0.0-beta | 自动任务队列：任务队列与自动执行、Agent 提问自动应答、预设任务、定时任务、任务历史与失败重试（后端逻辑在桌面端同名插件） |
| **File Transfer** | 1.0.0-beta | 内网文件传输（基于对等网络直连）：浏览对端共享目录、多选批量传输、队列暂停/恢复/取消/重试与失败原因、接收策略（逐批批准 / 自动接受 / 自动拒绝）、SAF 自选保存位置、历史记录 |

## 相关文档

- 根仓库 [README](../README.md) — 项目总览与安全模型
- [plugin-dev-mobile.md](plugin-dev-mobile.md) — 移动端插件开发指南
- [docs/code-map.md](docs/code-map.md) — 代码结构索引
- [docs/terminal-output-pipeline-optimization.md](docs/terminal-output-pipeline-optimization.md) — 终端输出链路优化记录

## 许可证

MIT - 详见 [LICENSE](../LICENSE)。
