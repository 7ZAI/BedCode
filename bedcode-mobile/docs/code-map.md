# BedCode Mobile Code Map

本文档是移动端代码探索的**目录级索引入口**：只记录**目录层级、模块职责与边界**，不索引具体文件 / 函数 / 协议细节——定位到目录后，用 `ls` / `rg` 在该目录内找文件。

**稳定性约定**：本文只放稳定信息（目录 / 职责 / 边界 / 锁），**改代码不需要同步更新本文**；仅在**目录增删、模块职责变化、新增防回接锁**时更新。

---

## 使用指引

**当用户命令包含以下动作时，先读本文档**：探索 / 查看代码结构、查找文件 / 定位模块、理解架构、修改某模块前了解上下文。

**阅读流程**：① 看 Project Structure 掌握目录布局 → ② 按 Core Modules 落到目标目录 → ③ 用 Quick Navigation 按功能反查 → ④ 进入目录后 `ls` / `rg` 找具体文件（命名遵循 AGENTS.md §6）。

---

## Project Structure

```
bedcode-mobile/                       # 移动端项目（Tauri 2 + Vue 3，远程终端）
├── scripts/                          # 构建/开发脚本（dev 编排、logcat 落盘、插件构建、adb-fd0 自愈）
├── packages/                         # 插件 SDK（plugin-sdk-mobile）+ 测试插件（plugin-component-test）
├── wasm-apps/                        # wasm 应用（业务主场；各独立 package）
│   ├── ai-chatbox/                   # 多供应商 OpenAI 兼容聊天
│   ├── bedcode/                      # BedCode 应用：远程终端控制端（终端消费 UI + 订阅协议 + 任务域，与桌面 terminal-session 同 id 不同职责）
│   └── file-transfer/                # 文件传输（双端单产物：wasm 产物源在桌面同一 crate，本目录只剩前端）
├── src/                              # Vue 3 前端
│   ├── components/ composables/      # UI 组件；业务 composable（连接/HTTP/mDNS/通知/前台服务…）
│   ├── stores/ views/                # Pinia 状态；页面（设备/会话/设置/工具箱…）
│   ├── plugin/                       # 前端插件系统（loader/registry/context/events/UI 宿主）
│   ├── shell/                        # **宿主壳**（WASM 应用运行平台，路由 /mobile/shell；新功能默认落点，AGENTS §6）
│   ├── services/ styles/ locales/ router/ config/ assets/
│   └── __tests__/                    # 前端测试
└── src-tauri/                        # Rust 后端（Tokio）
    └── src/                          # 模块按领域扁平组织
        ├── connection/               # 远程连接（WS 客户端 / 心跳重连 / 消息编解码 / 连接管理）
        ├── auth/                     # 认证（HTTP 认证 / JWT / 生物挑战；配对编排已迁插件）
        ├── commands/                 # Tauri 命令层（认证/连接/mDNS/会话/终端…）
        ├── handler/ router/          # WS 消息处理器；消息路由
        ├── mdns/                     # mDNS 服务发现与广播
        ├── model/ enums/ system/ state.rs session.rs
        ├── peer/                     # 对等网络适配模块（net 装配与委托 / ports 宿主端口 /
        │                             #   seam 平台缝 / transfer·receive·remote 委托 / migration；见 Core Modules）
        ├── file_service/             # SAF 文件树读取
        ├── plugin/                   # 插件系统（宿主自报域 / 移动独有域 / 端口接线 / SAF 抽象 / Kotlin 桥）
        ├── lib.rs                    # 库入口（模块声明 + 命令注册；setup init 顺序敏感）
        └── main.rs                   # 二进制入口
```

> **Android 原生层**：`src-tauri/gen/android/app/src/main/java/com/bedcode/mobile/` 有自定义 Kotlin 插件（前台服务 / SAF / 生物识别 / 组播锁 / 通知 / 设备信息…），手写 Kotlin 是版本跟踪的一部分；改 Kotlin 后跑 `./gradlew :app:compileUniversalDebugKotlin` 并同步 `src-tauri/android-backup/app-java/` 恢复副本。
> **插件机制**：双端共用仓库根 `packages/bedcode-wasm-core`（单形态，`default-features = false`）；移动宿主只装配面在 `src-tauri/src/plugin/`。

