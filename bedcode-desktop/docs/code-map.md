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
├── scripts/                          # 构建/开发工具脚本：dev-run（Tauri dev 编排）、tauri-build（updater
│                                     #   签名密钥解析）、插件构建与热重载（plugin-build/plugin-dev/plugin-watch）、
│                                     #   产物大小检查（check-target-size）、图标生成（generate-icons 等）、
│                                     #   Linux 依赖安装（install-tauri-deps.sh）；含 README
├── packages/                         # 共享包（供插件开发与测试使用）
│   ├── plugin-sdk-desktop/           # 插件开发工具包，Rust + TS 双侧 SDK（含 dev-shell 调试壳、
│   │   │                             #   插件模板 template/、脚手架 bin/）
│   │   ├── rust/                     # bedcode-plugin-api crate：WASM ABI 契约（单一事实来源）、
│   │   │                             #   BedcodePlugin/WasmPlugin trait、宿主能力接口（host/ 按功能域拆分）、
│   │   │                             #   权限/SQL/命令参数辅助宏；rust-macros/ 为配套过程宏 crate
│   │   └── src/                      # TS SDK：插件类型定义、共享模块运行时代理（__BEDCODE_SHARED__）、
│   │                                 #   Vite 构建插件（vue/pinia/vue-i18n 外部化）
│   ├── plugin-component-test/        # 测试用 WASM 插件 crate（Component Model 绑定验证与连通性测试，
│   │                                 #   覆盖宿主调用路径，供宿主 runtime 测试套件做签名验证）
│   ├── plugin-sdk-test/              # SDK 接口测试用插件 crate
│   ├── plugin-system-test/           # 系统组件形态测试插件 crate（导出 host-* 同形能力接口，验证能力装配
│   │                                 #   框架：注册表路由 / host-side 转发 / 依赖检查 / trap 隔离）
│   └── plugin-wasi-test/             # WASI preopen 测试插件（wasm32-wasip2，std::fs 直读写预打开目录；
│                                     #   同一 fixture 分钉可写档与只读档两种挂载）
├── wasm-apps/                        # wasm 应用源码目录（2026-09-25 语义：桌面端 wasm 插件对外称
│                                     #   wasm 应用；内部代码实现与插件 ID 契约不变。每个应用独立
│                                     #   package：plugin.json 元数据 +
│                                     #   rust/ WASM 后端 + src/ TS 前端 + vite.config.ts 独立构建）
│   ├── agent-hub/                    # Agent Hub 插件：Agent CLI 统一管理台——环境检测与一键安装、
│   │                                 #   Skills 管理（浏览/编辑/分发/GitHub 安装/本地导入）、
│   │                                 #   供应商统一配置、使用统计与会话日志解析
│   ├── ai-chatbox/                   # AI Chatbox 插件：多供应商 OpenAI 兼容客户端，
│   │                                 #   聊天 UI、供应商配置、提示词优化
│   ├── file-transfer/                # 文件传输插件：基于对等网络（peer_* 宿主模块）的在线对端发现与
│                                     #   切换、共享目录浏览、多任务并发传输（暂停/恢复/取消/重试，同批 ID
│                                     #   重发即断点续传）、接收策略与历史归档、本地目录挂载供对端访问
├── src/                              # Vue 3 前端（扁平化结构 + 领域子目录）
│   ├── components/                   # UI 组件：桌面布局、侧边栏、终端预览、
│   │                                 #   标题栏、通知卡片/徽章、退出确认、文件系统授权弹窗、通用基础组件；
│   │                                 #   settings/ 下为设置页分组子组件（外观/链路加密/系统/日志/关于；
│   │                                 #   配对「票 14」与会话分组已随域下沉 com.bedcode.terminal-session 插件）
│   ├── composables/                  # 业务逻辑 composable：桌面命令、网络（服务器）、插件管理、PTY 输出、
│   │                                 #   全局终端、快捷键、主题、字体、更新检查等（配对 / WSL / 设备一族
│   │                                 #   已随域下沉 com.bedcode.terminal-session 插件，设备连接通知亦在其内）；
│   │                                 #   terminal/ 已无：终端渲染/写入/IME 随票 01-05 整体下沉
│   │                                 #   wasm-apps/terminal-session（宿主只剩窗口编排原语
│   │                                 #   useSessionWindows 与 view 壳 PluginWindowHostView）；
│   │                                 #   commands.rs 为 Rust 命令封装聚合（九域：会话引擎事实/设置等），
│   │                                 #   useDesktopCommands 为聚合层 re-export
│   ├── stores/                       # Pinia 全局状态：会话（引擎事实 + 插件动作）、设置、i18n
│   ├── views/                        # 页面：插件、插件详情、插件配置、设置（编排层）、通用插件窗口、服务器
│   │                                 #   （设备 / 会话 / 会话配置页已随票 13/14 下沉 com.bedcode.terminal-session
│   │                                 #   插件；/server 为无侧边栏入口的诊断页，落地页为 /plugins）
│   ├── plugin/                       # 前端插件系统：加载器、注册表、权限、上下文、事件、命令、
│   │                                 #   共享模块运行时、运行时事件监听（runtime-listeners）；components/ 下为插件 UI 宿主组件
│   ├── utils/                        # 工具函数（Tauri invoke 封装、终端主题数据 terminalThemes、格式化 format 等）
│   ├── locales/                      # 国际化（zh-CN / en，各含 common / desktop / settings）
│   ├── router/                       # 路由
│   ├── dev/                          # 开发调试资源：终端 mock、PTY 输出 dump
│   └── __tests__/                    # 前端测试（组件、composable、store、路由、视图）
└── src-tauri/                        # Rust 后端（Tokio 异步）
    ├── resources/                    # 打包资源：应用配置 + 内置插件构建产物（wasm/js/plugin.json）
    └── src/                          # 模块按领域扁平组织，每领域配同名入口文件（commands.rs、db.rs 等）
        ├── commands.rs               # Tauri invoke 命令层（2026-09-22 单文件聚合）：会话引擎事实 /
        │                             #   PTY 输入 / 系统设置 / opener / devices / dev 日志转发 / 插件
        │                             #   re-export / 服务器九域，按 `// ====================` 分隔分组；
        │                             #   只保留宿主页面（外壳 / 终端引擎）直调的命令，业务面一律归插件命令面
        ├── db/                       # SQLite：连接管理、数据模型、CRUD 操作、Schema
        │                             #   v24：settings / plugin_storage / plugin_secrets 三表（业务表
        │                             #   pairings / connection_history / session_configs 已退役；
        │                             #   2026-09-23 裁定不再兼容旧版本用户，存量不迁移）
        ├── cloud_loopback.rs          # 云端回环节点（fabric dock）
        ├── crypto/                   # 加密引擎（票 01）：算法注册表（名称→实现+白名单）+ abstract trait，
        │                             #   引擎级能力——link_crypto 与 host-crypto 原语只依赖其抽象接口，
        │                             #   不再内联具体算法（WS/HTTP 过滤层 = 纯抽象层）
        ├── enums/                    # 枚举类型（终态 = 引擎级 + 传输面契约形状）：认证 wire 已随认证编排
        │                             #   下沉 session 插件删除（2026-09-25，宿主无消费者）；现只定义 PTY 引擎
        │                             #   枚举；特殊键 / 插件共享类型两个文件是 SDK 的 re-export
        │                             #   垫片（专项票 01；同步/概要/控制垫片已随 websocket 业务下沉票 08 删）
        ├── events/                   # （websocket 业务下沉票 08 已删除：AppEvent/publish/matcher 只服务
        │                             #   宿主 SyncEvent 同步桥，随 broadcast-sync 退役整体移除）
        ├── mdns/                     # mDNS 服务广播：将桌面端服务注册到局域网供移动端发现
        ├── plugin/                   # 插件系统（WASM 组件沙箱架构，核心模块，详见 Core Modules）
        ├── pty/                      # PTY 管理：进程生命周期、输出读取/缓存/监听、命令构建、WSL 支持
        ├── server/                   # 服务器（核心模块，详见 Core Modules）：core/ 传输无关内核 +
        │                             #   http/ 与 websocket/ 两个传输面，单端口组合物在 core/app.rs；
        │                             #   peer_net/ 对等网络引擎域——引擎接入中枢（节点身份/发现/生命周期、
        │                             #   host-peer bridge、事件适配）+ 三个引擎适配子模块（发送/接收/远端浏览），
        │                             #   宿主不持有传输历史、设置或任务真源
        ├── system/                   # 系统模块：应用上下文 (DI 容器)、配置、错误类型、生命周期钩子、
        │                             #   日志格式化、休眠阻止、进程创建工具（process.rs，create_command）；
        │                             #   constants.rs 按领域分组的常量（`// ====` 分隔）
        ├── utils/                    # 工具：auth/（JWT、配对、QR Token）、
        │                             #   crypto/（对称/非对称/混合加密：AES-GCM、ChaCha20-Poly1305、
        │                             #   RSA、X25519、KDF，用于 HTTP 报文与文件加密传输）、
        │                             #   session_gateway.rs（宿主调会话的**唯一收口点**，纯插件互调 api）
        ├── lib.rs                    # 库入口（模块声明 + 日志初始化 + Tauri 应用搭建；对等网络模块的
        │                             #   Tauri 命令也直接在此注册，不经 commands.rs）
        └── main.rs                   # 二进制入口（panic hook）
