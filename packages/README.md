# `packages/` —— 共享 crate 索引

本文件是仓库根 `packages/` 下各 crate 的**作用记录与导航索引**：回答「这个 crate 是干什么的、什么形态、谁在用」。

- **真源不在本文**：机制细节以各 crate 的 `Cargo.toml` description、`src/lib.rs` 头注与 `docs/adr/` 为准（ADR 0035 / 0037 / 0038 / 0040 / 0045 / 0047 / 0049 / 0052）；本文只做索引，不复制机制，避免双份漂移。
- **边界总则**（AGENTS §0 / §5.4）：能力域 / 传输面 / 引擎 crate 一律落本目录（与机制内核 `bedcode-host-kit` 同族）；能力域默认形态 = **纯引擎 + 端口抽象**（零 WIT / 零桌面 SDK 依赖），桌面 WIT 绑定层收进 `common-host` feature；长期门禁 = `bedcode-headless-host-probe`。
- **维护约定**：新增 / 删除 / 更名 crate、或某 crate 的职责与形态变化时，同步本文件与两端 `docs/code-map.md`（AGENTS §12 / §13）；引用路径 / 命令前先 `ls` / `rg` 核对（AGENTS §0「文档字面 ≠ 事实」）。

```text
插件（wasm-apps / 第三方）
   │  WIT host-* · bus · events · 互调
   ▼
能力域 crate（纯引擎 + 端口；common-host 才装 WIT 接线）   server-* / *-engine / notify-engine
   ▼
机制内核 host-kit（装配期：状态 / 模块契约 / 注册表）
   ▼
机制整核 wasm-core（运行期：统一壳 + core 绑定 + adapter，消费 host-api-core）
   ▼
两端宿主 src-tauri/src/（组合根 · 薄壳）
```

## 一览表

| crate | 作用（一句话） | 形态 / feature | WIT 真源 |
| --- | --- | --- | --- |
| `bedcode-host-kit` | 插件宿主**机制内核**：`WasmPluginState` / `HostModule` 契约 / 自动注册表 / 端口 marker / 限制与指标（装配期） | 无 feature | — |
| `bedcode-host-api-core` | host_api 各域**实现层**（运行期机制语义 + 端口 trait）：storage / bus / database / log / events / fs | 无 feature | — |
| `bedcode-wasm-core` | **插件机制整核**（单一形态）：wasmtime 组件宿主、统一运行时壳、core 绑定、host_api adapter、db 引擎面 | `desktop-assembly`（桌面装配面）· `test-support` | `wit/core.wit` |
| `bedcode-server-base` | server **叶子地基**：错误 / 常量 / 错误边界 / 系统信息 / 网络配置形状 / 端口 traits | `tauri-compat`（默认开） | — |
| `bedcode-server-core` | 与传输无关的 server 内核：生命周期 / 过滤链 / 链路加密 / 指标 / `TransportFace` 组合装配 | — | — |
| `bedcode-server-http` | HTTP 传输面 + `host-http` 绑定层（入站 2 + 出站 1 原语） | `common-host` | `wit/http.wit` |
| `bedcode-server-websocket` | WebSocket 传输面 + `host-websocket` 客户端域 WIT 接线与服务端引擎域函数 | `common-host` | `wit/ws.wit`（客户端域） |
| `bedcode-server-peer-net` | 对等网络引擎域 + `host-peer` 绑定层 | `common-host` | `wit/peer.wit` |
| `bedcode-discovery-engine` | mDNS 组播发现能力域：共享守护 + 浏览 / 广播句柄表 | `common-host` | `wit/mdns.wit` |
| `bedcode-pty-engine` | PTY 能力域 + 可复用伪终端引擎（进程生命周期 / 读取线程 / 环形缓冲） | `common-host` | `wit/pty.wit` |
| `bedcode-ws-client-engine` | WS **出站客户端**能力域：句柄表 / 双任务 / 心跳 / 退避重连 / 帧信封 / 停用回收 | 无 feature | `wit/ws.wit` |
| `bedcode-notify-engine` | 通知域**契约房**（只持 WIT，零实现） | 无 feature | `wit/notify.wit` |
| `bedcode-crypto-engine` | 宿主加密引擎：算法实现 + 名称寻址注册表（provider / registry） | 无 feature | — |
| `peer-net` | 对等网络共享底座：Ed25519 节点身份 / 自签证书 / TLS 直连 / 信任存储 / mDNS 发现 | 无 feature | — |
| `link-crypto` | 局域网链路报文加密协议核心（HTTP 信封 / WS 握手 / 帧信封） | 无 feature | — |
| `bedcode-file-transfer-core` | file-transfer **双端共享业务核**（可共享面收敛一份，差异全部落端口 trait） | 无 feature | — |
| `bedcode-componentize` | 构建工具（bin）：core module → Component Model 组件 | — | — |
| `bedcode-headless-host-probe` | 无头宿主探针（长期门禁）：非桌面形态引用全部能力域，钉死默认形态可被任意宿主引用 | — | — |

