# 更新日志

本文件记录本项目所有值得关注的变更。

格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.0.0/)，
版本遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

> 本文档为中文版本；英文版见 [`CHANGELOG.md`](./CHANGELOG.md)（GitHub Release 流程读取该文件）。

## [2.1.0] - 2026-09-10

> 功能 / Features · 基础建设 / Platform & Infrastructure · 改进 / Improvements · 修复 / Fixes · 安全 / Security · 测试 / Tests & Quality · 文档 / Documentation

### 功能

#### 对等网络 — P2P 直连链路（`packages/peer-net`）
- 新增双端共享的 Rust crate：节点身份、绑定公钥的自签证书、TLS 1.3 mTLS 直连、信任存储、专用 mDNS 发现、共享目录浏览与批级断点续传传输引擎
- 节点电源总线：集中的节点生命周期管理，双端完成插件生命周期接线与入站连接事件桥接
- 专用 `_bedcode-peer` mDNS 服务，带 TTL 在线缓存与可测试的注入缝
- TCP keepalive 活性探测、mDNS 周期性重广播/重查询实现可靠互发现、移除首帧超时
- 首连确认闸门、撤销并重确认流程、节点生命周期闸门消除 TOCTOU 竞态

#### 对等网络 — 加密核心（`packages/link-crypto`）
- 新增共享加密 crate：出站密钥 miss 观测、缓存容量护栏、GET 指标校正、query 路径归一（跨端）
- 移动端 pin 写入校验与指纹清理；HTTP GET/HEAD 空 body 协商；`X-BedCode-Crypto` 响应标记
- 生物凭证绑定/解绑迁移到 HTTP（脱离 WebSocket）

#### file-transfer 插件 — 对等化重写
- 旧的局域网文件服务整体退役，文件传输全链路改由对等栈接管
- V2 三段式主视图（移动端），业务逻辑由插件自持（Phase 3，双端）
- 设备发现刷新、端点记忆、历史快照、首连确认超时
- 双端共享目录多选；桌面端设置显示完整路径
- 端点清理/校验、设备单例、确认超时定点结算、根缓存重置
- 传输面板终态归档：进度 / 原因 / 打开所在文件夹 / 双端记账（两端各自记录同一次传输）
- 分区存储下的 SAF `content://` URI 中转复制兜底（解决 `EACCES`）

#### 认证与传输架构重做
- HTTP 生物认证与事件通道原语（桌面端）
- WebSocket 首消息 JWT 认证，以及专用的 `/ws/event` 事件通道
- 移动端常驻事件 WebSocket：认证后建连，断线自动自愈
- 移动端 HTTP 认证客户端；终端直连 WebSocket（`useTerminalSocket` + 状态机 store）

#### 终端输出管线
- 字节流 PTY 管线重写：基于序号的输出队列 + 快照订阅（废除字节偏移契约）
- 每会话终端路由，远端通道使用 TB v2 二进制帧；本地通道迁移到 TB v2 并支持快照重订阅
- 删除旧广播兼容通道与死代码
- 移动端终端写入管线让出主线程：128 KB 分块让出 + `flushing` 重入守卫
- `get_terminal_ws_info` 替代已移除的移动端输出链路；移动端支持 ack / yield / history 缓存

#### auto-task 插件
- 任务历史状态筛选由 chips 改为下拉 Select（默认「全部」）
- 各 Agent 的终端输入提交符统一为 `\r` Enter 字节

#### ai-chatbox 插件
- 供应商限流自动重试（双端）

### 基础建设

