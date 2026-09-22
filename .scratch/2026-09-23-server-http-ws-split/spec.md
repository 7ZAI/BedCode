# 桌面端 server 模块拆分：http 与 websocket 传输面独立化

Status: **approved（2026-09-23 拆票完成）**——D1–D8 已由用户裁决，实现拆为 `issues/01..09` 九张票，拓扑见 §6。开工前门禁（§0）仅票 03 受阻，票 01 / 02 可立即开工
Date: 2026-09-23
范围: **仅桌面端**（`bedcode-desktop/src-tauri/src/server/`）；移动端无 `server` 模块（实测 `bedcode-mobile/src-tauri/src/` 无该目录），零跟演义务
性质: **纯结构重构（move-only）**——不改任何行为、协议、路由语义、权限词汇、WIT/ABI 计数
关联: AGENTS.md §5 架构红线（高内聚低耦合）、§6 Rust 规范（`module.rs` + 同名目录，禁 `mod.rs`）、§0 最小改动原则、ADR 0022（裁剪线）、`.scratch/2026-09-18-ws-base-service/spec.md`（阶段 A 已在代码层分离「连接骨架 / 通道协议」，本票是其**目录层**收尾）、`.scratch/2026-09-20-host-business-decarriage/`（`services/` 归属的后续线）

---

## 0. 前置门禁（开工前必读，当前未满足）

**`server/` 内存在本票之外的未提交改动**（2026-09-23 实测 `git status --porcelain bedcode-desktop/src-tauri/src/server/`；全仓在途范围更大：69 modified + 171 deleted，主要是 `commands/` → `commands.rs` 聚合线）：

| 文件 | 在途 diff | 与本票的关系 |
| --- | --- | --- |
| `server/ws/message.rs` | 141 行删除（`-168/+31` 的一部分） | 本票整体搬移该文件——**冲突面最大** |
| `server/message.rs` | 转发壳的 `enums` 再导出清单收窄（去 `SessionConfig*`） | 本票**删除**该文件——直接覆盖对侧改动 |
| `server/ws/channel/event.rs` | 注释文案调整 | 同目录搬移 |
| `server/ws/conn.rs` | 40 行改动 | 同目录搬移 |
| `server/controllers/plugin_controller.rs` | 10 行改动 | 本票搬移到 `http/controllers/` |

在途工作线看内容是「`SessionConfig` 线协议退役」，与本票正交但**文件集重叠**。

**门禁**：开工前这 5 个文件必须干净（对侧提交或本票排队等待）。禁止在其上 `git mv`、禁止 `git checkout --` 逆向（AGENTS §11 文件回滚规范 1）。

> 本文档所有行数与站点计数是 **2026-09-23 工作区实测**（含上述在途改动）。开工时按 HEAD 重跑一次 §4 的计数表，差异只应来自那 5 个文件。

---

## 1. 需求与动机

### 1.1 用户指令

> 探索桌面端宿主代码 `bedcode-desktop/src-tauri/src/server` 内，请拆分出 http 和 websocket 独立文件夹模块，写 spec 文档。

### 1.2 现状盘点（实测：45 个 `.rs` / 16,248 行）

`server/` 根**平铺 15 个 `.rs` + 5 个子目录**：15 个文件里 6 个属 HTTP、5 个属 WS、4 个传输无关，且 `app.rs` 一个文件两种传输混装；`ws/` 子目录只住了 WS 的一部分。按归属分类（行数为该文件/该目录合计）：

| 归属 | 文件（行数） |
| --- | --- |
| **HTTP** | `app.rs`(372, 混合)、`gateway.rs`(1534)、`controllers.rs`(6)+`controllers/`(995)、`dtos.rs`(13)+`dtos/`(475)、`middleware.rs`(7)+`middleware/`(878)、`port_checker.rs`(163) |
| **WS** | `ws.rs`(15)+`ws/`(8,242)、`services.rs`(7)+`services/`(350, **零 HTTP 消费者**)、`message.rs`(18, 转发壳)、`connection_types.rs`(37)、`client_info.rs`(56, 死码) |
| **传输无关** | `supervisor.rs`(456)、`filter.rs`(519)、`metrics.rs`(313)、`link_crypto.rs`(1792) |

（三行合计 = 16,248，与 `find . -name '*.rs' | xargs wc -l` 的总数逐行对齐，可据此复核本表没有漏项。）

**跨面边（决定可分性，逐条实测）**：

| 方向 | 边 | 证据 | 处置 |
| --- | --- | --- | --- |
| HTTP → WS | 仅经 `server::message` 转发壳 | `services/session_control.rs:6,57,85,106,127`、`services/terminal_service.rs:6` | `services/` 整体划入 WS 面后**该方向零边** |
| HTTP → WS | `ClientInfo` | `services/session_sub.rs:5,14,29` | 死链，整删（§2 D3） |
| WS → HTTP | `server::services::*` | `ws/channel/event.rs:234,286`、`ws/channel/terminal.rs:237` | `services/` 进 `websocket/` → 变为**内部边** |
| WS → `app.rs` | `start_http_server` | `ws/websocket_manager.rs:115` | 保留（单端口 bootstrap，见 D4） |
| WS → `app.rs` | `ws_frame_limit()` | `ws/endpoint.rs:123`（+ 测试 `:318`） | 函数迁 `websocket/routes.rs`（D5）→ 边消失 |
| 双方 → 共享 | `server::filter` / `server::link_crypto` | `middleware/http_filter.rs:27,95`、`ws/conn.rs:26,371,410,446,722`、`ws/channel/{event:166,200, terminal:251,252,274}.rs` | 进 `core/`，两域各自向下依赖 |

**结论：`http` 与 `websocket` 之间不存在不可解的耦合**——三条现有边两条靠归属消解、一条靠死码删除消解。

**关键佐证（实测，本票可行性最硬的一条）**：两域共用的**认证档位词汇 `EndpointAuth` 真源不在 `server/`，而在 SDK**——`bedcode-desktop/packages/plugin-sdk-desktop/rust/src/types.rs:304` 定义，HTTP 侧 `gateway.rs:47` 与 `controllers/plugin_controller.rs:9` 都 `use bedcode_plugin_api::EndpointAuth`（**直连 SDK**），WS 侧 `ws/endpoint.rs:35` 只是 `pub use bedcode_plugin_api::EndpointAuth` 再导出。所以拆开后**不需要新建任何共享词汇模块**：两域各自向 SDK 取，正是 AGENTS §7「权限词汇唯一真源在桌面 SDK」那条纪律在传输面上的体现。（若当初把 `EndpointAuth` 定义在 `ws/endpoint.rs`，本票就得先做一次词汇外提——那才是真正的前置 prefactor 票。）

