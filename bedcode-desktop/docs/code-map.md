# BedCode Desktop Code Map

本文档作为桌面端项目代码探索的索引入口。**只记录目录层级与模块职责划分，不索引具体代码文件**——定位到目标目录后，再用 `ls` / `grep` 等工具在该目录内查找具体文件。

---

## 使用指引

**当用户命令包含以下动作时，请先阅读本文档：**

- 探索代码 / 查看代码 / 了解代码结构
- 查找文件 / 定位模块 / 寻找某个功能
- 理解架构 / 分析项目组成
- 修改某模块前需要了解上下文

**阅读流程：**

1. 先浏览 Project Structure 了解目录布局与各目录职责
2. 根据 Core Modules 深入理解核心模块的划分与协作
3. 用 Quick Navigation 按功能定位到目标目录
4. 进入目标目录后用 `ls` / `rg` 查找具体文件（文件命名遵循 AGENTS.md 规范）

---

## Project Structure

```
bedcode-desktop/                      # 桌面端项目 (Tauri 2.0 + Vue 3)
├── scripts/                          # 构建/开发工具脚本：图标生成、编译产物大小检查、插件构建与开发热重载
├── packages/                         # 共享包（供插件开发与测试使用）
│   ├── plugin-sdk-desktop/           # 插件开发工具包，Rust + TS 双侧 SDK（含 dev-shell 调试壳、
│   │   │                             #   插件模板 template/、脚手架 bin/）
│   │   ├── rust/                     # bedcode-plugin-api crate：WASM ABI 契约（单一事实来源）、
│   │   │                             #   BedcodePlugin/WasmPlugin trait、宿主能力接口（host/ 按功能域拆分）、
│   │   │                             #   权限/SQL/命令参数辅助宏；rust-macros/ 为配套过程宏 crate
│   │   └── src/                      # TS SDK：插件类型定义、共享模块运行时代理（__BEDCODE_SHARED__）、
│   │                                 #   Vite 构建插件（vue/pinia/vue-i18n 外部化）
│   ├── plugin-test/                  # 测试用 WASM 插件 crate，覆盖全部宿主调用路径，
│   │                                 #   供宿主 wasm_runtime 测试套件做签名验证与连通性测试
│   ├── plugin-sdk-test/              # SDK 接口测试用插件 crate
│   └── plugin-wasi-test/             # WASI preopen 测试插件（wasm32-wasip2，std::fs 直写预打开目录）
├── plugins/                          # 插件源码目录（每个插件独立 package：plugin.json 元数据 +
│                                     #   rust/ WASM 后端 + src/ TS 前端 + vite.config.ts 独立构建）
│   ├── ai-chatbox/                   # AI Chatbox 插件：多供应商 OpenAI 兼容客户端，
│   │                                 #   聊天 UI、供应商配置、提示词优化
│   ├── auto-task/                    # Auto Task 插件：Claude Code 任务状态同步与自动授权，
│   │                                 #   含 hooks 管理、任务状态/队列、HTTP 端点、Hook 脚本
│   ├── file-transfer/                # 文件传输插件：内网对端发现、远程目录浏览、多任务并发传输
│   │                                 #   （暂停/恢复/断点续传），本地目录挂载供对端访问
│   └── scheduler/                    # 调度器插件：cron 表达式触发执行 shell 脚本/内联命令，
│                                     #   执行记录可审计；cli/ 为独立管理 CLI（bedtask）
├── src/                              # Vue 3 前端（扁平化结构）
│   ├── components/                   # UI 组件：桌面布局、会话卡片/表单/列表、侧边栏、终端预览、
│   │                                 #   标题栏、通知卡片/徽章、退出确认、文件系统授权弹窗、通用基础组件
│   ├── composables/                  # 业务逻辑 composable：桌面命令、网络、配对、插件管理、PTY 输出、
│   │                                 #   全局终端、快捷键、主题、字体、WSL、更新检查等
│   ├── stores/                       # Pinia 全局状态：设备、会话、设置、输入助手、快捷操作、WSL、i18n
│   ├── views/                        # 页面：设备、插件、插件配置、会话管理/配置、设置、终端窗口、服务器
│   ├── plugin/                       # 前端插件系统：加载器、注册表、权限、上下文、事件、命令、
│   │                                 #   共享模块运行时；components/ 下为插件 UI 宿主组件
│   ├── utils/                        # 工具函数（Tauri invoke 封装等）
│   ├── locales/                      # 国际化（zh-CN / en，各含 common / desktop / settings）
│   ├── router/                       # 路由
│   ├── dev/                          # 开发调试资源：终端 mock、PTY 输出 dump
│   └── __tests__/                    # 前端测试（组件、composable、store、路由、视图）
└── src-tauri/                        # Rust 后端（Tokio 异步）
    ├── resources/                    # 打包资源：应用配置 + 内置插件构建产物（wasm/js/plugin.json）
    └── src/                          # 模块按领域扁平组织，每领域配同名入口文件（commands.rs、db.rs 等）
        ├── commands/                 # Tauri invoke 命令层：按领域拆分（devices、mdns、plugin、
        │                             #   pty_input、qr、server、session、session_config、settings、system、wsl）
        ├── db/                       # SQLite：连接管理、数据模型、CRUD 操作、Schema
        ├── enums/                    # 枚举类型：认证、控制、插件、PTY 状态、会话、Shell、特殊键、同步
        ├── events/                   # 全局事件系统：AppEvent trait、事件匹配、SessionManager→前端转发、
        │                             #   同步事件定义与处理（→ WebSocket 广播）
        ├── mdns/                     # mDNS 服务广播：将桌面端服务注册到局域网供移动端发现
        ├── plugin/                   # 插件系统（WASM 组件沙箱架构，核心模块，详见 Core Modules）
        ├── pty/                      # PTY 管理：进程生命周期、输出读取/缓存/监听、命令构建、WSL 支持
        ├── server/                   # Actix Web HTTP + WS 服务器（核心模块，详见 Core Modules）
        ├── session/                  # 会话管理（核心模块，详见 Core Modules）
        ├── system/                   # 系统模块：应用上下文 (DI 容器)、配置、错误类型、生命周期钩子、
        │                             #   日志格式化、休眠阻止；constants/ 下按领域分组的常量
        ├── utils/                    # 工具：auth/（JWT、配对、QR Token）、parser/（ANSI、Markdown 解析）、
        │                             #   crypto/（对称/非对称/混合加密：AES-GCM、ChaCha20-Poly1305、
        │                             #   RSA、X25519、KDF，用于 HTTP 报文与文件加密传输）
        ├── process.rs                # 进程工具（create_command）
        ├── lib.rs                    # 库入口（模块声明 + 日志初始化 + Tauri 应用搭建）
        └── main.rs                   # 二进制入口（panic hook）
```