- 两端 `package.json` 与 Tauri 配置版本升级至 **2.1.0**；新增 `.deb` 构建支持
- CI：在 Windows / macOS 之外新增 Linux 构建目标；cargo 编译资源限制
- CI：新增双端 Rust + vitest 回归测试门禁（分层独立 job），合并到 `master` / `uat` 时阻断
- CI：`release.yml` working-directory 相对仓库根解析；SDK 构建步骤由 `pnpm filter` 语法改为 working-directory 模式
- CI：verify-latest.json 改用 draft release 资产查询；签名密钥步骤注入 `TAURI_SIGNING_PRIVATE_KEY`
- **E2E 基础设施（桌面端）**：WebdriverIO + `tauri-plugin-wdio`（仅 debug 隔离）+ 外部 `tauri-driver`；smoke 断言验证 execute/IPC 链路。移除遗留的 `@playwright/test` 依赖
- **插件 SDK 更名并发布**：`@binblink/plugin-sdk-*` → `@binblink/bedcode-plugin-sdk-*`（桌面 + 移动），v0.1.1 发布到 npm 与 crates.io；SDK 包文件精简（dev-shell 排除 `node_modules` 与构建产物），补充 `.npmignore`
- 共享 Rust crate 迁入 `packages/`（`peer-net`、`link-crypto`）
- 前端包管理器全仓从 npm 迁移到 pnpm，各项目各自保留独立 lockfile
- `dev-run` 进程组回收（向整组发信号，而非只杀父进程）+ plugin-watch 孙进程自清理
- `doc-tracking.sh` 行尾修复，恢复 pre-commit 保护逻辑；`PROTECTED_PATHS` 扩展覆盖两端 `docs` 子目录
- AI 工具配置入库；`.agents/skills/` 统一供 pi / OpenCode / Codex / Claude Code 共用
- 会话产物不入库（pi-lens 备份、compactions）
- Rust MSRV 1.94（wasmtime 47）；插件 WASM 以 `--release` 构建并保留 names section

### 改进

#### 桌面端
- 终端：xterm 残影收敛 — 背压滞回、Channel 传输、渲染器决策、有序写入队列
- Linux 终端渲染优化；WebKitGTK IME 防护重构为单一 attach 点
- 登录 PTY 以 `-lic` 启动，使其继承 Linux 用户 PATH
- Windows 熄屏 / 休眠唤醒后 WebView2 黑屏自愈
- 启动屏 footer 版本号改为读取单一真源；调整窗口尺寸与启动背景色
- 插件列表简介字号 12px → 11px

#### 移动端
- 设置页拆为 `views/settings/` 子视图：外观、认证、连接、通知、关于（主视图由约 1130 行降至约 400 行）
- Android 启动主题简化 — 移除系统开屏定制；开屏深浅色跟随 + 启动窗口背景统一
- SplashScreen 开屏页暂时下线（注释保留，可一行恢复）
- 手势滑动不再与导航栏切换动画互相打断
- 键盘收起时退出终端输入编辑态
- 通知后台语义收口；死代码清理；导航图标对齐

#### 日志与可观测性
- **前端**：引入 loglevel 作为统一前端日志框架，替代直接 `console` 调用；仅 dev 构建转发到 `frontend.*.log`（桌面端）与 logcat（移动端），release 自动剥离
- **桌面宿主**：HTTP 请求全路径日志、JSON span 链、启动 bootstrap 通道、存量日志字段化迁移
- **桌面插件（WASM）**：启用 `wasm_backtrace_max_frames(32)`，使 trap 携带 WASM 调用栈；trap 在宿主侧留日志；调试模式（`BEDCODE_PLUGIN_DEBUG=1`）以 DWARF 构建插件并支持行号解析、燃料预算放大 32 倍；通过 `BEDCODE_PLUGIN_LOG=id=level` 按插件设定级别
- **移动端**：release 日志级别收敛；dev 日志保留治理（14 天滚动清理）
- 宿主错误处理加固：自描述 `io` 错误包装、span 插桩、spawn 错误边界

### 修复

- **对等网络**：停机 busy-loop、keepalive 留痕、目录 `size` 契约跨端归一；节点生命周期 TOCTOU 闸门；连接态重发补充 `deviceName`
- **文件传输**：移动端共享目录多选、mDNS 单守护恢复互发现、连接感知；桌面端启用死锁改为「启用先行」并把预授权弹窗前置于 loading；插件动态 UI 严格跟随启用状态（生命周期对称拆解 + mDNS 多播锁去重入）
- **终端**：opencode TUI 滚动残影 — rAF 合并写入默认开启 + 滚动停止补刷；移动端 `touchmove` cancelable 守卫；zoom 移除后同步 zoom-compensation 注释
- **移动端**：`SafPickerPlugin` 构造参数恢复为精确的 `android.app.Activity`（JNI 签名查找返回 `null` 导致 NPE）；platform-tools 升级后 adb fd0 shim 自愈；插件授权弹窗不再与 loading 同现；真机发布包联调缺陷
- **桌面端**：VerifyCode 配对成功的 `Authenticated` 响应补充 `device_name`；开屏 caret 残留清理；全局通知去重
- **构建 / 开发**：`plugin-build` 的 `JSON.parse` / `execSync` 补充错误上下文；V2 合并后的 file-transfer 组件导入路径修正

