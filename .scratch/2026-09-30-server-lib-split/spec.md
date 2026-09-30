# 桌面端 server 拆 lib spec（bedcode-server-core / http / websocket / peer-net）

Status: **实施中**（2026-09-30 立项；票 01-05 完成，票级状态见 §7）
Date: 2026-09-30
范围: **仅桌面端**（`bedcode-desktop/`）；移动端不动（ADR 0018 契约独立）；共享 crate（`packages/peer-net` / `packages/link-crypto`）契约零改动
关联: `docs/adr/0022-plugin-host-interface-primitive-boundary.md`（裁剪线）、AGENTS §3（无根 workspace / target 治理）、AGENTS §5.2（桌面端架构两层）、`docs/knowledge/build-process.md`
收益锚点: 拆分 lib 后 wasm-app 插件 crate 的测试代码可以 `dev-dependencies` 引入宿主核心 lib，自建无头宿主内核 + 自建 wasm 产物跑真实闭环（libp2p 式「每个 protocol crate 自带集成测试」），见会话讨论记录

---

## 1. 需求与动机

### 1.1 用户指令

桌面端 server 拆分为三个独立 lib：**http** / **websocket** / **peer_net**（libp2p 式 transport/protocol 模块化）。紧耦合代码先松耦合，以便拆分与组合；并期望拆分后 wasm-app 能直接引入依赖、集成宿主核心运行、进行真正的测试。

### 1.2 现状盘点（2026-09-30 实测）

宿主 `bedcode-desktop` 是**单一大 crate**（`src-tauri/Cargo.toml`，cdylib+rlib），`server/` 目录内四个子模块共 14443 行：

| 子模块 | 文件 | 行数 | 对外依赖（实测 `use crate::` 统计） |
| --- | --- | --- | --- |
| `core`（传输无关内核） | app.rs(100) supervisor.rs(457) filter.rs(519) metrics.rs(313) link_crypto.rs(1921) port_checker.rs(164) | ~3474 | system×16、utils×18、db×1、**websocket×6**、**http×3** |
| `http`（HTTP 传输面） | routes(161) registry(654) gateway(931) controllers/plugin_controller(662) middleware/auth_gateway(341)+http_filter(643) dtos/5 文件(345) | ~3737 | system×8、utils×3、core×4 |
| `websocket`（WS 传输面） | conn(669) channel/plugin(495) registry(892) endpoint(378) routes(128) websocket_manager(252) | ~2814 | system×13、**wasm_core×4**、utils×2、core×9 |
| `peer_net`（对等网络引擎域） | peer_net.rs(1808) + peer_engine_{transfer,receive,remote}(732+571+467) + source_collect(225) | ~3803 | system×1、`bedcode-peer-net` crate（共享） |

### 1.3 紧耦合清单（拆 lib 必须先松耦合的点）

**A. server ↔ wasm_core 双向环**（拆 lib 的硬障碍——单一 crate 内模块互引合法，跨 crate 必断）：

| 方向 | 位置 | 耦合内容 |
| --- | --- | --- |
| server → wasm_core | `websocket/channel/plugin.rs` | `wasm_core::bus::MessageBus`、`wasm_core::host_api::ws::deliver_endpoint_frame`、`wasm_core::runtime_util::ambient_handle` |
| server → wasm_core | `websocket/endpoint.rs` | `wasm_core::bus::MessageBus` |
| server → wasm_core（间接） | `http/controllers/plugin_controller.rs` | `AppContext::global().plugin_host().invoke_rust_command(owner, "_http_endpoint", …)`（经 system 间接，无 wasm_core 类型依赖——**已是最易端口化的形态**） |
| wasm_core → server | `host_api/peer.rs` | `server::peer_net::*`（dial/disconnect/cancel/consent/list/revoke/send/respond/pause… 18+ 处） |
| wasm_core → server | `host_api/http.rs` | `server::http::registry`（register/remove/purge/find/count 7 处） |
| wasm_core → server | `host_api/ws.rs` | `server::websocket::endpoint`（register/get/remove/list/purge 7 处）+ `registry::WsSessionRegistry` |
| wasm_core → server | `host_api/connection.rs` | `server::websocket::WebSocketManager::global()` |
| wasm_core → server | `host_api/auth.rs` | `server::core::link_crypto::identity_parts()` |
| wasm_core → server | `host_api/config.rs` | `server::core::supervisor::ServerSupervisor::global()` |
| wasm_core → server | `host_api/mdns.rs` | `server::peer_net::current_node_id()` |

**B. core 依赖传输面（违反 I2 字面，靠 CORE_TRANSPORT_FACE_ALLOWLIST 豁免）**：`core/supervisor.rs` 依赖 `websocket::WebSocketManager::global()`、`websocket::ServerEvent`、`websocket::registry::WsSessionRegistry`（3 处）。

**C. peer_net 依赖 AppHandle / 全局态**：`peer_net.rs` 大量 `app: &tauri::AppHandle`（`emit_json` → `app.emit`、`app_data_dir`、`node_owner`、`current_node_id`）+ `publish_bus_only` / `publish_mdns_bus` / `publish_engine_event`（总线/mDNS 直调）+ `PeerNetState` 全局态。

**D. 全局单例（LazyLock / OnceLock）**：`WebSocketManager::global()`、`WsSessionRegistry::global()`、`ServerSupervisor::global()`、`AppContext::global()`（进程级 OnceLock）、http registry / ws endpoint registry 的静态表——拆 lib 后若保持全局静态，插件侧集成测试无法起隔离实例（同进程多实例会撞 `path already registered`）。

**E. link_crypto（1921 行）在 core 且依赖 `utils::crypto`**：被 http 面（http_filter 加密）、ws 面（conn.rs 帧加密）、host_api/auth.rs（identity_parts）三方共用；`utils::crypto` 是宿主内模块（x25519/aes_gcm/hkdf/kdf），拆 lib 后 core lib 不能引用宿主内模块。

**F. 组合物 app.rs（100 行）认识两面**：`configure_routes` 同时调 `http::configure_routes` + `websocket::configure_routes`；`start_http_server` 的 `App::new()` 装配 TrafficFilter（http 面类型）+ MetricsCollector（core）。

### 1.4 现状中可直接复用的松耦合先例