```

---

## Core Modules（核心模块）

### 插件系统 — `src-tauri/src/wasm_core/`（WASM 内核五模块，组件沙箱架构）

基于 wasmtime Component Model 加载和执行插件，前端（`src/plugin/`）与 Rust 侧双层配合。
Rust 侧按内核五模块组织（`wasm_core.rs` 为唯一组合点/facade，外部消费方只经 facade 再导出引用）：

- **manager/（core-plugin-manager，核心）**：插件加载、注册、生命周期与运行时
  - **downloader**（`manager/downloader.rs`；2026-09-22 自 `plugin/downloader.rs` 归位于此，
    安装属 core-plugin-manager 职责，审计票 11 第 5 项）：插件 zip 包本地安装——
    解压（条目/体积/单文件上限）→ manifest 解析与必填校验（真源 `validation::parse_manifest_json`）→
    id 反向域名校验 → 路径穿越防护 → wasm 存在性与 `wasm_hash` 摘要校验 → 写来源标记 →
    移动到 `app_data_dir/plugins`。
    `wasm_hash` **有生产者**（审计票 14）：打包链 `packages/plugin-sdk-desktop/bin/wasm-hash.js` 把产物
    `<rustLibrary>.wasm` 的 SHA-256 注入**产物** plugin.json（源清单不带该键），
    `scripts/package-plugins.mjs` 出包前逐条复核——声明不再依赖发布者手填
  - **api_bridge**：插件 API 桥接 — 前端 PluginContext 的 API 调用经 Tauri invoke 到达此层，Rust 端权限校验后执行。
    **身份由凭证绑定而非参数自报**（审计票 06，见 `wasm_core/security/frontend_channel.rs`）：
    `plugin_*` 命令都带 `credential`（宿主面 loader 会话密钥 / 插件面通道令牌），参数里的 `plugin_id`
    只作目标；`plugin_frontend_loader_session`（宿主前端 bootstrap，首个调用者生效，页面加载重置）
    与 `plugin_channel_token`（用 loader 密钥为运行中插件换令牌，停用即回收）是两枚凭证的来源
  - **host / host/**：插件生命周期管理（加载/激活/停用）；host/ 子模块负责插件随包 CLI 的安装/卸载
    （bin 解析、PATH 条目维护、平台注册）及 commands/listeners/services 拆分
  - **loader / registry**：文件扫描与 `plugin.json` 解析、插件注册表。**loader 不做 WASM 实例化**——
    启动扫描与 zip 安装两条入口共用 `host/wasm.rs::instantiate_wasm_plugin`（唯一的实例化实现，
    审计票 11 第 2 项）；contributes 注册的唯一实现是 `host/register.rs::register_plugin_contributions`
    （启动期全量 / 安装 / 热重载三入口共用，票 11 第 3 项）
  - **runtime（`manager/runtime/`，原 wasm_runtime）**：wasmtime Engine/Store/Instance 生命周期管理（含 component.rs
    WASI preview2 接线）；实例互调（JSON-RPC 路由）。**WasmHostContext 本体已迁 `host_api/context.rs`**
    （票 04），runtime 只保留 `pub use` 兼容再导出；component.rs Host 绑定经 `self.host_ctx.as_ref()`
    coerce 到各域所需角色接口（票 05）
- **host_api（`wasm_core/host_api/`，原 wasm_runtime/host_impl，宿主对外接口模块）**：宿主能力实现
    按功能域拆分（api/app/storage/database/events/http/mdns/ws/pty/log/fs/
    config/bus/lifecycle/process/timer/peer/status/platform/wsl_fs；
    **api_bridge 前端命令桥已随票 06 归位 `manager/host/`**），统一注册到 Linker。
    **票 10 起无会话域**（`terminal.rs` / `session.rs` 随 `host-terminal` / `host-session` 两个
    interface 一并删除）；`connection.rs`（票 04）= 宿主 server 在册连接清单原语
    `host-connection`，判据 `connection:read`——它是唯一幸存的「会话域出身」原语，且已与会话解耦
  - **context.rs（票 04/05/07 后的装配面）**：`WasmHostContext` 本体（原定义于
    `manager::runtime`）、`PluginServices` / `CapabilityProvider` / `TaskEngine` 三个消费方定义
    trait（均两阶段注入：PluginHost 构造后 `set_services` / `set_task_engine`）、角色接口
    （DbScope / PermissionScope / StorageScope / FsAuthScope / BusScope / AppHandleScope /
    ServicesScope / ProcessScope / ApiRegistryScope / SecurityScope / CapabilityScope /
    SecretsScope——22 个能力域函数签名只取各自需要的 `&dyn` 窄接口，不再传上帝对象，
    `host_api/` 生产源码零 `&WasmHostContext` 参数）。**host_api → manager 依赖单向化**：
    除两处文档化的单点例外（`LoadedWasmPlugin` 装配域类型经 CapabilityProvider 签名、
    storage 能力转发 forward_storage_* 尚在 manager::capability）外，host_api 不依赖 manager
  - **unit_executor.rs（票 07）**：任务单元执行器策略接口 `UnitExecutor`（matches + execute）——
    fs/process/http 各自的域执行器（fs 执行器内置声明闸门 + fs_auth 已授权预检，绝不弹窗）
    经 `manager::task::register_unit_executor` 注册；core-task 执行单元时查注册表分发，
    不再直调 host_api 域函数
  - **capability**：能力注册表与系统组件装配（manifest `type: system|application` + `dependencies`）——
    能力名 → 宿主原语 / WASM 系统组件实例二选一装配；应用插件的 host-* import 由 Linker 经此
    host-side 转发到系统组件同形导出；系统组件内置、默认启用、先于应用插件激活
  - **storage / types / validation / watcher**：插件存储、类型定义、校验、开发模式热重载监听
- **（已退役）一次性 handoff 迁移链**：`quick_actions_migration` / `auth_records_migration` /
  `task_data_migration` / `session_db_migration` 四迁移（宿主 legacy 表 / 插件私有库旧 id 路径 →
  插件私有库）于 2026-09-23 用户裁定整体退役——不再兼容旧版本存量用户，旧库滞留表不读不迁
  不清理；`wasm_core/legacy/` 目录与 `db/models.rs` 的 `Legacy*` 只读视图一并删除
- **security/（core-security）**：资源授权——framework（统一授权框架：ResourceKind × 三段决策管线
  声明/审批/强制，fs / api-call 资源实现）、approval（用户 zip 安装插件的权限审批与内容钉扎，ADR 0020：
  批准记录 + 目录哈希，`PluginHost::activate_plugin` 前置 `approval_gate` 裁决，弹层 UI 为
  `PluginApprovalDialog.vue`）、fs_auth（文件系统访问**三层**校验：第一方具名集成目录预授权
  （`FIRST_PARTY_TRUSTED_DIRS`，逐条注释归属）→ 已授权路径前缀（持久化「记住」）→ 弹窗授权，
  弹窗 UI 为 `FsAuthDialog.vue`；票 07 退役了旧的「`.claude/` 子串路径白名单」与
  「内置插件 = 任意路径放行」两条特权，命中层随日志 `layer=` 输出，
  任务单元只走 `is_granted`（无弹窗、未授权即拒））、
  frontend_channel（前端通道身份：loader 会话密钥 / 插件通道令牌 → 身份，审计票 06）、
  api_registry（互调门，ADR 0017）
- **bus（core-bus）**：插件间 Topic 消息总线（发布/订阅，JSON + 二进制双载荷 + 背压），经 MessageDispatcher trait 解耦与 PluginHost 的循环引用。
  **`bus` 不是桌面端权限位**（移动端 SDK 有 `PERMISSION_BUS`，属 ADR 0018 双端契约分叉）：桌面总线订阅/发布不经权限门，
  访问控制归 **topic 命名空间**（票 05 已落地）——`<plugin-id>::<name>` 是某插件的收件箱，宿主按形态仲裁：
  只有属主（与宿主）能订阅、只有属主（与宿主）能发布，判定不查激活表也不查安装表；公开 topic（不含 `::`）
  仍是插件间广播道。`bedcode.api.reply.*` 回复道对 WASM 订阅面关闭、回复 `sender` 按 api 声明属主校验
  （见 `wasm_core/host_api/api.rs`）。形态原语在桌面 SDK `host::bus`（`owned_topic` / `topic_owner`），宿主与插件共用
- **config（core-config）/ monitor（core-monitor）**：Engine/Store 运行参数配置面（配置文件 + 运行时覆盖）、
  运行时指标埋点（指标注册表 + 快照导出；见 `.scratch/wasm-core/`）
- **permission**：权限词汇**只读再导出**（bedcode-plugin-api 再导出；真源在
  `packages/plugin-sdk-desktop/rust/src/permission.rs`）。打包 CLI 与前端合法集读的是生成物
  （`plugin-sdk-desktop/bin/permission-vocabulary.json`、`src/plugin/permission-vocabulary.ts`），
  加/拆权限位后跑 SDK 的 `pnpm run gen:permissions` 重出；三副本一致性与「每条词汇都有门禁落点」
  由本文件 `wasm_core/permission.rs` 的词汇漂移锁断言（票 01）

### 宿主能力实现域 · WebSocket 基础能力服务 — `host-websocket`（ABI v14）

WIT 契约 `host-websocket`（14 函数，SDK `rust/wit/bedcode.wit`）、可选导出 `events-ws`、
ABI v14；宿实现 `wasm_core/host_api/ws.rs`。**零业务代码红线（ADR 0022）**：
宿主只做引擎原语（连接生命周期 / 帧收发 / 句柄登记 / 属主仲裁 / 按属主回收 / 事件定向投递），
消息格式、房间、协议、重连策略一律归插件。权限按域拆 `ws:client`（出站暴露面）/ `ws:server`（入站暴露面）。

- **客户端域（出站）**：`ws_connect`（同步阻塞至握手完成，仅 `ws://`，`wss://` 显式拒绝）→
  句柄 `wsc-<uuid>`；`send-text` / `send-binary`（有界队列，满 → fail-visible `Err`）、
  `close`、`is-connected`；每连接读写任务，帧经可选导出 `events-ws` 回灌，**不自动重连**；
  单插件连接数上限 `PLUGIN_WS_MAX_CONNS_PER_PLUGIN`；