**次级佐证**：`core/filter.rs` 的 `FilterContext` 同时带 HTTP 专属字段（`negotiation:116`、`outbound_headers:122`）与 WS 语义（`peer` 双含义 `:110`），`link_crypto.rs` 单文件内 HTTP 信封与 WS 帧两分支共用同一身份与配置——**这两块是"必须共享"而非"没拆开"**，故归 `core/`（§9.4 记 link_crypto 的三面拆分风险）。

### 1.3 病灶

1. **归属不可读**：`services/` 名字像业务服务层，实际**唯一消费者是 WS 通道**（HTTP 侧零调用）；`message.rs`(18) 是个纯转发壳，注释写「WebSocket Message Types」却躺在根目录。新人无从判断哪块能独立改。
2. **`app.rs` 是一个文件两种传输的熔接点**：三个 WS 握手 handler（`:41`/`:59`/`:75`）+ 两个公开 HTTP 端点（`:149`/`:178`）+ `/api` scope（`:259`）+ 单端口 bootstrap（`:305`）挤在同一 372 行里；WS 专属参数 `ws_frame_limit()`（`:27`）寄居其中，被 `ws/endpoint.rs:123` 与 `host_impl/ws.rs:979` 反向依赖。
3. **WS 的家没住全**：`ws/` 已存在且已按骨架/通道分层（`.scratch/2026-09-18-ws-base-service` 阶段 A 成果），但 WS 专属的 `services/`、`message.rs`、`connection_types.rs`、`client_info.rs` 仍在根目录，与 HTTP 侧平级混排。
4. **服务器 bootstrap 所有权倒挂**：`ws/websocket_manager.rs:115` 调 `app::start_http_server`——名为 "WebSocket Manager" 的组件持有**同时承载 HTTP 的** actix `HttpServer` 生命周期，`supervisor → WebSocketManager → app` 三跳。本票只登记不修（D4 红线，§9 后续票）。
5. **确认死码 8 项**（含两条整链死码，见 D3 表：约 266 行，占 `server/` 的 1.6%）；另有 1 项**看着像死码但实际是活路径**（`session_control::handle_control` / `RefreshEvent`），已在 D3 表标为勿删。

### 1.4 非改造范围（红线）

- **wire 协议零变更**：`websocket/message.rs`(1520) 的 `Message` 枚举、TB v3 二进制帧、`/ws/event` 同步广播面——移动端兼容红线，只做路径搬运。
- **路由面零变更**：路由字面量集合、方法绑定、`/api` scope 的「网关挂在验签之后」硬约束（`gateway.rs:726`/`:756` 两条 `include_str!` 锁守护）本票必须**逐字保持**，且锁要随迁（§5.1）。
- **中间件层级零变更**：`TrafficFilter` 现挂在 `App` 级（覆盖 WS 升级请求与公开路由），**不得**顺手收进 `/api` scope——那是覆盖范围收窄的行为变更（§9）。
- **不动**：actix 单端口模型、`WebSocketManager` 的 bootstrap 所有权、WIT/ABI/权限词汇、`system/constants/server.rs` 常量归属、前端任何文件。
- 不做业务下沉（`websocket/services/` 的再归属另票）。

---

## 2. 定案决策（D1–D8）

| # | 决策点 | 定案 | 理由 |
| --- | --- | --- | --- |
| **D1** | 两个传输目录命名 | `server/http/` + `server/websocket/` | 用户选定。与 `http` 对称、语义自解释；代价是 `crate::server::ws::` 40 处机械改写（反正同批要改） |
| **D2** | 传输无关层 | 新建 `server/core/` 收 `app.rs` `supervisor.rs` `filter.rs` `link_crypto.rs` `metrics.rs` `port_checker.rs` | 用户选定。三层读法一眼分明：`core` 内核 / `http` `websocket` 两个传输面。代价是这 6 个文件的引用面随改：`server/` 内 27 处 + 外部 13 处 + 7 个集成测试文件 |
| **D3** | 死代码 | **本票一并删除**（下表 8 项，共约 266 行；表内另有一条「勿删」警示） | 用户选定。先删再搬，避免「把死码搬进新目录 → 第二票再删一遍同一路径」；每条判据是可编译期引用扫描为零，删除即由编译器背书 |
| **D4** | `app.rs` 归属与瘦身 | 迁 `core/app.rs`，**只做单端口组合物**：保留 `start_http_server` + 跨传输 wrap（CORS / Logger / metrics 计数）+ 一个只调两侧 `configure_routes` 的 `configure_routes`；WS 三个握手 handler 与 HTTP 公开端点+`/api` scope 分别抽到 `websocket/routes.rs` 与 `http/routes.rs` | HTTP 与 WS 共用**一个端口、一个 `HttpServer`**，组合物必然同时认识两面，放进任何一侧都会造出 `http → websocket`（或反向）的假边。bootstrap 所有权倒挂（病灶 4）本票不动，属行为面重构 |
| **D5** | `ws_frame_limit()` | 迁 `websocket/routes.rs`，保持 `pub(crate)` | 它是 WS 帧上限（读 `AppConfig::network.ws_*`），与 HTTP 无关；迁走后 `ws/endpoint.rs:123` 的反向依赖变成面内依赖，外部唯一消费者 `host_impl/ws.rs:979` 重指向新路径 |
| **D6** | `services/` 归属 | 迁 `websocket/services/`（`session_control.rs` + `terminal_service.rs`；`session_sub.rs` 删除） | 实测零 HTTP 消费者，只有 WS 通道调用。名字上的「业务服务」是既有错位，本票只搬不修（§9 首条），并在 `websocket/services.rs` 模块注释里写明「唯一消费者是 `websocket/channel/*`，归属应为会话业务」，防止下一个人再误读 |
| **D7** | 依赖方向不变量 | **I1 `http ↮ websocket`**（双向零 import，硬锁）；**I2 `http → core` / `websocket → core`** 允许；**I3 `core → 传输面` 的唯一豁免点是 `core/app.rs`**（组合物：路由装配 + wrap），且只能用于两面的 `configure_routes` 与 `http::middleware::http_filter::TrafficFilter` | 不变量必须**可机械验证**才算锁，否则一次「顺手引用」就烂掉。`TrafficFilter` 挂在 `App` 级是现状（红线 §1.4），故它是组合物必须 import HTTP 的第二条理由——写进白名单而不是造假边 |
| **D8** | facade 与兼容壳 | `server.rs` 只保留**既有**对外符号 `pub use`（实测外部只消费 `crate::server::DeviceConnectionInfo`，2 处，均在 `commands.rs:386,391`），改指 `websocket::connection_types`；**不新增任何转发壳** | 现状已有一个 `message.rs` 转发壳，它是本票的删除对象——再造一个等于重犯病灶 1。参照 `plugin.rs` 的 facade 先例：facade 只做「再导出」，不做「兼容别名」 |