- **host_api → manager 依赖已清零**：经窄接口 `CapabilityTarget`（`storage_get/set/delete`）转发，转发方不知道调用模型——本 spec 的端口化沿用同一手法。
- **http 面调插件已间接化**：`forward_to_plugin` 只依赖 `AppContext::global().plugin_host().invoke_rust_command`，不碰 wasm_core 类型。
- **结构锁先例**：`websocket.rs` 的 I1/I2/I3 源码文本锁（`CARGO_MANIFEST_DIR` + 递归 read_dir）——拆 lib 后迁移为 crate 边界断言。
- **共享 crate 先例**：`packages/peer-net` / `packages/link-crypto`（repo 根，双端 path 依赖，独立 Cargo.lock + tests/）。

---

## 2. 目标架构

```
bedcode-desktop/packages/                      # 桌面专属（repo 根 packages/ 是跨端共享，不动）
├── server-core/        # 传输无关内核：supervisor / filter / metrics / link_crypto / port_checker
│                       #   + 组合 API（TransportFace trait + serve()，SwarmBuilder 式）
├── server-http/        # HTTP 传输面：routes / gateway / registry / controllers / middleware / dtos
├── server-websocket/   # WS 传输面：conn / channel / registry / endpoint / routes / websocket_manager
└── server-peer-net/    # 对等网络引擎域：peer_net.rs + 3 引擎适配 + source_collect
                        #   （依赖 bedcode-peer-net crate，与 server-core 零依赖）
```

**依赖方向（维持既有不变量 I1/I2/I3 并升级为 crate 边界）**：

```text
                        ┌──────────────────────────────┐
                        │ 宿主壳 bedcode-desktop       │ ← 组合根：bootstrap + 注入装配
                        │ （wasm_core 仍是宿主内模块）  │     + 全局单例兼容面（global() 保留）
                        └──────┬───────┬───────┬──────┘
                    (直接调用)   │       │       │ (注入端口实现)
                               ▼       ▼       ▼
        ┌──────────────┐  ┌──────────┐  ┌──────────────┐
        │server-http   │→ │server-core│ ←│server-websocket│
        └──────┬───────┘  └──────────┘  └──────┬───────┘
               │           (I2：向下)           │
               └────── I1：http ↮ websocket 零横向 ──────┘
        ┌───────────────────────────────────────────────┐
        │server-peer-net（独立：不依赖 core / http / ws）  │
        └───────────────┬───────────────────────────────┘
                        ▼
               bedcode-peer-net（repo 根共享 crate，不动）
```

**端口 traits（依赖倒置，宿主壳注入实现）**——所有 server lib 不得引用 wasm_core / tauri / AppContext 类型：

| 端口 trait | 定义处 | 注入实现（宿主壳） | 服务对象 |
| --- | --- | --- | --- |
| `PluginInvoker` | server-http | 调 `PluginHost::invoke_rust_command` | http 转发面（替代 `AppContext::global().plugin_host()`） |
| `FrameDeliverer` | server-websocket | 调 `wasm_core::host_api::ws::deliver_endpoint_frame` | ws 通道（替代直接调 host_api） |
| `BusPort` | server-core（共享） | 调 `wasm_core::bus::MessageBus`（topic 命名空间仲裁留在宿主壳） | ws channel/endpoint、peer_net 事件 |
| `EventSink` | server-core（共享） | 调 `AppHandle::emit`（前端事件） | peer_net `emit_json` |
| `PathsPort` | server-core（共享） | 调 `AppContext` 数据目录解析 | peer_net `app_data_dir` / `resolve_download_dir` |
| `MdnsPort` | server-peer-net | 调 `crate::mdns` 模块 | peer_net `publish_mdns_bus` |
| `NodeIdentityProvider` | server-peer-net | 读节点身份 | peer_net `current_node_id` / `node_owner` |
| `ServerLifecyclePort` | server-core | 宿主壳把 supervisor 事件接到 ws manager | core/supervisor（替代依赖 websocket 面） |

**wasm_core → server libs 的引用**（`crate::server::xxx` → `bedcode_server_xxx::yyy`）：方向合法（宿主 crate → server libs crate，单向无环）。`host_api/*.rs` 的 30+ 引用点改路径即可，**不必**全部端口化（wasm_core 仍与宿主同 crate，跨 crate 单向引用不构成环）；端口化优先做「server libs 反向要宿主能力」的五处（B/C 类）。

---

## 3. 松耦合改造（拆分前置，宿主内完成）

### 3.1 决策清单

| # | 决策 | 定案 |
| --- | --- | --- |
| D1 | core 是否也拆 lib？ | **拆** `bedcode-server-core`（supervisor/filter/metrics/link_crypto/port_checker）。三个传输面 + wasm_core 都依赖它，link_crypto 有共同归属；不拆则 http/ws 两 lib 会各自复制或互相依赖内核 |
| D2 | peer_net 是否依赖 core？ | **不依赖**。peer_net 现状对 core 零引用（内部零跨子模块引用、对外只 system×1 + bedcode-peer-net），它是「引擎域」不是「传输面」，独立 lib |
| D3 | 组合物 app.rs 归属 | 组合 API **trait 化进 core**：`trait TransportFace { fn configure(&self, cfg: &mut web::ServiceConfig); }`，http/ws 各自实现；core 提供 `serve(config, faces)` 泛型装配（libp2p SwarmBuilder 式）。宿主壳只做「造实现 + 传参 + 注入端口」 |
| D4 | 全局单例（LazyLock/静态注册表） | **实例化 + 可构造**：`ServerState { ws_manager, ws_registry, ws_endpoints, http_registry, supervisor }` 可注入构造；宿主壳保留 `::global()` 兼容面（内部持 `ServerState`）。这是插件侧集成测试可并行起多个隔离宿主的**前提** |
| D5 | link_crypto 与 utils::crypto | core lib 内保留 link_crypto 本体；`utils::crypto`（x25519/aes_gcm/hkdf/kdf 引擎，462 行）**下沉为独立 crate** `bedcode-crypto-engine`（或复制进 core lib，实施时按 utils::crypto 的消费方清单裁决：若 only server 用则归 core，若 wasm_core host_api/crypto.rs 也用则独立） |
| D6 | dtos（http 面形状锁锚点） | 随 server-http 迁移；`common_dto` + 业务四组（config/file/git/session）的形状锁测试跟着走，锁语义不变（黄金形状仅供 `#[cfg(test)]`） |
| # | 决策 | 定案 |
| --- | --- | --- |
| D7 | 结构锁 I1/I2/I3 | 拆 lib 后升级为 **crate 边界断言**：锁分两层——① 面内锁（各 lib 的 `#[cfg(test)]`，钉死**自己**的清单与源码，见票 05/06）；② **全图锁**（宿主 `src/server/crate_boundary_lock.rs`，钉死**六份清单之间的边**、宿主清单完整性、双面认识点唯一、端口装配点唯一）。票 07 定案：两层都要，缺②则「横向边」全矩阵无人生守 |
| D8 | 包位置与命名 | `bedcode-desktop/packages/` 下：`bedcode-server-core` / `bedcode-server-http` / `bedcode-server-websocket` / `bedcode-server-peer-net`（桌面专属；repo 根 packages/ 只放跨端共享） |
| D9 | workspace | **不建**（AGENTS §3 无根 workspace 决策不变）：path 依赖 + 各自 crate 根跑 cargo test；新 lib 的 `.cargo/config.toml` 把 target 指向共享目录（遵循 AGENTS §3「不得写死 `<crate>/target/`」，实施时参照既有 wasm-apps/fixtures 的 config） |
| D10 | 插件侧集成测试 | 票 07 试点：terminal-session 一个闭环用例（dev-dependencies 引 server libs + 无头 PluginHost + 自建 wasm 产物），验证收益后推广 |