---

## Core Modules（核心模块）

> 只列常改模块，给「落点 + 一句话职责 + 改前注意」。协议 / 权限 / 设计理由在源码头注释与 `docs/adr/`，不在此重复。
> 边界红线见 AGENTS.md §5.1 / §5.4 / §6；防回接锁索引见文末。

### 远程连接 — `src-tauri/src/connection/`

| 落点 | 职责 |
| --- | --- |
| `ws_client` / `ws_connection` | WebSocket 客户端主实现与连接管理（HTTP 信封加密保留） |
| `event_ws` | session-control 插件端点常驻事件通道（极简认证帧 + 事件帧 → 前端 `ws_sync_*`） |
| `heartbeat` / `reconnect` | 心跳保活、断线重连（退避下限与同因熔断在 `system/constants/`） |
| `codec` / `request` / `request_response` | 消息编解码、请求-响应关联 |
| `manager` / `lifecycle` | 连接管理器与生命周期 |
| `client_router` / `default_handler` | 客户端消息路由与处理 |

**改前注意**：配对 / QR / 生物挑战的**流程编排**在 `com.bedcode.terminal-session` 插件 auth 域（WIT `host-auth`，凭据零过境）；宿主只剩引擎事实 / 凭据面（`ws_authenticate` / `ws_get_auth_credentials` / 生物凭证绑定）。

### 终端链路（协议客户端与消费 UI 在插件 · 窄转发在宿主）

**落点**：`wasm-apps/bedcode/`（协议事实面 + 终端消费 UI）+ `src-tauri/src/plugin/terminal_stream.rs`（宿主窄转发）。

- **插件侧**：`rust/src/`（订阅状态机 / 帧构造与按键翻译 / 命令面 / 会话控制域）；终端 UI 在 `src/terminal/`（TerminalView / 渲染器 / 输入 / 滚动 / 主题）
- **宿主侧（零解析窄转发，ADR 0022 薄壳④）**：`plugin/terminal_stream.rs`（页面 Channel 表 + `host-terminal-stream.forward-output` 裸字节转发）+ `plugin/connection.rs`（`host-connection.primary-target` 主连接目标事实）

**改前注意**：协议 wire 形状真源 = 桌面 `ws_terminal.rs`（跨端协议，改动两端同步评估 §9）；连接经 host-websocket（`jwt-auth` 宿主代发认证帧，token 零过境插件）。

### 宿主 WS 出站连接域 — `host-websocket`（客户端子集）

**落点**：引擎 `packages/bedcode-ws-client-engine`（句柄表 / 双任务 / 心跳 / 退避重连 / 帧信封，零 WIT 零端依赖）+ 移动薄适配 `src-tauri/src/plugin/host_domains/ws.rs`（5 原语转发 + 端口注入：权限门 / 总线投递 / 任务 / jwt 代发 / 退避钳制）。

**改前注意**：移动 WIT 只含客户端子集（服务端域不跟演，边界锁）；订阅须在 activate 期完成且 manifest 同声明 `ws:client` + `bus` 两权限位。

### 插件核心模块引导

移动端插件系统与桌面同架构（wasmtime 组件沙箱，双端共用 `packages/bedcode-wasm-core`）。做插件相关改动按层定位：

