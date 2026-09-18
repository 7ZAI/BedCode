# 桌面端 WebSocket 基础能力服务改造 + host-websocket 插件原语

Status: **done**（2026-09-19 收口）：阶段 A 票 02/03 done、票 01 经前置检查否决后 closed；阶段 B 票 04–07 done。唯一待补项为 §6「真机连通性」门禁（需用户在真机环境执行）与票 06 登记的前端证据（本机无 node/pnpm），详见各票 Comments
Date: 2026-09-18（v2 审核后修订）
范围: **仅桌面端**（`bedcode-desktop/`）；移动端不动（ADR 0018 契约独立，见 §7）
关联: `docs/adr/0022-plugin-host-interface-primitive-boundary.md`（裁剪线）、`docs/adr/0018-mobile-plugin-contract-independent.md`、`docs/adr/0017-plugin-inter-plugin-call-gate.md`、`docs/adr/0019-wasmtime-version-locked-across-ends.md`（本次不涉版本变更）
类比基线: `.scratch/2026-09-10-mdns-service-plugin/spec-basic-capability-service.md`（host-mdns v2 基础能力服务，本规格的同构模板）

修订记录（v1 → v2，2026-09-18 审核结论落地）：

| # | v1 问题 | v2 处置 | 位置 |
| --- | --- | --- | --- |
| 1 | 状态事件 topic 内嵌 handle 而非 owner，隔离弱于 mDNS 模式；且插件拿到句柄后才能订阅 → 一次性状态迁移事件必丢 | 事件 topic 改 **owner 作用域**（`ws:close.<owner>`），插件 activate 期即可订阅；宿主不缓冲不重放，配查询原语自愈 | D2/D3、§4.1 |
| 2 | `connect` 语义与 `ws:open` 事件自相矛盾 | 定案：connect 同步阻塞至握手完成，成功才发 `ws:open`，失败只回 Err 不发事件 | D4 |
| 3 | WIT 声明允许 `wss://`，但 `tokio-tungstenite 0.24` 未启用任何 TLS feature | 定案：本期仅 `ws://`，`wss://` 显式拒绝；wss 单独立项 | D7 |
| 4 | 服务端 `auth: "jwt"` 的首消息 wire 格式未定义 | 定案极简认证帧 `{"type":"auth","token":"<jwt>"}`，宿主不引入 `message.rs` | D8 |
| 5 | 未提过滤链 / 链路加密通道（AGENTS §8 安全边界） | 新增 `TrafficChannel::WsPlugin` + 帧过链；明确不参与链路加密及其理由 | D9、§4.6 |
| 6 | 属主隔离未在 WIT 文档声明；端点路径无 plugin-id 段（可抢占） | WIT 全函数写明「仅属主可调」；端点挂载 `/ws/plugin/<plugin-id>/<path>` | D5、§4.2 |
| 7 | 权限同步点只提 1 处（实测 4 处）；单权限覆盖两域 | 拆 `ws:client` / `ws:server`；同步点补全 4 处（含 `PERMISSION_API_MAP`） | D6、§3.3 B4 |
| 8 | 队列满 / 超限 / close code / `wasClean` 语义缺失 | 补齐发送队列 fail-visible 契约、超限拒绝方式、close code 表与 `wasClean` 定义 | D10/D11、§4.4/§4.5 |
| 9 | §7「移动端可继续加载共享插件产物」与实测不符（两端插件各自 SDK 构建） | 勘误重写，论证改为「A 阶段零 wire 变更 + 两端插件独立构建」 | §7 |
| 10 | WIT `interface abi` 版本表漂移（止于 v12 且 v12 语义错位）；行号引用偏差 | 列入 B1/B8 校正任务；行号更正（`component.rs:426-430`） | §3.3 B1/B8、§1.2 |
| 11 | 验证清单缺事件时序 / 跨插件隔离负向 / 降级测试 | §6 补齐 | §6 |

---

## 1. 需求与动机

### 1.1 用户指令

桌面端 WebSocket 基础服务改造：像 **mDNS / DB / HTTP** 一样，为插件提供**通用的 WebSocket 调用 API**（host-websocket 原语接口 + 内核基础能力服务）。

### 1.2 现状盘点（2026-09-18 实测）

**WebSocket 服务端**（`src-tauri/src/server/`）：

| 文件 | 行数 | 职责 | 现状问题 |
| --- | --- | --- | --- |
| `server/ws/websocket_manager.rs` | 417 | actix 服务器 bootstrap（init/start/stop/port + `ServerEvent{Started,Stopped}` 广播） | 基本通用，可保留（`ServerEvent` 仅 `supervisor.rs:179` 订阅，与插件无关联） |
| `server/ws/terminal_ws.rs` | 1857 | `TerminalWs` actor——**同一 actor 双模式**：终端通道（`new_for_session:180`，`bound_session=Some`）与事件通道（`new_event:190`，`bound_session=None`，`app.rs:56` 复用） | 连接骨架（握手/首消息认证/心跳/帧泵/优雅关闭）与终端语义（订阅/输出/ack/会话控制）**焊死在一个 1857 行文件** |
| `server/ws/registry.rs` | 571 | `WsSessionRegistry` 连接注册表（`Addr<TerminalWs>` 强类型依赖） | `ChannelType` 仅 `Event`/`Terminal` 两个闭合值且为 `Copy`（`registry.rs:17-24`），注册时定死，**无开放通道、无属主、无按插件回收** |
| `server/ws/message.rs` | 1650 | 终端 wire 协议（`Message` 枚举，`serde(tag="type", content="payload")`） | 终端专属，**本期不动**（移动端兼容红线） |
| `server/app.rs` | — | 路由注册：`/ws/terminal/session/{id}`、`/ws/event`、`/api/*`（JWT 中间件）、health、terminal-bg | 路由在服务器构建期静态注册；WS 两条路由均只设 `frame_size(ws_frame_limit())`（`app.rs:24-30/47-48/61-62`） |

**死代码判定（实测，供 A3 使用）**：

- `bound_session=None` 分支中，`Message::Auth`（`terminal_ws.rs:607` → `handle_auth:1203` → `handle_auth_jwt:1224`）是**活的**——事件通道首消息 JWT 认证走这里，**A3 必须保留**；
- `Message::Terminal`（`handle_terminal:1305` / `handle_subscribe:1354` / `handle_unsubscribe:1397`）与 `Message::SessionControl`（`handle_session_control:1422`）在现路由下**无生产触发者**（旧 `/ws/terminal` 兼容路由已删，`app.rs:156-159`），仅测试可达；`handle_session_mode:1013`（None→return）、`handle_session_input:1096-1102`（None→warn 丢弃）同属死代码；
- 心跳/超时：`HEARTBEAT_INTERVAL_SECS=5`、`REMOTE_CLIENT_TIMEOUT_SECS=45`、`WS_AUTH_TIMEOUT_SECS=10`（`system/constants/server.rs:16/22/28`）；
- 出站能力：文本（`SendTextMessage`，`registry.rs:158/195` 是唯一外部推送入口）与二进制（`TerminalOutputBinary`，仅终端输出桥接内部使用）**都存在**，但注册表对外只暴露文本路径。