### 安全

- **对等身份**：首启纯随机生成 Ed25519 节点身份并原子持久化，绑定进 rcgen 自签证书，通过 ring 校验 `CertificateVerify` — 伪造身份无法通过握手
- **信任存储**：首连确认闸门，支持撤销并重确认，使已信任的对端可被撤销后重新受审
- **传输**：WebSocket 首消息 JWT 认证 + 专用认证后的 `/ws/event` 通道；移动端生物凭证绑定/解绑由 WebSocket 迁移到 HTTP
- **沿用 2.0.0**：插件身份校验与权限审批、生物认证链路加固、为 Agent hooks 保留本地旁路的 JWT 网关

### 测试与质量

- **桌面端 Rust**：583 个测试；集成覆盖 HTTP 契约、WS 配对认证、PTY 会话链路、多客户端广播 + 优雅停机、契约 fixture 漂移对齐
- **移动端 Rust**：251 个测试；L1/L2 集成套件落地，并修复断线重连缺陷
- **peer-net**：102 个测试，含双节点 harness（发现注入、共享目录、传输会话）
- **前端**：桌面端 61 个文件 569 个测试；移动端 42 个文件 360 个测试（views / stores / composables / integration）
- **插件 SDK**：桌面端契约测试 5 → 85；移动端 2 → 79
- E2E smoke 套件，验证 WebdriverIO → tauri-driver → IPC 链路
- CI 回归门禁：任一层失败即阻断合并到 `master` / `uat`

### 文档

- README 按 2.1.0 重写：版本徽标（含 wasmtime 47）、平台补充 Linux、Claude Code → Pi/Opencode 文案、移除过时截图；`README_en.md` 同步
- 两端目录级代码地图（`bedcode-desktop/docs/code-map.md`、`bedcode-mobile/docs/code-map.md`）作为模块查找索引
- ai-chatbox / auto-task / file-transfer 双端插件架构图（Archify）
- 知识库：`pty-output-pipeline`、`mobile-terminal-optimization-reference`、`release-workflow`、`sdk-publish`、`github-actions-setup`、`build-process`、`feature-branch-isolation`
- ADR：移动端文件服务退役（对等栈接管）、代码查看器设计
- auto-task DAG 编排 spec；xterm 透明模式残影收敛 spec 与票据；desktop-e2e-webdriver spec 与票据；插件 WASM 日志 spec
- 特性分支隔离落地：桌面端定时调度插件 → `feature/task-scheduler`，移动端 OCR 插件 → `feature/ocr-plugin`，代码查看器设计 → `feature/code-viewer`

---

## [2.0.0] - 2026-08-16

### 新增

#### 插件系统 — WASM 平台
- 插件运行时迁移到 WASM Component Model（wasmtime）；移除 cdylib 动态加载，Component 成为唯一支持的形态
- ABI 演进 v2 → v6：类型化宿主 API、参数绑定 SQL、内存回收、out_ptr、签名校验、插件状态上报、InputSubmitted 观测扩展点
- 运行时加固：epoch 中断 + 资源限制、燃料看门狗、trap 自动重载恢复、AOT 缓存、wasmtime 47
- 安全：插件身份校验与权限审批（防冒充）
- 工具链：bedcode-plugin CLI（create / build / dev / validate / doctor / manifest）、Dev Shell 浏览器开发环境（双端，`--host` 供手机访问）、manifest-gen
- 生命周期：动态激活/停用并持久化状态、热重载、安装/卸载、loading 遮罩
- 能力：每插件独立 SQLite 数据库、插件间消息总线、host_notify、fs_auth 批量目录授权、文件服务挂载/传输、WSL 文件系统桥接
- SDK 内置共享 UI 组件库（Rust + TS，双端）