- **机制核 + 统一壳（双端同一份源码）**：`packages/bedcode-wasm-core/`（bus / config / db / security / storage / 引擎机制 + 统一运行时壳；移动不开 `desktop-assembly`，dev 依赖加 `test-support`；WIT 绑定全在宿主）
- **移动端引擎配置（端事实）**：`plugin/manager.rs::mobile_core_config` —— 配置文件覆盖面（`CoreConfig::load_or_default`）叠加移动固定项：**关信号式 trap**（`EngineTuning::signals_based_traps = Some(false)`）。Android 的 ART 经 `libsigchain` 接管信号处理器 ⇒ 信号式 trap 会被判成非 wasm 故障并交回上一个处理器（WebView/crashpad）⇒ 任何 guest trap 变成宿主进程 SIGILL；关闭后 trap 由生成代码显式抛出（详见 `docs/knowledge/plugin-wasm-logging.md` 补充段）
- **权限真源（桌面同构，2026-10-14）**：内核 `PermissionManager`——激活时按生效权限授予（受信来源 = manifest 全量；用户安装 = 批准 ∩ 请求），停用时撤销；宿主 adapter 一律经 `WasmHostContext::{has_permission, granted_permissions, revoke_permissions}` 进出，**没有**移动独有的 `granted_permissions` 镜像  
- **移动宿主自报域**：`src-tauri/src/plugin/host_domains/`（只留端差异 4 域 http / peer / mdns / ws；双端全等 8 域由内核 `core_binding` 单份实现）+ `bindings.rs`（宿主 bindgen + 白名单双向校验）+ `guest_events.rs`（移动 events 导出派发器）
- **移动独有域（整面住宿主）**：`src-tauri/src/plugin/{notify,fs_mobile,terminal_stream,connection,auth}.rs`（各自 `impl Host` + 域逻辑 + Android 桥；`notify` 本体是共享域契约 `host-notify`）
- **端口与测试基建**：`plugin/host_ports.rs`（`HostEnginePorts` 宿主引擎端口）+ `plugin/peer_ports.rs` + 内核 `test_support`（无头夹具）
- **Kotlin 桥**：`plugin/android_plugins/`（前台服务 / SAF / 生物识别 / 全部文件访问 / 组播锁 / 通知 / 设备信息 → 对应 Kotlin Plugin）
- **前端插件系统**：`src/plugin/`（加载器 / 运行期 store 作用域 / 注册表 / 权限 / 上下文 / 事件 / UI 宿主）；扩展点 `registerNavTab` / `registerTerminalView` / `registerSettingsSection` 等
- **插件 SDK**：`packages/plugin-sdk-mobile/`（Rust + TS 双侧；SAF / 通知 / 路由等移动专属能力 + 模板 + dev-shell；开发指南 `wasm-app-dev-mobile.md`）
- **WIT 生成物**：`packages/plugin-sdk-mobile/rust/wit/`（由 `compose.json` 驱动 `scripts/compose-wit.mjs` 拼装；真源 = 内核 `core.wit` + 能力域 `wit/<domain>.wit` + 本端 `rust/wit-src/cap-mobile.wit`；包版本真源 = 仓根 `contracts/bedcode-plugin-package.rs`）

**改前注意**：wasm-core 内零端名 / 零机制名（AGENTS §5.4）；新增宿主能力 → `plugin/host_domains/`（core 域）或 `plugin/<domain>.rs`（独有域）+ SDK `rust/src/host/` trait；新增 Android 能力 → Kotlin Plugin + `android_plugins/` 注册桥。

### 对等网络 — 宿主适配模块 `src-tauri/src/peer/` + 共享能力域

与桌面镜像的跨设备可信直连底座（与终端链路 `_bedcode._tcp` 独立）。

| 落点 | 职责 |
| --- | --- |
| `packages/peer-net` + `packages/link-crypto` | 底座（identity / cert / transport / trust / discovery / transfer） |
| `packages/bedcode-server-peer-net` | 引擎控制面 + 引擎事件翻译 + `host-peer`（双端共享，移动只链引擎面） |
| `src-tauri/src/peer/net.rs` | 装配（`peer_ctx`）+ 节点生命周期 + 引擎入口委托 |
| `src-tauri/src/peer/ports.rs` | 宿主端口实现（总线 / 事件 / 路径 / 系统信息 / mDNS）+ 平台资源预热 |
| `src-tauri/src/peer/seam.rs` | 平台缝（Android SAF 共享访问 / MediaStore 落点提升 / 内置共享条目） |
| `src-tauri/src/plugin/host_domains/peer.rs` | `host-peer` WIT 薄转发（拨号 / 发送 / 应答 / 共享根 / 浏览 / 拉取） |
| `peer/transfer.rs` / `receive.rs` / `remote.rs` | 发送委托 + 选源 / 接收委托 + 策略 / 远端浏览拉取委托 |
| `peer/migration.rs` | 旧数据 → 插件存储键一次性幂等迁移 |