### D3 死代码清单（逐条实测判据）

| 项 | 行数 | 判据（全仓 `*.rs` 引用扫描） |
| --- | --- | --- |
| `server/client_info.rs` | 56 | 仅 `server.rs:22` 再导出 + `services/session_sub.rs:5,14,29`（同为死码）+ 自身测试；已被 `ws/session.rs::WsSession` 与 `ws/registry.rs` 取代 |
| `server/services/session_sub.rs` | 38 | `subscribe_session:13` / `unsubscribe_session:28` 全仓零调用者（`ws/channel/terminal.rs:83,175` 命中的是同名私有方法 `handle_session_subscribe`，无关） |
| `server/message.rs` | 18 | 转发壳。消费者全在 WS/为 WS 服务（`ws/channel/event.rs:22`、`ws/websocket_manager.rs:9`、`services/session_control.rs:6`、`services/terminal_service.rs:6`）+ **3 个集成测试文件**各 1 处（`broadcast_shutdown.rs:43`、`pty_session_chain.rs:49`、`ws_auth_rules.rs:30`）；`ConnAuthPayload`/`ConnAuthStage` 别名（`:16`）**全仓零消费者** → 删壳、上述引用改指 `websocket::message::Message` 与 `crate::enums::*` |
| `server/middleware/cors.rs` | 14 | `cors_config()` 全仓零调用者（`app.rs:321` 是内联构造 `Cors::default()`） |
| `server/dtos/auth_dto.rs` | 116 | 11 个结构体逐条扫描（`PairingRequest` `PairingResponseData` `VerifyPairingRequest` `AuthTokenResponseData` `QrConnectRequest` `ReauthRequest` `Biometric*Request/ResponseData`）**各 0 消费者**，仅 `dtos.rs:5` 的 `pub mod` 声明。复核式：对每个 `pub struct` 名 `grep -rn "\bX\b" src/ tests/ \| grep -v auth_dto.rs` |
| `server/dtos/plugin_dto.rs` | 4 | 空模块，零 `pub` 项 |
| `session_control::{handle_control, RefreshEvent}` | — | **⚠ 不是死码，勿删**——`handle_control_message`（活口，`ws/channel/event.rs:286` 调用）在 `:187` 调 `handle_control`、在 `:196/:208` 构造 `RefreshEvent`。二者虽无**外部**调用者，但是同文件内的活路径；且 `RefreshEvent` 是 `sessions-refresh` Tauri 事件的 payload（前端在监听）。列在此处是为了挡住"零外部消费者即删"的误判 |
| `connection_types::PairingCodeGeneratedEvent` + `enums` 再导出 | ~14 | 该事件全仓从未构造；`:37` 的 `pub use crate::enums::{AuthPayload, AuthStage}` 与 `message.rs:10` 重复，随壳删除一并消失 |
| `dtos/git_dto.rs::GitCheckoutRequest` | ~6 | 仅 `git_dto.rs:24` 自身定义，全仓零引用 |

**实际删除 8 项、约 266 行**（上表的 `handle_control`/`RefreshEvent` 行是警示，不计入）。

**保留但生产零消费者的**（勿顺手删）：`dtos/{config,file,git}_dto.rs` 的其余结构体是 `gateway.rs:1314 business_endpoint_shapes_are_locked_for_dual_track` 与 `plugin/manager/wasm_runtime/tests/session_e2e.rs:1292-1320` 两条**形状契约锚点**的消费者，删了等于拆双端协议锁。

---

## 3. 目标结构

### 3.1 目标树

```
server/
├── server.rs                 # 模块根：声明三个子模块 + facade（仅 DeviceConnectionInfo）
│
├── core.rs                   # ┐
├── core/                     # │ 传输无关内核（D2）
│   ├── app.rs                # │ 单端口组合物：start_http_server + 跨传输 wrap + 两侧 configure_routes
│   ├── supervisor.rs         # │ 服务器生命周期 / mDNS 联动 / 指标采样
│   ├── port_checker.rs       # │ 端口探测与冲突弹窗
│   ├── filter.rs             # │ TrafficFilterChain + TrafficChannel{Http,WsTerminal,WsEvent,WsPlugin}
│   ├── metrics.rs            # │ 跨传输计数器（http_request / ws_sent / ws_recv / encrypted_frame）
│   └── link_crypto.rs        # │ 链路加密：身份+配置共享，HTTP 信封与 WS 帧两分支（不可拆，见 §9）
│
├── http.rs                   # ┐
├── http/                     # │ HTTP 传输面
│   ├── routes.rs             # │ 【新】公开 HTTP 端点（health / terminal-bg）+ /api scope 与其 wrap 链
│   ├── gateway.rs            # │ 业务 URL 别名表 + business_gateway 中间件
│   ├── controllers.rs + controllers/    # plugin_controller / session_controller
│   ├── dtos.rs + dtos/                  # common / session / config* / file* / git*（* = 契约锚点）
│   └── middleware.rs + middleware/      # http_filter / jwt_auth
│
├── websocket.rs              # ┐
└── websocket/                # │ WebSocket 传输面
    ├── routes.rs             # │ 【新】三个握手 handler + endpoint_owner_activated + ws_frame_limit
    ├── conn.rs               # │ 通用连接骨架（阶段 A 成果，零业务语义）
    ├── channel.rs + channel/ # │ terminal / event / plugin 三通道实现
    ├── registry.rs           # │ 连接注册表（ChannelKind + owner/endpoint_id）
    ├── endpoint.rs           # │ 插件端点注册表
    ├── subscription.rs       # │ 输出订阅原语（背压 ack + 桥接）
    ├── terminal_ws.rs + terminal_ws/    # control_frame / forward / subscriber
    ├── message.rs            # │ 移动端兼容红线 wire 协议（1520 行，只搬不改）
    ├── session.rs            # │ WsSession 连接态
    ├── websocket_manager.rs  # │ 生命周期与优雅停机（含 bootstrap 调用，D4 现状保留）
    ├── connection_types.rs   # │ 【收窄】只留 DeviceConnectionEvent / DeviceConnectionInfo
    └── services.rs + services/          # 【D6】session_control / terminal_service
```

