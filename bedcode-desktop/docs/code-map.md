# BedCode Desktop Code Map

本文档是桌面端代码探索的**目录级索引入口**：只记录**目录层级、模块职责与边界**，不索引具体文件 / 函数 / 协议细节——定位到目录后，用 `ls` / `rg` 在该目录内找文件。

**稳定性约定**：本文只放稳定信息（目录 / 职责 / 边界 / 锁），**改代码不需要同步更新本文**；仅在**目录增删、模块职责变化、新增防回接锁**时更新。

---

## 使用指引

**当用户命令包含以下动作时，先读本文档**：探索 / 查看代码结构、查找文件 / 定位模块、理解架构、修改某模块前了解上下文。

**阅读流程**：① 看 Project Structure 掌握目录布局 → ② 按 Core Modules 的「常改模块」落到目标目录 → ③ 用「常改任务路由」按动作反查落点 → ④ 进入目录后 `ls` / `rg` 找具体文件（命名遵循 AGENTS.md §6）。

---

## Project Structure

```
bedcode-desktop/                      # 桌面端项目（Tauri 2 + Vue 3）
├── scripts/                          # 构建/开发脚本（dev 编排、插件构建与热重载、产物检查、图标生成）
├── packages/                         # 插件契约与夹具 crate（plugin-*；机制/能力域/引擎 crate 已迁仓库根 packages/）
│   ├── plugin-sdk-desktop/           # 插件开发 SDK：Rust（bedcode-plugin-api + 宏）+ TS 双侧、模板、脚手架
│   └── plugin-*-test/                # 宿主测试夹具（sdk/http/task/pty/ws/wasip3 六合一）+ 系统形态 / WASI / 基准插件
├── wasm-apps/                        # wasm 应用（业务主场；各独立 package：plugin.json + rust/ + src/ + 独立构建）
│   ├── agent-hub/                    # Agent CLI 管理台（环境/技能/供应商/用量）
│   ├── ai-chatbox/                   # 多供应商 OpenAI 兼容聊天
│   ├── file-transfer/                # 文件传输（双端单产物；业务在共享核 packages/bedcode-file-transfer-core）
│   └── terminal-session/             # 终端会话中心（会话真源 / 配对认证 / HTTP 端点 / 自动化任务）
├── src/                              # Vue 3 前端
│   ├── components/ composables/      # UI 组件；业务 composable（桌面命令聚合 / 插件管理 / 窗口编排）
│   ├── stores/ views/                # Pinia 状态；页面（插件列表/详情/设置/诊断）
│   ├── plugin/                       # 前端插件系统（加载器/注册表/权限/上下文/事件/共享模块运行时/UI 宿主）
│   ├── utils/ locales/ router/       # 工具 / i18n（zh-CN+en）/ 路由
│   └── __tests__/                    # 前端测试
└── src-tauri/                        # Rust 后端（Tokio）
    ├── resources/                    # 打包资源（含内置插件产物）
    └── src/                          # 模块按领域扁平组织；plugin/<domain>.rs = 宿主插件能力面
        ├── commands.rs               # Tauri 命令层（宿主直调外壳；业务面归插件命令面）
        ├── server/                   # 服务器宿主壳（composition.rs 组合根 + 端口装配 + 边界锁）
        ├── system/                   # DI / 错误 / 生命周期 / 日志；config/opener/process 为 pub use 垫片
        ├── utils/                    # auth/（裁决桥接真源）+ session_gateway.rs（会话唯一收口）
        ├── lib.rs                    # 库入口（装配；wasm_core/db/enums 为 pub use 垫片）
        └── main.rs                   # 二进制入口（panic hook）
```

> 仓库根 `packages/bedcode-*`：机制内核 `bedcode-host-kit`、插件机制整核 `bedcode-wasm-core`、能力域 / 传输面 / 引擎 crate（server-* / crypto / discovery / pty / ws-client / host-api-core / headless-host-probe）。`bedcode-desktop/packages/` 只剩契约与夹具。双端 wasm 应用源码目录均名 `wasm-apps/`。

---

## Core Modules（核心模块）