### 3.2 紧耦合 → 松耦合映射（逐项处置）

| 紧耦合点 | 处置 |
| --- | --- |
| `websocket/channel/plugin.rs` → wasm_core（bus / deliver_endpoint_frame / ambient_handle） | `BusPort` + `FrameDeliverer` 端口注入；`ambient_handle` 收窄为 `EventSink`（或随 D4 的 ServerState 持有） |
| `websocket/endpoint.rs` → wasm_core::bus | `BusPort` 注入 |
| `http/plugin_controller.rs` → AppContext::global().plugin_host() | `PluginInvoker` 注入（`invoke_rust_command(owner, cmd, args)` 窄签名） |
| `core/supervisor.rs` → websocket 面（WebSocketManager/ServerEvent/WsSessionRegistry） | `ServerLifecyclePort` 反转（`on_started` / `on_stopped` / `connections_snapshot`），宿主壳接 |
| `peer_net.rs` → AppHandle / bus / mdns / 全局态 | `EventSink` / `BusPort` / `MdnsPort` / `PathsPort` / `NodeIdentityProvider` 注入 + `PeerNetState` 实例化（D4） |
| `host_api/*.rs` → `crate::server::*`（30+ 处） | 路径改为 `bedcode_server_*::*`（wasm_core 在宿主内，单向引用合法，不需端口化） |
| `server/core/link_crypto.rs` → utils::crypto | D5 下沉 |
| `core/app.rs` 认识两面 | D3 trait 化 |

### 3.3 边界约束（拆分不新增/不改变）

- **零行为变更**：纯结构重构（move-only + 端口注入），禁止顺手改逻辑、改 wire、改 WIT、改 ABI、改权限词汇、改 manifest 语义。
- **宿主侧无业务代码红线不因拆分松动**：拆分只移动引擎原语，不新增任何业务语义；`dtos` 业务组仍是纯形状锁锚点（生产路径不得构造）。
- **共享 crate 契约零改动**：`bedcode-peer-net` / `bedcode-link-crypto` 双端共享，连 Cargo.lock 都不动。
- **wasmHash / 插件产物链零影响**：拆分不触碰 `wasm-apps/*` 与 SDK。

---

## 4. 拆分顺序（票）

| 票 | 内容 | 出口判据 |
| --- | --- | --- |
| 01 | **端口化破环**（宿主内完成，零 crate 移动）：定义 §2 端口 traits + §3.2 全部注入点改造；`host_api/*.rs` 路径统一为 server 新模块引用形式 | 宿主 cargo test 全绿；结构锁 I1/I2/I3 原样通过 |
| 02 | 抽 `bedcode-server-core`：supervisor/filter/metrics/link_crypto/port_checker + `TransportFace`/`serve()` 组合 API + `ServerState` + 端口 traits 定义处；utils::crypto 按 D5 处置 | core lib 独立 cargo test 绿；宿主改 path 依赖后全绿 |
| 03 | 抽 `bedcode-server-http`：routes/gateway/registry/controllers/middleware/dtos + `PluginInvoker` 注入 | 同上（含 `server_integration` / `http_auth_biometric` 等宿主集成测试全绿） |
| 04 | 抽 `bedcode-server-websocket`：conn/channel/registry/endpoint/routes/websocket_manager + `BusPort`/`FrameDeliverer` 注入 + `ServerState` 落地 | 同上（ws_e2e / session_e2e / wasm_flow 相关用例全绿） |
| 05 | 抽 `bedcode-server-peer-net`：peer_net.rs + 3 引擎适配 + source_collect + `EventSink`/`MdnsPort`/`PathsPort`/`NodeIdentityProvider` 注入 + `PeerNetState` 实例化 | 同上（peer 相关用例全绿） |
| 06 | 宿主壳组合根：bootstrap（造 TransportFace 实现 + 注入端口 + 保留 `::global()` 兼容面）+ 结构锁迁移为 crate 边界断言 + target 治理（D9） | 宿主 cargo test 全量绿；`pnpm run tauri:dev` 冒烟 |
| 07 | **测试迁移 + 收益兑现**：宿主 server 测试随 lib 走（`server-*` 各 crate `tests/`）；插件侧集成测试试点（D10，terminal-session 一个闭环） | 试点用例在 `wasm-apps/terminal-session/rust` crate 根跑绿 |
| 08 | 文档与记录：code-map（两端）、AGENTS §3 命令/测试覆盖面、`docs/knowledge/build-process.md` target 治理、CHANGELOG 双语 | 文档核对引用无失效 |

---

## 5. 验证

- 每票出口：针对性单测（§4 判据）+ 收尾全量。
- 收尾全量：宿主 `cargo test` 全量（含 `src-tauri/tests/` 集成 target）；移动端 `cargo test` 跑一次（防共享 crate 意外）；`pnpm exec eslint .`（前端零改动，只做回归确认）；vitest 视前端改动与否决定。
- 手工验证项：`pnpm run tauri:dev` 冒烟（HTTP+WS 单端口 + 插件加载链路不因拆分变化）；真机移动端连接按 `docs/knowledge/mobile-desktop-auth.md`（若环境允许）。
- wasm 应用完整构建（`cd wasm-apps/<id> && pnpm run build`）抽一个确认拆分未扰动产物链。

## 6. 风险与不做