- **服务端域（入站）**：`register-endpoint` 在宿主 WS 服务器挂载 `/ws/plugin/<plugin-id>/<path>`
  （命名空间段由宿主注入，插件只给后缀 → 插件间不存在路径抢占）；认证策略 `auth: none | jwt`
  （后者校验首消息 `{"type":"auth","token":"<jwt>"}`，超时 / 失败 close 4001）；
  收发原语 `send-text-to-client` / `send-binary-to-client` / `broadcast-text` / `broadcast-binary`、
  踢出 `close-client`（缺省 4004）、注销 `unregister-endpoint`（含下线全部客户端 4005）、
  清单 `list-clients` / `list-endpoints`（丢失事件后的自愈快照）；
- **端点注册表**：`server/websocket/endpoint.rs`（端点句柄 → 属主 / 挂载路径 / 认证策略 / 上限 / 事件总线）；
- **双通道投递**：状态事件走消息总线**属主私有 topic**（`<owner>::ws:open|error|close`、
  `<owner>::ws:client-connect|client-disconnect`，标识在 payload；票 05 命名空间门禁——
  跨属主订阅在 Rust 端显式拒绝，他人也伪投递不进）；
  消息帧走 `events-ws` 回调（未导出 → 丢弃 + 首次 `warn` + 计数，宿主不缓存）；
- **回收**：插件停用 → `ws::purge_for_plugin` 关闭并摘除其全部出站连接与入站端点（只碰本人，4005）。