> 只列常改模块，给「落点 + 一句话职责 + 改前注意」。协议 / 权限 / 设计理由在源码头注释与 `docs/adr/`，不在此重复。
> 边界红线见 AGENTS.md §5.1（B1–B6）与 ADR 0022；防回接锁索引见文末。

### 分层总览（谁调谁）

```text
wasm-apps/<app-id>/rust/**    业务真源（会话 / 传输 / 配对 / 对话）
        │ 四条下行通道：WIT host-* · host-bus · host-events · 互调 api
        ▼
能力域 crate（纯引擎 + 端口抽象；common-host feature 才装 WIT 接线）  packages/bedcode-{server-*,*engine}
        ▼
机制内核 packages/bedcode-host-kit（组件状态 / 模块契约 / 自动注册表）
        ▼
插件机制整核 packages/bedcode-wasm-core（manager / host_api / security / bus / db）
        │ 宿主 lib.rs pub use 垫片
        ▼
宿主壳 src-tauri/src/（组合根 · 薄壳 · system · commands）
```

### 1 · 宿主壳 `src-tauri/src/`（薄壳，只装外壳与引擎接线）

| 落点 | 职责 |
| --- | --- |
| `lib.rs` | 应用装配：模块声明、日志、server 端口、插件启动、peer 命令注册；wasm_core/db/enums `pub use` 垫片 |
| `commands.rs` | 宿主页面直调命令面（按 `// =====` 分组）；业务命令面一律归插件 |
| `server/` | 服务器宿主壳：`composition.rs`（唯一组合根 + 端口装配点）、端口实现、`crate_boundary_lock.rs` |
| `system/` | DI 容器 / 错误 / 生命周期 / 日志 / 休眠阻止 |
| `utils/auth/` | 认证裁决桥接真源（fail-closed 裁决；入场密码学不在宿主） |
| `utils/session_gateway.rs` | 宿主读写会话事实的**唯一收口点**（零解析窄转发） |
| `crypto.rs` / `mdns.rs` | 薄壳（真源在 bedcode-crypto-engine / bedcode-discovery-engine） |

**改前注意**：不放业务类型 / 状态机 / 业务真源 / 业务默认值 / 业务生命周期回调（AGENTS §5.1）。

### 2 · 插件机制整核 `packages/bedcode-wasm-core/`

宿主侧 `lib.rs` 以 `pub use bedcode_wasm_core as wasm_core;` 垫片保持旧路径编译通过；`src-tauri/src/` 下无实现（反双份锁）。

| 模块 | 职责 |
| --- | --- |
| `manager/` | 加载 / 注册 / 验证 / 下载、生命周期与实例调用模型（`host/`）、任务引擎（`task.rs`）、能力路由（`capability.rs`）、热重载（`watcher.rs`） |
| `manager/runtime/` | **单形态统一壳**：引擎机制（`engine.rs`）· 内核唯一合法绑定（`core_binding.rs`，绑 `wit/core.wit` 的 `world core`）· WASI 装配（`wasi.rs`，开/关走 `CoreConfig::wasi`）· 事件循环（`guest_events.rs`，各端宿主实现注入）· `runtime.rs`/`component.rs`（WasmRuntime / AbiPolicy / linker / 导出调用骨架，**零 `bindgen!`**） |
| `host_api/` | 宿主能力域实现（一个文件一个 `host-*` 域；端差异域已迁宿主 `src/plugin/<domain>.rs`）；`context.rs` 装配面、`ports.rs` 端口 |
| `security/` | 授权：框架 / fs 三层校验 / 出站授权 / 策略与记录 / 安装审批 / 互调门 |
| `bus.rs` | topic 总线（属主命名空间仲裁、JSON + 二进制、有界队列背压） |
| `config.rs` / `monitor.rs` | 引擎运行参数 / 运行时指标 |
| `db/` / `enums/` / `system/` | 引擎面（schema.sql 单一事实源 / 枚举 / AppConfig 等）；`pub use` 垫片消费 |
| `utils/` | 连接身份与测试夹具；认证桥接 / 窄转发已回宿主 |