**插件宿主能力**（WIT `world plugin`，`bedcode-desktop/packages/plugin-sdk-desktop/rust/wit/bedcode.wit`）：已有 18 个 import 接口（host-storage / host-database / host-plugin-database / host-terminal / host-session / host-events / **host-http** / host-fs / host-config / host-log / host-bus / host-api-call / **host-peer** / **host-mdns** / host-platform / host-timer / host-process / host-app）。**缺 WebSocket**——「mDNS / DB / HTTP 等」清单里 WS 是最后一个未原语化的传输能力。

### 1.3 病灶（驱动本设计的动因）

1. **传输层与终端语义焊死**：`TerminalWs` 同时承载连接生命周期与终端业务；`ChannelType` 闭合；新通道类型（插件端点）无从挂载。
2. **插件无 WS 客户端能力**：只能走 host-http 轮询/SSE；WASI 0.2 无可用 socket——`component.rs:426-430` 虽经 `p2::add_to_linker_sync` 注册了 `wasi:sockets`，但 `WasiCtxBuilder` 未放行 socket 地址检查（默认 deny）且项目 world 未 import 它，插件**离宿主无法实现**，与 host-http 同属「宿主代发」场景。
3. **插件无 WS 服务端能力**：宿主 actix 服务器（HTTP+WS 单端口）是局域网客户端唯一可连接的入口；插件不能 bind 端口、不能自建握手栈——**离宿主无法实现**，与 host-mdns `advertise`（宿主 mDNS 监听器承载）同构。
4. **死代码**：`bound_session=None` 旧路由分支（scratchpad 2026-09-13 待办）。
5. **能力缺口对照**：host-http（出站代理）、host-mdns（入站发现/广播）、host-peer（TLS 会话）均已原语化；WS 是双向传输缺口——既有客户端域（出站连接）也有服务端域（入站端点），两域都零业务语义。

### 1.4 非改造范围（红线）

- **移动端**：本规格只改桌面端；移动端契约独立（ADR 0018），同名词义保持对齐（§7）。
- **终端/事件链路 wire 协议**（`message.rs`、TB v3 帧、`/ws/event` 同步广播、`/ws/terminal/session/{id}`）：移动端兼容红线，**不动**。插件端点的认证帧**不引入** `message.rs` 类型（D8）。
- **actix 框架选型 / 端口模型 / HTTP REST 面**：不动；`websocket_manager.rs` bootstrap 保留。
- **消息总线/互调机制**（host-bus、host-api-call）：不重构，只按既有模式消费（事件投递）。
- 插件间互调仍经 ADR 0017 门，WS 能力不构成互调旁路。
- **TLS（`wss://`）**：本期不做（D7）。

---

## 2. 裁剪线依据与定案决策

### 2.1 裁剪线（ADR 0022）

**宿主 import 接口只承载「离开宿主就无法实现」的基础能力，且能力不得携带任何业务语义。**

| 能力 | 留在宿主的理由 | 业务语义去向 |
| --- | --- | --- |
| WS 客户端（connect/send/close） | WASI 无 socket；帧编解码、连接池在宿主 tokio 栈 | 消息格式、重连编排、心跳业务协议 → 插件 |
| WS 服务端（端点挂载/收发/广播） | 插件不能 bind 端口；宿主 actix 端口是唯一入站入口；帧限制/通道注册表在宿主 | 认证协议、房间/会话模型、消息格式 → 插件 |
| 事件定向投递 | 与 host-mdns v2 同构（`mdns:found.<owner>` 先例）；属主隔离是安全边界 | 事件 payload 加工、派生视图 → 插件 |

**零业务代码红线（D1，同 mDNS spec D3）**：`WsService` 只做引擎原语——连接生命周期、帧收发、句柄登记、属主仲裁、按属主回收、事件定向投递；**不拼装、不解读任何业务字段**（消息格式、房间、协议、重连策略一律由插件构造/编排）。

### 2.2 事件面形态（D2，v2 修订）

- **状态事件**（open/close/error、client-connect/client-disconnect）→ 消息总线**owner 作用域 topic**（`ws:close.<owner>`，**topic 内嵌 owner 而非 handle**，见 D3）；
- **消息帧投递**（text+binary）→ **可选导出 `events-ws`**（宿主动态探测，同 `events-binary` 模式）。理由：WS 帧天然文本/二进制双形态，经 host-bus JSON 必然 base64 浪费——v12 总线二进制（`publish-binary`/`subscribe-binary`）正是为此否决过 base64；但 bus 的 JSON/二进制订阅偏好是互斥的（`subscribe` vs `subscribe-binary`），无法承载「同一连接上 text+binary 混收」，故消息走专用导出回调，状态走 bus owner topic——两通道职责分明；
- 不导出 `events-ws` 的插件：状态事件照收（bus），消息帧丢弃 + 首次 `warn!` + 计数（可接收面缺声明，宿主不缓存）。

### 2.3 业务隔离目标（需求①的形式化）

1. **操作面**：任何插件只能操作**自己**的句柄（连接 / 端点 / 对端 client-id）——跨插件调用一律 `Err("not owner of ws handle")`；插件停用即 `purge_for_plugin` 回收本人全部资源；
2. **数据面（事件）**：状态事件 topic 内嵌 owner，非属主**物理上订阅不到**（bus 精确 topic 分发，`bus.rs:106/162-255`）；消息帧经实例回调，宿主只投给属主实例；
3. **命名空间**：端点挂载在宿主按调用方插件注入的路径段下（`/ws/plugin/<plugin-id>/<path>`），插件之间不存在路径抢占与冲突；
4. **权限**：manifest 声明 + Rust 端仲裁（双重校验），且按域拆分（D6）。

### 2.4 本次定案（D3–D11，v2 新增；可被用户反转）