规模分布（D3 删除后，按 §3.2 拆 `app.rs` 重算）：`core/` ≈ 3,330 行（`app.rs` 瘦身为纯组合物约 90 行）· `http/` ≈ 3,910 行（含新 `routes.rs` ≈ 140）· `websocket/` ≈ 8,760 行（含新 `routes.rs` ≈ 115）。三块合计 ≈ 16,000 ≈ 「现状 16,248 − D3 死码 266」（`routes.rs` 是抽取不是复制；±20 行是 `app.rs` 三段切分的注释/签名归属误差）。

### 3.2 逐文件映射表

**动作图例**：`mv` = `git mv`（内容零改，仅内部 `use` 随 §4 改）；`split` = 从源文件抽出新文件；`trim` = 搬移同时删掉本文件内的死项。

| 旧路径 | 新路径 | 动作 |
| --- | --- | --- |
| `server/app.rs` | `server/core/app.rs` | mv + split（`:27-33 ws_frame_limit` → `websocket/routes.rs`；`:41-146` 三个 WS handler + `endpoint_owner_activated` → `websocket/routes.rs`；`:148-234` health/terminal-bg → `http/routes.rs`；`:237-299` `configure_routes` 一拆为三） |
| `server/supervisor.rs` | `server/core/supervisor.rs` | mv（`:20 use super::metrics` 同目录，**零改**） |
| `server/port_checker.rs` | `server/core/port_checker.rs` | mv + 头注释「WebSocket 服务器启动前」→「服务器启动前」（它管的就是共享端口） |
| `server/filter.rs` | `server/core/filter.rs` | mv + 修 `:53` 死链（§5.3） |
| `server/metrics.rs` | `server/core/metrics.rs` | mv |
| `server/link_crypto.rs` | `server/core/link_crypto.rs` | mv |
| `server/gateway.rs` | `server/http/gateway.rs` | mv + 两条 `include_str!("app.rs")` 重定向（§5.1） |
| `server/controllers.rs` / `controllers/{plugin,session}_controller.rs` | `server/http/…` 同名 | mv |
| `server/dtos.rs` / `dtos/{common,session,config,file,git}_dto.rs` | `server/http/…` 同名 | mv（`dtos.rs` 删掉 `auth_dto`/`plugin_dto` 两行声明） |
| `server/dtos/auth_dto.rs`、`server/dtos/plugin_dto.rs` | — | **delete**（D3） |
| `server/middleware.rs` / `middleware/{http_filter,jwt_auth}.rs` | `server/http/…` 同名 | mv |
| `server/middleware/cors.rs` | — | **delete**（D3） |
| `server/ws.rs` | `server/websocket.rs` | mv + 改写模块声明（新增 `routes`、`services`、`connection_types`） |
| `server/ws/*`（`channel/` `terminal_ws/` 两目录 + 9 个平铺 `.rs`：`conn` `endpoint` `message` `registry` `session` `subscription` `websocket_manager` `channel.rs` `terminal_ws.rs`） | `server/websocket/*` 同名同层 | mv（`channel/*.rs` 的 `use super::super::conn` 解析目标不变，**零改**） |
| `server/services.rs` + `services/{session_control,terminal_service}.rs` | `server/websocket/…` 同名 | mv（**不删** `handle_control`/`RefreshEvent`，见 D3 表的勿删警示） |
| `server/services/session_sub.rs` | — | **delete**（D3） |
| `server/message.rs` | — | **delete**（D3，转发壳） |
| `server/connection_types.rs` | `server/websocket/connection_types.rs` | mv + trim（删 `PairingCodeGeneratedEvent` 与 `enums` 再导出） |
| `server/client_info.rs` | — | **delete**（D3） |
| — | `server/http/routes.rs` | **new**（自 `app.rs` 抽出） |
| — | `server/websocket/routes.rs` | **new**（自 `app.rs` 抽出） |
| — | `server/core.rs`、`server/http.rs` | **new**（模块入口）——按 AGENTS §6「入口文件与目录同名、不用 `mod.rs`」，三个入口文件与各自目录**平级**放在 `server/` 下：`core.rs`+`core/`、`http.rs`+`http/`、`websocket.rs`+`websocket/`（后者由 `ws.rs` 改名而来） |

### 3.3 拆分后的接缝（三处 configure_routes）

```rust
// server/core/app.rs —— 单端口组合物（I3 唯一豁免点）
pub(crate) fn configure_routes(cfg: &mut web::ServiceConfig) {
    crate::server::http::configure_routes(cfg);        // 公开 HTTP 端点 + /api scope
    crate::server::websocket::configure_routes(cfg);   // 三条 WS 路由
}
// start_http_server 保持现状：App 级 CORS + Logger + metrics wrap_fn + TrafficFilter，
// 再 .configure(configure_routes)；bind(BIND_ADDRESS:port) 与全部 HttpServer 参数一字不改。

// server/http.rs
pub mod controllers; pub mod dtos; pub mod gateway; pub mod middleware; pub mod routes;
pub use routes::configure_routes;

// server/websocket.rs
pub mod channel; pub mod conn; pub mod endpoint; pub mod message; pub mod registry;
pub mod routes; pub mod services; pub mod session; pub mod subscription;
pub mod terminal_ws; pub mod websocket_manager;
pub use routes::ws_frame_limit;                                  // pub(crate)
pub use websocket_manager::{ClientSummary, ServerEvent, WebSocketManager};
```

`http/routes.rs::configure_routes` 内部保持**注册顺序**与 `wrap` 相对次序不变（`Scope::wrap` 后注册者先执行 → 网关写在验签之前，见 `app.rs:256-267` 的原注释，随代码整块搬走，禁止重排）。

---

## 4. 路径改写清单（编译面）

`crate::server::` 全仓实测 **150 处** = `server/` 内 **95** + 外部 **55**；另有约 3 处写成裸 `server::xxx::` 的文档注释字眼（如 `system/constants/plugin.rs:89`），不破编译但同批改。**`server/` 内部 95 处的前缀分布**（决定改写工作量）：

| 内部前缀 | 处数 | 内部前缀 | 处数 |
| --- | --- | --- | --- |
| `server::ws::` | 35 | `server::link_crypto::` | 4 |
| `server::metrics::` | 14 | `server::services::` / `::controllers::` / `::connection_types::` / `::app::` | 各 3 |
| `server::dtos::` | 9 | `server::supervisor::` / `::gateway::` / `::client_info::` | 各 1 |
| `server::message::` | 8 | `server::port_checker::` | 0 |
| `server::middleware::` / `::filter::` | 各 5 | | |

**外部 55 处的映射表**（按前缀逐个 `grep -rc` 实测；因少数行同时命中裸符号与文档字眼，逐行相加略高于 55，以本表为改写清单而非配平账）：