**改前注意**：`host_api/` 只做「权限门 → 宿主服务调用」，业务字段不进；`manager/host/api_bridge.rs` 身份由凭证绑定。

### 3 · 能力域 crate（仓库根 `packages/`）

能力域 = 纯引擎 + 端口抽象（零 WIT / 零 SDK / 零端依赖）；WIT 绑定层收进 `common-host` feature（ADR 0035）。**新增 / 移除能力域时三处同改**：① crate 内 `HostModule` 自报 ② 白名单条目 + 强制引用行 ③ 可路由域同步 `manager/capability.rs` 闭表；迁宿主能力域另加 `submit_domain_ports_installer!` 装配自报；分片化后另加 ④ 自持 `wit/<domain>.wit` ⑤ 端清单 `compose.json` 登记 + 重拼。

| 能力域 | 能力 crate | 宿主 adapter / 落点 |
| --- | --- | --- |
| `host-storage` / `host-database` / `host-plugin-database` | `bedcode-host-api-core` | 内核 `host_api/{storage,database}.rs` |
| `host-http` | `bedcode-server-http` | 已迁宿主 `src/plugin/http.rs` |
| `host-websocket` | `bedcode-server-websocket` | 已迁宿主 `src/plugin/ws.rs` |
| `host-peer` | `bedcode-server-peer-net` | 已迁宿主 `src/plugin/peer.rs` |
| `host-mdns` | `bedcode-discovery-engine` | 已迁宿主 `src/plugin/mdns.rs` |
| `host-pty` | `bedcode-pty-engine` | 已迁宿主 `src/plugin/pty.rs` |
| 加密算法 | `bedcode-crypto-engine` | WIT 面已迁宿主 `src/plugin/crypto.rs` |
| `host-auth` | 注册表留内核 `host_api/auth_center.rs` | WIT 面已迁宿主 `src/plugin/auth.rs` |
| `host-task` 等无 crate 域 | 机制面留内核 | 已迁宿主 `src/plugin/{task,process,app,timer,connection}.rs` |
| 互调 `host-api-call` | 编排留内核 `host_api/api.rs` | 宿主 `src/plugin/api_call.rs`（薄转发） |
| 切片域扩展 / 端共享契约 | 交集留内核 `host_api/` + `core.wit` | WIT impl 已迁宿主 `src/plugin/{fs,platform}.rs`；`notify` 共享域契约真源 `packages/bedcode-notify-engine/wit/notify.wit` |

**核心 WIT 与分片拼装**：契约真源 = `packages/bedcode-wasm-core/wit/core.wit`（core 只收机制必需面，白名单见 ADR 0052 D1）+ 能力域各自 `wit/<domain>.wit` + 端 `rust/wit-src/cap-*.wit`；端生成物 `packages/plugin-sdk-desktop/rust/wit/` 由 `compose.json` 驱动 `scripts/compose-wit.mjs` 拼装（`pnpm run check:wit` 漂移锁，CI + husky 同判据）。包版本真源 = 仓根 `contracts/bedcode-plugin-package.rs`（ADR 0051 D3；整数 ABI 已退役）。

### 4 · server 面族

**落点**：`packages/bedcode-server-{base,core,http,websocket,peer-net}/`。**依赖不变量**：传输面只向下依赖 base/core，面与面零横向 import；反依赖经端口 traits 倒置；唯一双面认识点 = `server/composition.rs`。

| crate | 职责 |
| --- | --- |
| `bedcode-server-base` | 错误 / 常量 / 系统信息 / 连接身份 / **端口 traits** |
| `bedcode-server-core` | 传输无关引擎：组合装配、生命周期、过滤链、链路加密 |
| `bedcode-server-http` | HTTP 传输面 + `host-http`（端点注册 / 网关 / 出站执行 / 授权裁决） |
| `bedcode-server-websocket` | WS 传输面 + `host-websocket`（连接骨架 / 唯一通道实现 / 端点注册） |
| `bedcode-server-peer-net` | 对等网络引擎控制面 + `host-peer` |

**改前注意**：业务 DTO / 协议 / 重连策略归插件；认证闸门必须排在业务网关之前。