| # | 决策点 | 定案 | 理由 |
| --- | --- | --- | --- |
| D3 | 状态事件 topic 形态与订阅时序 | topic 为 `ws:<event>.<owner>`（**不含 handle/endpoint-id**，标识在 payload 内）；**插件须在 activate 期（或首次 connect/register-endpoint 之前）订阅**；宿主**不缓冲、不重放**；丢失自愈靠查询原语（`is-connected` / `list-clients` / `list-endpoints`） | 现状 bus 无重放（`bus.rs:185-188` 无订阅者即丢弃），而 v1 的 `ws:close.<handle>` 要求插件拿到宿主生成的句柄后才能订阅 → open/close 是一次性状态迁移，丢失不可恢复。owner 作用域 topic 在 activate 期即可订阅，同时把隔离强度提到 mDNS 同等（topic 内嵌 owner） |
| D4 | `connect` 语义与 `ws:open` 关系 | `connect` **同步阻塞至握手完成**（上限 `connect-timeout-secs`，复用 `host_impl/http.rs` 非流式 `block_on_async` 先例）；成功返回句柄并发布 `ws:open.<owner>`；失败**只回 Err、不发事件**（无句柄可寻址，避免与返回值重复且不可寻址的 error 事件） | v1「失败以错误上抛」+ 保留 open 事件，两者叠加后 open 在句柄返回前发布、无法寻址。定案后：Err = 未建立；事件面只描述**已建立**连接的状态迁移 |
| D5 | 端点命名空间 | 插件只提供**后缀** `path`；宿主按调用方插件注入命名空间段 `/<plugin-id>/`，实际挂载 `/ws/plugin/<plugin-id>/<path>`；冲突只在同插件内可能（返回 Err） | 与既有插件 HTTP 端点 `/api/plugin/{plugin_id}/{path:.*}`（`app.rs:257-260`）一致；v1 的全局路径命名空间先注册先得，存在抢占/审计困难 |
| D6 | 权限粒度 | 拆 `PERMISSION_WS_CLIENT = "ws:client"`、`PERMISSION_WS_SERVER = "ws:server"` | 两域威胁面不同：`ws:server` 在局域网新增入站监听入口，`ws:client` 是出站连接（SSRF 面）。单权限通吃会让「只需出站」的插件被动获得入站暴露能力 |
| D7 | TLS 取舍 | 本期**仅 `ws://`**；`wss://` 在 `connect` 校验处显式拒绝并返回明确错误（提示未支持）；wss 单独立项（§9） | 实测 `Cargo.toml:123 tokio-tungstenite = "0.24"` 未启用任何 TLS feature（默认仅 connect/handshake），v1 声明允许 `wss://` 无实现基础；补 TLS 需引依赖 feature + 定信任源 + 双端/依赖影响评估（AGENTS 最小改动 + 依赖升级评估），超出本规格 |
| D8 | 服务端认证 wire | `auth: "none"`（默认）**跳过首消息认证状态机**（连接建立即可收发，`PLUGIN_WS_AUTH_TIMEOUT_SECS` 不适用）；`auth: "jwt"` 首消息固定为文本帧 `{"type":"auth","token":"<jwt>"}`，宿主只做「解析该 JSON → 取 `token` → `JwtService` 校验有效性/过期」，**不实例化 `message.rs` 的 `Message` 类型**（红线保持）；认证窗口上限 `PLUGIN_WS_AUTH_TIMEOUT_SECS`（默认对齐 `WS_AUTH_TIMEOUT_SECS=10`）；未认证期间的业务帧**丢弃 + `warn!`**（不缓存）；认证失败/超时 → close(4001) | v1 只写「复用既有首消息 JWT 校验原语」，会让实现者倾向复用终端 wire（`Message::Auth` 含 `payload.stage/device_id/session_token` 等配对语义），把插件端点绑死到终端协议演进上 |
| D9 | 过滤链与链路加密 | 新增 `TrafficChannel::WsPlugin`；**服务端域（入站端点）**的每一帧必经 `TrafficFilterChain`（inbound/outbound，AGENTS §8）；**不参与** `LinkEncryptionFilter`（不新增 `encrypt_ws_plugin` 开关），`should_process` 对 `WsPlugin` 恒 `false`；**客户端域（出站）本期不接链**（对端是第三方服务，无本端过滤通道语义；审计/加密出站流量需求出现时再评估） | 现状 `TrafficChannel` 只有 `Http/WsTerminal/WsEvent`（`filter.rs:64-71`），`traffic_channel()` 按连接类型映射（`terminal_ws.rs:217-222`）；链路加密是移动端配对设备专用协商协议，插件端点的第三方客户端与出站对端均不参与 |
| D10 | 发送队列语义 | 每连接有界发送队列；**入队成功即 `Ok`**（不代表已送达对端）；队列满 → 立即返回 `Err("ws send queue full")`（fail-visible，不静默丢弃）；`broadcast-*` 返回**成功入队**客户端数（部分失败不回滚、明细记 `debug!` + 计数）；跨插件调用失败不因他人错误而回滚本人 | v1「有界队列 + 满丢弃计数」未定义出口与返回值语义，插件无法区分送达/丢弃（AGENTS §6 禁止静默忽略错误） |
| D11 | close code 与 `wasClean` | 见 §4.5 码表；`wasClean = true` **仅当**对端主动发送 Close 帧且 code ∈ {1000, 1001}；宿主踢出（4004）、端点回收/属主停用（4005）、认证失败/超时（4001）、心跳超时、传输错误、服务器停机（1001 由宿主发起）→ `false` | v1 只有一个 `wasClean` 字段与「close-client 固定 1000」，插件无法区分「对端正常走」与「被宿主踢」 |

---

## 3. 总体设计

### 3.1 架构分层

```
┌───────────────────────────────────────────────────────────────┐
│            插件层（业务编排：协议/房间/重连/派生视图）             │
│  WIT host-websocket 原语（WASM 投影） ← events-ws 导出回调       │
│  订阅（activate 期）ws:*.<owner> 状态事件                        │
└──────────────────────────────┬────────────────────────────────┘
                               │ host_impl/ws.rs（权限门 + 属主仲裁 + 句柄表）
┌──────────────────────────────▼────────────────────────────────┐
│  WsService（内核单例，同 MdnsService 形态）                      │
│  · 通道注册表（开放 ChannelKind：Terminal / Event / Plugin）      │
│  · 端点表（{owner, path} → endpoint-id，路径含 owner 命名空间段） │
│  · 客户端连接表（client-id → {conn, owner, endpoint}）           │
│  · 广播/单发/踢出原语 · purge_for_plugin 按属主回收              │
│  · 状态事件 owner topic 投递（payload 带 handle/endpointId）      │
└──────────────────────────────┬────────────────────────────────┘
                               │ 通用连接骨架（握手/认证策略/心跳/帧泵/过滤链）
┌──────────────────────────────▼────────────────────────────────┐
│  actix 服务器（单端口 HTTP + WS，bootstrap 不变）               │
│  /ws/terminal/session/{id} · /ws/event                        │
│  · /ws/plugin/{plugin_id}/{path:.*}                           │
└───────────────────────────────────────────────────────────────┘
```

要点：

- **阶段 A**（基础服务改造）：从 `TerminalWs` 抽通用连接骨架，`ChannelType` 开放化，死代码清理——**不改 wire 协议、不动移动端**，纯结构性重构；
- **阶段 B**（host-websocket 原语）：WIT 接口 + 宿主实现 + SDK + fixture——**依赖阶段 A 的开放通道层**（服务端域）。

### 3.2 阶段 A：基础服务改造（结构性重构，先落地）