> 夹具 crate `bedcode-host-kit/fixtures/forced-link-probe` 未入表，见文末「夹具」段。

## 机制内核层

### `bedcode-host-kit` —— 装配期机制锚点

- **是什么**：`state`（插件实例状态，含 WASI 槽）/ `ports`（宿主能力端口 marker + 向下转型出口）/ `module`（`HostModule` 能力模块契约 + 提交类型）/ `lifecycle`（装载 / 停用钩子）/ `assembly`（端口装配自报）/ `route`（能力路由词汇）/ `registry`（收集 → 排序 → 白名单 → linker 装配）/ `limits` / `metrics`。
- **为什么独立**（头注有实测记录）：① 被链接性——`inventory::submit!` 是 linker-section 静态，未被引用的 rlib 不进最终二进制；② Cargo 环路——能力 crate 必须能命名 `WasmPluginState` 与提交类型，若住在宿主 bin crate 会与宿主形成依赖环（Cargo 硬拒）。
- **消费方**：双端宿主、全部能力域 crate（`common-host`）、`bedcode-wasm-core`。

### `bedcode-host-api-core` —— host_api 实现层（运行期机制语义）

- **是什么**：每域两层中的**实现层**——`gate`（权限门入参 `PermissionGate`，权限词汇经参数传入）/ `storage` / `bus` / `database`（插件私有库：超时护栏 / 行列字节护栏 / 批次事务 / 裸事务控制拒绝）/ `log` / `events` / `fs`（`FsHostPorts::platform_delete` 等平台边经端口注入）。各端 adapter 与 WIT 绑定留宿主侧（本 crate 各域在 `bedcode-wasm-core/src/host_api/*.rs` 有薄转发）。
- **边界锁**（`src/boundary_lock.rs`，`#[cfg(test)]` 模块）：实现层不得出现 `plugin_api`（双端 SDK）、`wasm_core`（回引内核）、`tauri`、`tokio`；只允许机制级依赖（实际仅 `serde_json` / `tracing` / `rusqlite`）。域文件在场亦被锁住（防「重构」清空）。
- **不抽判据**：`config` / `http` 已判定不抽（config 引用各端 SDK 枚举；http 属能力域 crate 各自形态）。
- **消费方**：`bedcode-wasm-core`（adapter）、`bedcode-server-base`（re-export `PLUGIN_DB_*` 四护栏常量，故**不可并入 wasm-core**——会形成 `server-base → wasm-core → server-base` 依赖环）、移动 purity 锁（判据 3 钉「共享实现核不得消失」）。

### `bedcode-wasm-core` —— 插件机制整核（单形态）

- **是什么**：`manager/`（加载 / 生命周期 / 实例调用模型 / 任务引擎 / 能力路由 / 热重载）、`manager/runtime/`（**统一运行时壳**：引擎机制 / 唯一合法 core 绑定 / WASI 装配 / 事件循环 / 导出调用骨架）、`host_api/`（adapter + 端口装配）、`security/`、`bus`、`db/`（主库 `schema.sql` 与幂等迁移真源）、`config` / `monitor` / `enums` / `system`。
- **形态**：单形态（ADR 0045 / 0049），双端共用；移动宿主 `default-features = false`（不开 `desktop-assembly`），用自有 `PluginManager` / `host_domains` 组装。
- **契约**：唯一合法绑定 = `manager/runtime/core_binding.rs` 绑 `wit/core.wit`（ADR 0052 D1：① 内核自己实现 ② 双端同源 ③ 机制必需或全部插件共同需要）；端差异域与端 caps 住各端宿主。
- **防回接**：移动 `mobile_wasm_core_purity_lock.rs` 七判据（域回流 / 绑定回流 / 共享核在场 / 壳行数上限 / 退役垫片 / http 引擎 / 端词汇全集）钉住。

## server 基础与传输面

### `bedcode-server-base` —— 叶子地基

- **是什么**：`AppError`（统一错误 + Serialize 信封）、常量（含 `PLUGIN_DB_*` 的 re-export）、错误边界（`catch_unwind` 包装）、系统信息、网络配置形状、连接身份、`ports` 端口 traits。
- **形态**：`tauri-compat`（默认开）只为 `From<tauri::Error> for AppError` 一个 impl 存在；非 Tauri 宿主（无头 / CLI / 嵌入式）用 `default-features = false`，代价是边界处需显式 `map_err`。
- **消费方**：crypto / core / http / ws / peer-net / pty / wasm-core / 两端宿主 / cross-end-tests / wasm-app 等十余个 crate——它在地基位置，改依赖面影响全仓。