### 5 · PTY 引擎

- 引擎面 + 能力域：`packages/bedcode-pty-engine/`（进程生命周期 / 读取线程 / 输出投递 / `host-pty` 原语面）
- 宿主 adapter：`src-tauri/src/plugin/pty.rs`（端口实现 + 白名单 + 装配自报四件同处）
- 业务 argv 构建：`wasm-apps/terminal-session/rust/src/launch.rs`（唯一实现，引擎不包装）

**改前注意**：引擎只收调用方算好的 argv，不做 shell 包装 / WSL 转换 / 业务环境变量注入。

### 6 · 认证（宿主只剩裁决桥接）

- 裁决桥接（宿主薄壳真源）：`src-tauri/src/utils/auth/auth_center.rs`
- 注册表 / WIT 绑定（机制）：`packages/bedcode-wasm-core/src/host_api/auth_center.rs`
- 配对 / 签发 / 验签 / 密钥环真源：`wasm-apps/terminal-session/rust/src/{pairing/,auth_http/,keys.rs,trust/}`

**改前注意**：入场密码学与生物凭证验签不在宿主（防回接锁）；无中心 / 调用失败 / 拒绝一律拒（fail-closed）。

### 7 · 对等网络（peer-net）

- 底座：`packages/peer-net`（身份 / 证书 / 传输 / 信任存储 / 发现）+ `packages/link-crypto`
- 引擎控制面 + 能力域：`packages/bedcode-server-peer-net`
- 宿主命令壳：`src-tauri/src/server/peer_net_cmds.rs`
- 业务真源：共享核 `packages/bedcode-file-transfer-core/src/`（传输 / 设置 / 根 / 会话，双端单产物）

**改前注意**：宿主不持传输任务 / 设置 / 历史真源（防回接锁）。

### 8 · 存储（SQLite）

- 引擎面：`packages/bedcode-wasm-core/src/db/`（`schema.sql` 主库真源、幂等迁移）
- 机制面：同 crate `host_api/{database,storage}.rs`（权限门 / 表名前缀 / 护栏 / 属主分区）

**改前注意**：迁移必须幂等；业务表不进宿主主库（真源在各插件私有库）。

### 9 · 前端宿主壳 `src/`

| 落点 | 职责 |
| --- | --- |
| `plugin/` | 前端插件运行时（loader / registry / context / events / permission） |
| `composables/` | 宿主业务逻辑 + `useDesktopCommands.ts` 聚合 + `commands/` 分域封装 |
| `views/` / `components/` | 页面与外壳组件 |
| `stores/` / `locales/{zh-CN,en}/` | 状态与 i18n（key 双文件同步） |
| `__tests__/` | 前端测试（vitest include 为准） |

**改前注意**：前端零资源访问（禁 fetch/WS/文件直读，发起权收归 Rust）；UI 改动先读 `frontend-styles` skill；`commands.rs` ↔ `useDesktopCommands.ts` ↔ `composables/commands/` 三层同改。

### 10 · wasm 应用工程 `wasm-apps/<app-id>/`（业务主场）

每个应用独立工程：`plugin.json` + `rust/`（独立 crate）+ `src/` 前端 + 独立构建。

| 业务域 | 落点 |
| --- | --- |
| 会话 / 生命周期 / 输入输出编排 / wire 形状 | `terminal-session/rust/src/session/` |
| 配对 / 认证 / 密钥环 / 设备信任 | `terminal-session/rust/src/{pairing/,auth_http/,keys.rs,trust/}` |
| sessions REST 与 HTTP 端点 | `terminal-session/rust/src/{sessions_http.rs,http_routes.rs}` |
| 自动化任务域 | `terminal-session/rust/src/task/` |
| 传输 / 接收 / 根 / 历史 | 共享核 `packages/bedcode-file-transfer-core/src/` |
| Agent CLI / Skills / 供应商 / 用量 | `agent-hub/rust/src/` |
| 多供应商对话 | `ai-chatbox/rust/src/` |