| 旧前缀 | 新前缀 | 外部站点 | 涉及文件（外部） |
| --- | --- | --- | --- |
| `server::ws::` | `server::websocket::` | **40** | `host_impl/ws.rs`(24) `events/sync_handler.rs`(6) `tests/ws_e2e.rs`(5) `commands.rs`(1) `host_impl/session.rs`(1) `session/session_output.rs`(1, 文档注释) `lib.rs`(1) `tests/session_e2e.rs`(1) |
| `server::dtos::` | `server::http::dtos::` | 5 | `plugin/manager/wasm_runtime/tests/session_e2e.rs` |
| `server::supervisor::` | `server::core::supervisor::` | 4 | `lib.rs:504` `system/lifecycle.rs:276` `commands.rs:13` `host_impl/config.rs:30` |
| `server::app::` | `server::core::app::`；`ws_frame_limit` → `server::websocket::routes::` | 3 + 1 文档 | `host_impl/ws.rs:979`（`ws_frame_limit`）、`tests/ws_e2e.rs:275,765`（`start_http_server`）、`system/constants/plugin.rs:89`（文档字眼） |
| `server::link_crypto::` | `server::core::link_crypto::` | 3 | `commands.rs:13` `lib.rs:512` `host_impl/auth.rs:356` |
| `server::metrics::` | `server::core::metrics::` | 1 | `commands.rs:13` |
| `server::port_checker::` | `server::core::port_checker::` | 1 | `lib.rs:348` |
| `server::filter::` | `server::core::filter::` | 0（内部 5） | — |
| `server::message::` | `server::websocket::message::` / `crate::enums::` | 0（内部 8） | 集成测试按 `bedcode_lib::server::message::{…}` 使用，见下 |
| `server::controllers::` `server::services::` `server::gateway::` `server::middleware::` `server::connection_types::` | 加中间层 | **0** | — |
| `server::DeviceConnectionInfo` | **不变**（facade，D8） | 2 | `commands.rs:386,391` |

**crate 公共面（集成测试 `src-tauri/tests/`，8 个文件、其中 7 个共 **22 处** `bedcode_lib::server::…`，实测 `grep -rc`）** —— 走的是 crate 公开 API，`--lib` 编译不到，逐项列：

| 测试文件（处数） | 现引用 | 改成 |
| --- | --- | --- |
| `broadcast_shutdown.rs`(5) `:42-45,275` | `server::app::start_http_server`、`server::message::{AuthPayload,AuthStage,Message,SessionControlAction}`、`server::ws::registry::WsSessionRegistry`、`server::ws::WebSocketManager`、`server::ws::registry::ClientSummary` | `server::core::app::…`、`enums::{…}` + `server::websocket::message::Message`、`server::websocket::registry::…`、`server::websocket::WebSocketManager` |
| `link_crypto_http.rs`(8) `:11-12,58-85` | `server::filter::{TrafficFilterChain,Direction}`、`server::link_crypto::{…}` | `server::core::filter::…`、`server::core::link_crypto::…` |
| `ws_auth_rules.rs`(3) `:29-31` | `server::app::…`、`server::message::{…}`、`server::ws::WebSocketManager` | 同上两行口径 |
| `pty_session_chain.rs`(2) `:48-49` | `server::app::start_http_server`、`server::message::{…5 项}` | 同上 |
| `server_integration.rs`(2) `:23-24` | `server::app::…`、`server::supervisor::ServerSupervisor` | `server::core::…` |
| `http_auth_biometric.rs`(1) `:25`、`ws_session_route.rs`(1) `:26` | `server::app::start_http_server` | `server::core::app::start_http_server` |
| `build_manifest_smoke.rs`(0) | 无 | — |

> 这批文件是 **CRLF**（`broadcast_shutdown.rs` / `ws_session_route.rs` 已实测），改写禁用 python text 模式，只走 Edit 工具，改完核 `git diff --ignore-cr-at-eol`。

**`server/` 内部** 95 处按 §3.2 逐目录改，大户：`ws/conn.rs`(15) `app.rs`(11) `link_crypto.rs`(9) `ws/registry.rs`(7) `gateway.rs`(7) `ws/channel/event.rs`(5) `supervisor.rs`(5) `services/session_control.rs`(5) `ws/channel/terminal.rs`(4) `middleware/http_filter.rs`(4)。

---

## 5. 易漏点（不修就是假绿，逐条带证据）

### 5.1 `include_str!("app.rs")` 两条锁会**静默恒真**

`gateway.rs:728` 与 `:758` 用 `include_str!("app.rs")` 扫宿主路由字面量。搬移后：

- 相对路径失效——`gateway.rs` 在 `http/`，路由在 `http/routes.rs`，`include_str!` 编译期就报，这一半编译器会抓；
- **危险的是另一半**：`BUSINESS_ROUTES` 现有 **17 条，全部 `FallbackPolicy::PluginRequired`**（实测 `gateway.rs:142-297`），而该分支断言的是 `!APP_RS.contains(path)`。**只要扫到一个不含路由字面量的文件，17 条断言全部恒真**——锁从「守护业务面不再长回来」退化成空转，而且全绿。

**必修**：重定向到 `include_str!("routes.rs")`（即 `http/routes.rs`——别名表只管 `/api` 面，本就该扫 HTTP 侧），并在同一条测试里加**自校准前置**：扫描目标必须命中 `/api` 面的活路由字面量，命中数为 0 即 `panic!("routes.rs 扫描目标无路由字面量，锁已空转")`。

> **【票 01 已落，2026-09-23，实跑见 `issues/01` Comments】两点与上文原表述不同，票 04/05/06/07 按此接线：**
> ① 两条锁不再各写一份 `include_str!`，而是共用取源点 `const APP_RS`（现 `gateway.rs:727`，随附 `APP_RS_LABEL`
> 供失败消息点名目标）——**重指向只改这两行**（漏改 LABEL 会让失败消息点假名，票 04 变异验证实测到），前置自动跟着验新文件；② 前置判据取**基线数**而非「命中数为 0」
> （`HOST_ROUTE_IDENTIFIERS_BASELINE = 14`，票 07 收窄为 HTTP 侧后改钉 11）：0 命中只是它的一个特例，
> 「文件还在、路由已被搬空」这种半死不活态只有数量判据抓得住。第二条锁另挂「非空 + 体内有 `.route(`」前置
> （其 handler 名零命中是合法现状，不能拿数量当它的判据）。上文行号 `:728`/`:758` 已随本票移动。
>
> **【票 04 已落，2026-09-23 commit `36428e0a5`】** 取源点现指 `include_str!("core/app.rs")`——**不是**票面 ①
> 预期的 `../core/app.rs`：`gateway.rs` 此刻仍在 `server/` 根，`../` 形态要到票 05 把它搬进 `http/` 才对，
> **票 05 落地时同批改这一行**。另：§4 的引用计数（`server/` 内 27 + 外部 13 + 测试 7）漏了两类真实引用，
> 票 05/06/07 的验收 grep 要按 `grep -rnE "(^|[^:a-zA-Z_])server::<模块>::"` 扫（否则 `lib.rs` 的裸
> `server::app::` 漏网，只有编译器抓得到），并另扫 `bedcode_lib::server::` 的 rustdoc doctest 形态
> （`filter.rs` 有一条，改漏了 `cargo check` 不红、`cargo test --doc` 红）。