| # | 任务 | 说明 |
| --- | --- | --- |
| A1 | **通用连接骨架抽取**：`TerminalWs` 拆分为 `WsConnBase`（连接级状态机：握手 → 认证策略 → 心跳 → 帧泵 → 优雅关闭）+ **通道处理器 trait**（`ChannelHandler`：`on_frame` / `on_close` / `on_auth_ok` / `auth_mode`） | 消除 1857 行单文件混合；**行为零变化**。`auth_mode() -> AuthMode::{Required, None}` 是 B 阶段 `auth:"none"` 端点的地基（避免 B 再改骨架） |
| A2 | **注册表开放化（定案表示，不留分叉）** | `ChannelType` 拆为 `ChannelKind`（`Copy`：`Terminal`/`Event`/`Plugin`）+ 条目新增 `owner: Option<String>`、`endpoint_id: Option<String>` 两个字段。**不采用「给 `ChannelType` 加 String 变体」方案**——现状 `ChannelType` 是 `Copy`（`registry.rs:17`），加 String 会破坏 `Copy` 并波及 `terminal_ws.rs:218` 的 match 与测试 helper |
| A3 | **注册表能力扩展** | 按 `endpoint_id` 寻址、按 `owner` 过滤、`purge_for_plugin(plugin_id)` 批量回收、`disconnect_by_endpoint(endpoint_id, close_code)` |
| A4 | **死代码清理（含保留项，防误删）** | **删除**：`Message::Terminal`（`handle_terminal`/`handle_subscribe`/`handle_unsubscribe`）、`Message::SessionControl`（`handle_session_control`）、`handle_session_mode` 的 None 分支、`handle_session_input` 的 None 分支（`terminal_ws.rs:1013/1096-1102`），注明恢复方式。**前置检查**：确认 event 通道消息流不含 Terminal 类消息（scratchpad 2026-09-13 待办条件）。**必须保留**：`bound_session=None` 下的 `Message::Auth` 分支（`607` → `handle_auth:1203`）——事件通道首消息认证是活路径，删除会直接打断移动端事件通道 |
| A5 | **通用原语下沉** | `send_to_channel` / `broadcast` / `disconnect_client` 以通道为单位的通用 API 供 WsService 复用；终端语义（订阅/ack/会话控制）留在终端 handler。**回归断言**：既有 `broadcast_targets` 语义零变化——仅已认证 Event 通道 + `exclude_device_name` 排除 + 同 fingerprint 去重（`registry.rs:364-391`），插件通道不得被卷入 |

**改造波及点清单（A1/A2 必须一并处理，避免中途编译破坏面失控）**：`Addr<TerminalWs>` 出现于 `server/app.rs`（2 处构造）、`server/ws/registry.rs`（条目类型 + 571 行内的测试 helper `entry()`）、`server/ws/terminal_ws.rs`；`server/ws/websocket_manager.rs` 仅在注释中提及（无类型依赖）。

**阶段 A 验收**：`cargo test` 全绿（**现有 registry/terminal_ws 测试原样通过**即行为等价基线）、零 wire 协议变更、移动端零感知。

### 3.3 阶段 B：host-websocket 原语

#### B1. WIT（`bedcode-desktop/packages/plugin-sdk-desktop/rust/wit/bedcode.wit`）

```wit
/// WebSocket 基础能力服务（ADR 0022）：宿主 WS 传输原语，插件零业务语义。
///
/// 客户端域（出站）：连接外部 WS 服务器，连接句柄寻址。
/// 服务端域（入站）：在宿主 WS 服务器挂载插件端点；挂载路径由宿主按调用方
/// 插件注入命名空间段 —— `/ws/plugin/<plugin-id>/<path>`（D5）。
///
/// 属主隔离（§2.3 / D6）：**全部函数仅属主可调**（他人句柄/端点 → Err，
/// `not owner of ws handle`）；插件停用时宿主自动回收其全部句柄与端点。
/// 权限：客户端域需 `ws:client`，服务端域需 `ws:server`。
///
/// 事件投递（D2/D3）：状态事件经 host-bus 订阅，topic **内嵌 owner**
/// （非属主物理上订阅不到），标识在 payload：
///   客户端域  `ws:open.<owner>` / `ws:error.<owner>` / `ws:close.<owner>`
///   服务端域  `ws:client-connect.<owner>` / `ws:client-disconnect.<owner>`
/// 插件**须在 activate 期（或首次 connect/register-endpoint 之前）完成订阅**：
/// 宿主不缓冲、不重放，晚订阅期间的事件永久丢失；丢失自愈靠
/// `is-connected` / `list-clients` / `list-endpoints` 查询原语。
/// 消息帧（text + binary）经可选导出 `events-ws` 回调投递。
interface host-websocket {
    // ==================== 客户端域（出站连接） ====================
    /// 建立出站 WS 连接：**同步阻塞至握手完成**（上限 connect-timeout-secs，
    /// 与 host-http 非流式模式同款 block_on 语义，D4）。
    /// config-json（camelCase）纯引擎参数：
    /// `{ url, headers?, protocols?, connect-timeout-secs?, max-message-bytes? }`
    /// url **仅接受 `ws://`**（`wss://` 本期不支持，返回明确错误，D7）。
    /// 成功 → 返回连接句柄 `wsc-<uuid>` 并发布 `ws:open.<owner>`；
    /// 失败 → 错误上抛且**不发布任何事件**（无句柄可寻址）。
    connect: func(config-json: string) -> result<string, string>;
    /// 发送文本帧（UTF-8）；连接不存在/已关闭/发送队列满 → 错误（D10）
    send-text: func(handle: string, text: string) -> result<_, string>;
    /// 发送二进制帧；错误语义同 send-text
    send-binary: func(handle: string, payload: list<u8>) -> result<_, string>;
    /// 主动关闭连接（close-json：`{ code?, reason? }`，缺省 1000）；返回是否命中。
    /// 该连接的 `ws:close.<owner>` 随即上报（wasClean=true 当且仅当 code ∈ {1000,1001}）
    close: func(handle: string, close-json: string) -> result<bool, string>;
    /// 查询连接是否处于 open 态（握手完成且未关闭）；仅属主可查。
    /// 供插件在丢失状态事件后自愈（D3）
    is-connected: func(handle: string) -> result<bool, string>;

    // ==================== 服务端域（入站端点） ====================
    /// 在宿主 WS 服务器挂载插件端点（实际路径 `/ws/plugin/<plugin-id>/<path>`，
    /// 命名空间段由宿主注入，插件只提供后缀，D5）。
    /// config-json（camelCase）纯引擎参数：
    /// `{ path, auth?, max-message-bytes?, max-clients? }`
    /// auth = "none"（默认：跳过首消息认证状态机，插件自管认证）
    ///      | "jwt"（宿主校验首消息 `{"type":"auth","token":"<jwt>"}`，D8）。
    /// path 非法（空、含 `/`、含 `.`、超长）或与本插件已注册端点冲突 → 错误
    /// （跨插件不冲突：路径含 plugin-id 段）。返回端点句柄 `wse-<uuid>`。
    /// 端点存在期间的每一帧都经宿主流过滤器链（D9）
    register-endpoint: func(config-json: string) -> result<string, string>;
    /// 向端点的指定客户端发文本帧；客户端不存在/队列满 → 错误（D10）
    send-text-to-client: func(endpoint-id: string, client-id: string, text: string) -> result<_, string>;
    /// 向端点的指定客户端发二进制帧
    send-binary-to-client: func(endpoint-id: string, client-id: string, payload: list<u8>) -> result<_, string>;
    /// 向端点全部客户端广播文本帧 → 成功入队客户端数（部分失败不回滚，D10）
    broadcast-text: func(endpoint-id: string, text: string) -> result<u32, string>;
    /// 向端点全部客户端广播二进制帧 → 成功入队客户端数
    broadcast-binary: func(endpoint-id: string, payload: list<u8>) -> result<u32, string>;
    /// 主动踢出端点的单个客户端（close-json：`{ code?, reason? }`，缺省 4004）；
    /// 返回是否命中；随后该客户端经 `ws:client-disconnect.<owner>` 上报
    close-client: func(endpoint-id: string, client-id: string, close-json: string) -> result<bool, string>;
    /// 关闭端点并回收句柄（含下线全部客户端，close code 4005）；返回是否存在该端点
    unregister-endpoint: func(endpoint-id: string) -> result<bool, string>;
    /// 端点在线的客户端清单 → JSON 数组
    /// `[{ clientId, addr, authenticated, connectedAt }]`（camelCase）；
    /// 供插件在丢失 client-connect 事件后自愈（D3）；仅属主可查
    list-clients: func(endpoint-id: string) -> result<string, string>;
    /// 本插件已注册端点清单 → JSON 数组 `[{ endpointId, path, clientCount }]`；
    /// 仅属主可查
    list-endpoints: func() -> result<string, string>;
}

