<div align="center">

<img src="public/favicon.svg" width="96" alt="BedCode Desktop logo">

# BedCode Desktop

**BedCode 的桌面端主机** — 在 Windows / Linux 上运行多个 Agent CLI / 终端会话（Claude Code、pi、opencode、Codex 等），供手机在同一 WiFi 下远程接管。

[![Version](https://img.shields.io/badge/version-3.0.0-blue.svg)](https://github.com/7ZAI/BedCode)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](../LICENSE)
[![Tauri](https://img.shields.io/badge/Tauri-2.0-orange.svg)](https://v2.tauri.app/)
[![Wasmtime](https://img.shields.io/badge/wasmtime-48-%232F6FED.svg)](https://wasmtime.dev/)
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
- **插件系统** — WASM（wasmtime 沙箱，WASM Component Model），权限控制、hooks 集成、会话 ID 绑定
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
| 插件   | wasmtime 48（WASM 组件运行时，LTS；WIT 契约 `bedcode:plugin@0.2.0`） |
| 其他   | shiki、ECharts、qrcode、vue-i18n@9、tracing 日志            |

## 目录结构

```
bedcode-desktop/                        # 桌面端项目（Tauri 2 + Vue 3）
├── scripts/                            # 构建/开发脚本（dev 编排、插件构建与热重载、产物检查）
├── packages/                           # 插件契约与夹具 crate（机制/能力域/引擎 crate 已迁仓库根 packages/）
│   ├── plugin-sdk-desktop/             # 插件开发 SDK：Rust（bedcode-plugin-api + 宏）+ TS 双侧、模板、脚手架、dev-shell
│   ├── plugin-sdk-fixtures/            # 宿主 wasm 闭环夹具
│   ├── plugin-system-test/             # 覆盖全部宿主调用路径的系统形态测试插件
│   ├── plugin-p3-async-host-import-test/ # WASI p3 async host-import 测试插件
│   ├── plugin-bench-test/              # 性能基准夹具
│   └── plugin-wasi-test/               # WASI preopen 测试插件（wasm32-wasip2）
├── wasm-apps/                          # wasm 应用（业务主场；各独立 package：plugin.json + rust/ + src/ + 独立构建）
│   ├── agent-hub/                      # Agent CLI 管理台（环境 / 技能 / 供应商 / 用量）
│   ├── ai-chatbox/                     # 多供应商 OpenAI 兼容聊天
│   ├── file-transfer/                  # 文件传输（双端单产物；业务核在 packages/bedcode-file-transfer-core）
│   └── terminal-session/               # 终端会话中心（会话真源 / 配对认证 / HTTP 端点 / 自动化任务）
├── src/                                # Vue 3 前端
│   ├── components/ composables/        # UI 组件；业务 composable（桌面命令聚合 / 插件管理 / 窗口编排）
│   ├── stores/ views/                  # Pinia 状态；页面（终端 / 设备 / 会话 / 插件列表·详情·配置 / 设置）
│   ├── plugin/                         # 前端插件系统（加载器 / 注册表 / 权限 / 上下文 / 事件 / 共享模块运行时 / UI 宿主）
│   ├── shell/                          # 桌面端壳层
│   ├── utils/ locales/ router/ dev/    # 工具 / i18n（zh-CN + en）/ 路由 / 开发调试资源
│   └── __tests__/                      # 前端测试
├── src-tauri/                          # Rust 后端（Tokio）
│   ├── resources/                      # 打包资源（config.properties + 内置 wasm 应用构建产物）
│   └── src/                            # 按领域扁平组织；plugin/<domain>.rs = 宿主插件能力面（bindgen + 各域 Host impl）
│       ├── commands.rs                 # Tauri 命令层（宿主直调外壳；业务面归插件命令面）
│       ├── plugin/                     # 端差异域的 WIT 绑定 + Host impl（fs / platform / notify / peer / ws / auth 等）
│       ├── server/                     # 服务器宿主壳（composition.rs 组合根 + 端口装配 + 边界锁）
│       ├── system/                     # DI / 错误 / 生命周期 / 日志；config/opener/process 为 pub use 垫片
│       ├── utils/                      # auth/（裁决桥接真源）+ session_gateway.rs（会话唯一收口）
│       ├── lib.rs                      # 库入口（装配；wasm_core/db/enums 为 pub use 垫片）
│       └── main.rs                     # 二进制入口（panic hook）
├── e2e/                                # E2E 测试（wdio）
└── docs/                               # 项目文档（code-map.md、linux-build.md 等）
```

> 仓库根 `packages/bedcode-*`：机制内核 `bedcode-host-kit`、插件机制整核 `bedcode-wasm-core`（单一 crate 单形态，
> WASM 运行时 / 存储 / 数据库 / 授权与 `core.wit` 真源均在此）、能力域与传输面引擎
> （server-* / crypto / discovery / pty / ws-client / notify / peer-net / link-crypto / host-api-core /
> file-transfer-core / headless-host-probe）。索引见 [`../packages/README.md`](../packages/README.md)。

## 快速开始

### 环境要求

- [Node.js](https://nodejs.org/) ≥ 18、[Rust](https://www.rust-lang.org/tools/install) ≥ 1.95（wasmtime 48 MSRV）
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
| `pnpm run plugins:build`        | 构建全部内置 wasm 应用（wasm + 前端产物）                          |
| `pnpm run plugins:dev`          | 插件开发热重载                                                    |
| `pnpm run lint` / `format`      | ESLint / Prettier                                                 |
| `pnpm run target:size`          | 检查 `src-tauri/target` 目录大小（超过 15GB 建议 `target:clean`） |

## 插件系统

桌面端插件基于 **wasmtime 48 运行时（WASM Component Model）**：应用由 Rust / TypeScript 编译为 WASM 组件，
在宿主内沙箱加载运行，契约由 WIT 定义（`bedcode:plugin@0.2.0`）。应用可观察和扩展宿主会话行为：

- **WASM 沙箱运行时** — 资源受限、内存隔离，应用崩溃不影响宿主
- **动态加载** — 运行时扫描 `resources/plugins/desktop/<id>/plugin.json` 加载，无需重编译宿主
- **宿主能力桥接** — 经 WIT ABI 与 `host-*` 原语访问引擎能力（进程/网络/存储/安全/通信），
  按功能域分片，双端共享的机制落在仓库根 `packages/bedcode-*`，端差异域落各自宿主 `src-tauri/src/plugin/`，
  统一权限校验
- **插件互调** — 对外可调 API 须在 manifest `api` 字段声明，经 JSON-RPC 2.0 跨插件调用；消息总线按 topic 发布/订阅
- **权限控制** — 应用声明所需权限，前端快速失败 + 宿主最终仲裁；文件系统走三层校验（路径白名单 → 应用白名单 → 弹窗授权）
- **Hooks 集成** — 会话启动自动配置项目级 hooks，经 HTTP API 推送任务状态（idle / in_progress / asking / completed / interrupted）
- **会话 ID 绑定** — 终端会话应用经 `host-pty.spawn` 注入 `BEDCODE_SESSION_ID`（argv / env 均由应用自算，引擎不包装），绑定 Agent CLI 会话与 BedCode 会话

### 内置 wasm 应用

| 应用                    | 版本       | 说明                                                                        |
| ----------------------- | ---------- | --------------------------------------------------------------------------- |
| **AI Chatbox**          | 1.0.0-beta | AI 大模型对话：接入任意 OpenAI 兼容供应商（OpenAI / Anthropic / DeepSeek / 通义千问），流式对话、多会话管理、JSONL 对话日志落盘 |
| **Terminal Session**    | 1.0.0-beta | 终端会话中心：Agent 任务队列与自动授权（适配 Claude Code / pi / opencode / Codex）、任务状态同步、队列调度、预设任务、定时任务、配对与信任、会话生命周期编排——会话事实真源 |
| **File Transfer**       | 1.0.0-beta | 内网文件传输（基于对等网络直连）：在线对端发现与切换、远程目录浏览、多任务并发传输（暂停 / 恢复 / 断点续传 / 失败重试）、本地目录挂载供对端访问 |
| **Agent Hub**           | 0.1.0      | Agent CLI 统一管理台：环境检测与一键安装 / 卸载、Skills 管理、供应商统一配置、使用统计与会话日志解析 |

开发自己的 wasm 应用：使用 [`@binblink/bedcode-plugin-sdk-desktop`](packages/plugin-sdk-desktop/README.md)（TS SDK + Rust `bedcode-plugin-api` crate），完整指南见 [wasm-app-dev-desktop.md](wasm-app-dev-desktop.md)。

## 相关文档

- 根仓库 [README](../README.md) — 项目总览与安全模型
- [wasm-app-dev-desktop.md](wasm-app-dev-desktop.md) — 桌面端 wasm 应用开发指南（从零创建到真机验证）
- [docs/knowledge/wasm-app-dev-guide.md](docs/knowledge/wasm-app-dev-guide.md) — 插件清单 / PluginContext API / 扩展点参考
- [`../docs/knowledge/wasm-app-development-checklist.md`](../docs/knowledge/wasm-app-development-checklist.md) — 开发红线（权限 / 存储 / 事件 / 认证中心 / HTTP 面）
- [docs/code-map.md](docs/code-map.md) — 代码结构索引（含 scripts 说明）
- [docs/linux-build.md](docs/linux-build.md) — Linux 构建依赖与交叉编译
- [scripts/README.md](scripts/README.md) — 构建脚本说明

## 许可证

MIT - 详见 [LICENSE](../LICENSE)。
