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
2. 再读 Core Modules，按其中列出的**常改模块**落到具体目录
3. 用 Core Modules 末尾的「常改任务路由」按动作反查落点
4. 进入目标目录后用 `ls` / `rg` 查找具体文件（文件命名遵循 AGENTS.md 规范）

---

## Project Structure

```
bedcode-desktop/                      # 桌面端项目 (Tauri 2.0 + Vue 3)
├── scripts/                          # 构建/开发工具脚本：dev-run（Tauri dev 编排）、tauri-build（updater
│                                     #   签名密钥解析）、插件构建与热重载（plugin-build/plugin-dev/plugin-watch）、
│                                     #   产物大小检查（check-target-size）、图标生成（generate-icons 等）、
│                                     #   Linux 依赖安装（install-tauri-deps.sh）；含 README
│                                     #   ⚠ 插件机制整核 crate bedcode-wasm-core 已随能力域迁至仓库根
│                                     #   packages/（2026-10-08，见仓库根 packages/ 树；bedcode-desktop/packages/
│                                     #   只剩 plugin-* 契约与夹具 crate）
│   ├── plugin-sdk-desktop/           # 插件开发工具包，Rust + TS 双侧 SDK（含 dev-shell 调试壳、
│   │   │                             #   插件模板 template/、脚手架 bin/）
│   │   ├── rust/                     # bedcode-plugin-api crate：WASM ABI 契约（单一事实来源）、
│   │   │                             #   BedcodePlugin/WasmPlugin trait、宿主能力接口（host/ 按功能域拆分）、
│   │   │                             #   权限/SQL/命令参数辅助宏；rust-macros/ 为配套过程宏 crate
│   │   └── src/                      # TS SDK：插件类型定义、共享模块运行时代理（__BEDCODE_SHARED__）、
│   │                                 #   Vite 构建插件（vue/pinia/vue-i18n 外部化）
│   ├── plugin-sdk-fixtures/         # 宿主测试夹具合集（SDK 绑定形态）：http / task / pty / sdk /
│   │                                 #   ws / wasip3 六个夹具合一 crate（bench 独立，见下），按 cargo
│   │                                 #   feature 选夹具（**互斥**：一次构建一个，理由见该 crate
│   │                                 #   Cargo.toml）；各夹具 manifest 按 feature 分文件放
│   │                                 #   http.json / pty.json / …，根 plugin.json 归 sdk 夹具
│   │                                 #   （#[plugin_api] 宏编译期硬读该路径）；宿主侧
│   │                                 #   build_sdk_fixture(feature) 互斥锁串行构建并按 feature 归档产物
│   ├── plugin-p3-async-host-import-test/ # p3 async host import 测试插件（手写 wit-bindgen 绑定）
│   ├── plugin-bench-test/            # 性能基准夹具（独立 crate：749 行 / 29 命令，与功能闭环夹具性质不同）
│   ├── plugin-system-test/           # 系统组件形态测试插件 crate（导出 host-* 同形能力接口，验证能力装配
│   │                                 #   框架：注册表路由 / host-side 转发 / 依赖检查 / trap 隔离）
│   └── plugin-wasi-test/             # WASI preopen 测试插件（wasm32-wasip2，std::fs 直读写预打开目录；
│                                     #   同一 fixture 分钉可写档与只读档两种挂载）。**不参与夹具合并**：
│                                     #   实测 preopen 在 wasip3 上不工作（trap 于
│                                     #   filesystem_method_descriptor_open_at），合并不要求统一 target）
│                                     #   **preopen 仅 worker 类别可用（ADR 0034）**：本夹具是 worker 预留
│                                     #   能力的机制守门测试（在策略闸门之下，不走 manifest 校验路径）
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
│                                     #   **rust/ 已是薄层**：业务实现（台账归约 / 判据 / 注册表 /
│                                     #   设置 / 会话表）在共享核 `packages/bedcode-file-transfer-core`
│                                     #   （ADR 0044），端内只剩 `adapters.rs`（端口实现：plugin-db
│                                     #   注册表 / `path` 载荷 / 节点电源 / 多选与 reveal / 落点可自定义）
│                                     #   + 模块包装 + `peer.rs` 编排；编排已对齐移动端票 08（重试判据
│                                     #   前置 / 发送闸门 / 排队批派发失败落终态行 / 拉取意图先入队 +
│                                     #   事件通路挂载 / 停用清空进程内意图）；接线防漂移锁见文末索引
│   ├── terminal-session/             # 终端会话中心插件：**会话真源**（登记 / 状态机 / 生命周期分发 /
│   │                                 #   输入输出编排 / wire 形状）+ 配对与认证（编排 / 签发 / 验签 / 密钥环 /
│   │                                 #   设备信任）+ sessions REST 与其它 HTTP 端点注册 + 自动化任务域 +
│   │                                 #   会话 argv 构建
├── src/                              # Vue 3 前端（扁平化结构 + 领域子目录）
│   ├── components/                   # UI 组件：桌面布局、侧边栏、标题栏、退出确认、授权与审批弹窗、
│   │                                 #   通用基础组件（Button/Input/Modal/Toggle/Tooltip/…）；
│   │                                 #   settings/ 下为设置页分组子组件（外观/链路加密/系统/日志/关于/应用授权；
│   │                                 #   配对与会话分组已随域下沉 com.bedcode.terminal-session 插件）
│   ├── composables/                  # 业务逻辑 composable：桌面命令聚合（useDesktopCommands + commands/）、
│   │                                 #   服务器、插件管理、窗口编排、快捷键、主题、字体、日志设置、更新检查等
│   │                                 #   （配对 / WSL / 设备一族已随域下沉 com.bedcode.terminal-session 插件，
│   │                                 #   设备连接通知亦在其内；宿主只剩窗口编排原语 useSessionWindows）
│   ├── stores/                       # Pinia 全局状态：设置、i18n（会话状态已下沉 wasm-apps/terminal-session）
│   ├── views/                        # 页面：插件列表 / 插件详情 / 插件配置 / 通用插件窗口 / 服务器诊断页 /
│   │                                 #   设置编排层 / 应用授权总览（设备与会话页已随域下沉
│   │                                 #   com.bedcode.terminal-session 插件；落地页为 /plugins）
│   ├── plugin/                       # 前端插件系统：加载器、注册表、权限、上下文、事件、命令、
│   │                                 #   共享模块运行时、运行时事件监听（runtime-listeners）；
│   │                                 #   components/ 下为插件 UI 宿主组件（视图宿主 / 标题栏项 / 工具栏 /
│   │                                 #   状态栏 / 命令面板 / 设置分组）
│   ├── utils/                        # 工具函数（Tauri invoke 封装、格式化、前端 logger、用户可见错误、
│   │                                 #   授权策略读取、终端初始网格、页面过渡）
│   ├── locales/                      # 国际化（zh-CN / en，各含 common / desktop / settings）
│   ├── router/                       # 路由
│   ├── dev/                          # 开发调试资源：终端 mock、PTY 输出 dump
│   └── __tests__/                    # 前端测试（组件、composable、store、路由、视图）
└── src-tauri/                        # Rust 后端（Tokio 异步）
    ├── resources/                    # 打包资源：应用配置 + 内置插件构建产物（wasm/js/plugin.json）
    └── src/                          # 模块按领域扁平组织，每领域配同名入口文件（commands.rs、system.rs、pty.rs 等）
        ├── commands.rs               # Tauri invoke 命令层（单文件聚合）：宿主页面直调面——系统设置 / 终端背景 /
        │                             #   窗口关闭 / opener / updater / dev 日志转发 / 插件命令 re-export /
        │                             #   服务器控制 / 链路加密配置，按 `// ====================` 分隔分组；
        │                             #   只保留外壳 / 诊断 / 引擎事实，业务面一律归插件命令面
        ├── crypto.rs                 # 宿主加密引擎薄壳（保留 `crate::crypto::*` 公开名字）；真源与算法实现见
        │                             #   packages/bedcode-crypto-engine（注册表 + 各算法，link_crypto 与
        │                             #   host-crypto 只依赖其抽象接口，不再内联具体算法）
        ├── first_party_dirs.rs       # 第一方免弹窗目录归属清单（**产品数据真源**，ADR 0038 P0-2）：宿主
        │                             #   lib 唯一一份「哪些插件、哪些目录免弹窗」的清单，开机经
        │                             #   PluginHost::new 第 6 参注入 FsAuthChecker；判定逻辑（first_party_
        │                             #   dir_matches_with_home）与 TrustedDir 形态留 wasm-core security/
        │                             #   fs_auth.rs（机制与真源分离），空表 = 无豁免项
        ├── server/                   # 服务器宿主壳（传输面与内核已下沉 packages/bedcode-server-*，详见 Core
        │                             #   Modules）：composition.rs 组合根（唯一双面认识点 + 唯一端口装配点）、
        │                             #   ports_impl.rs 端口 traits 的宿主实现、host_port.rs 端口探测与冲突弹窗、
        │                             #   peer_net_cmds.rs peer 命令壳与上下文装配、crate_boundary_lock.rs
        │                             #   边界全图锁
        ├── system/                   # 系统模块：应用上下文 (DI 容器)、错误类型、错误边界、系统信息、
        │                             #   生命周期钩子、日志格式化、休眠阻止；constants.rs / error.rs /
        │                             #   error_boundary.rs 是 bedcode-server-base 同源模块的 `pub use` 薄壳；
        │                             #   config（AppConfig）/ opener / process 已随整核迁入
        │                             #   packages/bedcode-wasm-core（本目录 `pub use` 垫片，§6 规范）
        ├── utils/                    # 工具：auth/（auth_center.rs 桥接门/裁决面真源，纯净性收口票 05
        │                             #   回迁 lib；identity.rs 身份形状、真源 bedcode-server-base）、
        │                             #   session_gateway.rs（宿主调会话的**唯一收口点**真源，票 05 回迁——
        │                             #   纯插件互调 api，全接口 serde_json::Value 零解析透传）
        ├── lib.rs                    # 库入口（模块声明 + 日志初始化 + Tauri 应用搭建；对等网络模块的
        │                             #   Tauri 命令也直接在此注册，不经 commands.rs；wasm_core / db /
        │                             #   enums 为 `pub use bedcode_wasm_core` 垫片，见 §2 导引；
        │                             #   pty 已迁 bedcode-pty-engine，调用点写显式路径）
        └── main.rs                   # 二进制入口（panic hook）