### 宿主能力实现域 · 认证记录面 — `host-auth`（ABI v18）

WIT 契约 `host-auth`（v15 密钥托管四函数 + v18 记录面四函数，SDK `rust/wit/bedcode.wit`）、
**无可选导出**，ABI desktop 17 → 18（mobile 不跟演，见 ADR 0022「双端偏离」）；宿实现
`wasm_core/host_api/auth.rs`。**v24 认证记录下沉**：记录面四函数
（trusted-devices-list / revoke、connection-history-list）随 `pairings` / `connection_history`
表退役——配对设备 / 连接历史真源在认证中心私有库 `auth_records` 域（插件
`auth_records/`，表 `auth_pairings` / `auth_connection_history`）；存量迁移链
2026-09-23 用户裁定整体退役（不再兼容旧版本存量用户，旧库滞留表不读不迁不清理）；
`host-auth` 只保留密钥托管 / 生物凭证原语（bound/verify/bind，公钥在 `plugin_secrets`
key=`biometric:<fp>`）/ device-token / link-identity / setting。**裁剪线（ADR 0022）**：宿主只给
引擎级原语，排序、过滤、解读与展示组织全部归插件。

- **（v24 退役）** `trusted-devices-list` / `trusted-device-revoke` /
  `connection-history-list`：宿主主库 `pairings` / `connection_history` 已删表，三原语不再提供——
  配对记录读写归认证中心私有库（插件 `auth_records::records/revoke`），撤销语义不变
  （`is_active = 0` 软删 + 连带删连接历史；软删行保留供撤销检测）；
- **`auth-setting-set(key, value)`**：内核 `settings` 表写入，键白名单 `pairing_code_ttl` /
  `qr_token_ttl` + 正整数校验（宿主命令面据此取 TTL；读取走宿主配置 / 命令面）；
- **凭据红线（AGENTS §8）**：`pairings.session_token` / `public_key` 不出口，日志只记长度。

### 宿主能力实现域 · PTY 基础能力服务 — `host-pty`（ABI v16）

WIT 契约 `host-pty`（6 函数，SDK `rust/wit/bedcode.wit`）、**无可选导出**（push 输出模型已被
spec D3 否决），ABI desktop 15 → 16（v15 归 `host-auth`；mobile 不跟演，见 ADR 0022「双端偏离」）；
宿实现 `wasm_core/host_api/pty.rs`，引擎侧 `src-tauri/src/pty/`
（`pty_process.rs` / `pty_reader.rs` / `output_sink.rs` / `lifecycle.rs` / `pty_ring.rs`）。
**零业务代码红线（ADR 0022）**：只给裸伪终端原语，宿主不做 shell 包装 / 会话语义。
**P1-b（2026-09-24）后本接口就是业务会话的唯一 PTY 出口**——`com.bedcode.terminal-session` 用
`spawn` 创建业务会话（会话 id 插件自产、`BEDCODE_SESSION_ID` 由插件写进 `env`）、
`write`/`kill`/`resize`/`ring-fetch` 驱动输入停止尺寸与输出拉取。
**票 11（2026-09-24）后只剩这一张注册表**：内核 `SessionComponents` 的 PTY 注册表与
`GlobalOutputManager`（业务输出环）随 `src-tauri/src/session/` 整目录删除，业务会话与插件私有
PTY **同为引擎句柄**——输出读取一律归插件 `ring-fetch`（websocket 业务下沉票 08 起
宿主不再持有「会话 id → pty 句柄」广播映射，`hostBroadcastSessionId` 已删除；会话输出环
只有经插件互调 / 插件 WS 端点可达）。

- **创建域（`pty:spawn`）**：`spawn` 收 config-json `{command, args?, env?, workingDir?, cols?, rows?, ringBytes?}`
  （裸 argv exec，宿主不做 shell 包装 / WSL 转换 / 危险字符校验）→ 句柄 `pty-<uuid>` 并登记属主；
  `kill`（优雅 Ctrl-C → 兜底强杀）；每插件在册条数上限按 **manifest `ptyQuota` 自我声明**
  （加载期区间仲裁：0 或 > `PLUGIN_PTY_SESSIONS_CEILING_PER_PLUGIN`(64) 即拒 manifest，不夹取；
  未声明回落默认档 `PLUGIN_PTY_MAX_SESSIONS_PER_PLUGIN`(8)），`spawn` 判据取本属主生效配额；
- **数据域（`pty:io`）**：`write`（单次准入上限 `PLUGIN_PTY_MAX_WRITE_BYTES`，超限零写入）、
  `resize`（透传 winsize，不承诺同步生效时序）、`ring-fetch`（游标拉取，单次截到
  `PLUGIN_PTY_RING_FETCH_MAX_BYTES`，`truncated` 即 resync 信号）、`is-running`（自愈快照，
  判据 = `running && !output_terminated`）；
- **输出面**：每句柄一条 `PtyRing`（单生产者 + 全局偏移 + 字节/条目双上限，容量是 spawn 的插件声明
  参数，宿主以 `PLUGIN_PTY_RING_MAX_BYTES` 仲裁）；读线程经 `PtyRingSink`（`PtyOutputSink` 实现）
  投递，源侧零等待；
- **生命周期**：唯一事件 `<owner>::pty:exit`（属主私有 topic，票 05；payload `{ptyId, reason: stopped|killed|error, exitCode?}`），
  由 spawn 时起动的退出监听在 `PtyTerminationGate`（EOF + 子进程回收）齐备后发布，**exit 即摘除句柄与环**；
  终止 / 摘除 / 发布遵循单一发布者不变量（只有 `remove` 成功者发布，故三条路径恰好一条事件）；
- **回收**：插件停用 → `pty::purge_for_plugin`（`host.rs::deactivate_plugin_inner`，紧邻 mdns / ws
  回收、先于订阅注销）kill 并摘除本人全部 PTY、逐条补发 killed 事件；
- **属主隔离**：六函数一律先过权限门再查属主，他人句柄 `not owner of pty handle`，摘除后
  `pty handle not found`；fixture 闭环见 `packages/plugin-pty-test`（wasm32-wasip3）与
  `manager/runtime.rs` 的 `test_pty_*` 矩阵。

### 宿主能力实现域 · 并发任务 — `host-task`（ABI v20）

WIT 契约 `host-task`（5 函数：execute-batch / submit / status / cancel / list-jobs，desktop 独有
双端偏离——同 host-pty 先例）、可选导出 `events-task#on-task-event`（独立 world `plugin-task`
仅供 SDK 绑定，宿主实例化后动态探测，未导出降级丢弃 + 计数）。WASM 插件无法创建 OS 线程，
全部宿主调用同步阻塞——本接口让插件把「单元操作计划」交宿主专用 OS 线程池（
`PLUGIN_TASK_POOL_THREADS`）真并行执行既有宿主原语（fs.* / process.run-sync / http.fetch，
零新 DTO）。