- **风险**：拆分期间宿主 `server/` 目录大挪移，防回接锁（`retired_*`）与结构锁若漏迁移会静默失效——票 02-05 每票后跑一次全量含锁断言；`CORE_TRANSPORT_FACE_ALLOWLIST` 豁免随 supervisor 端口化（票 01）逐步消解。
- **不做**：wasm_core 拆 lib（后续专项，本次只做 server）；pty/db/crypto/mdns 拆分（后续）；移动端 server 改造（ADR 0018 契约独立）；server libs 之间引入消息总线作为跨面通信（维持 I1 零横向）。

## 7. 实施状态（票号与本文 §4 有偏移：base 层单列一票，故整体后移一位）

| 实施票 | 内容 | 对应 §4 | 状态（2026-09-30） |
| --- | --- | --- | --- |
| 01 | 端口化破环（§2 端口 traits + 宿主壳实现 `server/ports_impl.rs`） | 01 | ✅ 完成 |
| 02 | 抽 `bedcode-server-base`（error/constants/info/config/identity/ports）+ `bedcode-crypto-engine`（D5） | 02 前置 | ✅ 完成 |
| 03 | 抽 `bedcode-server-core`（supervisor/filter/metrics/link_crypto + D3 `TransportFace`/`serve`） | 02 | ✅ 完成（`port_checker` 偏离：留宿主 `server/host_port.rs`） |
| 04 | 抽 `bedcode-server-http`（routes/gateway/registry/controllers/middleware/dtos + `HttpTransportFace`） | 03 | ✅ 完成（CI 补 server libs 门禁步骤；见 §7.1） |
| 05 | 抽 `bedcode-server-websocket`（conn/channel/registry/endpoint/routes/manager + `WebSocketTransportFace`） | 04 | ✅ 完成（结构锁迁 crate；见 §7.2） |
| 06 | 抽 `bedcode-server-peer-net` | 05 | ✅ 完成（结构锁新写；发现 cross-end 阻塞归票 07；见 §7.3） |
| 07 | 宿主壳组合根收尾 + 结构锁迁移为 crate 边界断言 | 06 | ✅ 完成（8 项新锁；跨端 rig 7/8 绿，8th 当时报为缺陷，票 08 复跑未能复现；见 §7.4） |
| 08 | 测试迁移 + 插件侧集成测试试点（D10） | 07 | ✅ 完成（7 项测试迁出宿主；D10 两条腿 + CI 门禁补齐；见 §7.5） |
| 09 | 文档与记录 | 08 | ⏳ |

**票 03 相对 §2/§3 的实施裁决**（细节见 crate `lib.rs` 头注释与交接文档）：

- 内核 crate 不接宿主类型：`link_crypto` 的 DB 读写收 `&rusqlite::Connection`（宿主经
  `Database::conn()` 传入），`init_at_startup` / `ensure_identity_fingerprint` 收 `&Path`；
  解析动作在宿主组合根 `server/composition.rs` 的薄壳里。**`init_at_startup` 定为同步 `fn`**
  （原 `async`）：`&Connection` 跨 `.await` 会让调用方任务要求 `Connection: Sync`（rusqlite
  连接 Send !Sync），而该函数内部无等待点。
- 宿主 `server::core` 模块整体删除，引用改 `bedcode_server_core::*`；组合根提供
  `start_http_server(port, config)` 兼容面（8 个调用点签名不变）。
- 结构锁（`websocket.rs`）的 I3/`core` 面断言随宿主 core 目录消失而退役——「内核反向引用
  传输面」变成跨 crate 引用，不声明依赖即编译不过，强于文本锁；锁保留 I1 与旧平铺路径断言，
  并新增段边界契约例（M1/M2 变异自检已实测杀死）。crate 之间依赖清单断言归票 07。


### 7.1 票 04（抽 `bedcode-server-http`）的实施裁决

- **面内已无宿主类型**：票 01 的端口化让本面经 `bedcode_server_base::ports::get()` 取
  `PluginInvoker` / `AuthCenter` / `PathsPort`，本票只做搬运 + face 下沉；唯一的宿主
  符号残留是 `auth_gateway.rs` 测试里「无 AppContext」的前提断言，改为断言
  `ports::get().is_none()`（面不认识 AppContext 后，注入与否的唯一可观测形态就是端口注册表）。
- **`HttpTransportFace` 随面下沉**：face 实现（configure + `TrafficFilter` 的 boxed wrap）
  从宿主组合根搬进 crate；宿主 `composition.rs` 此后只 `Arc::new(HttpTransportFace)` +
  WS 面适配器（票 05 收口）。宿主 `actix-service` 依赖（票 03 为装箱临时加的）随之撤回。
- **顺手修掉搬移文件里的真缺陷**：`middleware/http_filter.rs` 的 `mod tests` 在宿主里
  **没有 `#[cfg(test)]`**（测试代码被编进 lib 产物，也是那批 `#[allow(dead_code)]` 与
  `clippy --lib` unused-import 噪音的根因）。搬入新 crate 后它需要 dev-only 的
  `tracing-subscriber`，无门禁的 `mod tests` 会让该依赖变成生产依赖——故补上
  `#[cfg(test)]`（行为不变，仅把测试代码移出生产构建）。
- **锁的同步（不是绕过）**：
  - 结构锁 `websocket.rs`：http 面目录已不在宿主，扫描面收为 WS 侧单侧；断言 A 新增
    **crate 名词边界**形态（`bedcode_server_http::…` 也算横向引用），http→ws 方向由
    crate 边界静态保证（本 crate 不依赖宿主 crate）。变异自检 M1-M4 实测杀死
    （M3 = ws 侧 `use bedcode_server_http::registry`；M4 = ws 侧旧平铺路径）。
  - L2 gating 锁：`auth_gateway` 是 L2 桥接面（经 `AuthCenter` 端口）的消费点，搬家后
    扫描根必须跟着走 → 新增 `L2_SCAN_ROOTS = ["src", "../packages/bedcode-server-http/src"]`
    并改白名单路径；**不**把 base/core 纳进扫描根（它们是端口*定义*方，扫进来等于给
    词汇定义开白名单，失去「登记 = 显式裁决」语义）。
- **门禁执行人（AGENTS §10 / CI）**：票 02-04 迁出宿主的 143 项测试原本在 CI 上无人执行
  （test.yml 只跑两端 `src-tauri` 与 SDK crate）。本票补 `Cargo test (desktop server libs)`
  步骤，逐 crate `cd` 进根跑 `cargo test`（cargo 只从 cwd 向上读 `.cargo/config.toml`，
  用 `--manifest-path` 会漏掉 `target/server-libs` 重定向）。

### 7.2 票 05（抽 `bedcode-server-websocket`）的实施裁决