#### auto-task 插件
- 多 Agent 支持：Claude Code / pi / opencode / Codex（注册表驱动的 Agent 适配架构）
- 任务队列：调度、自动执行、自动应答、预设任务（一次性）、定时任务状态机、任务历史与统计、筛选与重试
- 移动工具箱：任务历史 / 定时任务面板
- TUI-agent 首次派发兜底（15s 宽限）与按 Agent 的终端 hooks

#### file-transfer 插件
- 局域网文件传输插件（WASM 核心 + 桌面/移动 UI + 打包分发）
- 双向传输：发送到手机（上行）、带策略审批的接收、异步批量审批、传输历史、断点重新入队、专用 downloads_dir
- Android SAF 流式传输：共享目录、带 pfd 强引用的 SAF 选择器、全盘访问授权引导

#### ai-chatbox 插件
- 纯 AI 对话重写（双端）：多供应商、流式 SSE 解析、thinking 模式、Shiki 高亮、代码渲染配置、JSONL 持久化

#### 移动端
- 插件系统启用：动态路由、插件管理页、工具箱入口、插件导航页签
- Android SAF 文件/目录选择器（startActivityForResult）
- 生物认证：密钥、认证设置页、质询-响应、设备身份持久化
- 终端：会话预载、游标式输出订阅、TUI 滚动兼容（SGR）、Agent CLI 命令预设、16 键默认快捷键
- 强调色色板（与桌面端同源）

#### 桌面端
- 终端 PTY 回放与历史播放（Rust 侧恢复窗口关闭期间丢失的输出）
- 字节流 PTY 输出管线：字节偏移契约、游标增量重订阅
- 四套主题色板（forest / ocean / sunset / violet）
- 生物质询-响应与连接历史
- SystemInfo 采集与设备名广播、通用加密工具模块

### 变更

- 插件后端完全 WASM 化（移除 cdylib）；Rust MSRV 提升至 1.94（wasmtime 47）
- Toast 迁移到 vue-sonner（双端）
- 桌面 UI 基于 Warm Workbench 设计重建；移动 UI 统一为分组卡片风格；字号 token 化
- 终端尺寸控制改为远端优先，移动端支持暂停/恢复订阅
- 输出管线迁移到本地 WS 单通道（桌面端）；游标式订阅取代 2MB 前端环形缓冲（移动端）
- 构建：rust-lld 链接器、thin LTO、版本升级脚本、安装包 release 后缀重命名、CI 以 wasm32 目标构建插件产物 + Windows 签名指纹注入
- Skills 统一至 `.agents/skills/`，供 pi / OpenCode / Codex / Claude Code 共享

### 修复

- 终端：输出连续性（游标增量重订阅解决重放风暴）、长时间运行页面崩溃、丢帧（异步插件回调、drain/reset）、重连后尺寸同步
- 文件传输：文件名冲突（409 + 拒绝原因）、通知风暴、任务竞态、Windows 路径分隔符、资源管理器定位、`.part` 残留清理
- 插件：多插件 PluginContext 污染导致 i18n 失效、WASM trap 恢复、燃料耗尽 trap、loader 句柄释放、WSL 子进程超时
- 移动端：心跳 blocking_write panic、Activity 重建后选择器失效（EBADF）、订阅泄漏、重连状态不一致
- 桌面端：设置保存循环（内容快照比对）、端口输入、删除后会话命名回退

### 安全

- 插件身份校验与权限审批（防冒充）
- 生物认证链路加固：IPC 序列化、DER 解析、绑定守卫自检
- 为 Agent hooks 保留本地旁路的 JWT 网关（token 从 hook 脚本中移除）

### 测试

- 前端 +175，桌面端 Rust +204，移动端 +116，SDK 契约测试（桌面 5→85，移动 2→79），file-transfer 宿主单元测试

---

## [1.1.0] - 2026-07-05

### 新增