**改前注意**：宿主不持传输任务 / 设置 / 历史真源（在 file-transfer 插件共享核）；命令面零 Tauri 注册（防回接锁）。

### 前端终端链路（已迁插件，宿主只剩薄壳）

- 终端消费 UI 与协议客户端已整体迁 `wasm-apps/bedcode/src/terminal/`（AGENTS §6 壳 / 插件架构）
- 宿主侧只剩：`views/TerminalView.vue`（薄壳渲染插件注册的视图）+ `plugin/terminal-stream.ts`（页面字节通道）+ `plugin/host-events.ts`（会话事件白名单投影）

**改前注意**：终端 buffer / 渲染 / 滚动 / 键盘避让在插件工程内；宿主不实现终端业务逻辑。

### 自动化任务执行机制（移动端视角）

桌面插件任务域 → WebSocket 事件 → 宿主事件桥 → 前端同步 UI / 系统通知；模式切换走 HTTP（JWT 认证）→ 广播回同步。

**涉及目录**：`src/composables/`（useMobileConnection / useNotification / useHttpApi）+ `wasm-apps/bedcode/src/task/`（任务面板，业务在插件工程）。

---

## Quick Navigation

### 按功能查找（定位到目录）

| 功能 | 目录 |
| --- | --- |
| WebSocket 客户端 / 心跳 / 重连 | `src-tauri/src/connection/` |
| 终端链路（协议 + UI 在插件，宿主窄转发） | `wasm-apps/bedcode/` + `src-tauri/src/plugin/terminal_stream.rs` |
| 消息路由 / WS 处理器 | `src-tauri/src/router/`、`src-tauri/src/handler/` |
| 认证 | `src-tauri/src/auth/` |
| mDNS 发现与广播 | `src-tauri/src/mdns/` |
| Tauri 命令 | `src-tauri/src/commands/`、`src-tauri/src/system/` |
| 文件传输（SAF） | `wasm-apps/file-transfer/` + `src-tauri/src/plugin/{saf_io,saf_path}.rs` + `file_service/` |
| 对等网络（节点 / 传输 / 接收 / 浏览） | `src-tauri/src/peer/` + `packages/bedcode-server-peer-net` |
| 链路加密（HTTP 信封） | `src/services/linkCrypto.ts` |
| 多播锁（mDNS 前置） | `src-tauri/src/plugin/android_plugins/multicast_lock.rs` + Kotlin `MulticastLockPlugin` |
| 插件系统（Rust / 前端） | `src-tauri/src/plugin/`、`src/plugin/` |
| **移动端前端默认落点（宿主壳）** | `src/shell/`（规则见 AGENTS.md §6） |
| 插件开发 SDK | `packages/plugin-sdk-mobile/` |
| 插件源码 | `wasm-apps/*/` |
| 系统常量 / 错误类型 / 状态 | `src-tauri/src/system/`、`src-tauri/src/state.rs` |
| 国际化 | `src/locales/` |

### 按类型查找

| 类型 | 路径模式 |
| --- | --- |
| Tauri Commands | `src-tauri/src/commands/*.rs`、`src-tauri/src/system/commands.rs` |
| 数据模型 / 枚举 / 常量 | `src-tauri/src/model/`、`enums/`、`system/constants/` |
| 消息处理器 / 路由 | `src-tauri/src/handler/`、`src-tauri/src/router/` |
| Pinia Stores / Composables | `src/stores/*.ts`、`src/composables/*.ts` |
| 对等网络 | `src-tauri/src/peer/`（零 Tauri 命令面，见防回接锁索引） |
| 插件系统（Rust / 前端） | `src-tauri/src/plugin/*.rs`、`src/plugin/` |
| 插件源码 | `wasm-apps/*/` |

---

## 防回接锁索引（改宿主 / 改前端壳前扫一眼）

越线回接会直接测红。锁 → 所在文件：