**改前注意**：验证在各自 crate 根跑 `cargo test`；改插件前通读 `docs/knowledge/wasm-app-development-checklist.md`。

### 11 · 插件 SDK 与守门夹具

| 落点 | 职责 |
| --- | --- |
| `packages/plugin-sdk-desktop/rust/wit/` | **生成物**（由 compose-wit.mjs 拼装；改动契约改真源后重拼） |
| `packages/plugin-sdk-desktop/compose.json` | 端清单（caps / worlds / package 包名） |
| `packages/plugin-sdk-desktop/rust/src/` | SDK crate：WASM 契约 / trait / 权限词汇真源 / 打包脚本 |
| `packages/plugin-sdk-fixtures/` | 宿主 wasm 闭环夹具（feature 互斥） |
| `packages/plugin-system-test/` 等 | 系统形态 / WASI preopen / 性能守门插件 |

---

## 验证命令速查

| 改了什么 | 必跑（cwd = 该 crate / 工程根） |
| --- | --- |
| 宿主 Rust | `cd bedcode-desktop/src-tauri && cargo test` |
| 任一 `bedcode-*` crate | `cd packages/<crate> && cargo test` |
| 共享引擎（`packages/peer-net` `packages/link-crypto`） | `cd packages/<crate> && cargo test` |
| wasm 应用 | `cd bedcode-desktop/wasm-apps/<id>/rust && cargo test` |
| 前端 | `cd bedcode-desktop && pnpm run test:run`；根 `pnpm exec eslint .` |
| 跨端协议 / 认证·会话·终端面 | `cd cross-end-tests && cargo test` |
| WIT 拼装 | 根 `pnpm run check:wit` |

命令字眼以 AGENTS.md §3 / `docs/commands.md` 为准。

---

## 常改任务路由

| 我要改… | 去这里 |
| --- | --- |
| 插件加载 / 注册 / 激活 / 停用 | `packages/bedcode-wasm-core/src/manager/` |
| `host-*` 原语 | 能力域 crate（§3）；未 crate 化则 `packages/bedcode-wasm-core/src/host_api/` |
| 权限位增删 | `packages/plugin-sdk-desktop/rust/src/permission.rs`（真源）→ 重跑 `gen:permissions` |
| 授权策略 / 第一方免弹窗 | `packages/bedcode-wasm-core/src/security/`；清单数据 `src-tauri/src/first_party_dirs.rs` |
| 插件间通信 / 互调 | `packages/bedcode-wasm-core/src/bus.rs` / `security/api_registry.rs` |
| PTY / 会话 argv | `packages/bedcode-pty-engine/` + `src/plugin/pty.rs` + `terminal-session/rust/src/launch.rs` |
| 会话事实 | `src-tauri/src/utils/session_gateway.rs`；注册表真源在插件 `session/` |
| HTTP / WS 端点与路由 | `packages/bedcode-server-{http,websocket}/` + `src-tauri/src/server/composition.rs` |
| 数据库 / 迁移 | `packages/bedcode-wasm-core/src/db/` + 同 crate `host_api/{database,storage}.rs` |
| 传输与设备信任 | `packages/bedcode-server-peer-net/` + `packages/peer-net/` |
| 前端页面 / 逻辑 / i18n | `bedcode-desktop/src/{views,components,composables,locales}/` |

---

## 防回接锁索引（改宿主 / 改能力 crate 前扫一眼）

越线回接会直接测红。锁 → 所在文件：