```

**整核抽出后的结构要点**（wasm-core-whole-crate 票 02/05 + ADR 0039 + 2026-10-07 能力域
lib 迁根 + 2026-10-08 整核本体迁根）：插件机制真源已不在
`src-tauri/src/`——`wasm_core/`、`db/`、`enums/` 三者在
`packages/bedcode-wasm-core/`（2026-10-08 由 `bedcode-desktop/packages/` 迁根至仓库根），宿主侧只保留 `pub use` 垫片（lib.rs）与组合根
（`pty` 另有归属：`packages/bedcode-pty-engine`，ADR 0039）
（system/app_context.rs、server/）。**能力域 / 传输面 / 引擎 crate、机制内核
`bedcode-host-kit` 与整核本体 `bedcode-wasm-core` 同落仓库根 `packages/`**（迁根前在 `bedcode-desktop/packages/`，故本文
正文里裸写的 `packages/bedcode-*` 现在是仓库根路径）。
导航时先看本节 packages 树与仓库根 `packages/` 目录；改宿主侧任何「机制 / 引擎面」代码前
先确认落点是 crate（改错侧会被 `tests/wasm_core_whole_crate_lock.rs` 直接测红）。

---

## Core Modules（核心模块）

> 本文只回答「改 X 去哪个目录」。**只列常改的单元模块**，每个模块给「落点 + 一句话职责 + 改前注意」。
> 协议细节 / 权限语义 / 生命周期 / 设计理由在**源码文件头注释**与 `docs/adr/` 里，不在本文重复。
> 路径基准：相对仓库根 `BedCode/`；出现裸文件名时以该小节「落点」目录为基准。
> 文档不是事实：`ls` / `rg` 与本文冲突时以源码为准，并顺手修本文。
> 边界红线见 AGENTS.md §5.1（B1-B6）+ `docs/adr/0022-plugin-host-interface-primitive-boundary.md`；防回接锁索引见本文末。

### 分层总览（谁调谁）

```text
wasm-apps/<app-id>/rust/src/**      业务真源：会话 / 传输任务 / 配对 / Agent 任务 / 对话
        │ 四条下行通道：WIT host-* · host-bus · host-events · 互调 api（ADR 0017）
        ▼
能力域 crate（机制默认；`desktop-host` feature 装配 WIT 接线 + HostModule 自报——脱绑 ADR 0035）   packages/bedcode-{server-*,discovery-engine,crypto-engine}
        │ 端口 traits（bedcode-server-base::ports + 各能力域自己的 ports.rs）
        ▼
机制内核 packages/bedcode-host-kit（组件状态 / 能力模块契约 / 自动注册表）
        │ 装配（HOST_MODULES 白名单 + 强制引用行）
        ▼
插件机制整核 packages/bedcode-wasm-core（manager / host_api / security / bus / 引擎面 db·enums
        │   · 引擎级配置 system{config,opener,process}·HostBusPort；ADR 0037；
        │   auth_center/session_gateway 已回宿主 lib，PTY 引擎在 bedcode-pty-engine）
        │ 宿主 lib.rs `pub use` 垫片（wasm_core / db / enums；`pty` 已退出名单——ADR 0039）
        ▼
宿主壳 src-tauri/src/  组合根（app_context · server/composition）· 薄壳垫片 · system · commands
```

---

### 1 · 宿主壳 `src-tauri/src/`（薄壳，只装外壳与引擎接线）

| 落点 | 职责 / 导航要点 |
| --- | --- |
| `lib.rs` | 应用搭建与 bootstrap：模块声明、日志初始化、server 端口装配、插件启动、peer 命令注册 |
| `commands.rs` | 宿主页面直调命令面（`// ====================` 分组）；业务命令面一律归插件 |
| `server/` | 服务器宿主壳：`composition.rs` 组合根（唯一双面认识点 + 唯一端口装配点）、`ports_impl.rs` 端口实现、`host_port.rs` 端口探测弹窗、`peer_net_cmds.rs` peer 命令壳、`crate_boundary_lock.rs` 边界全图锁 |
| `system/` | DI 容器（`app_context.rs`）、常量、错误类型与错误边界、系统信息、生命周期、日志、休眠阻止；`config` / `opener` / `process` 为 `pub use` 垫片（真源随整核迁入 crate） |
| `utils/auth/` | 认证裁决桥接真源：`auth_center.rs`（L2 桥接门/裁决面，纯净性收口票 05 回迁）+ `identity.rs`（身份形状，真源 bedcode-server-base）；入场密码学不在宿主 |
| `utils/session_gateway.rs` | 宿主读 / 写会话事实的**唯一收口点**真源（纯净性收口票 05 回迁；纯互调 api，产品语义归宿主，不污染机制 crate） |
| `lib.rs` | wasm_core / db / enums 三个 `pub use bedcode_wasm_core` 垫片 + `pty` 的**反向**断言（ADR 0039 D3；见 §2 与 `tests/wasm_core_whole_crate_lock.rs`） |
| `crypto.rs` / `mdns.rs` | 薄壳：真源在 `bedcode-crypto-engine` / `bedcode-discovery-engine` |

**改前注意**：这里不放业务类型 / 状态机 / 业务真源表 / 业务 DTO 翻译 / 业务默认值 / 业务生命周期回调（AGENTS §5.1）。

---

### 2 · 插件系统内核（已整核抽出为 crate）

**落点**：`packages/bedcode-wasm-core/`（wasm-core-whole-crate 票 02/05）。宿主侧
`lib.rs` 以 `pub use bedcode_wasm_core as wasm_core;` 垫片保持既有
`crate::wasm_core::*` / `crate::db::*` / `crate::enums::*` 路径
零改动编译通过；`src-tauri/src/` 下**不再有** `wasm_core/` / `db/` / `enums/`
实现（反双份锁 `tests/wasm_core_whole_crate_lock.rs`；`pty` 连垫片也已退出名单，
该锁对此改为反向断言——ADR 0039 D3）。

**为什么整核抽成 crate**（spec §1/§2，ADR 0037）：插件机制本体 54,394 行 +
引擎面（db/pty/enums/AppConfig/opener/process/auth_center/session_gateway/
HostBusPort）曾长在 bin crate 里，不是库、无法被复用；任何 Tauri 宿主想要
插件机制只能把整栈搬进自己的 bin crate。抽出后 `bedcode-wasm-core` 可直接
path 依赖复用（D1/D2），宿主只剩组合根 + 薄壳垫片（D3），机制与真源同侧
（ADR 0036 延续，D4）。

| 模块 | 职责 / 导航要点 |
| --- | --- |
| `manager/` | 加载注册（`loader` `registry` `validation` `downloader`）、生命周期与实例调用模型（`host/` + `host.rs`）、wasmtime 运行时与能力模块白名单（`runtime/component.rs`）、任务引擎（`task.rs`）、能力路由闭表（`capability.rs`）、dev 热重载（`watcher.rs`） |
| `host_api/` | 宿主能力域实现，**一个文件 = 一个 `host-*` 域**；`context.rs` 是装配面（`WasmHostContext` + 角色窄接口 + 两阶段注入） |
| `security/` | 授权：框架（`framework.rs`）、fs 三层校验（`fs_auth.rs`）、出站授权（`network_auth.rs`）、策略与记录真源（`auth_policy.rs` + `strategy.rs`）、安装审批（`approval.rs`）、通道身份（`frontend_channel.rs`）、互调门（`api_registry.rs`） |
| `bus.rs` | topic 总线：命名空间仲裁 `<plugin-id>::<name>`、JSON + 二进制载荷、有界队列背压 |
| `config.rs` / `monitor.rs` | Engine / Store 运行参数（含灰度开关）／运行时指标埋点。`config.rs` 的 `EngineConfig.tuning` 是**wasmtime 定制面**（`.scratch/2026-10-07-wasmtime-engine-config/`）：显式钉死组（`component_model_async` / `backtrace_details` / `memory_may_move`，恒调用）+ 跟随默认组（其余 `Option`，`None` = 不调用 API = 逐字继承 wasmtime 默认）；逃生舱 `manager/runtime.rs` 的 `EngineCustomizer` / `EngineSetup`，优先级链与锁定 4 项见该处注释 |
| `runtime_util.rs` / `intercall.rs` / `storage.rs` / `permission.rs` | 中立层：同步↔异步桥／JSON-RPC 互调客户端／`PluginStorage`／权限词汇只读再导出 + 漂移锁 |
| `db/` / `enums/` | 引擎面（ADR 0036「机制与真源同侧」）：主库 schema.sql 单一事实源／引擎枚举。**无 `pty.rs`**：PTY 引擎与 host-pty 能力域已整面迁到 `bedcode-pty-engine`（ADR 0039），垫片与 `enums/pty_status.rs` 一并删除 |
| `system/` | AppConfig（引擎级配置）、opener、process——宿主 `system.rs` 经 `pub use` 垫片零改动消费 |
| `utils/` | 连接身份（`identity`，真源 base）与测试夹具（`test_tokens`，常编译 pub，lib 集成测试消费）；认证桥接（`auth_center`）与窄转发（`session_gateway`）**已回宿主 lib**（票 05） |
| `bus.rs` 的 `HostBusPort` | 包 `MessageBus` 的 BusPort 实现（lib `server/ports_impl.rs` 垫片再导出） |

**改前注意**：`host_api/` 只做「权限门 → 宿主服务调用」，业务字段与默认值不进这里；`manager/host/api_bridge.rs` 的身份由凭证绑定，参数自报 `plugin_id` 无效。
**先读**：该 crate 入口 `lib.rs` 的 `//!` 与各子模块头注释（职责 / 真源 / 不变量都写在那里）。

---

### 3 · 机制内核与能力域 crate

**落点**：**仓库根 `packages/`**——`bedcode-host-kit`（机制内核）+ `bedcode-host-api-core`（host_api 共享实现层，票 18 起）+ `bedcode-server-{base,core,http,websocket,peer-net}` / `bedcode-{crypto,discovery,pty}-engine`（能力域 / 传输面 / 引擎，2026-10-07 自 `bedcode-desktop/packages/` 迁根）。
**为什么不能合回宿主**：`inventory::submit!` 依赖被链接性，且能力 crate 必须能命名 `WasmPluginState` ⇒ Cargo 环路（ADR 0035）。

| 能力域 | 能力域 crate | 宿主端口 adapter |
| --- | --- | --- |
| `host-storage` + `host-database` + `host-plugin-database` | `bedcode-host-api-core`（实现层，票 18：批次 1 storage 已抽、database 待续；ADR 0036 端口机制仍留核心） | 同域即宿主 adapter：`packages/bedcode-wasm-core/src/host_api/{storage,database,sqlite,sqlite_ports}.rs` |
| `host-http`（入站端点 + 出站 fetch） | `bedcode-server-http`（`plugin_binding{.rs,/egress.rs}`） | `packages/bedcode-wasm-core/src/host_api/http.rs` |
| `host-websocket`（客户端域 + 服务端域） | `bedcode-server-websocket`（`plugin_binding.rs` `ports.rs` `endpoint.rs`） | `packages/bedcode-wasm-core/src/host_api/ws.rs` |
| `host-peer` | `bedcode-server-peer-net`（`plugin_binding{.rs,/ports.rs}`） | `packages/bedcode-wasm-core/src/host_api/peer.rs` |
| `host-mdns` | `bedcode-discovery-engine` | `packages/bedcode-wasm-core/src/host_api/mdns.rs` |
| 加密算法（`host-crypto` 与链路加密共用） | `bedcode-crypto-engine` | `src-tauri/src/crypto.rs` |

**新增 / 移除一个能力域模块时三处必须同改**（漏一处的能力会静默退回宿主原语）：① 能力 crate 内 `HostModule` 自报 ② `manager/runtime/component.rs` 的 `HOST_MODULES` + 强制引用行 `use <crate> as _;`（两行同处）③ 若可路由则 `manager/capability.rs` 的闭表 + 三层同名转发方法。
**改前注意**：能力 crate 只放机制，描述符只允许「接口路径 / 权限位 / ABI 下界」三类属性，禁产品名词（AGENTS §5.1 B1/B5）。

---

### 4 · server 面族

**落点**：仓库根 `packages/bedcode-server-{base,core,http,websocket,peer-net}/`
**依赖方向不变量**：传输面只向下依赖 base / core，面与面零横向 import；反依赖经端口 traits 倒置；唯一双面认识点是 `server/composition.rs`。

| crate | 职责 |
| --- | --- |
| `bedcode-server-base` | 错误 / 常量 / 错误边界 / 系统信息 / 网络配置形状 / 连接身份 + **端口 traits**（`ports.rs`） |
| `bedcode-server-core` | 传输无关引擎：组合装配（`app.rs` 的 `TransportFace` + `serve`）、生命周期、跨传输过滤链、链路加密、指标 |
| `bedcode-server-http` | HTTP 传输面 + `host-http`：`routes.rs`（宿主自有端点与 wrap 顺序）、`registry.rs`（插件端点动态注册表）、`gateway.rs`（协议网关）、`controllers/`、`middleware/auth_gateway.rs`、`dtos/` |
| `bedcode-server-websocket` | WS 传输面 + `host-websocket`：`routes.rs`、`conn.rs`（通用连接骨架）、`channel/plugin.rs`（唯一通道实现）、`registry.rs` / `endpoint.rs` |
| `bedcode-server-peer-net` | 对等网络引擎控制面 + `host-peer`（见 §7） |

**改前注意**：消息格式 / 房间 / 协议 / 重连策略 / 业务 DTO 一律归插件；宿主只做生命周期、帧收发、句柄登记、属主仲裁与按属主回收。认证闸门必须排在业务网关**之前**（wrap 顺序硬约束）。

---

### 5 · PTY 引擎

**落点**（wasm-core 纯净性收口票 02/06，两处）：

- **引擎面 + 能力域层（同一 crate）** `packages/bedcode-pty-engine/`——引擎面（进程生命周期 `pty_process`、读取线程 `pty_reader`、游标环 `pty_ring`、投递 sink `output_sink`、终态门 `lifecycle`、`PtySessionStatus` 词汇）零业务、可复用；能力域层 `src/plugin_binding.rs`（`bindgen!` + `HostModule` 自报 + 6 条原语）与 `src/plugin_binding/{primitives,registry,output,ports}.rs`（域机制 + `PtyPorts` 窄端口边界）是 **host-pty 的唯一实现面**（ADR 0039 D1/D2，与 http / ws / peer-net / mdns 四域同形）；
- **宿主侧只剩 adapter** `packages/bedcode-wasm-core/src/host_api/pty.rs`（`HostPtyPorts` 五方法实现 + `install` 双通道登记 + 白名单名 `HOST_MODULE_NAME`）；接线漂移锁 `host_api/tests/pty_wiring.rs`；内核反向锁 `tests::pty_module_must_not_return_to_wasm_core`（lib 侧对称锁 `wasm_core_whole_crate_lock.rs`）。

**改前注意**：引擎只收调用方算好的 argv——不做 shell 包装 / WSL 路径转换 / 业务环境变量注入，三者唯一实现在 `wasm-apps/terminal-session/rust/src/launch.rs`。终态事件到达 ≠ sink 已收到尾帧（投递异步按序），断言需有界轮询。引擎不感知宿主配置：容量与缓冲大小经 `PtyEngineConfig` 传入（默认 16/4096，与旧 AppConfig 默认一致）。

---

### 6 · 认证（宿主只剩裁决桥接）

**落点**（wasm-core 纯净性收口票 05 收口后）：

- **桥接门 / 裁决面（宿主薄壳真源）** `src-tauri/src/utils/auth/auth_center.rs`（`enforce_connection_policy` fail-closed 裁决 + `session_active` 激活门）——ADR 0022 四类薄壳②安全闸门，曾随 ADR 0037 迁入 crate，用户裁定回宿主 lib；
- **注册表 / WIT 绑定面（机制）** `packages/bedcode-wasm-core/src/host_api/{auth,auth_center}.rs`（secret-store / 设置写入 / link-identity / 认证中心注册面 + `auth-method-invoke` 实现链）与连接身份 `packages/bedcode-wasm-core/src/utils/auth/identity.rs`（真源 bedcode-server-base）；
- **测试夹具** `packages/bedcode-wasm-core/src/utils/auth/test_tokens.rs`（常编译 pub，依赖 crate 内部 `test_seed_plugin_secret`；lib 集成测试经 `bedcode_wasm_core::utils::auth::test_tokens` 消费）。
**真源在插件**：`wasm-apps/terminal-session/rust/src/{pairing/,auth_http/,auth_records/,keys.rs,trust/,devices.rs}`——配对编排、签发与验签、密钥环、生物公钥托管与验签、认证记录。

**改前注意**：入场密码学与生物凭证验签**不在宿主**（防回接锁 `host_has_no_entry_token_crypto`）；宿主绝不回查本地凭据表；无中心 / 调用失败 / 中心拒绝一律拒。

---

### 7 · 对等网络（peer-net）

**落点**：`packages/peer-net`（身份 / 证书 / TLS / 信任存储 / 专用 mDNS 发现 / 传输引擎）+ `packages/link-crypto`（链路密码学，双端共享）；引擎控制面与能力域在 `packages/bedcode-server-peer-net`；宿主命令壳 `src-tauri/src/server/peer_net_cmds.rs`。
**业务真源在插件**：`wasm-apps/file-transfer/rust/src/{transfer_store,settings_store,roots_registry}.rs`（**实现已迁双端共享核** `packages/bedcode-file-transfer-core/src/{transfer,settings,roots,sessions}.rs`，ADR 0044；端内文件现为薄包装 + `adapters.rs`，`transfer_store.rs` 是纯转出）。

**改前注意**：宿主不持传输任务 / 设置 / 历史真源（防回接锁 `retired_peer_transfer_orchestration_is_not_reintroduced`）。

---

### 8 · 存储（SQLite）

**落点**：核心内两处，均在 `packages/bedcode-wasm-core/src/`（wasm-core-whole-crate 票 02 随整核迁入；ADR 0036「机制与真源同侧」的落点从宿主 bin 平移至 crate）——

- **引擎面** `db/`（`db.rs` + `db/{database,models,operations}.rs` + `db/schema.sql`）：连接管理、主库单一事实源、幂等迁移、模型与设置项查询；
- **机制面** `host_api/`（`database.rs` 主库 5 / 插件私有库 5、`storage.rs` kv 3、`sqlite_ports.rs` 端口 trait、`sqlite.rs` 端口实现）：权限门、表名前缀纵深、护栏、属主分区、能力路由。

`bedcode-desktop/packages/bedcode-sqlite-engine/` 曾同时装这两面（wasm-core-lib-split 票 07/08），**已由 ADR 0036 整体撤销**：这三 interface 是 wasm_core 自己设计的插件机制，其实现（权限门 / 纵深 / 护栏）与真源（`plugin_auth_*` / `plugin_secrets` / `plugin_storage` 表、权限判定）必须同处一侧，拆开会让归属出现两个答案。端口 trait 保留为**可测性缝**（域逻辑用假端口跑护栏与隔离用例），不是架构边界。

**改前注意**：迁移必须幂等；业务表不进宿主主库（真源在各插件私有库）；执行护栏（超时 / 行数 / 字节 / 批次上限）与批次「全成或全回」语义是安全边界，不可由插件参数放宽。

---

### 9 · 前端宿主壳 `src/`

| 落点 | 职责 / 导航要点 |
| --- | --- |
| `plugin/` | 前端插件运行时：loader / registry / context / events / commands / permission / runtime-listeners / shared-runtime；`permission-vocabulary.ts` 是生成物 |
| `composables/` | 宿主业务逻辑（`usePluginManager` `useServer` `useSessionWindows` `useLinkCrypto` …）+ `useDesktopCommands.ts` 聚合层 + `commands/` 分域封装 |
| `views/` / `components/` | 页面与外壳组件（`components/settings/` 六个设置分组） |
| `stores/` / `locales/{zh-CN,en}/` | i18n 与设置状态；新增 key 两文件同步 |
| `__tests__/` | 前端测试；覆盖面以 `bedcode-desktop/vitest.config.ts` 的 `include` 为准 |

**改前注意**：前端零资源访问（禁 `fetch` / `XMLHttpRequest` / `WebSocket` / `EventSource` / `sendBeacon`，禁直读文件），能力发起权收归 Rust；UI 改动先读 `.agents/skills/frontend-styles/SKILL.md`；Rust `commands.rs` ↔ `useDesktopCommands.ts` ↔ `composables/commands/` 三层同改。

---

### 10 · wasm 应用工程 `wasm-apps/<app-id>/`（业务主场）

每个应用独立工程：`plugin.json` manifest + `rust/`（独立 crate）+ `src/` 前端 + `scripts/build.js`（wasm 产物 + wasmHash 注入）。

| 业务域 | 落点 |
| --- | --- |
| 会话：登记 / 状态机 / 生命周期 / 输入输出编排 / wire 形状 | `terminal-session/rust/src/session/` |
| 配对 / 认证 / 密钥环 / 设备信任 | `terminal-session/rust/src/{pairing/,auth_http/,auth_records/,keys.rs,trust/,devices.rs}` |
| sessions REST 与其它 HTTP 端点（激活期注册） | `terminal-session/rust/src/{sessions_http.rs,http_routes.rs}` |
| 自动化任务域 | `terminal-session/rust/src/task/` |
| 传输任务 / 接收策略 / 共享根 / 历史 | 共享核 `packages/bedcode-file-transfer-core/src/`（端内 `file-transfer/rust/src/` 为包装 + `adapters.rs`） |
| Agent CLI / Skills / 供应商 / 用量 | `agent-hub/rust/src/` |
| 多供应商对话 | `ai-chatbox/rust/src/` |

**改前注意**：验证在**各自 crate 根**跑 `cargo test`，宿主 `cargo test` 不覆盖；改插件前通读 `docs/knowledge/plugin-development-checklist.md`。

---

### 11 · 插件 SDK 与守门夹具

| 落点 | 职责 |
| --- | --- |
| `packages/plugin-sdk-desktop/rust/wit/bedcode.wit` | **WIT 真源（双端各一份）**：`host-*` interface 与可选导出（`events-*`）；改动须双端同步评估（ADR 0019 / 0022） |
| `packages/plugin-sdk-desktop/rust/src/` | `abi.rs` 宿主 / 插件共同契约、`host/` 能力 trait、`wire/` 会话 wire 形状、`permission.rs` 权限词汇真源、打包脚本 `bin/`（manifest 校验 / 词汇自检 / wasmHash 注入） |
| `packages/plugin-sdk-fixtures/` | 宿主 wasm 闭环夹具（feature 互斥：http / task / pty / sdk / ws / wasip3），由 `manager/runtime/fixture_build.rs` 串行构建 |
| `packages/plugin-system-test/` / `plugin-wasi-test/` / `plugin-bench-test/` | 系统组件形态、preopen（仅 worker 类别）、性能基准守门插件 |

---

### 验证命令速查

| 改了什么 | 必跑（cwd = 该 crate / 工程根） |
| --- | --- |
| 宿主 Rust | `cd bedcode-desktop/src-tauri && cargo test` |
| 任一 `bedcode-*` crate | 能力域 / 传输面 / 机制内核 / 插件机制整核：`cd packages/<crate> && cargo test`（2026-10-08 整核本体已与能力域同落仓库根 `packages/`） |
| 仓库根共享引擎（`packages/peer-net` `packages/link-crypto`） | `cd packages/<crate> && cargo test` |
| wasm 应用 | `cd bedcode-desktop/wasm-apps/<id>/rust && cargo test` |
| 前端 | `cd bedcode-desktop && pnpm run test:run`；仓库根 `pnpm exec eslint .`（0 error 门禁） |
| 跨端协议 / 任一端认证·会话·终端面 | 仓库根 `cd cross-end-tests && cargo test`（对面是移动端真实客户端代码） |
| `gen/android` Kotlin | `cd bedcode-mobile/src-tauri/gen/android && ./gradlew :app:compileUniversalDebugKotlin` |

命令字眼以 AGENTS §3 / `docs/commands.md` 为准（禁 `npm`、禁 `pnpm run test` 监听模式）。开发中只跑针对性过滤，收尾跑全量并贴结果。

---

### 常改任务路由

| 我要改… | 去这里 |
| --- | --- |
| 宿主直调命令 | `src-tauri/src/commands.rs`（业务命令归插件） |
| 插件加载 / 注册 / zip 安装 | `packages/bedcode-wasm-core/src/manager/{loader,registry,validation,downloader}.rs` |
| 插件激活 / 停用 / 资源回收 / 分类顺序 | `packages/bedcode-wasm-core/src/manager/host/{boot,activation}.rs` |
| `host-*` 原语 | 能力域 crate（§3）；仅当该域未 crate 化时落 `packages/bedcode-wasm-core/src/host_api/`；同时改 WIT + ABI 下界 + 旧产物 fail-visible |
| 权限位增删 | `packages/plugin-sdk-desktop/rust/src/permission.rs`（唯一真源）→ 重跑 `gen:permissions`，门禁落点须在宿主侧存在 |
| 授权策略 / 授权记录 / 弹窗判据 | `packages/bedcode-wasm-core/src/security/{auth_policy,strategy,fs_auth,network_auth}.rs`；**第一方免弹窗清单数据**在宿主 `src-tauri/src/first_party_dirs.rs`（ADR 0038 P0-2 注入 `FsAuthChecker`，机制与产品数据分离） |
| 前端调插件（invoke → 权限 → 执行） | `packages/bedcode-wasm-core/src/manager/host/api_bridge.rs` |
| 插件间通信 | `packages/bedcode-wasm-core/src/bus.rs`（topic 命名空间）／`packages/bedcode-wasm-core/src/security/api_registry.rs`（互调） |
| 并发任务 / 单元执行器 | `packages/bedcode-wasm-core/src/manager/task.rs` + `host_api/unit_executor.rs` |
| PTY 行为 / 会话 argv | `packages/bedcode-pty-engine/src/plugin_binding/`（域机制 + WIT 接线，ADR 0039）／ wasm-core `host_api/pty.rs`（仅端口 adapter）／ `wasm-apps/terminal-session/rust/src/launch.rs` |
| 会话事实 | `src-tauri/src/utils/session_gateway.rs`（真源，纯互调 api）；注册表真源在插件 `session/` |
| HTTP / WS 端点与路由 | `packages/bedcode-server-{http,websocket}/` + 组合根 `src-tauri/src/server/composition.rs` |
| 链路加密 / 过滤链 | `packages/bedcode-server-core/src/{link_crypto,filter}.rs` |
| 数据库 / 迁移 | `packages/bedcode-wasm-core/src/db/`（引擎面 + schema.sql 真源）+ 同 crate `host_api/{database,storage}.rs`（机制面） |
| 传输与设备信任 | `packages/bedcode-server-peer-net/` + `packages/peer-net/` |
| 前端页面 / 逻辑 / i18n | `bedcode-desktop/src/{views,components,composables,locales}/` |

---

### 防回接锁索引（改宿主 / 改能力 crate 前扫一眼）

越线回接会直接测红。锁名 → 所在文件：

- 内核会话域 / 会话观察面 / 传输编排真源 / 认证中心发现 → `packages/bedcode-wasm-core/src/manager/host/tests/wasm_flow_test.rs`（`retired_*_is_not_reintroduced` 四条）
- 宿主侧会话命令面 → `packages/bedcode-wasm-core/src/manager/host/api_bridge.rs`
- 宿主无入场密码学 → `packages/bedcode-wasm-core/src/manager/host/tests/l2_gating_test.rs`
- 已退役业务表不在宿主主库重建 → `packages/bedcode-wasm-core/src/db/database.rs`
- 可路由能力表 ↔ 转发方法同步 → `packages/bedcode-wasm-core/src/manager/capability.rs`
- 能力模块白名单双向一致 → `packages/bedcode-wasm-core/src/manager/runtime/component.rs`（同处 `stale_artifact_rebuild_hint`）
- 拆分 crate 零横向 / 向下边存在 / 双面认识点唯一 / 端口装配点唯一 → `src-tauri/src/server/crate_boundary_lock.rs`（登记表单一事源上提 `packages/bedcode-wasm-core/src/crate_boundary_lock.rs`，lib → crate 单向引用；含 `bedcode-pty-engine` 登记与 wasm-core → pty-engine 边）+ 各面 `dependency_direction_lock.rs`
- 整核抽出垫片（宿主侧 `wasm_core/` 无实现文件；lib.rs 的 wasm_core/db/pty/enums 是 `pub use` 垫片）→ `src-tauri/tests/wasm_core_whole_crate_lock.rs`（wasm-core-whole-crate 票 05 新增）
- PTY **反向**防回接锁（wasm-core 不得再有 `src/pty.rs` / `mod pty;` / `enums/pty_status.rs`——ADR 0039 D6）→ `packages/bedcode-wasm-core/src/lib.rs`（`pty_module_must_not_return_to_wasm_core`）+ lib 侧 `tests/wasm_core_whole_crate_lock.rs` 的 `pty` 反向断言
- 机制内核 fail-visible 三形态 → `packages/bedcode-host-kit/tests/registry.rs`；跨 crate 强制引用须两个测试二进制 `forced_link.rs` / `forced_link_absent.rs`
- 跨端 wire 形状 → `packages/bedcode-server-http/src/gateway/tests/`
- **能力 crate 的产品插件 id / 退役面词汇**（结构锁管不到的那一面：合法边位置上装的是不是业务代码）→ `src-tauri/tests/capability_crates_no_product_ids.rs`（扫 `packages/` 下能力 crate 的生产文本；登记表 `SCANNED_CRATES` + `PENDING_SCAN_CRATES` 两桶构成完整覆盖，新增能力 crate 必须归桶；`bedcode-pty-engine` / `bedcode-ws-client-engine`〔ADR 0043 抽根即入〕已入桶）
- **file-transfer 双端共享核的边界与接线防漂移**（ADR 0044）→ `packages/bedcode-file-transfer-core/src/boundary_lock.rs`（核内零 SDK / 零平台 needle + 零产品身份字面量 + `[dependencies]` 白名单 + 锁定面文件在场）+ 同目录 `src/wiring_lock.rs`（**接线防漂移** 5 例：端内不得复活核内 23 个判据函数 · 双端 `transfer_store.rs` 必须纯转出 · 双端 `adapters.rs` 必须实现已消费端口 · 核 `ports.rs` 的 `pub trait` 集合与登记清单精确相等；变异自检 5/5）
- **拆分 crate 不得长 `tests/` 目录 / `[dev-dependencies]` 不得拉入 bedcode crate**（crate 根 tests/ 是独立测试二进制 = 额外对外行为面 + 图上的真实 dev 边）→ `src-tauri/tests/capability_crates_unit_tests_only.rs`（**注意：该锁的治理面按 `packages/bedcode-*` 目录约定自动推导** ⇒ 新 crate 必须把锁写在 `src/` 并由 `#[cfg(test)] mod` 引入；`bedcode-file-transfer-core` 即如此，`bedcode-host-api-core` 的 crate 根 `tests/` 属在途红）
- **拆分产物 crate 的单测纯净性**（crate 根不得有 `tests/` / `benches/` / `examples/` 与 `[[test]]` 段；`[dev-dependencies]` 不得含任何内部 crate）→ `src-tauri/tests/capability_crates_unit_tests_only.rs`（治理面按 `packages/bedcode-*` 目录约定推导，新增 crate 自动纳入；`PENDING_GOVERNANCE` 现只登记 `bedcode-host-kit`——其链接期集成测试按设计保留，处置完删条目即自动纳入；含 C-5 正面钉住宿主侧迁移落点防“直接删掉”）。已迁走的两个集成测试落宿主：`src-tauri/tests/link_crypto_http.rs`（原 `bedcode-server-http/tests/`）、`src-tauri/tests/error_envelope_ipc.rs`（原 `bedcode-server-base/tests/`）
- 权限词汇三副本漂移 → `packages/bedcode-wasm-core/src/permission.rs`（真源 SDK `permission.rs`）
- 前端零资源访问三层封锁 → `src-tauri/tests/capabilities_lock.rs`；热路径日志 → `hot_path_logging_lock.rs`（`LOCKED_SITES` 随热路径迁至 `packages/bedcode-wasm-core`）
- 空目录残留（`foo.rs` + 空 `foo/` 双壳，git 与 CI 都看不见，会无声堆积）→ `src-tauri/tests/empty_dir_lock.rs`；覆盖面按**目录约定**（`*/src`）推导，新增 crate 自动纳入，不靠枚举名单

移动端自身的结构见 `bedcode-mobile/docs/code-map.md`。