| 防的是什么 | 所在文件 |
| --- | --- |
| 前端零资源访问（源码静态锁 + 运行期 ACL + 引擎级 CSP 三层） | 根 `eslint.config.js`（`bedcode/frontend-no-resource-access`）+ `src-tauri/capabilities/mobile.json` + `src-tauri/tauri.conf.json`（`security.csp`） |
| 壳层约束 L1–L7（零资源访问 / 不裸 invoke / 平台弹窗 / 无静默 catch / i18n / 无硬编码色 / **壳内自足**） | `src/__tests__/shell/shellConstraintLocks.test.ts` |
| 壳注册桥与插件域机制（运行面解析 / 设置门类 / 连接事件投影 / 业务设置域） | `src/__tests__/{plugin/pluginContextShell,shell/useShellAppsSurface,shell/shellSettingsScreen,plugin/connectionEvents}.test.ts` + `wasm-apps/bedcode/src/{host,task,app,settings}/__tests__/**` |
| 旧宿主 UI 退役面不回接（R1–R5 退役路由 / 组件 / 扩展点；含壳内底部系统条 `ShellSystemBar` 退役，R3 新增「不得回到磁盘」正面钉） | `src/__tests__/shell/retiredHostUIRetirementLocks.test.ts` |
| 发送 / 接收编排退役面（含 `src/peer_receive/tests/` 不得复活） | `src-tauri/tests/retired_mobile_send_orchestration_lock.rs`、`retired_mobile_receive_orchestration_lock.rs` |
| 传输 / 远端浏览调度与设置命令面退役 | `src-tauri/tests/retired_mobile_peer_transfer_command_face_lock.rs` |
| 发现投影 / 设备列表投影退役 | `src-tauri/tests/retired_mobile_peer_discovery_projection_lock.rs` |
| 终端订阅协议客户端退役 / host-terminal·terminal-hooks 整面退役 | `src-tauri/tests/retired_mobile_terminal_link_lock.rs`、`retired_mobile_host_terminal_hooks_lock.rs` |
| auto-task 独立插件退役 / 本地配对码编排退役 / 认证编排命令面退役 / 会话控制客户端退役 | `src-tauri/tests/retired_mobile_{auto_task_plugin,local_pairing_code_face,auth_orchestration_command_face,session_control_face}_lock.rs` |
| 插件 KV 真源 = 主库 `plugin_storage` 表 | `src-tauri/tests/plugin_storage_db_backed_lock.rs` |
| **主库 schema 单真源 = 内核 `db/schema.sql`（宿主不自持 DDL / 退役 `db_schema` 副本不回接 / 真源接线在场）** | `src-tauri/tests/kernel_schema_single_truth_lock.rs` |
| host-websocket 客户端子集域边界 | `src-tauri/tests/mobile_host_websocket_client_domain_lock.rs` |
| egress 三档策略架构不变量 | `src-tauri/tests/egress_tier_mapping_single_point_lock.rs` |
| 移动 fork crate 退役面（第二份 wasm-core 织体，5 判据） | `src-tauri/tests/retired_mobile_wasm_core_fork_lock.rs` |
| **wasm-core 纯净性防回接（7 判据：域不回流 / 绑定不回流 / 共享核在场 / 壳行数上限 / 退役垫片不回流 / 引擎不复活 / 端词汇全集）** | `src-tauri/tests/mobile_wasm_core_purity_lock.rs` |
| 能力模块描述符零产品名词（移动扫描点） | `src-tauri/tests/host_module_desc_noun_lock.rs`（词表真源 `packages/bedcode-host-kit/src/registry.rs`） |
| 契约归属边界三把锁（跨端） | `bedcode-desktop/src-tauri/tests/domain_contract_structure_locks.rs` |
| 对等网络能力域集中化 / mDNS 广播死面退役 | `src-tauri/tests/{peer_engine_consolidation_lock,retired_mobile_mdns_advertise_lock}.rs` |
| 移动 WIT 分片拼装组合（包名 / 版本行 / include 集） | `src-tauri/tests/mobile_wit_composition_lock.rs` + 根 `pnpm run check:wit` |
| Tauri 命令面注册完备性（`#[tauri::command]` 须进 `generate_handler!`） | `src-tauri/tests/command_registration_completeness_lock.rs` |
| 出站 HTTP egress 闸门覆盖（reqwest 客户端可追到 egress 引用） | `src-tauri/tests/outbound_egress_gate_coverage_lock.rs` |

桌面端自身的结构见 `bedcode-desktop/docs/code-map.md`。