#### 插件系统
- 支持 cdylib 动态加载的 Rust 插件 API crate
- 插件清单类型与权限系统
- PluginHost 与 API 桥接 Tauri 命令
- UI 插槽的扩展点注册表
- 插件加载器、存储与 `AppError::Plugin` 变体
- 完整的前端插件系统（PluginRegistry）
- 自动生成配置表单的 PluginConfigView 页面
- 带列表、开关、可展开详情的 PluginsView 页面
- usePluginManager composable
- PluginTerminalToolbar 与 PluginTitleBarItems 渲染组件
- registerTerminalToolbarItem 与 registerTitleBarItem 代理 API
- AI chatbox 插件重写为独立 cdylib 插件
- 资源目录插件加载与 API 安全
- 插件侧边栏 / 工具箱视图路由与导航
- 插件页面 i18n key

#### 移动端
- 面向性能的 Buffer-Only 终端架构
- mDNS 服务发现与广播
- 按会话的任务通知系统
- 自动执行任务引擎与终端集成
- WebSocket 心跳保活与重连改进
- 侧边栏 + 代码显示布局的 CodeExplorerView
- 支持行级着色的 Diff 渲染
- 带 diff 模式的 FileViewerModal
- 带类型徽标、状态与操作菜单的 PresetTaskCard 组件
- 基于 localStorage 持久化的 usePresetTasks composable
- 快捷键配置弹窗与终端输入栏无限轮播
- loading 遮罩与交互改进
- 快捷栏按钮颜色与快捷键面板一致
- 所有弹窗的平滑开合动画
- 集成带通配符 scope 权限的 tauri-plugin-http
- ForegroundService 中的 WakeLock

#### 桌面端
- Actix Web 服务器的高级网络配置
- 带配置迁移到 properties 格式的服务器管理页
- 用于设备识别的指纹跟踪
- 启动时端口可用性检查
- FileSidebar 标题栏中的 Git 分支切换器
- 电源管理功能
- Claude Code hooks 从全局配置迁移到项目级配置
- 带 session ID 绑定的全局化 hooks

#### 服务端 / 后端
- 在现有 WsServer 旁新增 Actix Web HTTP 服务器
- Actix Web HTTP 控制器、DTO 与中间件
- 用于终端 I/O 的 Actix WS actor
- WS 指标与配置端点
- HTTP + WS 双协议支持
- 文件内容 / diff 树 HTTP API
- 终端输出缓冲区以减少 WebSocket 消息数
- 当前行输入跟踪与插件事件响应

#### 国际化
- 带语言持久化的 vue-i18n 基础设施
- 全部视图、组件、composable 的 i18n + 错误码系统
- 带语言切换 UI 的 i18n 设置页
- 导航、布局与共享组件的 i18n
- 终端视图与输入栏的 i18n
- BottomSheet 与 PairingInput 组件的 i18n
- 桌面端 SessionManager、SessionsConfig 与组件文件的 i18n

#### 代码查看器
- useCodeHighlight 多主题支持
- 用于代码查看器设置的 useCodeViewerStore
- CodeViewerSettingsModal 组件
- 在 FileViewerModal 与 CodeExplorerView 中集成代码查看器设置

### 变更

- 插件重构为任务状态管理器，引入 KeyCombo 系统与自动审批模式
- 用进程内 Actix Web 取代 IPC 子进程
- 将 event/ 合并进 events/，修复 IPC 运行时
- 移除 desktop/ 与 shared/ 层级，Rust 模块按领域扁平化
- 移动端模块结构扁平化并迁移 Android 包名
- 移动端：移除 auto-executor、抽出 FileExplorer、新增浅色代码主题
- 移动端：TerminalView 重构并简化预设任务
- 桌面端：重组 Rust 模块、新增 mDNS、基于设计 token 重做 UI
- 桌面端：服务器重置默认值 + UI 打磨
- 移动端通知迁移
- 任务选择器重构

### 修复

- 移动端连接错误处理与状态一致性
- 路径分隔符归一为正斜杠
- 侧边栏动画改进
- 重连处理与特殊键修饰符
- 返回会话列表后移动端终端滑回问题
- 插件状态类型处理与表头
- PluginViewHost props 路由
- IPC reader 实现与 sysinfo 指标
- 按钮符号清理

---

## [1.0.0] - 2026-06-30

### 新增

#### 核心架构
- 多项目 monorepo：bedcode-desktop + bedcode-mobile 作为独立项目
- 桌面端与移动端之间的 WebSocket + HTTP 双协议通信
- 用于设备配对的 X25519 密钥交换
- 所有通信的 AES-GCM 加密
- 基于系统 keychain / secret service 的安全存储
- 带 60 秒过期时间的 6 位配对码认证