### `bedcode-server-core` —— server 内核（传输无关）

- **是什么**：`supervisor`（生命周期）、`filter`（跨传输流量过滤链）、`link_crypto`、`metrics`、`app`（`TransportFace` + `serve` 组合装配，依赖倒置）。
- **不变量**：http / ws 传输面只**向下**依赖本 crate，彼此零横向 import；本 crate 不反向引用任何传输面。

### `bedcode-server-http` —— HTTP 传输面 + `host-http`

- **是什么**：动态端点注册表 / 协议网关 / 插件代理控制器 / 中间件链 / DTO 锚点 + `plugin_binding`（`host-http` 3 原语：入站注册 / 注销 + 出站 `fetch` 含 SSE）。
- **形态**：默认纯引擎 + 端口（`bedcode_server_base::ports::get()` 注入，端口缺席 fail-visible）；`common-host` 装配 WIT 绑定层。`wire` 自持词汇副本由 `wire::drift_lock` 钉死。
- **契约**：`wit/http.wit`；改动 = 改跨端 wire 契约，两端同步评估。
- **注意**：`dtos` 的生产残留只有 `common_dto`；其余 DTO 是跨端 wire 黄金样本，已 `#[cfg(test)]` 门控。

### `bedcode-server-websocket` —— WebSocket 传输面 + `host-websocket`

- **是什么**：连接骨架 / 插件端点通道（`/ws/plugin/{plugin_id}/{path}`）/ 连接与端点注册表 / 生命周期与优雅停机 + `plugin_binding`。
- **终态**：只剩通用传输能力（认证策略 / 帧过滤 / 限流 / 按属主回收）；业务路由与协议归插件。
- **契约分工**：`plugin_binding` 做 `host-websocket` **客户端域** WIT 接线（真源 `bedcode-ws-client-engine/wit/ws.wit`）与服务端引擎域函数；**服务端** `host-websocket-server` 的声明与 Host impl 住桌面宿主 `plugin/ws.rs`（ADR 0052 D4，端扩展）。

### `bedcode-server-peer-net` —— 对等网络引擎域 + `host-peer`

- **是什么**：节点身份初始化 / 节点与发现守护装配 / `host-peer` 原语转发唇口 + 引擎连接态事件（`peer-connected` / `peer-disconnected` / `peer-consent-requested`）前端桥。
- **定位**：产品业务（传输任务 / 策略 / 历史 / 共享根 / 浏览编排）真源全在 file-transfer 插件；本 crate 只留「离宿主无法实现且无业务语义」的引擎原语与生命周期。
- **形态**：默认纯引擎 + 端口（`BusPort` / `EventSink` / `PathsPort` / `SystemInfoPort` / `MdnsPort`）；`common-host` 装配绑定层；`wit/peer.wit`。

## 能力域引擎 / 契约房

| crate | 要点 |
| --- | --- |
| `bedcode-crypto-engine` | 中性算法原语（aes_gcm / chacha / hybrid / kdf / rsa / x25519）+ 名称寻址 `provider` / `registry`；**不做认证编排 / 协商 / 密钥策略**，不触碰宿主身份密钥。WIT 面已迁宿主 `plugin/crypto.rs` |
| `bedcode-discovery-engine` | mDNS 共享守护 + 浏览 / 广播双句柄表 + 宿主自播面；`common-host` 装配绑定层；契约 `wit/mdns.wit`（ADR 0052 D2：域契约真源在域 crate） |
| `bedcode-pty-engine` | 引擎面（`pty_process` / `pty_reader` / `pty_ring` / `lifecycle` / `output_sink`，跨平台、零 wasm）+ 能力域（句柄表 / 配额仲裁 / 退出事件 / 限频通知 + 6 原语）；只接受调用方算好的 argv，不做 shell 包装；`wit/pty.wit` |
| `bedcode-ws-client-engine` | `host-websocket` **客户端子集** 5 原语 + 停用回收：句柄表 / reader+writer 双任务 / 心跳 / 退避重连 / 帧信封；平台差四类（权限门 / 投递 / 后台任务 / jwt 代发 / 退避）全经 `WsClientPorts` 注入；零 WIT 零端依赖；契约 `wit/ws.wit` |
| `bedcode-notify-engine` | 通知域**契约房**：只持 WIT + 端口词汇（当前无端口），零实现；双端实现按端住宿主（`src-tauri/src/plugin/notify.rs`）；平台不支持项静默忽略写死在契约文档；契约 `wit/notify.wit` |