- **启动接缝换向**：`WebSocketManager::start(port)` → `start(port, faces: Vec<Arc<dyn
  TransportFace>>)`，内部改调 `bedcode_server_core::app::serve`。理由：ws 面不能反过来
  调宿主组合根（`server::composition`），而 faces 由宿主 `ports_impl::
  HostServerLifecyclePort::start_server` 传入——**I1 靠注入而非引用**，装配点仍只有宿主
  组合根一处。`transport_faces()` 此后就是「两个面的 face + 交给 serve」的纯壳。
- **结构锁迁主**：宿主 `server/websocket.rs` 随面删除，其文本锁退役，改由
  `packages/bedcode-server-websocket/src/dependency_direction_lock.rs` 承担，且强度只增：
  I1 双向 + I2 向下由**两份 Cargo.toml** 钉死（本面不得依赖 http/宿主，http 不得依赖
  本面/宿主，本面必须依赖 core+base），源码文本锁只守「不回接宿主路径 / 不横向引用
  `bedcode_server_http`」与旧平铺路径。M5（往清单里加 `bedcode-server-http`）、
  M6（往源码里加两种形态的横向引用）均实测杀死。
  宿主的防回接锁（`retired_*` / api_bridge 命令面锁）仍只扫宿主 `src`，**server lib crate
  未纳入**——归票 07 统一把 crate 根加进扫描面（本票已把 L2 gating 的扫描根扩到两个面 crate）。
- `ws_frame_limit()` 由 `pub(crate)` 改 `pub`：宿主 `host-websocket` 原语按同一上限裁剪
  插件域/客户端域帧，帧上限只有一个真源，跨 crate 供读是刻意的（文档已写明）。
- **格式化收口**：五个 server lib crate 各加 `rustfmt.toml`（与宿主同 `max_width=120`，
  搬移文件不产生重排噪声）；`cargo fmt` 在 crate 内保留 CRLF，**不得**再对产物做
  `\n→\r\n` 二次转换（会变成裸 CR，rustfmt 直接报错）。票 01-04 遗留的过/欠包裹
  （peer_net / host_api/peer / commands / 三个集成二进制）一并归零：本任务改过的文件
  现在都不比 HEAD 差。

### 7.3 票 06（抽 `bedcode-server-peer-net`）的实施裁决

- **本票几乎是纯搬运**：票 01 的端口化已经把 `peer_net` 里的宿主类型全部换成
  `bedcode_server_base::ports` 端口，搬移时全仓只剩两类宿主引用——`crate::Result` /
  `crate::AppError`（改为 `use bedcode_server_base::error::{AppError, Result}`，逐文件
  import）与 `peer_net_cmds` 里的 `AppHandle`（留在宿主，引擎函数一律收 `PeerCtx`）。
  `server/peer_net.rs` → crate 根 `src/lib.rs`（保持 `pub mod` 声明与子目录同形）。
- **一个命名冲突**：`peer_engine_transfer.rs` 内有本文件私有的
  `fn dial_for_send(...) -> Result<Connection, String>`（**两参** `std::result::Result`）。
  直接 `use ...::Result` 会把它遮蔽成单参别名（E0107 ×18）。裁决：该模块只
  `use bedcode_server_base::error::AppError`，`Result` 以别名 `PeerResult` 引入——
  宁可别名难看，也不改搬移前就存在的私有签名。
- **14 个引擎入口由 `pub(crate)` 升 `pub`**：宿主 `wasm_core/host_api/{peer,mdns}.rs` 与
  `manager/host/activation.rs` 跨 crate 调它们（`dial_peer_endpoint` / `*_for_plugin` /
  `current_node_id` / 三个远端 DTO 的 `pub use`）。其余 20 余个 `pub(crate)` 保持原样
  （crate 内私用），可见面只开到宿主真正消费的那一层。三个引擎状态
  （`PeerTransferState` / `PeerReceiveState` / `PeerRemoteState`）与 `PeerNetState`
  本来就是 `pub`（`app.manage` 需要）。
- **结构锁新写**（`dependency_direction_lock.rs`，与票 05 的 WS 面锁同款但断言不同）：
  ① **D2 引擎域独立**——本 crate 清单不得含 `bedcode-server-{core,http,websocket}` /
  `bedcode-desktop` / `tauri` / `actix`（传输栈）；② **D5 向下取用**——必须含
  `bedcode-server-base` + `bedcode-peer-net`；③ **叶子性**——其余四个 server lib 清单
  不得反向依赖本 crate；④ 源码文本锁守「不引用传输面 crate 名 / 不回接宿主路径与
  `tauri::`/`wasm_core::`/`AppContext::` 形态」。
  - **锁自身的判据 bug（变异自检抓出，值得记）**：前缀针脚（`AppContext::`、`tauri::`）
    的右边界判定会把**每一个真命中**挡掉——前缀右边必然紧跟标识符
    （`AppContext::global`），于是「右字符非标识符」恒假 → 锁静默失效（M4 追加
    `AppContext::global()` 到 `lib.rs` 实测假绿）。前缀针脚只判**左**边界。
  - **契约例必须共用判据函数**：首版契约例把匹配循环**重写了一遍**（自带一份
    `find_word_positions` 语义），于是它验证的是「自己那份实现」而不是锁真正用的那份；
    改成与锁本体共用 `find_prefix_hits` / `find_word_positions` 才会随锁一起漂移。
  - 变异自检 M1-M6（清单加 `bedcode-server-core` / 加 `tauri` / 兄弟清单反向依赖 /
    源码引用 `bedcode_server_websocket` / 源码 `use crate::server::peer_net::…` / 源码
    `AppContext::global()`）全部实测打红；「枚举空转」一项用「挪走源文件」验不了
    （`pub mod` 先编译失败，测试根本没跑），该防护改由锁自身的 `SENTINEL_FILES` +
    `MIN_FILES` 断言承担。
- **CI 门禁**：`test.yml` 的 `Cargo test (desktop server libs)` 循环加第六个 crate
  （六个 crate 合计 194 项测试，票 06 贡献 28 项）。
- **本票顺带修的编译断链（均为本 spec 早期票遗留，不改行为）**：
  - `cross-end-tests/tests/common/desktop_ctx.rs`：`server::core::app::start_http_server`
    → `server::composition::start_http_server`（票 03 遗留）；同文件里被重复追加的
    `SESSION_PLUGIN_ID` 常量删掉一份。
  - `cross-end-tests/tests/terminal_output_pressure.rs`（今日早些时候写的、从未编译过
    的跨端压力测试）三处类型错：`u64/u128` 混用、format 串第三个 `{}` 无实参、
    `Arc` 跨 async move 后又被借用。
