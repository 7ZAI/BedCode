<div align="center">

# BedCode Mobile

**BedCode 的移动端远程终端** — 手机变成带优化触控界面的远程终端，在同一 WiFi 下随时接管桌面端运行的 Agent CLI / 终端会话（Claude Code、opencode 等）。躺床上也能编程。

[![Version](https://img.shields.io/badge/version-2.0.0-blue.svg)](https://github.com/7ZAI/BedCode)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](../LICENSE)
[![Tauri](https://img.shields.io/badge/Tauri-2.0-orange.svg)](https://v2.tauri.app/)
[![Platform](https://img.shields.io/badge/platform-Android-lightgrey.svg)](https://github.com/7ZAI/BedCode)

[English](README_en.md) | 简体中文

</div>

本仓库是 BedCode 单仓库中的移动端项目（Tauri 2.0 + Vue 3 + Rust，Android）。桌面端主机项目见 [`bedcode-desktop/`](../bedcode-desktop/)，整体介绍见根目录 [README](../README.md)。

## 功能特性

- **设备发现与配对** — mDNS 自动发现桌面端，扫码或输入配对码安全连接
- **终端输出** — 增强模式（ANSI / Markdown 解析）与原始模式切换，TUI 兼容滚动
- **智能输入栏** — 特殊按键（Tab、Ctrl+C、Esc、方向键）、输入助手、快捷键配置
- **代码浏览器** — 远程项目文件浏览、语法高亮（shiki）、Git diff 渲染
- **预设任务** — 任务卡片，类型标签、编辑、一键执行
- **工具箱** — 快捷操作面板，自定义命令
- **任务通知** — 按会话任务状态推送系统通知，前台服务保活并保持屏幕唤醒
- **自动重连** — 意外断开自动重连，边到边全屏显示（含刘海 / 手势条安全区适配）
- **生物认证** — 生物凭证绑定公钥，挑战-应答验签后签发会话凭证（防重放）
- **插件系统** — 与桌面端一致的插件架构，支持 SAF 存储访问、系统返回键等移动端专属能力
- **国际化** — vue-i18n（zh-CN / en）完整支持

## 技术栈

| 分类 | 技术 |
|------|------|
| 框架 | Tauri 2.0（Android），Node.js + Rust |
| 前端 | Vue 3 + TypeScript + Vite + TailwindCSS |
| 状态 | Pinia + vue-router |
| 后端 | Rust（Tokio）、tokio-tungstenite（WS 客户端） |
| 终端 | @xterm/xterm + addon-fit / unicode11 / web-links / webgl |
| 认证 | JWT（HS256）、ECDSA 生物凭证（p256）、设备指纹 |
| 文件 | SAF（Storage Access Framework）目录树遍历与中转复制 |
| 插件 | WASM 组件（wasmtime）运行时 |
| 其他 | shiki、html5-qrcode、marked、vue-i18n@9、tracing 日志（logcat） |

## 目录结构

```
bedcode-mobile/
├── src/                    # Vue 3 前端
│   ├── components/         # UI 组件（TerminalView、MobileNav、InputBar 等）
│   ├── composables/        # 业务逻辑（useMobileConnection、usePresetTasks、useFileTree 等）
│   ├── stores/             # Pinia store（settings、terminalBuffer、codeViewer 等）
│   ├── views/              # 页面（设备发现、扫描、会话、终端、代码浏览器、工具箱、预设任务等）
│   ├── plugin/             # 前端插件加载器与权限映射
│   └── locales/            # i18n（zh-CN / en）
├── src-tauri/              # Rust 后端
│   ├── src/
│   │   ├── connection/     # WebSocket 连接与路由
│   │   ├── auth/           # 配对与生物认证
│   │   ├── file_service/   # 文件服务与 SAF 访问
│   │   ├── plugin/         # 插件宿主：wasmtime 运行时
│   │   ├── mdns/           # mDNS 服务发现
│   │   ├── session/        # 会话模型
│   │   └── commands/       # Tauri commands
│   └── gen/android/        # Android 工程（含自定义 Kotlin 插件：前台服务、SAF、生物识别等）
├── packages/
│   └── plugin-sdk-mobile/  # 插件开发 SDK（TS + Rust），见其 README
├── plugins/                # 官方插件（ai-chatbox / auto-task / file-transfer）
├── scripts/                # 开发构建脚本
└── docs/                   # 项目文档（code-map.md 等）
```

## 快速开始

### 环境要求

- [Node.js](https://nodejs.org/) >= 18、[Rust](https://www.rust-lang.org/tools/install) >= 1.70
- [Tauri 2.0 CLI](https://v2.tauri.app/start/prerequisites/) 及 Android SDK / NDK 环境
- 一台电脑运行 [BedCode Desktop](../bedcode-desktop/) 作为主机

### 安装与运行

```bash
npm install

# 开发模式：构建并安装到 Android 设备（真机或模拟器）
npm run tauri:android:dev

# 开发模式 + 电脑端日志落盘（logcat 同时写入 .dev-logs/，便于排查）
npm run tauri:android:dev:log

# 生产构建（aarch64）
npm run tauri:android:build
```

> [!NOTE]
> `gen/android` 重建后需恢复自定义 Kotlin 文件（前台服务、SAF、生物识别、下载目录等插件）与 AndroidManifest.xml、签名密钥等，详见根仓库 [AGENTS.md](../AGENTS.md) 的 Android 一节。

### 测试

```bash
npm run test:run            # 前端单元测试（vitest run，注意不要用 npm run test 的 watch 模式）
cd src-tauri && cargo test  # Rust 测试
```

> 修改 `src-tauri/gen/android/` 下自定义 Kotlin 插件后，必须额外运行
> `./gradlew :app:compileUniversalDebugKotlin`（在 `src-tauri/gen/android/` 目录）验证编译，
> `cargo test` / 前端测试无法覆盖 Kotlin 代码。

### 其他常用脚本

| 命令 | 说明 |
|------|------|
| `npm run build` / `build:fast` | 前端类型检查 + 构建 / 仅构建 |
| `npm run tauri:android:build:fast` | 跳过类型检查的 debug 快速构建 |
| `npm run plugins:build` | 构建官方插件（WASM 产物） |
| `npm run target:size` | 检查 `src-tauri/target` 目录大小（超过 15GB 建议 `target:clean`） |

## 插件系统

移动端插件与桌面端共享同一套插件架构（WASM Component Model + 权限控制），并额外封装移动端专属能力：**SAF 存储访问**、对话框 / 系统通知、**动态路由**、生命周期钩子、Android 系统返回键接管，以及 dev-shell 演示数据协议（浏览器 HMR 开发环境）。

开发自己的插件：使用 [`@binblink/plugin-sdk-mobile`](packages/plugin-sdk-mobile/README.md)（TS SDK + Rust `bedcode-plugin-api-mobile` crate），完整指南见 [plugin-dev-mobile.md](plugin-dev-mobile.md)。

### 官方插件

| 插件 | 版本 | 说明 |
|------|------|------|
| **AI Chatbox** | 1.0.0-beta | AI 大模型对话：接入任意 OpenAI 兼容供应商，流式对话、多会话管理 |
| **Auto Task** | 1.0.0-beta | Agent 任务队列与自动授权：任务状态同步、队列调度、预设任务、定时任务 |
| **File Transfer** | 1.0.0-beta | 内网文件传输：在线对端发现、远程目录浏览、多任务并发传输（断点续传 / 失败重试） |

## 相关文档

- 根仓库 [README](../README.md) — 项目总览与安全模型
- [plugin-dev-mobile.md](plugin-dev-mobile.md) — 移动端插件开发指南
- [docs/code-map.md](docs/code-map.md) — 代码结构索引

## 许可证

MIT - 详见 [LICENSE](../LICENSE)。