---

## Core Modules（核心模块）

### 插件系统 — `src-tauri/src/plugin/`（WASM 组件沙箱架构）

基于 wasmtime Component Model 加载和执行插件，前端（`src/plugin/`）与 Rust 侧双层配合：

- **api_bridge / api_registry**：插件 API 桥接与注册 — 前端 PluginContext 的 API 调用经 Tauri invoke 到达此层，Rust 端权限校验后执行
- **host / host/**：插件生命周期管理（加载/激活/停用）；host/ 子模块负责插件随包 CLI 的安装/卸载
  （bin 解析、PATH 条目维护、平台注册）及 commands/listeners/services 拆分
- **loader / registry**：文件扫描 + WASM 组件加载、插件注册表
- **wasm_runtime + wasm_runtime/host_impl/**：wasmtime Engine/Store/Instance 生命周期管理（含 component.rs
  WASI preview2 接线）；宿主能力实现按功能域拆分于 host_impl/（storage/database/terminal/session/events/
  http/log/fs/config/bus/lifecycle/process/timer/peer/wsl_fs 等），统一注册到 Linker
- **approval / validation**：插件审批与校验
- **message_bus**：插件间 Topic 消息总线（发布/订阅），经 MessageDispatcher trait 解耦与 PluginHost 的循环引用
- **fs_auth**：文件系统访问三层校验（路径白名单 → 插件白名单 → 弹窗授权，弹窗 UI 为 `FsAuthDialog.vue`）
- **permission / storage / types / watcher**：权限管理、插件存储、类型定义、开发模式热重载监听

### 服务器 — `src-tauri/src/server/`（Actix Web HTTP + WS）

移动端与桌面端通信的唯一入口：

- **controllers/ + dtos/**：HTTP REST 控制器与请求/响应 DTO（auth、config、file、git、plugin、session）
- **middleware/**：CORS、JWT 网关（公开路径/插件路径放行规则）、HTTP 流量过滤器中间件
- **filter.rs**：传输层流量过滤器责任链（预留加密机制）——TrafficFilter trait + 全局
  TrafficFilterChain 单例；HTTP（请求体/响应体）与 WS（收发帧）统一接入，
  过滤器可观察/改写收发数据（配合 utils/crypto 实现报文加解密）
- **services/**：业务服务（认证、配对、会话配置/控制、终端服务）
- **ws/**：WebSocket 终端链路（消息类型、连接注册表、WS actor、管理器）
- **app.rs / supervisor.rs**：路由配置与服务器启动、服务器生命周期管理；另有端口检查、指标

### 会话管理 — `src-tauri/src/session/`

- **session_manager / storage / session_config**：会话管理器、会话存储、配置 CRUD
- **session_output**：输出管理（缓存/队列/订阅/全局），支撑多端输出回放
- **session_lifecycle**：生命周期事件（Creating/Created/Stopping/Stopped）与监听器机制，插件扩展点
- **input_line**：会话输入扩展点（SessionInputListener + 提交行重构）
- **event_bus / session_event**：统一事件广播与会话事件模型

### PTY 管理 — `src-tauri/src/pty/`

PTY 进程生命周期（启动时注入 `BEDCODE_SESSION_ID` 环境变量）、输出读取与前端输出分发、命令构建、WSL 支持。

### 全局事件系统 — `src-tauri/src/events/`

AppEvent trait + 事件匹配处理器；SessionManager 事件双路分发：转发 Tauri 前端（forwarder）与转 WebSocket 同步广播（sync_event/sync_handler）。

### 插件开发 SDK — `packages/plugin-sdk-desktop/`

Rust 侧以 `abi.rs` 为宿主/插件共同引用的单一事实来源（签名漂移由测试暴露）；`wasm_host.rs` 以 WASM import 后端实现全部宿主能力 trait。TS 侧通过共享模块运行时（`__BEDCODE_SHARED__`）复用宿主的 Vue/Pinia/vue-i18n。

---

## Quick Navigation

### 按功能查找（定位到目录）

| 功能 | 目录 |
|------|------|
| Tauri 命令 | `src-tauri/src/commands/` |
| PTY 进程与输出 | `src-tauri/src/pty/` |
| 会话管理与生命周期 | `src-tauri/src/session/` |
| HTTP/WS 服务器、REST 控制器、终端 WS | `src-tauri/src/server/` |
| 设备认证 / 配对 / QR Token | `src-tauri/src/utils/auth/` |
| ANSI / Markdown 解析 | `src-tauri/src/utils/parser/` |
| 数据库 | `src-tauri/src/db/` |
| 全局事件系统 | `src-tauri/src/events/` |
| mDNS 广播 | `src-tauri/src/mdns/` |
| 插件系统 (Rust) | `src-tauri/src/plugin/` |
| 插件系统 (前端) | `src/plugin/`、`src/composables/`（usePluginManager） |
| 插件开发 SDK | `packages/plugin-sdk-desktop/` |
| 测试插件 | `packages/plugin-test/`、`plugin-sdk-test/`、`plugin-wasi-test/` |
| 插件源码 | `plugins/ai-chatbox/`、`plugins/auto-task/`、`plugins/file-transfer/`、`plugins/scheduler/` |
| 系统常量 / 错误类型 / 生命周期 | `src-tauri/src/system/`（constants/ 按领域分组） |
| 应用上下文 (DI) | `src-tauri/src/system/`（app_context） |
| 前端页面 / 组件 / 状态 | `src/views/`、`src/components/`、`src/stores/` |
| 前端业务逻辑 | `src/composables/` |
| 国际化 | `src/locales/` |
| 前端测试 | `src/__tests__/` |
| 构建/开发脚本 | `scripts/` |

### 自动化任务执行机制（跨端链路概览）

BedCode 通过 Auto Task 插件（WASM）+ HTTP API + WebSocket 事件链路实现移动端远程自动执行多个任务：

```
Claude Code Hook (Python, plugins/auto-task/scripts/)
    ↓ HTTP POST /api/plugin/com.bedcode.auto-task/...
Auto Task WASM 插件（任务状态/队列/模式，经 plugin_controller.rs 动态代理路由）
    ↓ DesktopSyncEvent → sync_handler → WebSocket broadcast
Mobile Tauri Event → useAutoExecutor 状态机（移动端）
    ↓ sendInput / HTTP API
Claude Code (PTY)
```

关键机制：

- **会话 ID 绑定**：PTY 启动时注入 `BEDCODE_SESSION_ID` 环境变量，Claude Code 子进程继承，hook 脚本读取后上报，插件维护 Claude session ↔ BedCode session 映射
- **模式切换**：移动端 HTTP 设置自动/手动模式 → 插件持久化 + 广播 → Python PreToolUse hook 查询模式决定 auto-approve
- **生命周期扩展**：插件经 `SessionLifecycleListener` 在 Creating 阶段注入 hooks、Stopped 阶段清理
- **输入扩展点**：插件经 `SessionInputListener` 观察提交的输入行

涉及目录：`plugins/auto-task/`、`src-tauri/src/plugin/`、`src-tauri/src/server/controllers/`（plugin_controller）、`src-tauri/src/events/`、`src-tauri/src/session/`（lifecycle/input_line）、`src-tauri/src/pty/`。

### 按类型查找

| 类型 | 路径模式 |
|------|----------|
| Tauri Commands | `src-tauri/src/commands/*.rs` |
| 错误处理 | `src-tauri/src/system/error.rs` |
| 系统常量 | `src-tauri/src/system/constants/*.rs` |
| 数据库 | `src-tauri/src/db/*.rs` |
| 枚举类型 | `src-tauri/src/enums/*.rs` |
| 业务服务 | `src-tauri/src/server/services/*.rs` |
| DTO | `src-tauri/src/server/dtos/*.rs` |
| 前端插件系统 | `src/plugin/` |
| 插件源码 | `plugins/*/` |
| 插件 SDK | `packages/plugin-sdk-desktop/` |
| WASM 宿主能力 | `src-tauri/src/plugin/wasm_runtime/host_impl/` |
| 插件随包 CLI 安装/卸载 | `src-tauri/src/plugin/host/` |
| 加密工具（报文/文件传输加密） | `src-tauri/src/utils/crypto/` |
| 认证工具 | `src-tauri/src/utils/auth/*.rs` |
| 解析器 | `src-tauri/src/utils/parser/*.rs` |
| 前端测试 | `src/__tests__/**/*.test.ts` |