- **⚠️ 票 07 的头号前置（本票实测发现，非本票引入）**：`cross-end-tests` 修完编译后
  **七个场景全红**，根因同一处——**跨端 rig 从不装配 server 端口**
  （`desktop_ctx::init_app_context_inner` 里没有 `bedcode_server_base::ports::init(…)`，
  而宿主 `src-tauri/src/lib.rs:444` 的 bootstrap 有）。网关 `business_gateway` 的
  `ports::get() == None` 分支判 `PassThrough`，于是插件注册的全部宿主别名
  （`/api/auth/*` / `/api/sessions*` / `/api/configs` …）一律 404。**票 07 必须让跨端 rig
  走同一装配面**（`server::ports_impl::assemble()` + 给 `cross-end-tests` 加
  `bedcode-server-base` 依赖），否则票 08 的跨端试点无从谈起。
  另需并票解决：进程级 `OnceLock` 的 `AppContext`/端口注册表与多测试二进制的隔离问题。
- **顺带修的文档事实错误（AGENTS §0「文档字面 ≠ 事实」）**：六个 server lib crate 的
  `.cargo/config.toml` 注释都写着「落到 `bedcode-desktop/target/server-libs`」，而
  `target-dir = "../../../target/server-libs"` 从各 crate 根解析实际落在**仓库根**
  `BedCode/target/server-libs`（实测 13G；`bedcode-desktop/target/server-libs` 不存在）。
  票 06 起六份注释 + `test.yml` 注释统一改为「仓库根」并写明相对路径。
  **附带发现（归票 09）**：`bedcode-desktop/scripts/check-target-size.js` 的
  `rootTargetDirs` 只列了 `cross-end-tests/target`，**不含仓库根 `target/`**——
  13G 的 server-libs 产物对 `pnpm run target:size` 完全不可见，与 AGENTS §3
  「构建前检查 target 大小」的初衷相悖。

### 7.4 票 07（宿主壳组合根收尾 + crate 边界锁）的实施裁决

#### A. 头号前置：跨端 rig 走同一装配面（跨端 0/7 → 7/8）

票 06 记录的前置已落地，但**走的是比 spec 更强的一步**：

- spec 原写「给 `cross-end-tests` 加 `bedcode-server-base` 依赖 + 在 rig 里调
  `ports::init(assemble())`」；实施改成**宿主组合根新增单一装配点**
  `server::composition::install_server_ports()`，GUI bootstrap（`lib.rs` setup）与
  四个宿主集成 harness（`pty_session_chain` / `http_auth_biometric` /
  `ws_auth_rules` / `broadcast_shutdown`）与跨端 rig **调同一个函数**。
  spec 的写法让「两个装配面」在文本上相同、但结构上可漂移；单一函数让漂移**不可能**。
  `cross-end-tests` 仍然加了 `bedcode-server-base` 依赖，但**只用于自检直读**
  `ports::get()`（判据必须不经过被测代码），装配本身不经它。
- **rig 装配顺序也对齐 GUI**：原 rig 是「建 AppContext → 激活插件 → （无端口）」；
  实施改为「建 AppContext → 装端口 → 激活插件」。两处依据：① `assemble()` 取总线
  走 `AppContext::try_global()`，早一步会装上**占位空总线**（插件的 `bus-subscribe`
  便永收不到互调请求，表现为 5s 超时而非订阅竞态）；② 插件激活期要登记端点，
  端点表与 `PluginInvoker` 端口此刻都应就位——生产 bootstrap 正是这个顺序。
- **新增自检段** `rig_assembles_the_same_server_ports_face_as_gui_boot`
  （`harness_selfcheck.rs` 第 2 段）：判据是 `ports::get()` 取到的端口面
  `plugin_invoker.is_activated(认证中心) == true`，不是只查注册表非空——装了个空壳
  端口面也能过前者。
- **跨端回归结果**：`cargo test --no-fail-fast` → **7/8 绿**（票 06 记录时是 0/7 全红）。
  唯一仍红的 `terminal_output_pressure` 当时报为真实产品缺陷，见下方「新发现」；
  **票 08 复跑未能复现（8/8 绿），该结论已在 §7.5 下调**。

#### B. 结构锁 → crate 边界断言（`src/server/crate_boundary_lock.rs`，8 项）

票 03-05 的面内锁有个共同盲区：**每把锁只看得见自己与被点名的少数对侧**。
例如 HTTP 面的清单里若被加上 `bedcode-server-peer-net`，没有任何一把面内锁转红——
而「传输面之间零横向」恰是它们声称守住的不变量。宿主 crate 是**唯一同时看得见全部六份
清单**的地方（且宿主 `cargo test` 一定跑），故「全图」这一层放宿主侧：

| 断言 | 守住什么 |
| --- | --- |
| `server_lib_manifests_have_no_lateral_or_upward_edges` | 六份清单的全矩阵：无横向、无越级、无反向依赖宿主。**横向边在 `dev-dependencies` 里也算横向**（单向引用即可编译，是真实可发生的越线形态） |
| `required_downward_edges_exist_in_production_deps` | 必需向下边（base 是所有面的地基；两个面必须吃 core） |
| `host_manifest_declares_every_server_lib_crate` | 拆分产物没被悄悄摘掉（`SERVER_LIB_CRATES` 同时是 CI 循环与尺寸脚本的口径源） |
| `composition_root_is_the_only_module_knowing_both_faces` | I1 在宿主侧的表达：双面认识点唯一。登记表 = `{server/composition.rs}`，测试面（路径含 `tests` 段或 `_test.rs`）不计入 |
| `server_ports_registry_has_exactly_one_install_call_site` | 组合根唯一性：**生产源码与集成 harness 皆然**——这把锁就是为了让「无头 rig 不装端口」那类分裂以可执行判据复发不了 |
| 另 3 项 | 判据自身的契约例（清单分段解析 / 测试面分类 / 段与词边界判定） |

**变异自检 M1-M4 实测全红**：① http 加 `bedcode-server-websocket` 到
`[dev-dependencies]`（**编译通过**，锁报「横向/越级边违反」——这是最真实的越线形态）；
② http 加 `bedcode-server-peer-net`（跨面→引擎域）；③ 登记表加一个不存在的 crate
（同时打出「拆分产物缺失」与「宿主依赖清单缺少」，证明枚举非空转）；④ 宿主另建一个
同时引用两个面 crate 的生产模块（`host_port.rs` 里加旁路接线）。