前置的判据清单**只列 HTTP 侧**（实测 `http/routes.rs` 应含 11 项）：`"/api"`、`"/plugin/{plugin_id}/{path:.*}"`、`"/sessions"`、`"/sessions/start"`、`"/sessions/{id}/{stop,resize,input,history,remove}"`、`"/static/terminal-bg"`、`API_HEALTH_PATH`。**禁止**把 `/ws/*` 三条与 `WS_EVENT_PATH` 写进这条前置——它们在拆分后归 `websocket/routes.rs`，钉在 HTTP 锁里会让票 07 一开工就红，然后被人顺手删掉，锁就没了。

这条前置加在本票，不加就是拿一个空锁换掉一个真锁。

### 5.2 路由面「多重集相等」是本票唯一的行为证明

`move-only` 的验收不能只靠「编译过了」。门禁：拆分前后各跑一次路由标识符扫描于**本票涉及的路由装配代码集合**，两个**多重集必须相等**（命令见 §7）。这是防「抽取 `http/routes.rs` 时漏掉一行 `.route(...)`」的唯一硬手段——那一行漏了编译照样过。

**⚠ 这条门禁自己也有一个坑（已实测）**：`grep -o '"/[^"]*"'` 只抓字面量，而 `configure_routes` 里有 **2 条路由用的是常量**——`cfg.route(API_HEALTH_PATH, …)`（`app.rs:251`）与 `cfg.route(WS_EVENT_PATH, …)`（`app.rs:244`）。只比字面量的话，把 `/api/health` 那条整行删掉门禁照样绿。所以扫描式必须把两个常量名一并纳入（§7 的命令已按此写成 `awk` 取 `configure_routes` 函数体 + `grep -oE` 合并式），基线 = **12 条字面量 + 2 个常量标识符 = 14 项**（实测：HTTP 侧 11 项 / WS 侧 3 项）。

### 5.3 rustdoc 内链与文档路径不是编译错误

- `core/filter.rs:53` 的 `[`super::http_filter`]` —— `filter.rs` 进 `core/`、`http_filter.rs` 进 `http/middleware/`，`super::` 解析断链；`:53` 的文字「`server/app.rs` 最内层 wrap_fn」与 `:55` 的「WS 接线点 `server/ws/terminal_ws.rs`」（**现状已过期**，阶段 A 后实际在 `ws/conn.rs`）一并修。
- 全仓文档注释里的 `crate::server::…` / `server/ws/…` 文本路径 **20 处 / 13 文件**，大户 `host_impl/ws.rs:14,15,388`、`session/session_output.rs:352`、`utils/auth/auth_center.rs:139,140`、`system/constants/plugin.rs:89`。`cargo check` 零告警，**只有 `cargo doc` 会列 broken intra-doc link**——门禁必须包含它。

### 5.4 CRLF / 混合行尾地雷

`server/` 下实测 **18 个 CRLF 文件**，其中 5 个随 D3 删除（`client_info.rs` `message.rs` `middleware/cors.rs` `services/session_sub.rs` `dtos/auth_dto.rs`），**剩 13 个存活**：`supervisor.rs`(456/456) `ws/websocket_manager.rs`(430/430) `dtos.rs` `dtos/{common,file,session,git,config}_dto.rs` `ws.rs` `ws/session.rs` `middleware.rs` `connection_types.rs` `services/terminal_service.rs`。

风险集中在**既 CRLF 又要改内容**的两个大文件：`supervisor.rs`（5 处 `crate::server::` + 1 处 `super::metrics`，后者同目录**零改**）与 `ws/websocket_manager.rs`（3 处 + `server::message` + `server::app`）。

另有一颗独立地雷：**`dtos/session_dto.rs` 行尾混合**（cr=71 / lines=93）——`git mv` 不受影响，但任何内容改写都可能顺手把全文转成 LF。

**写法约束**：`git mv` 优先（零内容改写）；必须改内容时只用 Edit 工具，**禁止 python text 模式读写**；`cargo fmt` 禁止整 crate 跑（会把 CRLF 统一成 LF，一行改动变千行 diff），只对新文件跑 `rustfmt --edition 2021 <file>`；每个 CRLF 文件改完核 `tr -dc '\r' | wc -c` 等于 HEAD 的 CR 数，并核 `git diff --ignore-cr-at-eol` 只剩目标行。

### 5.5 删码类改动 `cargo check --lib` 是假绿

D3 删 8 项，其中 **2 项带测试引用**：`message.rs` 转发壳被 **3 个**集成测试文件引（`broadcast_shutdown.rs:43`、`pty_session_chain.rs:49`、`ws_auth_rules.rs:30`）、`client_info.rs` 被自身 `mod tests`（`:39-55`）引。这两处删了之后 `cargo check --lib` **不报**（`--lib` 不编译 `#[cfg(test)]`），必须 `cargo check --lib --tests`（既有踩坑记录：2026-09-21 票 06 删函数后残留测试引用完全不报）。

### 5.6 facade 收窄会误伤 glob 再导出

`server.rs` 现状是 `:5-19` 十五个 `pub mod` 声明 + `:21-25` 五段 `pub use`：`crate::enums::control::SessionControlAction`、`client_info::ClientInfo`、`connection_types::*`、`filter::{Direction, FilterContext, Rejection, TrafficChannel, TrafficFilter, TrafficFilterChain, Verdict}`、`message::*`。

**实测这五段的裸符号消费者（`crate::server::X` 形式，扫 `src/` + `tests/`）只有 2 处，全是同一段浮上来的 `DeviceConnectionInfo`**（`commands.rs:386,391`）。故 facade 收窄到**一行**：

- **删** `pub use crate::enums::control::SessionControlAction`、`pub use client_info::ClientInfo`（随文件删除）、`pub use message::*`（随壳删除）、`pub use filter::{…7 项}`（零裸消费者；模块路径 `server::core::filter::` 照旧可用，`filter.rs:27` 的文档示例走的就是模块路径，不受影响）；
- **留** `pub use websocket::connection_types::DeviceConnectionInfo;`（改成显式单项，不再用 `*`——glob 正是「谁都能从 `server::` 捞一把」的成因）。