/// v14：消息帧接收（可选导出，**不进 `plugin` world 必选导出列表**——同 events-binary）
///
/// 宿主实例化后动态探测 `bedcode:plugin/events-ws#*`；未导出则：
/// 状态事件照收（bus），消息帧丢弃并首次 `warn!` + 计数（宿主不缓存）。
/// kind = "text" | "binary"，payload 统一列表（text 为 UTF-8 字节——
/// 零 JSON 转义、非 UTF-8 直通）。分片/压缩/opcode 已在宿主侧归一，
/// **同一连接内的帧按到达序投递**（保序，D2）。
/// 无返回值（观察型回调，同 terminal-hooks/events-binary）：插件处理失败经
/// host-log 记录；宿主仅 `error!` 记录 trap（含 endpoint-id/client-id 字段）
/// 并计数，不断开连接、不中断后续帧投递。
interface events-ws {
    /// 客户端域：handle 为连接句柄 `wsc-<uuid>`
    on-message: func(handle: string, kind: string, payload: list<u8>);
    /// 服务端域：endpoint-id 为端点句柄、client-id 为对端连接 id
    on-client-message: func(endpoint-id: string, client-id: string, kind: string, payload: list<u8>);
}

/// v14：events-ws 绑定专用 world（仅 SDK 绑定用，宿主不实例化它）——同 plugin-binary
world plugin-ws {
    export events-ws;
}
```

- `world plugin` 增 `import host-websocket;`——**旧插件（v13 及更早产物）不 import 本接口**，linker 注册惰性（`component.rs:426-430`：`linker 中无对应 import 的注册是惰性的`），加载零回归（host-mdns v2 新增即此路径验证过）；
- `abi.version()` → **v14**（`rust/src/abi.rs` `ABI_VERSION`，演进注释补 v14 段；`test_abi_version_is_v13` 测试同步更名）；
- **同时校正 WIT 版本表漂移**：`interface abi` 注释止于 v12 且把 v12 标为 host-mdns v2（`bedcode.wit:334-340`），而 `abi.rs:41-45` 是 v12=总线二进制、v13=host-mdns v2——补 v13 条目并纠正 v12 语义后再加 v14，否则 v14 建立在错位表上；
- 移动端 `bedcode.wit` **不动**（§7）。

#### B2. 宿主实现 `host_impl/ws.rs`（逻辑层，同 host_impl/mdns.rs 形态）

- **权限门**：11→14 个函数先 `check_permission(host_ctx, plugin_id, 域权限, ...)`（客户端域 `PERMISSION_WS_CLIENT`、服务端域 `PERMISSION_WS_SERVER`），再属主校验（`owner != entry.owner` → `not owner of ws handle`，且拒绝不消费句柄——同 `mdns.rs:91/210-227`）；
- **客户端域**：`LazyLock` 全局连接表 `handle → { task, owner, url, state }`（复用 `tokio-tungstenite 0.24`，已在依赖树）；`connect` 经 `block_on_async` 阻塞至握手完成（D4）；每连接独立消费任务 → 帧经 `events-ws` 回灌 / 状态事件发 bus；**不自动重连**（编排归插件，宿主只报 close 事件——D1）；**不启用 TLS**（D7，wss 直接 `Err`）；
- **服务端域**：**通配路由单点分发**——`app.rs::configure_routes` 增 `cfg.route("/ws/plugin/{plugin_id}/{path:.*}", web::get().to(plugin_ws))`（**不做 actix 动态加路由**，actix 4 路由构建期静态；`plugin_ws` 校验「插件已激活 + `{plugin_id}/{path}` 在端点表中」→ 否则 404）。`plugin_ws` 用通用连接骨架（A1，`AuthMode` 取自端点配置）+ 插件通道 handler（frame 经 `events-ws.on-client-message` / 状态事件经 bus）；
- **事件时序保证（D3，强制）**：① `client-connect` 的 bus 发布发生在该连接**首帧投递之前**；② `client-disconnect` 发布发生在句柄回收之前（发布后 `is-connected` 立即为 false）；③ `connect` 成功时 `ws:open` 发布先于返回值到达插件；④ 每连接 `ws:close` / `ws:client-disconnect` **恰好一次**（含宿主踢出、端点回收、插件停用、服务器停机路径）；
- **限流常量**（`system/constants/plugin.rs`，命名对齐 `PLUGIN_HTTP_*`）：`PLUGIN_WS_CONNECT_TIMEOUT_SECS`、`PLUGIN_WS_MAX_MESSAGE_BYTES`（默认对齐 `app.rs::ws_frame_limit()` 的取值来源，路由侧传 `frame_size`）、`PLUGIN_WS_MAX_CLIENTS_PER_ENDPOINT`、`PLUGIN_WS_MAX_CONNS_PER_PLUGIN`、`PLUGIN_WS_MAX_ENDPOINTS_PER_PLUGIN`、`PLUGIN_WS_AUTH_TIMEOUT_SECS`（默认 10，对齐 `WS_AUTH_TIMEOUT_SECS`）；超限行为见 §4.4；
- **回收**：插件停用（deactivate）→ `purge_for_plugin(plugin_id)` 回收该插件全部连接句柄与端点（**只碰本人**，host 登记不受影响）——同 mDNS 双表回收，调用点 `plugin/manager/host.rs:1343` 旁。

#### B3. 接线

- `component.rs`：`impl bedcode::plugin::host_websocket::Host for WasmPluginState`（14 函数转发 host_impl/ws.rs）+ `add_to_linker` 注册 `bedcode::plugin::host_websocket::add_to_linker::<WasmPluginState, D>`；
- 实例化后动态探测 `events-ws` 导出（同 events-binary 探测路径，`component.rs:583-598`；注意必须用 `iface.func` 点号 `ItemName` 语法）。

#### B4. 权限与同步点（5 个同步点，缺一不可）

- `packages/plugin-sdk-desktop/rust/src/permission.rs`：`PERMISSION_WS_CLIENT` / `PERMISSION_WS_SERVER` 常量（§9-42 区）+ 加入 `VALID_PERMISSIONS`（45-67）+ **`PERMISSION_API_MAP` 补条目**（70-127；`check_api` 未登记即返回 false，若插件在 manifest `api` 中声明 ws 相关 API，互调会被误拒）；
- `packages/plugin-sdk-desktop/bin/cli.js:411`：合法权限集合（插件打包/校验 CLI）；
- `bedcode-desktop/src/plugin/permission.ts:9`：前端合法权限集合（AGENTS §7 前端快速失败）；
- 宿主薄封装 `plugin/permission.rs:6`（re-export，跟随 SDK）。
- SDK 侧：`wasm_entry!` 宏无条件导出 `events-ws` 默认空实现（同 events-binary 机制）。

#### B5. SDK（Rust）

- `rust/src/host/ws.rs`：`HostWebsocket` trait（签名与 WIT 一一对应，同 `host/mdns.rs` 风格）；
- `rust/src/host/mod.rs` + `wasm_host.rs`：`fn ws_connect` / `ws_send_text` / … 包装（`host_err("ws_connect", e)` 模式）；
- **SDK 提示**：文档注释与示例必须写清「activate 期订阅 `ws:*.<owner>`」的时序要求（D3），并提供 `ws_event_topic(plugin_id)` 常量/助手生成 topic 串，避免插件手拼错 owner 导致收不到事件（漏订阅不会报错，只会静默丢事件）。

#### B6. 路由与过滤链（`app.rs` + `server/filter.rs` + `link_crypto.rs`）

- 路由：`/ws/plugin/{plugin_id}/{path:.*}`（B2）；
- `server/filter.rs`：`TrafficChannel` 增 `WsPlugin` 变体 + `as_str() -> "ws-plugin"`；
- **枚举扩展的穷尽匹配波及**：`link_crypto.rs:687-692`（`should_process` 的通道→开关映射，`WsPlugin` 恒 false，D9）与 `link_crypto.rs:844-857`（`on_inbound/on_outbound` 分支，`WsPlugin` 走 `on_ws_frame`）；
- 帧级过滤：**插件端点（服务端域）**的 inbound/outbound 均走 `TrafficFilterChain::global().run_*`（同 `terminal_ws.rs:280-348/540-548`）；客户端域本轮不入链（D9）。

#### B7. fixture 插件与测试

- 新 fixture **`bedcode-desktop/packages/plugin-ws-test`**（与既有 `packages/plugin-sdk-test` / `plugin-wasi-test` 同层；不并入既有 fixture，避免权限/依赖混杂）：`ws-client-echo` 命令（连宿主自建 echo 端点 / 外部 mock server，发文本+二进制，断言回文）+ `ws-endpoint-echo` 命令（注册端点、收 `on-client-message`、broadcast 回显、`list-clients` 快照断言）；
- 宿主侧单测：句柄表/属主仲裁（14 函数跨插件拒绝）/路径非法与同插件冲突/purge（连接 + 端点双表）/连接数与端点数上限/事件时序（connect 先于首帧、disconnect 先于回收）/ `is-connected` 与 `list-clients` 快照一致性；
- 集成测试：mock WS server（测试后清理进程，AGENTS §3）+ 端点闭环 + 认证两种模式（none / jwt）。

#### B8. 文档

- `abi.rs` 演进注释（已并入 B1）、WIT `interface abi` 版本表校正（B1）；
- `bedcode-desktop/docs/code-map.md`（host_impl 域列表 + ws 服务层）；
- ADR 0022 追加 host-websocket 裁决（D1/D5/D6/D9）；
- `CONTEXT.md` 术语（如需）；
- 本 spec 状态翻转。

---

## 4. 数据/协议细节

### 4.1 状态事件（bus owner 作用域 topic，宿主零加工，camelCase）

| topic | payload |
| --- | --- |
| `ws:open.<owner>` | `{ handle, url, protocol? }` |
| `ws:error.<owner>` | `{ handle, message }` |
| `ws:close.<owner>` | `{ handle, code?, reason?, wasClean }` |
| `ws:client-connect.<owner>` | `{ endpointId, clientId, addr, authenticated }` |
| `ws:client-disconnect.<owner>` | `{ endpointId, clientId, code?, reason?, wasClean }` |

- `<owner>` = 调用方 `plugin_id`（与 `mdns:found.<owner>` 同形）；**topic 不含 handle/endpoint-id**，多连接事件在同一 topic 上由 payload 区分；
- 非属主插件因 topic 精确匹配而**物理上订阅不到**（继承 mDNS v2 隔离机制）；
- **订阅时序（D3，硬约束）**：插件须在 `activate`（或首次 `connect`/`register-endpoint` 之前）订阅；宿主不缓冲、不重放。SDK 需在文档与示例中显式提示，并提供 topic 生成助手（B5）；
- **投递顺序与次数（B2 时序保证）**：`client-connect` 先于该连接首个 `on-client-message`；`client-disconnect` 先于句柄回收；每连接 disconnect/close 恰好一次；
- **宿主主动断开也发事件**：`close-client`(4004) / `unregister-endpoint`(4005) / `purge_for_plugin`(4005) / 服务器停机(1001) 均发 `ws:close` / `ws:client-disconnect`，`wasClean=false`（D11）；
- **丢失自愈**：`is-connected(handle)` / `list-clients(endpoint-id)` / `list-endpoints()` 提供快照，插件对账后可恢复状态机（宿主不做重放）。

### 4.2 句柄与命名空间

- 客户端连接句柄 `wsc-<uuid>`；端点句柄 `wse-<uuid>`；对端 client-id 由宿主分配 `wsc-<uuid>`（与客户端域句柄同格式，但登记在端点名下，语义不同——`events-ws` 两回调签名已区分）；
- 端点路径：插件提供后缀（如 `chat`），宿主注入 owner 段 → 实际挂载 `/ws/plugin/com.bedcode.chat/chat`（D5），与 `/ws/terminal/...`、`/ws/event` 不冲突，插件之间不冲突；
- `path` 校验：非空、不含 `/`、不含 `.`（防 `..` 与相对段）、长度上限（常量）；同插件内冲突 → Err。

### 4.3 认证

- **客户端域**：headers/protocols 由插件传入（如 `Authorization` 头）；宿主不做业务认证（D1）；
- **服务端域**（D8）：`auth: "none"`（默认）跳过首消息认证状态机；`auth: "jwt"` 首消息固定文本帧 `{"type":"auth","token":"<jwt>"}`，宿主只解析该 JSON 取 `token` 并调 `JwtService`（`utils/auth/jwt.rs`）校验有效性与过期；**不实例化 `message.rs` 的 `Message` 类型**；认证窗口 `PLUGIN_WS_AUTH_TIMEOUT_SECS`（默认 10）内未认证 → close(4001) 并停止 actor；未认证期间的业务帧丢弃 + `warn!`（不缓存）；
- 认证结果反映在 `ws:client-connect.<owner>.authenticated`；`auth: "none"` 时该字段恒 `false`（语义 = 「宿主未做认证」，插件自管）。

### 4.4 限流、背压与返回值语义（D10）

- 帧大小上限：`PLUGIN_WS_MAX_MESSAGE_BYTES`（取值来源对齐 `app.rs::ws_frame_limit()`，握手时 `frame_size` 生效）；端点/连接可配置但**上限截断为常量**；
- 超限拒绝方式：
  - **入站连接数超限**（服务端域，`PLUGIN_WS_MAX_CLIENTS_PER_ENDPOINT`）：**升级前**返回 HTTP `503 Service Unavailable`（不进入 actor，不产生 connect 事件）；
  - **出站连接数超限**（客户端域，`PLUGIN_WS_MAX_CONNS_PER_PLUGIN`）：`connect` 返回 `Err`（不发 `ws:open`/`ws:error`，D4）；
  - **端点数超限**（`PLUGIN_WS_MAX_ENDPOINTS_PER_PLUGIN`）：`register-endpoint` 返回 `Err`，不产生副作用；
- 发送队列：每连接有界；入队成功即 `Ok`（**不代表已送达对端**）；队列满 → `Err("ws send queue full")`（fail-visible）；`broadcast-*` 返回成功入队数，失败明细 `debug!` + 计数；
- 慢消费者：队列满即报错，宿主不做无界缓冲、不做背压等待（D1）；
- 心跳：沿用骨架 `HEARTBEAT_INTERVAL_SECS=5` ping / `REMOTE_CLIENT_TIMEOUT_SECS=45` 超时（浏览器/标准库自动回 pong 即可满足）。

### 4.5 close code 与 `wasClean`（D11）

| 场景 | code | `wasClean` |
| --- | --- | --- |
| 对端主动关闭（正常/going away） | 对端 code（1000/1001） | `true` |
| 对端异常断开（TCP 断、心跳超时、传输错误） | 无 / 1006 语义（payload `code` 省略） | `false` |
| 认证失败/超时（`auth:"jwt"`） | 4001 | `false` |
| 链路加密失败（既有终端通道语义，插件端点不适用） | 4003 | `false` |
| 宿主踢出（`close-client`，可配置） | 4004（缺省） | `false` |
| 端点注销 / 插件停用回收（`unregister-endpoint` / purge） | 4005 | `false` |
| 宿主服务器停机 | 1001 | `false`（宿主发起） |

### 4.6 过滤链与链路加密（D9）

- **服务端域（入站端点）**：每一帧 **inbound/outbound 均过 `TrafficFilterChain`**（AGENTS §8 安全边界；WS 中间件层已对 `/ws` 前缀快速透传，帧级过滤在 actor 层执行，见 `http_filter.rs:79-82` 与 `terminal_ws.rs:280-348/540-548`）；
- 新增 `TrafficChannel::WsPlugin`（`filter.rs`）；`LinkEncryptionFilter.should_process` 对 `WsPlugin` 恒 `false`，`on_inbound/on_outbound` 走 `on_ws_frame` 的透传分支——链路加密是移动端配对设备专用协商协议，插件端点的第三方客户端不参与；
- **客户端域（出站）本期不接链**：出站对端是第三方 WS 服务，没有本端「接入点过滤」语义；如后续需要出站审计/加密，单独立项（可能新增 `TrafficChannel::WsClientOut` 与方向语义）；
- spec 不额外引入业务过滤；链非空时按链实现逐帧执行。

---

## 5. 分阶段任务清单

**阶段 A（基础服务改造，先行）**——全部收口于票 02/03（票 01 见下）

- [x] A1 通用连接骨架抽取（`WsConnBase` + `ChannelHandler` trait（含 `auth_mode`），Terminal/Event 两实现）→ 票 02
- [x] A2 注册表开放化（`ChannelKind`(Copy) + `owner` + `endpoint_id` 字段）→ 票 03
- [x] A3 注册表能力扩展（按 endpoint/owner 寻址 + `purge_for_plugin` + `disconnect_by_endpoint`）→ 票 03
- [x] A4 死代码清理 —— **前置检查否决后关闭**：`Message::Terminal`（Input）与 `Message::SessionControl`（ListSessions）是移动端活路径（TUI 滚动输入 / 插件 `session.list`，经常驻 `/ws/event` 通道实发），删除违反「行为零变化」；`Message::Auth` 分支保持不变（票 01 Comments 有完整证据链）
- [x] A5 通用收发/广播原语下沉（既有 `broadcast_targets` 语义零变化断言）→ 票 02/03
- [x] A-门禁：cargo test 全绿（现有测试原样通过 = 行为等价基线）、wire 协议零变更、移动端零感知（真机连通性见 §6，待用户补做）

**阶段 B（host-websocket 原语，依赖 A）**——收口于票 04–07

- [x] B1 WIT：`host-websocket`（14 函数 + owner 隔离文档）+ `world plugin` import + `events-ws` 可选导出 + `plugin-ws` world + ABI v14（`abi.rs` 注释 + 测试更名 + **WIT `interface abi` 版本表校正**：v11–v14 逐条对齐，见 `bedcode.wit:418-432`）
- [x] B2 `host_impl/ws.rs` 逻辑层（域权限门 + 属主仲裁 + 句柄/端点表 + owner topic 投递 + 时序保证 + 回收）
- [x] B3 `component.rs` 接线（Host impl + add_to_linker + events-ws 动态探测）
- [x] B4 权限：`ws:client` / `ws:server`（SDK 常量 + `VALID_PERMISSIONS` + `PERMISSION_API_MAP` + `cli.js` + 前端 `permission.ts`，5 处同步）
- [x] B5 SDK Rust（`host/ws.rs` trait + `wasm_host.rs` 包装 + `wasm_entry!` 默认导出 events-ws + topic 生成助手 + 订阅时序文档）
- [x] B6 `app.rs` 通配路由 `/ws/plugin/{plugin_id}/{path:.*}` + `plugin_ws` handler；`TrafficChannel::WsPlugin` 及其穷尽匹配波及（`link_crypto.rs`）
- [x] B7 fixture `packages/plugin-ws-test` 闭环 + 宿主单测 + 集成测试（含事件时序/隔离负向/降级）——遗留：`auth:"jwt"` 成功分支端到端断言（票 05 Comments）
- [x] B8 文档（code-map / ADR 0022 追加 / CONTEXT 术语 / 本 spec 状态翻转）

**收口证据**：见 `issues/05-ws-server-domain-loop.md`、`06-isolation-and-timing-contract.md` 的 Comments（含命令与结果；全量 `cargo test` = lib 914 passed / 0 failed + 集成全绿，2026-09-19 连跑 2 轮无 flaky）。

---

## 6. 验证（完成定义）

| 项 | 命令/方式 | 门槛 |
| --- | --- | --- |
| 阶段 A 行为等价 | `cd bedcode-desktop/src-tauri && cargo test` | 全绿，且**既有 registry/terminal_ws 测试未改动即通过** |
| 桌面 lib/集成测试 | `cd bedcode-desktop/src-tauri && cargo test` | 全绿（含新增 ws 单测与 fixture 闭环） |
| 事件时序 | 单测：订阅后 connect/register 的 open/connect 事件；未订阅期事件按 D3 语义**丢失**（作为契约断言）；connect 先于首帧、disconnect 先于回收 | 断言通过（契约档） |
| 事件自愈 | 单测：丢失 close 事件后 `is-connected` 返回 false；丢失 connect 后 `list-clients` 快照与真实一致 | 断言通过 |
| 跨插件隔离负向 | 单测：插件 B 对插件 A 的 handle/endpoint/client-id 调用 14 函数全部 `Err`；B 订阅 A 的 topic 收不到任何投递 | 断言通过 |
| 降级路径 | 单测：fixture 不导出 `events-ws` → 状态事件照收、消息帧丢弃 + `warn!` 一次 + 计数；`wss://` 返回明确错误 | 断言通过 |
| 服务端认证 | 单测：`auth:"none"` 无首消息即可收帧；`auth:"jwt"` 正确/错误/超时三条路径（4001） | 断言通过 |
| 上限与拒绝 | 单测：连接数/端点数超限 → 503 / `Err`，无副作用 | 断言通过 |
| 过滤链 | 单测：`TrafficChannel::WsPlugin` 在链非空时 inbound/outbound 均被调用；`LinkEncryptionFilter` 跳过 | 断言通过 |
| SDK | `cd bedcode-desktop/packages/plugin-sdk-desktop/rust && cargo test` + `cargo check --target wasm32-unknown-unknown --features wasm` | 全绿 |
| 插件回归 | 既有插件（file-transfer 等）实例化/激活测试 | 零回归（host-websocket import 惰性、events-ws 探测不命中不影响） |
| 前端 | `cd bedcode-desktop && pnpm run test:run`（抽跑确认无连带）+ 根目录 `pnpm exec eslint .` | 全绿 / 0 error |
| 文档一致性 | ABI 演进注释 / WIT `interface abi` 版本表 / code-map / ADR / CONTEXT 术语 | 一致 |
| 真机连通性（阶段 A 后必做） | 桌面 `pnpm run tauri:dev` + 移动端真机连 `/ws/event` 与 `/ws/terminal/session/{id}` | 连接、认证、终端收发正常（留证：操作记录/截图） |
| 收尾 | `lens_diagnostics mode=all` | 无 blocker |
| 测试后清理 | 检查并关闭 mock server / 残留进程 | 无残留 |