## 共享底座与业务核

- **`peer-net`** —— 对等网络底座：`identity`（Ed25519 节点身份，重装即新身份，与设备身份刻意分离）、`cert`（自签证书，公钥即节点身份）、`transport` / `tls`（TLS 1.3 直连 + 指纹钉扎 verifier + 已认证连接移交缝）、`trust_store`（首连确认闸门）、`discovery`（`_bedcode-peer._tcp.local.` 在线缓存），另有 `node` / `frame` / `transfer` / `shared` 等模块。设计依据 ADR 0027 / 0028。
- **`link-crypto`** —— 链路报文加密协议核心：HTTP 信封、WS 握手 IKM / salt / OKM、二进制帧 `[ver][seq][ct]`、nonce 派生；字节级与移动 TS 实现一致。只做纯字节协议，配置域 / 身份落盘 / 过滤器接线留宿主。
- **`bedcode-file-transfer-core`** —— file-transfer 双端共享**业务核**（桌面 4.4k 行 / 移动 3.4k 行中约 70% 可共享面收敛为一份）；差异全部落 `ports` trait（共享根存储 / wire codec / 节点电源 / 平台能力等）。依赖纪律：只有 `serde` / `serde_json`，零宿主 SDK 引用、零产品身份字面量（`PluginIdentity` 运行期注入），由边界锁执行。

## 构建与验证

- **`bedcode-componentize`** —— 仓库级构建工具（bin，双端共用）：把 wit-bindgen 产出的 core module（含 `component-type` 段）编码为 Component Model 组件，等价 `wasm-tools component new`；产物已是组件时幂等复制。插件构建脚本（`wasm-apps/*/scripts/build.js`）在 cargo build 后调用；用法 `componentize <input.wasm> -o <output.wasm>`。
- **`bedcode-headless-host-probe`** —— **长期门禁**（能力域脱绑 P6）：以非桌面宿主形态默认引用全部能力域 crate（永远不开 `common-host`），任何 crate 把 WIT / 桌面 SDK / wasmtime 依赖漏回默认形态都会在这里的 `cargo tree` 门禁现形；刻意零 dev-dependencies 以保持证明力。

## 夹具（本目录内）

- **`bedcode-host-kit/fixtures/forced-link-probe`** —— 强制链接实证探针（dev-dep 环）：与 `bedcode-host-kit/tests/forced_link.rs` / `forced_link_absent.rs` 两侧配套，证明「能力 crate 被引用 → 注册生效 / 未被提及 → 注册丢失」是真实失效模式。

## 不在本目录的相邻 crate（边界说明）

| 落点 | 内容 |
| --- | --- |
| `bedcode-desktop/packages/plugin-*` | 桌面插件**契约与夹具**：`plugin-sdk-desktop/rust`（SDK crate + 宏）、`plugin-sdk-fixtures`、`plugin-system-test`、`plugin-wasi-test`、`plugin-bench-test` 等 |
| `bedcode-mobile/packages/plugin-*` | 移动插件契约与夹具：`plugin-sdk-mobile/rust`、`plugin-component-test` |
| 两端 `wasm-apps/` | wasm 应用业务源码（真源在各自 crate） |
| 两端 `src-tauri/` | 宿主壳与组合根；`packages/plugin-sdk-desktop/rust/wit/` 等 WIT **生成物**由 `compose.json` 驱动 `scripts/compose-wit.mjs` 拼装（真源 = `core.wit` + 各域 `wit/<domain>.wit` + 端 `rust/wit-src/cap-*.wit`，ADR 0052 / 0051） |
| `cross-end-tests/` | 双端互连契约门禁（独立 crate，仓库根） |

## 形态约定速查

| 约定 | 语义 |
| --- | --- |
| 能力域 `default = []` + `common-host` | 默认纯引擎（任意宿主可引用）；开 feature 才装配 `bindgen!` / `HostModule` / `inventory` / `impl Host` |
| `bedcode-wasm-core` 的 `desktop-assembly` | 桌面专属装配面（PluginHost / api_bridge / watcher / intercall / auth_center / WS 帧回灌窄端口）；移动不开 |
| `bedcode-wasm-core` 的 `test-support` | 测试支持面（无头装配 / 夹具构建器 / 转发面），不随生产形态发布 |
| `bedcode-server-base` 的 `tauri-compat` | 只为 `From<tauri::Error>` 存在；非 Tauri 宿主 `default-features = false` |
| WIT 分片 | `core`（内核机制必需，ADR 0052 D1）+ 域 `wit/<domain>.wit`（能力域 crate 自持）+ 端 cap（端独有，住端宿主）；不做 union world（ADR 0018 契约各自演进） |