不要整段留、也不要整段删：留=复制病灶，删=破 `commands.rs` 两处编译。

### 5.7 闭环测试会静默 skip

`cargo test` 前必须重出插件产物（`node plugins/terminal-session/scripts/build.js` 等），否则 `test_session_*` / pty / ws 闭环用例 `[skip]` + return **算过**；报数时注明 `[skip]=0`。

---

## 6. 票据拓扑（2026-09-23 拆票后重写）

> **作废标记**：本节原为「四个 commit（C1–C4）」的划分，已被 `issues/01..09` 九张票取代。映射关系：C1 → 票 02 + 03（按撞车面劈成两票）；C2 → 票 04 + 05 + 06（**按目录一趟**，不是三合一）；C3 → 票 07；C4 → 票 08 + 09。新增票 01（前置 prefactor，原方案里没有）。commit 划分在各自票内定，票与 commit 不再一一对应。

**拆分策略裁决**：本任务按**目录原子进**（每票「`git mv` + 该目录自身引用全改写」，单票自己编译全绿），**不采用** wide-refactor 常规的 expand–contract 兼容壳。理由：本任务是互不相交的**分区搬移**，不是改一个共享符号——加壳买不到「每批都绿」（它本来就绿），只会留下只活两三票的别名，正是票 03 要删的那个 `message.rs` 壳那种债。blast radius 172 处全部编译期可见，编译器就是 oracle。

| 票 | 标题 | Blocked by | 撞车面 |
| --- | --- | --- | --- |
| 01 | 网关别名锁自校准前置 | 无 | 仅 `gateway.rs`（今日干净）→ **现在就能开工** |
| 02 | 根死码清理（7 项）+ facade 收窄 | 无 | 与在途会话零重叠 → **现在就能开工** |
| 03 | 删 `server::message` 转发壳 | 02 | ⚠ `server/message.rs`、`ws/channel/event.rs` 在途未提交 → **needs-triage，等门禁** |
| 04 | `core/` 层落地（六文件） | 01, 02 | 含锁第一次重指向 |
| 05 | `http/` 面落地 | 04 | `server.rs` 只增删自己那五行 |
| 06 | `websocket/` 面落地 + `ws` 改名 + `services/` 收编 | 03, 04, 05\* | \*05 那条边是 `server.rs` 的**物理串行**，非逻辑依赖；两票可并行推，归属规则见两票票面 |
| 07 | `app.rs` 一拆为三 | 04, 05, 06 | 九票里唯一带行为风险的一票 |
| 08 | 零互依结构锁 + 真机六链路冒烟 | 07 | 变异自检 + 真机 |
| 09 | 文档同步 | 07 | 可与 08 并行 |

**Frontier（此刻可开工）**：票 01、票 02。票 04 需要 01+02 双双落地；票 03 独立等于一个等待条件——**它才是本任务真正的瓶颈**，其余票全都在它下游或可与它并行。

各票的门禁细节写在票面，本节只定拓扑。下面 §6.1 保留为票 08 的实现口径（未作废）。

### 6.1 依赖方向锁的实现

沿用本仓**自家先例**：`gateway.rs:728/758` 用 `include_str!` 扫自身源码、`wasm_runtime/tests/pty_e2e.rs:288` 用 `include_str!` 扫 `host/activation.rs`——源码文本断言在本案是既有手法，不是新发明。

新增 `server/websocket.rs` 的 `#[cfg(test)]`（一处即可，锁两侧）：

- 用 `env!("CARGO_MANIFEST_DIR")` + `std::fs::read_dir` 递归取 `src/server/http/**/*.rs` 与 `src/server/websocket/**/*.rs` 两个清单（**必须动态枚举**，硬编码文件清单会让新增文件绕过锁）；
- 断言：`http` 侧任一文件正文不含 `server::websocket` 与 `super::super::websocket`；`websocket` 侧任一文件不含 `server::http`；两侧都不含 `crate::server::controllers|dtos|gateway|middleware|services|ws::`（旧路径复发即红）；
- 另断言 `core/` 侧除 `core/app.rs` 外不含 `server::http::` / `server::websocket::`（D7 的 I3 豁免白名单只一个文件）；
- 断言的**每个** `contains` 用「命中即列出 `文件:行`」的失败消息，便于定位；
- 变异自检：临时在 `http/gateway.rs` 加一行 `use crate::server::websocket::message::Message;`，锁必须红，然后撤销。

---

## 7. 完成定义（必须实跑并贴结果）

```bash
cd bedcode-desktop/src-tauri

# 票 02–07 每个 commit 前
node ../../plugins/terminal-session/scripts/build.js     # 产物重出，否则闭环用例静默 skip
cargo check --lib --tests 2>&1 | tail -20                # 必须 0 error / 0 unused 警告
cargo test 2>&1 | tail -40                               # 核 [skip] 计数 = 0
cargo doc --no-deps 2>&1 | grep -i "broken\|unresolved"  # §5.3 内链锁

# §5.2 路由面不变（在票 07 里跑；只扫 configure_routes 函数体，避免 import 行噪声）
# 基线 14 项已实测：12 条字面量 + API_HEALTH_PATH + WS_EVENT_PATH（两个常量必须纳入，见 §5.2 的坑）
ROUTES_RE='"/[^"]*"|API_HEALTH_PATH|WS_EVENT_PATH'
git show HEAD:bedcode-desktop/src-tauri/src/server/core/app.rs \
  | awk '/^pub(\(crate\))? fn configure_routes/,/^}/' | grep -oE "$ROUTES_RE" | sort > /tmp/routes_before.txt
cat src/server/http/routes.rs src/server/websocket/routes.rs \
  | awk '/^pub(\(crate\))? fn configure_routes/,/^}/' | grep -oE "$ROUTES_RE" | sort > /tmp/routes_after.txt
wc -l /tmp/routes_before.txt /tmp/routes_after.txt      # 两边都必须是 14
diff /tmp/routes_before.txt /tmp/routes_after.txt && echo "路由面一致"

# §6.1 结构锁
cargo test http_and_websocket_are_independent
```

其余逐项核对：