## 7. 双端评估（移动端）

- **移动端不动**（用户指令）：`bedcode-mobile/packages/plugin-sdk-mobile/rust/wit/bedcode.wit` 不增 host-websocket，移动端 ABI（v11 线）不 bump；
- **实际影响面（v2 勘误）**：两端插件是**各自 SDK 构建的独立工程**（`bedcode-desktop/plugins/file-transfer/rust/Cargo.toml:17` 用 `plugin-sdk-desktop`；`bedcode-mobile/plugins/file-transfer/rust/Cargo.toml:16` 用 `plugin-sdk-mobile`），**不存在「共享插件产物」**——v1 的「移动端可继续加载共享产物」前提不成立（且桌面 ABI 13 / 移动 11 本就不互通）。真实影响面只有：① 桌面 WIT 增量（新增 import 接口，非既有接口语义变更）；② 桌面 ABI 13→14；③ 桌面插件需重编译方可使用新能力（旧产物零回归）；
- **契约独立依据**（ADR 0018）：两端各持独立 bedcode.wit，契约随产品能力分化各自演进，同名词义保持对齐——本次为桌面端新增能力，不违反「同名词义对齐」（移动端无同名接口）；
- **AGENTS.md §7「改 WIT 必须双端同步」与 §9「协议改动两端同步部署」的偏离**：本规格的 WIT 变更是桌面宿主新能力且 **wire 协议零改动**（`message.rs` 不动、既有 WS 路由不动），移动端不存在需要同步的契约面；文档化此偏离（同 wasmtime-48 分叉先例，见 `.scratch/2026-09-18-wasmtime-48-upgrade/spec.md`）；
- **移动端后续**：若移动端插件出现 WS 原语需求，按 ADR 0018 独立评估，复用本规格契约形状（同名词义对齐），独立立项。