- **执行引擎（core-task）**：`wasm_core/manager/task.rs`（TaskRegistry + 专用线程池 + 每任务
  并发窗口 + condvar 终态通知 + 每插件有界回调 channel/消费派发任务）。入口 `wasm_core/host_api/task.rs`
  做权限门（`task:run`）后经 **`TaskEngine` 接口**（两阶段注入，票 07）调用；单元执行经
  **`UnitExecutor` 注册表**分发（fs/process/http 执行器各自注入；`ensure_unit_path_granted`
  已随 fs 执行器归位 host_api/fs.rs）——host_api 与 manager 的双向依赖环在任务域收束
  （`task:run` 权限门 + plan 解析 + 配额仲裁；单元执行时另过 kind 对应域权限门——双门结构）；
- **两档 API**：`execute-batch` 同步扇出→join（阻塞 Store，同 run-sync 语义，仅限快操作）；
  `submit` 异步登记返句柄 `task-<hex>`，进度/终态经 `dispatch_task_event` → `events-task`
  回调（started → progress* → completed/failed/cancelled；回调尽力投递，`status`/`list-jobs`
  自愈快照是权威真源）；
- **配额（`system/constants.rs` `PLUGIN_TASK_*`）**：池 8 线程 / 每插件在册任务 4 /
  单 plan 256 单元 / 单元结果 1 MiB 截断 / 缺省单元 600s、任务 3600s 超时 / 回调队列 64 /
  status 终态结果保留 64 条——超限 fail-visible；
- **协作式取消/超时**：phase 翻转后未开始单元 skipped（快照补条目），运行中单元跑完结果照记；
  墙钟超时 → cancelled；
- **回收**：插件停用 → `task::purge_for_plugin`（`host.rs::deactivate_plugin_inner`，紧邻 pty
  回收）cancel 全部在册任务 + 清回调队列；
- **重入红线（spec §8）**：池线程永不回调进插件（回调只经消费派发任务 + 实例锁）；插件禁止
  在 guest 调用栈内同步等待自己任务的事件（自死锁），等待一律走 execute-batch；
- **fixture 闭环**：`packages/plugin-task-test`（wasm32-wasip3）+ `manager/runtime.rs` 的
  `test_task_*`（并行保序 / 事件管道 / status / cancel 幂等 / legacy 降级 / 双门权限）。

### 服务器 — `src-tauri/src/server/`（Actix Web HTTP + WS 单端口）

移动端与桌面端通信的唯一入口。三层读法：`core/` 传输无关内核、`http/` / `websocket/` 两个传输面；
`core/app.rs` 是**唯一**同时认识两面的文件（单端口组合物，I3 豁免，票 08 结构锁钉死）。
每个传输面的路由改动只落在自己目录的 `routes.rs`。