- [ ] `server/` 下无 `mod.rs`，`core.rs` / `http.rs` / `websocket.rs` 三个入口文件与目录同名（AGENTS §6）
- [ ] `grep -rn "crate::server::ws\b\|server::message::\|server::controllers::\|server::gateway::\|server::middleware::\|server::services::" src/ tests/` == 0
- [ ] D3 八项删除**各自附**判据（引用扫描命令 + 空输出），不是「看着像死码」
- [ ] `dtos/{config,file,git}_dto.rs` 及其两条形状契约锚点测试**完整存活**（§2 D3 保留项）
- [ ] 移动端零改动（`git status bedcode-mobile/` 干净）
- [ ] 前端零改动 → `pnpm run test:run` / `pnpm exec eslint .` 不适用，在 commit message 里写明理由
- [ ] 手工冒烟六条链路（真机/浏览器）：`/api/health`、`/static/terminal-bg`、`/api/sessions`、`/ws/event`、`/ws/terminal/session/{id}`、`/ws/plugin/com.bedcode.terminal-session/{path}` + 一条链路加密开关开/关各跑一次
- [ ] 测试后清理残留进程与监听端口（AGENTS §3）

---

## 8. 文档同步（票 09）

| 文档 | 待改 | 说明 |
| --- | --- | --- |
| `bedcode-desktop/docs/code-map.md` | 服务器节（`:274-323`）重写为三层；`:192` 端点注册表路径；`:405/:414/:449/:460/:461/:468` Quick Navigation 与按类型查找表 | 目录层级变化即触发维护义务（AGENTS §12） |
| `docs/knowledge/mobile-desktop-auth.md` | 8 处 | **顺带修既存失真**：`:210/:220/:463/:476-478` 指向 `server/services/{pairing,auth}_service.rs` 与 `controllers/auth_controller.rs`——这些文件**在本票之前就不存在**（认证编排已下沉 `com.bedcode.terminal-session`）。按 AGENTS §12「描述与实际不符时以实际为准并顺手修正」处理，并在 commit message 里注明是修既存失真而非本票造成 |
| `docs/knowledge/pty-output-pipeline.md` | 3 处（`:81/:87/:90` `server/ws/terminal_ws/*`） | |
| `docs/diagrams/README.md` | 1 处（`:47` `server/ws/terminal_ws.rs`） | 同样含既存失真（该文件已只剩 9 行声明） |
| `bedcode-desktop/docs/plugin-system-refactor.md` | 1 处（`:39`） | |
| `AGENTS.md` / `docs/adr/*` | **0 处** | 实测无 `server/` 路径字眼；ADR 全量 grep 亦无 |
| `.scratch/**` 历史票据（38 文件含 `server::` 字眼） | **不批量改写** | 它们是带日期的过程记录，改写照成「当时现状变了」。在 `.scratch/2026-09-23-server-http-ws-split/followups.md` 登记一条：「本票映射表是路径的当前真源，历史 spec 内的 `server/ws/…` 等字眼一律按其自身日期理解」 |

---

## 9. 后续票（本票明确不做）

1. **`websocket/services/` 的再归属**——它承载的是会话控制与终端输入业务，不是 WS 传输原语（ADR 0022 裁剪线视角）。并入 `.scratch/2026-09-20-host-business-decarriage` 线，本票只搬不改就是为了不让它蹭上这班车。
2. **bootstrap 所有权正名**——`websocket_manager.rs:115` 持有承载 HTTP 的 `HttpServer`，应抽出 `core::server_runtime`（名字即职责），使 `supervisor → runtime → {http, websocket}` 两跳到位。这是行为面重构，风险与本票的 move-only 性质不兼容。
3. **`TrafficFilter` 从 `App` 级收进 `/api` scope**——会收窄其覆盖范围（现在 WS 升级请求与公开路由也过链），属安全边界变更，需独立论证（AGENTS §8 安全红线优先级）。
4. **`core/link_crypto.rs`(1792) 三面拆分**——实测其 HTTP 分支（`HttpTrafficKeys:298` / `derive_http_traffic_keys:309`、`on_http_inbound:740`、HTTP 密钥缓存 `:598-652`）与 WS 分支（`WsSessionCiphers:337`、`derive_ws_session_ciphers:354`、`on_ws_frame:700`）边界清楚，**但**身份与落盘（`LinkIdentity:170`/`init_identity:263`）、单一 `LinkCryptoConfig`（`:73-89` 同时携带 `encrypt_http` + `encrypt_ws_terminal` + `encrypt_ws_event`）、`LinkEncryptionFilter` 的 `should_process` 通道表三处共享。拆开要么 fork 配置状态、要么造第四个共享模块——本票不越线，另票给方案。
5. **`websocket/message.rs`(1520) 移动端兼容面瘦身**——`SessionConfig*` 一族正在被另一条工作线退役（§0），本票不参与。
6. **`dtos/{config,file,git}_dto.rs` 契约锚点的去留**——17 条别名全 PluginRequired，双轨期已结束的话，锚点测试与 DTO 可作 contract 删除；需先确认移动端不再依赖这些形状。

---

## 10. 风险与回退

| 风险 | 概率 | 缓解 | 回退 |
| --- | --- | --- | --- |
| 抽取 `routes.rs` 时漏挂一条 `.route(...)`，编译过但端点消失 | 中 | §5.2 路由标识符多重集守恒（唯一硬手段）+ 六条链路冒烟 | 票 07 独立成票、可单独 revert，不牵动票 04/05/06 的纯搬移 |
| 两条 `include_str!` 锁被重定向到错文件后**静默恒真**（全 PluginRequired） | 高（不做 §5.1 前置则必然发生） | 票 01 的自校准前置 + 票 07 重指向时当场变异验证 | 同上 |
| `ws → websocket` 改名漏改一处，靠 facade 再导出「碰巧编译过」，实际留下隐式兼容壳 | 低 | `server.rs` 不引入任何 `pub use websocket::…` 之外的别名（D8）+ §7 的 `grep` 清零门禁 | — |
| CRLF / 混合行尾文件被内容改写整文件重排，diff 淹没真实改动 | 中 | §5.4：`git mv` 优先（零内容改写），必须改内容的文件走 Edit 工具 + `git diff --ignore-cr-at-eol` 核对 | — |
| 在途 `SessionConfig` 退役线与本票撞文件 | **高（§0 已实测）** | 前置门禁：对侧提交后才开工；期间本票只允许停留在文档 | 不动对侧任何文件（AGENTS §11 回滚规范 1） |
| 集成测试面（crate 公共 API）改名破坏 `tests/` 且被 `--lib` 掩盖 | 中 | 一律 `cargo check --lib --tests`；§4 表格逐文件核对 | — |

**为什么不做成「兼容壳 + 后续票再改调用点」**：`message.rs` 转发壳就是那条路的既成结果——它自 2026-07 模块扁平化以来就是壳，末次提交（2026-08-21）还只是一次全量 rustfmt，直到今天成了病灶 1。本票的对外公共面变化只落在 crate 内 + `tests/`，编译器全覆盖，留壳反而是净负债。