| 防的是什么 | 所在文件 |
| --- | --- |
| 内核会话域 / 传输编排 / 认证中心发现回接 | `packages/bedcode-wasm-core/src/manager/host/tests/wasm_flow_test.rs` |
| 宿主侧会话命令面回接 | `packages/bedcode-wasm-core/src/manager/host/api_bridge.rs` |
| 宿主无入场密码学 | `packages/bedcode-wasm-core/src/manager/host/tests/l2_gating_test.rs` |
| 退役业务表在宿主主库重建 | `packages/bedcode-wasm-core/src/db/database.rs` |
| 可路由能力表 ↔ 转发方法同步 | `packages/bedcode-wasm-core/src/manager/capability.rs` |
| 能力模块白名单双向一致 | `packages/bedcode-wasm-core/src/manager/runtime/component.rs`（同处 `stale_artifact_rebuild_hint`） |
| 拆分 crate 边界 / 双面认识点唯一 / 端口装配点唯一 | `src-tauri/src/server/crate_boundary_lock.rs` + 各面 `dependency_direction_lock.rs` |
| 契约分片组合面（cap 真源 / 双向覆盖 / 包版本单点化） | `src/plugin/bindings.rs` cfg(test) + `src-tauri/tests/wit_package_version_lock.rs` + 根 `pnpm run check:wit` |
| 能力域迁宿主后的接线（白名单/装配器/权限同步） | `src-tauri/tests/pty_wiring.rs` + 各域 adapter 单测 + wasm-core `lib.rs::pty_module_must_not_return_to_wasm_core` |
| 整核抽出垫片（宿主无 wasm_core 实现） | `src-tauri/tests/wasm_core_whole_crate_lock.rs` |
| PTY 反向防回接（内核不得有 pty 面） | `packages/bedcode-wasm-core/src/lib.rs`（`pty_module_must_not_return_to_wasm_core`） |
| 路径 B 域反向防回接（auth/crypto/task 等 WIT impl） | `packages/bedcode-wasm-core/src/lib.rs`（`auth/crypto/path_b_domains_must_not_return_to_wasm_core`） |
| v36 切片域桌面扩展反向防回接 | `packages/bedcode-wasm-core/src/lib.rs::sliced_interfaces_must_not_return_to_wasm_core` |
| 端绑定面禁回接（内核唯一合法绑定 = core 交集） | `packages/bedcode-wasm-core/src/lib.rs::binding_surface_must_not_return_to_wasm_core` |
| wasm-core 端词汇全集（判据 7a/7b/7c） | `bedcode-mobile/src-tauri/tests/mobile_wasm_core_purity_lock.rs::wasm_core_carries_no_end_slice_or_sdk_vocabulary` |
| 第二份 wasm-core 织体禁回接（5 判据） | `bedcode-mobile/src-tauri/tests/retired_mobile_wasm_core_fork_lock.rs` |
| 契约归属边界三把锁（core 纯净 / caps 组合 / 域端中立） | `src-tauri/tests/domain_contract_structure_locks.rs` |
| 机制内核 fail-visible 三形态 / 强制引用 | `packages/bedcode-host-kit/tests/registry.rs` + `forced_link.rs` / `forced_link_absent.rs` |
| **能力模块描述符零产品名词（ADR 0035 D4）** | `packages/bedcode-host-kit/src/registry.rs`（词表真源）+ 扫描点 `packages/bedcode-wasm-core/src/manager/runtime/component/tests/whitelist.rs`、`src-tauri/tests/host_module_desc_noun_lock.rs`、`bedcode-mobile/src-tauri/tests/host_module_desc_noun_lock.rs` |
| 跨端 wire 形状 | `packages/bedcode-server-http/src/gateway/tests/` |
| 能力 crate 的产品插件 id / 退役面词汇 | `src-tauri/tests/capability_crates_no_product_ids.rs`（`SCANNED_CRATES` + `PENDING_SCAN_CRATES` 两桶） |
| file-transfer 共享核边界 / 单产物接线防漂移 | `packages/bedcode-file-transfer-core/src/{boundary_lock,wiring_lock}.rs` |
| file-transfer 单产物平台分支与移动桩 | `bedcode-mobile/src-tauri/src/plugin/desktop_compat.rs` |
| 拆分 crate 不得长 `tests/` / dev-dependencies 禁拉 bedcode crate | `src-tauri/tests/capability_crates_unit_tests_only.rs` |
| 权限词汇三副本漂移 | `packages/bedcode-wasm-core/src/permission.rs` |
| 前端零资源访问三层封锁 / 热路径日志 / 空目录残留 | `src-tauri/tests/{capabilities_lock,hot_path_logging_lock,empty_dir_lock}.rs` |

移动端自身的结构见 `bedcode-mobile/docs/code-map.md`。