#### C. 退役面防回接锁的扫描面扩到六个 crate（票 05 遗留项）

宿主四个文本扫描锁此前只扫宿主 `src`，面抽 crate 后「退役面被回接到 crate 里」它们
全绿。扫描面改为宿主 `src` + 六个 crate 的 `src`，crate 清单与 B 的
`SERVER_LIB_CRATES` **共用同一张表**（不各抄一份）。新增自锁规避：crate 侧的
`dependency_direction_lock.rs` 同样以字符串携带禁用字面量，故加入跳过名单。

**变异自检 M5-M7 实测全红**：peer-net 里加 `HISTORY_CAP:`（传输编排常量回接）→
打红并点名 `packages/bedcode-server-peer-net/src/lib.rs:1828`；WS 面里加
`commands::list_sessions` 形态 → 会话命令面锁打红；peer-net 里加一个
`ports.get().auth_center.enforce_connection_policy(...)` → L2 gating 锁打红。
L2 扫描根同时补齐 `core` / `peer-net` / `crypto-engine`（`base` 仍排除：它是
`AuthCenter` 端口的**定义方**，扫进来等于给词汇定义开白名单，票 04 裁决不变）。

**M6 未杀，已记为已知判据边界**（写进锁的文档注释）：会话命令面锁的 Rust 侧 needle
只认 `commands::` 限定形态，面 crate 里一个**裸**的 `list_sessions` 不被拦（Rust 里
裸名与前端不同，会大量误中无关标识符；裸名函数进不了 `generate_handler!` 也就调不到）。
兜底是对偶退役面 `retired_session_observation_*`（扫全 crate 的裸形态标识符）。

#### 新发现（本票未修）：跨端终端输出压力下**静默丢字节**（⚠️ 票 08 未能复现，见 §7.5 下调）

`cross-end-tests/tests/terminal_output_pressure.rs` 阶段 A（C-101「客户端正常确认时
输出零缺口」）在三次运行中**稳定失败**：

```
缺口位置=[5404, 8383]，示例=[(6640, Some(11298)), (14276, Some(34069))]
缺口位置=[6500, 7989, 7990]，示例=[(8553, Some(13093)), (14581, Some(1)), (1, Some(33764))]
缺口位置=[4642, 7621]，示例=[(6064, Some(10517)), (13495, Some(33791))]
```

- **不是本票引入的**：端口未装时网关判 `PassThrough` → `/api/sessions/*/input` 走
  路由表 404，产出命令根本送不到插件，阶段 A 只会「收齐末序号超时」而不是报缺口。
  这条用例在本票之前**从未跑到这一步**（票 06 记录它连编译都没过）。
- **不是测试前提错**：`SESSION_PTY_RING_BYTES` = 4 MiB，阶段 A 产出 ≈ 360 KiB，
  `PtyRing` 不可能淘汰 ⇒ 缺口只可能来自链路上某一跳的**静默丢弃**（游标越过了未
  拉取字节），且 `resync_count` 为 0，即**没有 fail-visible 信号**。
- **待查线索**（下次接手的入口）：第二次运行里出现 `(14581, Some(1))` 与
  `(1, Some(33764))`——序号**中途回落到 1 再跳到 33764**。阶段 A 只有一个会话，
  所以这不像环重锚（重锚应从最旧存活字节续拉，即从一个**大**序号 +1），更像
  「另一条流的字节串进了同一个 recorder」或「重基准帧被按旧偏移切片」。
  优先查：移动端 `TerminalLinkManager` 的帧→会话路由，与插件 `ws_terminal` / `output`
  的游标推进在并发拉取下的交互。
- 按 §8「真源换了地方就要 fail-visible」，当时选择让红灯**保持亮着**（不静默降级、
  不放宽断言）——这个处置本身是对的，但**「三次红 = 缺陷」这个推断不成立**：
  票 08 复跑 8/8 绿（含 11× CPU 负载下），且期间唯一相关的变化是 **wasm 产物重建**。
  故本条降为「未复现的疑似竞态」，详见 §7.5。

### 7.5 票 08（测试迁移 + D10 试点）的实施裁决

#### A. 测试迁移：只有 7 项能走，其余有硬理由留在宿主

两条**方向性**理由（不是「懒得搬」）：

1. **测试依赖方向不得与 crate 依赖方向相反**。票 02-06 的迁移让被测类型进了
   server lib crate，但它们的测试还住在宿主 `src-tauri/tests/` 并经 `bedcode_desktop_lib::*`
   取那些类型——那是**只在测试里存在**的「宿主 → server-lib」反向边，等于给
   「面认识宿主」留了个后门。两条都改了：

   | 用例 | 新家 | 改动 |
   | --- | --- | --- |
   | `link_crypto_http.rs`（4 项） | `bedcode-server-http/tests/` | x25519 客户端侧改直取 `bedcode_crypto_engine` |
   | `error_envelope_integration.rs`（3 项） | `bedcode-server-base/tests/error_envelope_ipc.rs` | `AppError` 直取定义处 |

   **放哪个 crate 不是随意选的**：`link_crypto_http` 的被测单元是
   `TrafficFilter`（http 面）× `link_crypto` / `TrafficFilterChain`（core），两侧都在。
   放 core 就得 dev-depend http ⇒ **横向边在 `dev-dependencies` 里也是横向边**
   （单向引用即可编译，是真实可发生的越线形态；票 07 的边界锁对全段清单判横向）。
   故放 http——core 本来就是它的生产依赖，零新增横向。代价是 `http` 多一条
   `bedcode-crypto-engine` 的 dev-dep（只因测试要扮演客户端自建密钥对），
   已显式登记进 `crate_boundary_lock::ALLOWED_DOWNWARD_EDGES` 并注明**仅 dev 段**。

2. **剩下 4 个宿主集成测试带不走**：`pty_session_chain` / `http_auth_biometric` /
   `ws_auth_rules` / `broadcast_shutdown` 都需要**真实 AppContext + 真实 wasm 产物**
   （进程级 `OnceLock` 单例）。server lib crate 不得依赖宿主 crate（I2），所以这四个
   在架构上就只能留在宿主。`server_integration.rs` 则是**组合根自己的集成测试**
   （验「宿主装配出什么」），留在宿主是语义正确的归属，不算遗漏。

#### B. D10 试点：本插件的路由公理 × **真实**宿主网关

`wasm-apps/terminal-session/rust/src/d10_contract_test.rs`（两条腿，零 server 侧 mock）：