- **core/**（传输无关内核）：`app.rs` 单端口组合物（`start_http_server` + App 级 wrap——CORS / Logger /
  metrics 计数 / `TrafficFilter`——+ 只调两侧 `configure_routes` 的装配）；`supervisor.rs` 服务器生命周期 /
  mDNS 联动 / 指标采样；`port_checker.rs` 端口探测与冲突弹窗；`filter.rs` 跨传输流量过滤器链
  （TrafficFilterChain + TrafficChannel{Http,WsTerminal,WsEvent,WsPlugin}）；`metrics.rs` 跨传输计数器；
  `link_crypto.rs` 链路加密（HTTP 信封 + WS 帧两分支共享身份与配置，不可拆）
- **http/**（HTTP 传输面）：
  - **routes.rs**：两个公开端点（`/api/health` 健康检查、`/static/terminal-bg` 背景图，均不经 JWT——
    理由见各自注释；terminal-bg 按动态注册表门控）+ `/api` scope 与其 wrap 链（JWT 验签 → 业务网关，顺序硬约束）。
    ABI v29（HTTP 路由代码注册下沉）起宿主不再注册任何业务路由（`/api/sessions*` 七条与 `/api/auth/*` 归插件）
  - **registry.rs（ABI v29 新增）**：**插件 HTTP 端点动态注册表**——key = `(owner, host_path, method)`，
    命名空间由插件名承载（激活期同名拒绝）；内部端点路径 `/api/plugin/<owner>/<path>` 单独索引；
    对外 URL 空间唯一仲裁（同 `host+method` 冲突 → 后注册者 `Err`，不覆盖在位者）；host 路径支持
    `{id}` 模板段（捕获值经 `params` 注入插件，宿主不拿捕获值构造路径）；停用回收
    （`purge_for_plugin` 只碰本人）；宿主自有面（`/api/health`、`/api/plugin/*`）不可被插件别名占用
  - **gateway.rs**：**HTTP 协议网关**（平台基础服务）——查动态注册表（精确 + 模板）命中对外 URL 别名 →
    判定（纯函数 `decide(entry_auth, verified, activated)`：已验签 × 属主激活 × 档位）→ 转发；未命中 →
    原样放行交路由表。静态 `BUSINESS_ROUTES` / `BusinessDomain` / `SESSION_PLUGIN` / `FallbackPolicy`
    （双轨期）已随 ABI v29 删除——网关零业务路由常量。
    转发复用 `controllers/plugin_controller.rs::forward_to_plugin` 同一内核（不另发明传输机制），
    调用方身份同一出处（`caller_identity` → `caller` = device/localhost/anonymous + `device` 上下文）；
    载荷纪律：非转发分支不 `into_parts`，payload 原样留给宿主 handler
  - **controllers/**：`plugin_controller.rs` `ANY /api/plugin/{id}/{path}` 代理——属主解析（旧前缀别名）→
    **只认动态注册表登记**的内部路径（ABI v29 起，manifest 静态声明面已退役）→ 端点级认证档位
    （`auth: "none"` 之外一律要求已验签）→ 同一 `forward_to_plugin` 内核；`session_controller.rs` 已随
    sessions REST 下沉插件删除；**dtos/** 请求/响应 DTO（config / file / git / session 四组保留为形状
    契约锚点——插件面必须逐字节复刻）
  - **middleware/**：`jwt_auth` JWT 网关（宿主自持公开端点 `/api/health` 白名单 + 插件公开别名
    走注册表档位判定（`auth: "none"` 即公开，精确匹配非前缀）；具名中间件 `jwt_gateway`，
    协议网关必须挂在它**之后**——`Scope::wrap` 后注册者先执行，故 `http/routes.rs` 里网关写在验签之前）、
    `http_filter` HTTP 流量过滤器中间件
- **websocket/**（WS 传输面，websocket 业务下沉票 08 终态 = 通用 transport）：
  - **routes.rs**：只有一个握手端点 `/ws/plugin/{plugin_id}/{path}`（未注册 / 属主未激活 404、
    连接数超限**升级前** 503）+ `ws_frame_limit` + 属主激活闸门 `endpoint_owner_activated`。
    旧 `/ws/event` 与 `/ws/terminal/session/{id}` 已删除（请求得到通用 404，无 alias/fallback）
  - **conn.rs**：**通用连接骨架**（零业务语义）——心跳（5s ping / 45s 超时）、首消息认证策略
    `AuthMode{Required,None}` 与认证窗口、帧级流量过滤链（inbound / outbound）、注册表登记与
    认证态同步、连接终止原因（`CloseOutcome`）透出、优雅关闭；`ChannelHandler` trait 是通道协议
    的唯一切口（`auth_mode` / `auth_timeout_close_code` / 各生命周期回调）；连接级状态只剩
    地址 + JWT 主体身份（sub/deviceName/fingerprint），无订阅集合
  - **channel/plugin.rs**：唯一通道实现——插件端点（认证策略由端点声明、帧转投属主插件、
    接入/断开事件上报）；旧 terminal/event 通道已随业务硬切删除。**新增通道 = 新增一个实现 + 路由构造点，不改骨架**
  - **registry.rs / endpoint.rs**：连接注册表（owner/endpoint_id、端点域寻址与按属主回收；
    旧 `ChannelKind` / Event/Terminal 广播过滤已删除——只剩插件端点一类连接）；插件端点
    注册表（属主 + 挂载路径 + 认证策略 + 上限 + 总线；只碰本人的回收）
  - **websocket_manager.rs**：服务器生命周期与优雅停机（停机前对插件端点客户端下发 1001）+
    连接事实清单（`list_clients`/`client_count`，host-connection 原语入口）；宿主业务
    `Message` 发送/广播 API 已删除

### 会话 wire 形状（已退役 2026-09-25：形状契约归插件产出口）

> **零业务类型的收口**：`src-tauri/src/protocol/`（含 `session.rs` 的 `SessionInfo` /
> `SessionInfoView` / `SessionStatus` / `RendererSource` / `ResizeOutcome` 与会话形状锁）已
> **整目录删除**——宿主不再持有任何会话业务类型，窄转发层
> `utils/session_gateway.rs` 全部接口 `serde_json::Value` 原样透传插件 reply（
> 零解析零解释）。会话视图的形状契约（含错误 payload 形态与字段集合）锁在插件产出口
> `wasm-apps/terminal-session/rust/src/session/view.rs` 的形状锁测试 + 插件集成测试；
> 宿主 e2e 只做行为断言（创建/命名/裁决/filter 透传终态），不再复锁形状。
> `enums.rs` 的 `SessionStatus` / `SessionType` re-export 同批删除（引擎枚举
> `pty_status` 保留）。
> 按键组合 → 转义字节的翻译已迁插件（票 06），宿主 pty 只收裸字节。
> **SDK wire 真源不受影响**（会话事件下沉专项票 01）：同步载荷 / 会话概要 / WS 控制
> 与终端帧 / 按键组合四类形状仍在 `bedcode-plugin-api::wire`，宿主 `enums/` 对应四个
> 文件仍是 `pub use` 垫片 + 身份锁，移动端平行副本由
> `mobile_parallel_copy_shape_lock` 钉住。

### 会话 —— 真源在插件，宿主零会话对象（终态，票 11 / 2026-09-24）

> **`src-tauri/src/session/` 目录已整体删除**。会话登记 / 状态机 / 生命周期分发 / 创建 /
> 停止 / 输入 / 尺寸裁决 / 注解槽 / 业务输出环，全部只在
> `wasm-apps/terminal-session/rust/src/session/`（私有库 `sessions` / `session_annotations` 两表）。
> 对外 wire 形状契约（`SessionInfo` / `SessionInfoView` / `ResizeOutcome` /
> `RendererSource` 的 JSON 形态）2026-09-25 起已不驻宿主：`protocol/` 整目录删除，
> 形状锁归插件产出口 `session/view.rs`（宿主 `session_gateway` 零类型透传）。
>
> 宿主侧与会话相关的只剩三样，**都无业务语义**：
> ① **PTY 引擎** `src-tauri/src/pty/` + `host-pty` 原语（业务会话就是一个引擎句柄）；
> ② **在册连接清单** `host-connection` 原语（票 04，判据 `connection:read`）；
> ③ **互调窄转发层** `utils/session_gateway.rs`（宿主读/写会话事实的**唯一收口点**，纯互调 api，
> 插件未激活显性报错）。
>
> 已退役、不得回接：`host-session` / `host-terminal` 两个 WIT interface（ABI v27）、
> `terminal-hooks` 导出、`events` 的 `on-session-lifecycle` / `on-input-submitted`、
> 权限位 `session:write` / `terminal:observe`、内核输出环 `GlobalOutputManager` 与
> 「业务线 PTY 注册表」（票 11 起引擎注册表是唯一一张）。防回接锁
> `retired_kernel_session_domain_is_not_reintroduced`；边界裁决见 ADR 0022 v16。

### PTY 管理 — `src-tauri/src/pty/`

PTY 进程生命周期、输出读取与分发、游标环、WSL 发行版列举。**引擎零业务语义**
（2026-09-23 PTY 解耦票）：只接受调用方算好的 argv（`CommandBuilder`）——不做 shell 包装
（`bash -lic` / PowerShell `-Command` / CMD `/K`）、不做 WSL 路径转换、不注入业务环境变量。
旧 `pty/command.rs::build_command` 与 `pty/wsl.rs::windows_to_wsl_path` 已**删除**；shell 包装
与 WSL 路径转换的唯一实现是插件 `terminal-session/rust/src/launch.rs::build_argv`
（P1-b 起业务会话 argv 由插件自己算好经 `host-pty.spawn` 送入；内核线的 `create-with-spec.commandArgs`
**缺省即被显性拒绝**，仅剩测试与 legacy 消费方）。
`pty/wsl.rs` 只留发行版列举（`host-platform.wsl-distros` 原语）。构造入口两个：

- `PtySession::with_command`（**内核会话线**，P1-b 起生产零流量：sink = `session::SessionOutputSink`
  → `GlobalOutputManager`，`BEDCODE_SESSION_ID` 由 `session/session_manager.rs::launch_command` 注入——
  该函数是「业务配置 → 引擎 argv」的宿主侧翻译点，含 cwd 仅原生环境设置的规则）
- `PtySession::with_private_command`（host-pty 插件私有 PTY：`PtyRingSink` → `PtyRing` 游标环，
  argv 原样 exec、不注入任何业务环境变量。**P1-b 起业务会话也走这条**——插件在 spawn 的 `env`
  参数里自行注入 `BEDCODE_SESSION_ID`，配额由 manifest `ptyQuota` 声明）

`PtySlaveFdPolicy`（票 3）已统一为 spawn 后释放 slave（`ReleaseOnSpawn`），枚举退役——自然退出
在读取线程上以 EOF 可观测。终态由 `PtyTerminationGate` 汇聚「读线程 EOF + 子进程回收」两路信号，
恰好一条 `PtyTerminated{status, exit_code, killed}`（回收走专属 OS 线程阻塞 `wait()`，无轮询）。
**投递时序注意**：`mark_reader_closed` 只表示「尾帧已入有序队列」，`sink.on_bytes` 在独立消费者
任务里按序异步完成 → 终态事件到达 ≠ sink 已收到尾帧（消费方断言需有界轮询）。

### 全局事件系统 — `src-tauri/src/events/`（websocket 业务下沉票 08 已退役删除）

宿主 `AppEvent` trait + 统一 publish 入口 + 事件匹配处理器只服务一个消费者：插件
`host-events.broadcast-sync` → `HostSyncEvent` → `sync_handler` → WS `Message::SyncData` 广播。
websocket 业务下沉票 08 起宿主同步广播面整体删除（插件事件改 `host-bus.publish` +
`host-events.emit`，载荷插件自定义 JSON），`src/events/` 目录（app_event / matcher /
host_sync_event / sync_handler）随之移除；`AppContext.sync_tx` 与 `SYNC_EVENT_BROADCAST_CAPACITY`
同步清理。

**防回接锁**（随目录删除一并移除）：`retired_session_event_mirror_is_not_reintroduced` 等
三条锁锁的是「宿主事件面不解释产品事件」——真源清空后命名空间本就不存在，不再需要锁。

_历史（2026-09-24 专项票 01–04）_：插件 `host-events.broadcast_sync(event-json)` → 宿主反序列化
SDK `SyncEvent`（未知/畸形/旧格式即点名拒绝）→ `HostSyncEvent` 薄适配 → `events::publish`
（校验 → 查源 → 投递）→ `sync_handler`（折载荷 + 按源设备排除 + 广播）。WIT `broadcast-sync`
无返回值（ABI 稳定），宿主侧失败只落 `error!`。
### 对等网络 — `src-tauri/src/server/peer_net/` + `packages/peer-net`

跨设备可信直连底座，与终端链路（`_bedcode._tcp`）完全独立互不感知。
**传输编排已整体下沉插件（2026-09-25 票 1–3，WIT v31 终态）**：

- **底座 crate**：`packages/peer-net`（节点身份 identity / 自签证书 cert / TLS 1.3 直连 transport /
  信任存储 trust_store / `_bedcode-peer._tcp` 专用 mDNS 发现 discovery / 传输引擎 transfer）与
  `packages/link-crypto`（链路密码学），桌面端与移动端共享
- **server/peer_net.rs（引擎接入中枢）**：宿主侧薄封装——`NodeIdentity` 首启纯随机生成、与 `DeviceIdentity` 刻意分离
  （重装即新身份，不做设备标识派生）；节点生命周期由属主插件经原语驱动（谁起谁停）；首连确认闸门经
  `peer-consent-requested` 事件桥接（迁移规则匹配在前端层，见插件 useConsent）
- **插件入口（真入口）— WIT `host-peer`（v31）**：定义于 SDK `rust/wit/bedcode.wit`，宿主实现在
  `wasm_core/host_api/peer.rs`：`dial-peer`（按 endpoint 拨号，返 session 句柄）、`close`
  （session/传输句柄统一关闭）、`respond-consent`、`list-trusted`、`revoke-trusted`、
  `send-files`（**v31 收窄：一次调用 = 一个会话立即发起**，无宿主并发闸门；载荷 `concurrency`
  字段退役，出现即显性报错）、`respond-transfer`、`set-receive-policy`、`pause-transfer`、
  `resume-transfer`（活跃会话写 Resume 帧 / 会话已死按句柄表源清单重拨续传）、
  `set-shared-roots`、`list-shared-roots`、`browse-directory`、`pull-files`（v31 逐文件即发，
  会话发起直推 `pull-started` 事件）、`set-download-dir`、`start-node` / `stop-node`（审计票 12
  属主原语）、`active-transfers`（v31 实现改三处句柄面投影：send 会话句柄表 + receive pending
  询问表 + pull 会话表）、`collect-outgoing`（发送源枚举）。**v31 退役**：`resume-all-transfers`
  （「全部恢复」编排归插件逐批调用；旧产物实例化期 fail-visible 点名 v31 重建）；
  插件侧经 `HostPeer` trait 调用（`wasm-apps/file-transfer/rust/src/peer.rs`）
- **命令面（注册于 `lib.rs`）**：只剩节点生命周期（`start_peer_node` /
  `stop_peer_node`）、首连确认（`respond_peer_consent`）与信任管理
  （`list_trusted_peers` / `revoke_trusted_peer`）——后三者函数体同时是 host-peer 原语的
  真源（`wasm_core/host_api/peer.rs` 直接调用）。接收配置（策略/落点）、历史查询、
  远端浏览、发送编排等宿主命令已全部注销：设置面由插件经 `set-receive-policy` /
  `set-download-dir` 等原语推送引擎闸门（`peer_net::*_for_plugin`），任务/历史/并发/策略
  真源全部在 file-transfer 插件私有库
- **数据面（`server/peer_net/`，v31 终态 = 纯引擎控制面）**：
  `peer_engine_transfer.rs` = 发送会话句柄表（`batch_id → SendSessionHandle`：
  CancelToken/PauseSlot/epoch/sources——sources 是 redial 续传必需的引擎事实）+ send/serve
  两条通道的**引擎事件桥**（TransferEvent 逐条直推，Progress 150ms 节流——纯性能）；
  `peer_engine_receive.rs` = 询问回执表（OfferPending 的 oneshot 通道不可序列化必须留宿主）+
  接收事件桥 + 策略闸门（`PeerTransferSettings` 只剩 policy/timeout/download_dir）；
  `peer_engine_remote.rs` = 浏览/拉取会话（pull 句柄表 + pull-started 事件直推）；
  `source_collect.rs` = 发送源目录递归收集（纯文件系统事实，send-files 内部收集与
  collect-outgoing 原语共用）。**宿主不持有任务、设置或历史真源**——防回接锁
  `retired_peer_transfer_orchestration_is_not_reintroduced`（wasm_flow_test，15 符号源码扫描）
- **事件范式（v31 终态）**：引擎原始事件直推 `peer:transfer-event`（send 方向：本端发起批 +
  pull-served 供流记账）/ `peer:receive-event`（receive 方向：offer-pending 询问 +
  progress/terminal/paused/resumed + pull-started），载荷带 `tsMs`（wasm32 无时钟）。
  **旧快照 topic `peer:transfer` / `peer:receive` 已退役**。file-transfer 插件以事件归约
  状态机为唯一任务真源（`transfer_store.rs`：建行/推进/终态/原因码/封顶/重试回放单点），
  快照 merge 已随票 3 删除消费路径
- **业务归属**：传输 UI、任务状态、重试、策略、历史、共享根与发送并发闸门（插件侧
  `PENDING_SENDS` 队列自控）全部在 file-transfer 插件；
  宿主只提供 peer-net 引擎和 host-peer 原语。移动端不跟演（受损清单见
  `.scratch/2026-09-25-peer-transfer-orchestration-downsink/mobile-impact.md`）

### 插件开发 SDK — `packages/plugin-sdk-desktop/`

Rust 侧以 `abi.rs` 为宿主/插件共同引用的单一事实来源（签名漂移由测试暴露）；`wasm_host.rs` 以 WASM import 后端实现全部宿主能力 trait。TS 侧通过共享模块运行时（`__BEDCODE_SHARED__`）复用宿主的 Vue/Pinia/vue-i18n。

---

## Quick Navigation

### 按功能查找（定位到目录）

| 功能 | 目录 |
|------|------|
| Tauri 命令 | `src-tauri/src/commands.rs` |
| PTY 进程与输出 | `src-tauri/src/pty/` |
| **会话真源（登记 / 状态机 / 生命周期 / 输入输出编排）——宿主侧**唯一答案** | `wasm-apps/terminal-session/rust/src/session/`（P1-b，2026-09-24） |
| 宿主会话唯一入口（窄转发层，纯互调 api） | `src-tauri/src/utils/session_gateway.rs` |
| 会话引擎（PTY）与宿主直读输出环 | `src-tauri/src/pty/`、`wasm_core/host_api/pty.rs`（票 11 起唯一的 PTY 注册表与输出环） |
| HTTP/WS 服务器（core/http/websocket 三层）、REST 控制器、终端 WS | `src-tauri/src/server/` |
| 设备认证 / 配对 / QR Token | `src-tauri/src/utils/auth/` |
| 数据库 | `src-tauri/src/db/` |
| 全局事件系统 | `src-tauri/src/events/` |
| mDNS 广播 | `src-tauri/src/mdns/` |
| 对等网络（节点/信任/发现） | `src-tauri/src/server/peer_net.rs` |
| 对等传输引擎适配（发送/接收/远端浏览） | `src-tauri/src/server/peer_net/`（peer_engine_transfer / peer_engine_receive / peer_engine_remote） |
| 对等网络底座 crate | `../packages/peer-net`、`../packages/link-crypto` |
| 链路加密（HTTP 信封 + WS 帧） | `src-tauri/src/server/core/link_crypto.rs`、`src/composables/`（useLinkCrypto） |
| 插件系统 (Rust) | `src-tauri/src/wasm_core/` |
| 插件系统 (前端) | `src/plugin/`、`src/composables/`（usePluginManager） |
| 插件开发 SDK | `packages/plugin-sdk-desktop/` |
| 测试插件 | `packages/plugin-component-test/`、`plugin-sdk-test/`、`plugin-system-test/`、`plugin-wasi-test/` |
| wasm 应用源码 | `wasm-apps/agent-hub/`、`wasm-apps/ai-chatbox/`、`wasm-apps/file-transfer/`、`wasm-apps/terminal-session/`（终端会话中心：**会话真源登记域**（`rust/src/session/`，P1-b 起含状态机 / 生命周期分发 / 提交行重建 / 经 `host-pty` 的创建停止输入尺寸输出）+ 配对与信任 + 会话编排 + Agent 任务域 + 快捷指令域（票 02）+ 文件浏览域（票 03）+ **WS 会话控制词表分派**（`rust/src/ws_control.rs`，票 09b）+ **HTTP 路由代码注册**（`rust/src/http_routes.rs`，ABI v29：activate 期经 `host-http.register-endpoint` 注册全部路由）+ **sessions REST 域**（`rust/src/sessions_http.rs`：/api/sessions* 七条），票 17 起顶替旧 `com.bedcode.auto-task` 插件；HTTP 业务端点经动态注册表接管 /api/configs /api/quick-actions / 文件浏览五端点 / /api/auth/* / /api/sessions* / /static/terminal-bg） |
| 系统常量 / 错误类型 / 生命周期 | `src-tauri/src/system/`（constants.rs 按领域分组） |
| 应用上下文 (DI) | `src-tauri/src/system/`（app_context） |
| 前端页面 / 组件 / 状态 | `src/views/`、`src/components/`、`src/stores/` |
| 前端业务逻辑 | `src/composables/` |
| 国际化 | `src/locales/` |
| 前端测试 | `src/__tests__/` |
| 构建/开发脚本 | `scripts/` |

### 自动化任务执行机制（跨端链路概览）

BedCode 通过 `com.bedcode.terminal-session` 插件的任务域（WASM）+ HTTP API + WebSocket 事件链路实现移动端远程自动执行多个任务：

```
Claude Code Hook (Python/TS, wasm-apps/terminal-session/scripts/ 随包)
    ↓ HTTP POST /api/plugin/com.bedcode.terminal-session/...（旧 auto-task 前缀由宿主别名表应答）
com.bedcode.terminal-session WASM 任务域（任务状态/队列/模式，经 plugin_controller.rs 声明式端点路由）
    ↓ host-bus.publish（属主私有 topic）+ host-events.emit（插件自定义 JSON 载荷；
    websocket 业务下沉票 08 起宿主同步广播面已删——消息经 WS 插件端点原始帧直达移动端）
Mobile Tauri Event → useAutoExecutor 状态机（移动端）
    ↓ sendInput / HTTP API
Claude Code (PTY)
```

关键机制：

- **会话 ID 绑定**：PTY 启动时注入 `BEDCODE_SESSION_ID` 环境变量，Claude Code 子进程继承，hook 脚本读取后上报，插件维护 Claude session ↔ BedCode session 映射
- **模式切换**：移动端 HTTP 设置自动/手动模式 → 插件持久化 + 广播 → Python PreToolUse hook 查询模式决定 auto-approve
- **生命周期与输入观察**：v27 起 `terminal-hooks` 导出（`on_session_lifecycle` / `on_input_submitted`）
  已随会话原语域退役——agent 集成注入与提交行观察都在 `com.bedcode.terminal-session` 自己的
  编排路径里（`launch::run_creating_integration` / `session::input_line`），宿主不再回调插件

涉及目录：`wasm-apps/terminal-session/`（`rust/src/task/` + `rust/src/session/` + `src/components/TaskHistoryView.vue` / `TaskQueueModal.vue`）、`src-tauri/src/wasm_core/`、`src-tauri/src/server/http/controllers/`（plugin_controller）、`src-tauri/src/events/`、`src-tauri/src/pty/`。

### 按类型查找

| 类型 | 路径模式 |
|------|----------|
| Tauri Commands | `src-tauri/src/commands.rs` |
| 错误处理 | `src-tauri/src/system/error.rs` |
| 系统常量 | `src-tauri/src/system/constants.rs`（单一文件，`// ====` 按领域分组） |
| 数据库 | `src-tauri/src/db/*.rs` |
| 枚举类型 | `src-tauri/src/enums/*.rs` |
| WS 通用传输（server/websocket） | `src-tauri/src/server/websocket/`（插件端点 / 连接骨架 / 注册表） |
| DTO | `src-tauri/src/server/http/dtos/*.rs` |
| 前端插件系统 | `src/plugin/` |
| wasm 应用源码 | `wasm-apps/*/` |
| 插件 SDK | `packages/plugin-sdk-desktop/` |
| WASM 宿主能力 | `src-tauri/src/wasm_core/host_api/` |
| 插件随包 CLI 安装/卸载 | `src-tauri/src/wasm_core/host/` |
| 对等网络 | `src-tauri/src/peer_*.rs`（命令注册于 `lib.rs`） |
| 链路加密 | `src-tauri/src/server/core/link_crypto.rs` |
| 加密工具（报文/文件传输加密） | `src-tauri/src/utils/crypto/` |
| 认证工具 | `src-tauri/src/utils/auth/*.rs` |
| 前端测试 | `src/__tests__/**/*.test.ts` |