#### 桌面端
- 会话管理界面（创建、编辑、删除会话）
- 带二维码显示的设备配对界面
- 集成 xterm.js 的终端预览
- 带快捷操作的系统托盘
- 网络与外观配置的设置页
- 面向 Windows 与 WSL2 的 PTY（伪终端）管理
- 基于 SQLite 持久化的会话配置管理
- 面向移动端的 WebSocket 服务器
- mDNS 设备发现服务
- Tmux 会话集成

#### 移动端
- 设备发现与配对流程
- 支持增强 / 原始模式切换的终端输出显示
- 带特殊键（Tab、Ctrl+C、Esc 等）的输入栏
- 可自定义命令的快捷操作网格
- 带搜索功能的历史记录
- 带通知偏好的设置页

#### 后端（Rust）
- 基于 SQLite 的数据层（pairings、sessions、messages、quick actions）
- 基于 portable-pty 的 PTY 进程管理
- 带路径转换的 WSL2 支持
- WebSocket 消息协议
- ANSI 转义序列解析器
- Markdown 代码块提取器
- 带等待输入检测的输出解析器
- 带免打扰时段的通知服务

### 安全
- 所有 WebSocket 通信经 WSS 加密
- 配对码 60 秒后过期
- 连接时校验设备指纹

---

## [0.1.0] - 2026-04-30

### 新增

#### 核心功能
- 基于 Tauri 2.0 + Vue 3 + TypeScript 的初始项目结构
- 面向 Windows 与 WSL2 的 PTY（伪终端）管理
- 基于 SQLite 持久化的会话配置管理
- 面向移动端的 WebSocket 服务器
- mDNS 设备发现服务
- 6 位配对码认证

#### 桌面 UI
- 会话管理界面（创建、编辑、删除会话）
- 带二维码显示的设备配对界面
- 集成 xterm.js 的终端预览
- 带快捷操作的系统托盘
- 网络与外观配置的设置页

#### 移动 UI
- 设备发现与配对流程
- 支持增强 / 原始模式切换的终端输出显示
- 带特殊键（Tab、Ctrl+C、Esc 等）的输入栏
- 可自定义命令的快捷操作网格
- 带搜索功能的历史记录
- 带通知偏好的设置页

#### 后端（Rust）
- 基于 SQLite 的数据层（pairings、sessions、messages、quick actions）
- 基于 portable-pty 的 PTY 进程管理
- WSL2 支持（含路径转换）
- Tmux 会话集成
- WebSocket 消息协议
- ANSI 转义序列解析器
- Markdown 代码块提取器
- 带等待输入检测的输出解析器
- 带免打扰时段的通知服务

#### 安全
- 用于设备配对的 X25519 密钥交换
- 通信的 AES-GCM 加密
- 基于系统 keychain / secret service 的安全存储

### 变更
- 无（首次发布）

### 修复
- 无（首次发布）

### 安全
- 所有 WebSocket 通信经 WSS 加密
- 配对码 60 秒后过期
- 连接时校验设备指纹

---

## 版本历史

| 版本 | 日期 | 说明 |
|---------|------|-------------|
| 2.1.0 | 2026-09-10 | 对等网络（`packages/peer-net` + `link-crypto`）含 TLS 1.3 mTLS 与信任存储、file-transfer 对等化重写、JWT + 常驻事件 WebSocket、终端输出管线重写、统一前端日志、WASM trap 日志、E2E + CI 门禁、SDK 更名为 `@binblink/bedcode-plugin-sdk-*` |
| 2.0.0 | 2026-08-16 | WASM Component Model 插件平台、auto-task / file-transfer / ai-chatbox 插件、移动端插件系统、生物认证、UI 重做 |
| 1.1.0 | 2026-07-05 | 插件系统、移动端终端重构、国际化、Actix Web 服务器 |
| 1.0.0 | 2026-06-30 | 多项目 monorepo，桌面 + 移动稳定版发布 |
| 0.1.0 | 2026-04-30 | 含核心功能的首次发布 |