## 8. 风险与回退

| 风险 | 控制 |
| --- | --- |
| 阶段 A 结构重构引入行为回归（终端/事件链路是移动端依赖的线上协议） | A 门禁：wire 协议零变更、cargo test 全绿（既有测试原样通过）、真机移动端连通性确认（§6 必做项） |
| `ChannelType` → `ChannelKind` + 字段改造波及大 | A2 已定案表示（避免加 String 破坏 `Copy`）；§3.2 已列波及点清单（app.rs/registry.rs/terminal_ws.rs） |
| actix 4 路由构建期静态、无法动态加路由 | 已定案：通配路由 `/ws/plugin/{plugin_id}/{path:.*}` 单点分发，端点表查 registry——不依赖动态路由 |
| 状态事件丢失（插件晚订阅 / 事件早于句柄返回） | D3：owner topic 可 activate 期订阅 + 查询原语自愈 + SDK 文档硬提示；测试以「契约档」锁定语义，不留隐性假设 |
| 慢客户端阻塞广播 | 有界发送队列 + 队列满即 `Err`（D10），宿主不背压、不静默丢弃 |
| 客户端域新增引擎复杂度（每连接消费任务） | 复用 `tokio-tungstenite 0.24`（已在依赖树）；连接数上限常量约束 |
| `wss://` 缺失导致插件无法连外部加密端点 | D7 显式拒绝 + 错误文案说明；真实需求出现时按 §9 单独立项（评估 TLS feature、信任源、依赖影响） |
| 事件面双通道（bus 状态 + events-ws 消息）增加插件理解成本 | SDK 提供 topic 生成助手与注释模板（B5）；文档化；TS 插件消费方出现时补 `runtime.ts` 包装（§9） |
| 插件停用时对端客户端被强制下线 | 语义明确（4005 + `client-disconnect`），插件可据此做业务收尾；不做延时回收（D1） |
| 回退 | 阶段 A/B 各自独立可回退：A 为纯重构（回退 = revert 结构拆分），B 为增量（回退 = 移除 import + 路由行 + `TrafficChannel::WsPlugin`，ABI 回 v13） |