- **腿 A（HTTP）**：把本插件 `http_routes::ROUTES` 逐条注册进**真实**
  `bedcode_server_http::registry`（档位解析按宿主 `host_api/http.rs` 逐字口径），起**真**
  `serve()`（HTTP + WS 两个 face），真实 `reqwest` 从外部连入。断言 8 组：
  ① 冻结别名表 ↔ ROUTES 逐条一致 · ② 24 条网关别名 × 声明方法全部真到达插件边界 ·
  ③ 转发入参逐字（模板捕获 `params` / body / query / device / caller）· ④ 档位 A/B
  （`none` 免凭证 200 ↔ `jwt` 无凭证 401，且 401 不得触达插件边界）· ⑤ 未声明方法 404 ·
  ⑥ 仅内部端点不对外可达且内部路径可达 · ⑥b **内部路径上的档位**
  （`task-status` / `session-mode` 免凭证可达；`jwt` 档内部端点无凭证必 401
  ⇒ 内部前缀不是免鉴权后门）· ⑦ 宿主静态别名 `/static/terminal-bg` 四跳可达链。
- **腿 B（WS）**：manifest `contributes.wsEndpoints` → **真实**
  `bedcode_server_websocket::endpoint::register`（挂载路径含属主段）→ 真 WS 升级 →
  认证闸门 **A/B**（错 token ⇒ 立刻 close 4001；对 token ⇒ 出现
  `<plugin-id>::ws:client-connect` 接入事件且 `authenticated: true`）。另断言
  **manifest 与插件代码里的端点常量不漂移**（`ws_events::SESSION_CONTROL_PATH` /
  `ws_terminal::ENDPOINT_PATH` 必须在 manifest 里声明）——两份清单此前无共同断言，
  manifest 少写一条 ⇒ 宿主不挂载 ⇒ 插件广播出口静默丢帧。

**形态：为什么是 `src/` 下的 `#[cfg(test)]` 模块而不是 `tests/*.rs` 集成目标**
（实测踩到）：集成目标需要本 crate 以**可链接形式**（rlib）产出，而 `crate-type` 加上
`rlib` 后 cargo 会连带为**宿主目标**构建 x86_64 cdylib，其 wit-bindgen 导出名
（`bedcode:plugin/abi#form`）在 ELF version script 里非法 ⇒ 链接期直接失败。单测 harness
不需要 crate-type 可链接。SDK 能用 `["lib","cdylib"]` 是因为它没有 WIT 导出。

**「魔 token」A/B 而不是起两个进程**：`bedcode_server_base::ports` 是进程级 `OnceLock`
且 `init` 重复调用 panic，所以两条腿共用一次装配；`AuthCenter` 实现只认一个固定 token，
把「验签成功/失败」压进 header，从而在**同一个服务器**里对拍 `none` / `jwt` 档。

#### C. 首版设计的两个缺陷（都是变异自检抓出来的）

1. **「注册被测表、又用被测表发请求」抓不到别名拼错**。变异实测：把 `configs` 的别名
   `/api/configs` 改成 `/api/cfgz`，用例**全绿**（注册的与请求的都是 `/api/cfgz`，自洽）。
   而 `/api/configs` 是移动端 `bedcode-mobile/src-tauri/src/commands/session.rs` 的字面量，
   拼错在生产上就是移动端配置页整块 404。仓内**没有第二个 URL 真源**可依赖（移动端 crate
   不能被桌面插件 dev-depend；manifest 也不含 HTTP 面，ABI v29 已退役静态声明面）。
   故按本仓先例（黄金形状锁）**冻结一份字面量** `FROZEN_GATEWAY_ALIASES`，改为
   「注册 ROUTES、请求冻结表」，两个来源不一致即红。
2. **`/static/terminal-bg` 不是网关别名**（宿主自有静态路由，取文件在宿主）。首版把它
   混在「全部别名都转发」里，得到一个**假红**（404）。拆成专用断言：先证「无图片 ⇒ 404」
   且「无凭证 ≠ 401」，再真的放一张 `terminal_bg.png` 证 200 + `image/png`——四跳各自坏
   都表现为同一个 404，不放图就无法区分。

**变异自检 M1-M4 全部实测打红**：① 别名拼错（冻结表对不上）；② 方法写错
（`DELETE`→`POST`，冻结表对不上）；③ 档位越权（`session-mode` `none`→`jwt`，
「环回 hook 免凭证」断言打红）；④ manifest 少写一条 `wsEndpoints`（端点常量漂移断言打红，
消息逐字点名 `terminal`）。

#### D. CI 门禁补齐（否则新测试在合并门禁上无人看守）

`test.yml` 一直只跑两端 `src-tauri` + SDK + 六个 server lib crate（票 04 补的）——
**四个 wasm 应用的插件侧 crate 从来没进过 CI**（AGENTS §3 列了命令，门禁上没跑）。
本票新增 `Cargo test (desktop wasm app crates)` 步骤逐个 `cd` 进 crate 根跑 `cargo test`
（native 单测，不是 wasm32-wasip3；产物链由既有的 `plugins:build` 负责）。
四个 crate 实测：terminal-session **423** / agent-hub 148 / file-transfer 67 / ai-chatbox 15。

#### ⚠️ §7.4「新发现」的下调：终端输出缺口**未能复现**

票 07 报 `terminal_output_pressure` 阶段 A **三次连续失败**并定性为真实产品缺陷。
票 08 复跑：**全量 8/8 绿**，该场景**单独再跑 4 次全绿**，并在**约 11× CPU 负载**下
（负载 10.9，耗时从 14s 涨到 28.7s）**仍绿**。期间与运行时相关的唯一变化是
**wasm 产物重建**（`node scripts/build.js`，wasmHash 已重新注入）；插件源码改动全是
`#[cfg(test)]` 与 `mod`/`pub mod` 的等价回退。

⇒ 结论从「真实产品缺陷（3/3 稳定复现）」下调为「**未复现的疑似竞态**」。仍需独立排查
（入口不变：移动端 `TerminalLinkManager` 帧→会话路由 × 插件 `ws_terminal`/`output` 游标
推进的并发交互），但**不再有确定性证据**支撑，不应按已确认缺陷排期。
方法论教训：首跑红就定性缺陷太早——三次连续红在低概率竞态下完全可能来自运气，
而「负载敏感」这个最自然的假设实测**不成立**（CPU 负载只影响耗时），
真正的变量（wasm 产物重建）当时没被识别。**下一定性的门槛：可复现的最小场景。**