## 9. 后续（不在本期）

- **`wss://`（TLS）**：评估 `tokio-tungstenite` 的 rustls feature、证书信任源（webpki-roots vs 自签策略）、依赖影响与两端一致性（ADR 0019 精神）；
- 移动端 host-websocket 独立评估（§7）；
- TS SDK 包装（`runtime.ts`，含 `ws:*.<owner>` topic 助手与订阅时序提示）与 devMock（若出现 TS 插件消费方）；
- 客户端域「重连/心跳业务协议」helper 上插件 SDK 层（编排能力，非宿主职责）；
- 若出现「同一插件需要海量连接」的诉求，再评估会话快照/批量事件聚合（当前 owner topic + payload 区分已足够）。

## 10. 参考

- `docs/adr/0022-plugin-host-interface-primitive-boundary.md`（裁剪线 + host-mdns v2 裁决）
- `docs/adr/0018-mobile-plugin-contract-independent.md`（移动端契约独立）
- `.scratch/2026-09-10-mdns-service-plugin/spec-basic-capability-service.md`（同构模板：单守护/事件定向投递/双表回收/零业务红线）
- `bedcode-desktop/packages/plugin-sdk-desktop/rust/wit/bedcode.wit`（host-mdns / host-http / events-binary / plugin-binary 形态）
- `bedcode-desktop/src-tauri/src/plugin/manager/wasm_runtime/host_impl/mdns.rs`（权限门 + 属主校验 + 双表 purge + 定向投递实现形态）
- `bedcode-desktop/src-tauri/src/plugin/bus.rs`（精确 topic 分发、无重放——D3 依据）
- `bedcode-desktop/src-tauri/src/server/filter.rs` + `server/middleware/http_filter.rs` + `server/link_crypto.rs`（过滤链与链路加密通道，D9 依据）
- `bedcode-desktop/src-tauri/src/server/ws/*.rs` + `server/app.rs`（现状接线）
- scratchpad 2026-09-13（terminal_ws.rs 死代码待办）
