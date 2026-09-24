# WebSocket 业务下沉与宿主通用传输面收敛

Status: **done**（2026-09-25；P0–P5 全部落地——connection-context/通用连接核心为票 02，插件端点与终端数据面为票 03/04，事件与连接事实迁移为票 05/06/07，宿主硬切为票 08，见 `.scratch/.../issues/` 与 handoff 执行表；终态全量门禁与文档复核留待票 09）
Date: 2026-09-25
范围: **仅桌面端**（`bedcode-desktop/`）；`bedcode-mobile/` 不修改、不测试、不承诺兼容
决策依据: 用户 2026-09-25 当前指令（宿主剥离业务代码；不提供移动端兼容；不兼容旧版本）、AGENTS §5（无业务内核）、ADR 0022（宿主只暴露“离宿主无法实现且无业务语义”的原语）、`.scratch/2026-09-18-ws-base-service/spec.md`、`.scratch/2026-09-23-session-engine-downsink/spec.md`
承接: `host-websocket` 通用原语、`com.bedcode.terminal-session` 会话控制端点、`host-pty.ring-fetch` 插件输出拉取链路均已存在；本专项收敛宿主 `server/websocket` 残留

> 本 spec 是**硬切方案**，不是 expand–contract 迁移方案。实施期间可以分阶段保持分支可编译，但任何可交付版本都不得同时保留旧 WS 业务协议或旧路由作为兼容轨。

---

## 0. 用户裁决与硬约束

1. WebSocket 宿主面只提供通用传输能力，不提供会话、终端、设备、同步任务等产品类型接口。
2. 所有 WS 业务编排、协议解析、会话状态、终端输入输出、设备在线/连接历史、刷新通知和重连策略归插件。
3. **不提供移动端兼容**：移动端现有 WS 客户端、路由、wire 形状不纳入本专项，不做双端同步部署，不保留旧端可达路径。
4. **不兼容旧版本**：允许删除旧 interface、字段、路由、消息枚举和旧插件产物；不做迁移窗口、不做双读、不做 fallback、不做旧 wire 适配器。
5. 宿主仍必须保留 WASM 无法实现或安全边界必须集中裁决的能力：监听/握手/帧编解码、连接注册表、权限、属主隔离、JWT 验证与认证策略、帧过滤、限流、优雅关闭、PTY 引擎。
6. 插件停用、端点注销、服务器停机和连接断开必须显式清理；不得以“旧连接仍能工作”作为兼容兜底。

### 0.1 工作区基线提醒

当前工作区存在其他任务未提交改动，且 `wasm_core/host_api/ws.rs` 等文件存在在途签名重构。开工前必须先检查 `git status` 与相关 diff；本专项不得用整文件回滚覆盖在途改动。spec 本身不修改任何现有源码。

---

## 1. Problem Statement：宿主 WS 仍承载产品业务

### 1.1 当前宿主面

| 位置 | 当前职责 | 业务耦合证据 | 目标 |
| --- | --- | --- | --- |
| `server/websocket/conn.rs:182-209,480-554,633-727` | 连接 actor、JWT、设备身份、设备在线/离线、认证记录、会话订阅、链路状态 | 同时 import `AppContext`、`JwtService`、认证中心、设备事件、会话订阅 | 只保留连接生命周期、认证策略、过滤、限流、关闭 |
| `server/websocket/message.rs:34-163` | `Terminal / Auth / SessionControl / SyncData` 等统一业务消息 | 直接依赖 `SessionControlAction`、`TerminalPayload`、`AuthPayload`、`SyncPayload` | 删除宿主业务消息枚举 |
| `server/websocket/channel/event.rs:34-338` | 旧移动端事件协议、JWT、终端输入、会话控制 | 业务消息分派和 `session_gateway` 转发 | 删除，插件端点接管 |
| `server/websocket/channel/terminal.rs:43-302` | 终端控制帧、会话存在性、PTY 输入、输出订阅 | 直接调用 `broadcast_handle_for_session` 与 terminal service | 删除，插件经 `host-pty` + `host-websocket` 自持 |
| `server/websocket/services/session_control.rs:27-181` | 硬编码 `com.bedcode.terminal-session`、端点路径、互调 API、会话回包信封、前端刷新 | 即使动作解释已在插件，宿主仍知道具体插件和会话产品 | 删除具体业务转发，插件直接处理帧 |
| `server/websocket/services/terminal_service.rs:1-53` | 终端输入解析、按键/特殊键、会话网关调用 | 宿主直接解释终端业务 | 删除，插件处理 |
| `server/websocket/registry.rs:19-27,222-248,586-625` | 通用端点注册与 `Event/Terminal` 通道、设备去重、在线判定混在一起 | `ChannelKind`、fingerprint、device name、Event broadcast | 拆为纯连接/端点注册表 |
| `server/websocket/websocket_manager.rs:9,301-365` | 服务器生命周期与 `BusinessMessage` 发送 API 混合 | 直接 import `Message as BusinessMessage` | 拆为服务器运行时与通用连接目录 |
| `server/websocket/connection_types.rs:7-26`、`server.rs:13` | `DeviceConnectionEvent`、`DeviceConnectionInfo(session_count)` | 设备派生视图带 `session_count` | 删除产品 DTO，改为通用连接事实 |
| `server/websocket/subscription.rs:1-25,108-133,266-743` | 终端输出订阅、TB v3、ACK、会话停止帧、PTY 环 | 传输引擎与会话协议耦合 | 删除，由插件自持游标和协议 |
| `server/websocket/terminal_ws/**` | 终端控制帧、合帧策略、订阅者执行体 | `WatchMode`、`SessionStopped`、TB v3、session_id | 删除，协议归插件 |
| `host_api/pty.rs:22-30,168-173,194-248,592` | `hostBroadcastSessionId` 会话映射供宿主 WS 直读 | PTY 原语知道“宿主广播会话”产品概念 | 删除映射，插件直接持有 pty_id |
| `host_api/events.rs:26-49`、`events/host_sync_event.rs`、`events/sync_handler.rs` | `broadcast-sync`、会话/任务同步载荷、WS 广播 | 宿主持有产品事件类型与广播策略 | 迁插件事件/bus，删除宿主同步桥 |

### 1.2 已有可复用的正确基础

- `host-websocket` WIT（`packages/plugin-sdk-desktop/rust/wit/bedcode.wit:294-365`）已经是通用接口：连接、发送、关闭、端点注册、单发、广播、踢出、清单。
- `server/websocket/channel/plugin.rs:1-25,184-304` 已示范正确形态：只执行 `none/jwt` 认证策略，转发原始 text/binary 帧，发布属主生命周期事件，不解释插件业务。
- `com.bedcode.terminal-session` 已有 `session-control` 声明端点、`ws_control.rs` 动作分派和 `events-ws` 回调（`plugins/terminal-session/rust/src/ws_control.rs:178-208`）。
- 插件已有 `host-pty.ring-fetch` 拉取链路（`plugins/terminal-session/rust/src/output.rs:24-63`），可替代宿主 `server/websocket/subscription.rs` 的业务输出环。

### 1.3 终态差异

当前是：

```
Actix server
  ├── /ws/event                 → 宿主解析会话/终端/认证业务
  ├── /ws/terminal/session/{id} → 宿主解析终端协议并直读会话 PTY
  └── /ws/plugin/{id}/{path}    → 通用插件端点
```

终态是：

```
Actix server
  └── /ws/plugin/{plugin-id}/{path}
        └── 原始 text/binary 帧 → 属主插件

host-websocket
  ├── handshake / auth policy / frame filter / limit
  ├── connection registry / endpoint registry
  ├── send / broadcast / close / purge
  └── connection context（仅安全/连接事实）

terminal-session plugin
  ├── session-control endpoint
  ├── terminal-stream endpoint
  ├── 会话/终端/设备/同步业务协议
  ├── host-pty 输入与 ring-fetch 输出
  ├── ACK、游标、合帧、重连/批量策略
  └── host-bus / host-events 业务事件
```

---

## 2. 目标、非目标与不可违反的边界

### 2.1 目标

1. `server/websocket/**` 只剩通用 WS server/connection/endpoint/plugin-channel 能力。
2. 宿主不再 import 或暴露 `Message`、`SessionControlAction`、`TerminalAction`、`SyncPayload`、`DeviceConnectionEvent`、`SessionStopped`、`session_id` 等产品概念。
3. 插件端点成为唯一业务 WS 入口；宿主不硬编码任何插件 ID、端点路径或业务 API 名称。
4. 终端输入、输出、历史/回放、ACK、截断重同步、会话停止通知全部由插件协议定义。
5. 设备在线/离线、配对连接历史 touch/close、会话刷新、任务/会话同步通知全部由插件处理。
6. 旧 `/ws/event`、`/ws/terminal/session/{id}`、旧 `Message` 和旧 ABI 直接退役。
7. 宿主 WS 安全边界不削弱：认证、权限、属主隔离、过滤链、限流、关闭码、事件顺序保持 fail-visible。

### 2.2 非目标

- 不实现 `wss://`；若未来需要 TLS，另立 spec 并评估 `tokio-tungstenite` 依赖与信任模型。
- 不把 PTY 进程实现移入 WASM；PTY 仍是宿主引擎原语。
- 不为第三方旧客户端保留 wire 兼容层。
- 不在本专项内设计移动端新协议；移动端未来若需要该能力，另立项目并重新评估双端契约。
- 不把所有低层网络实现都抽象成泛型 trait；只收敛当前业务耦合面，避免无消费者的“万能 WS API”。

### 2.3 宿主允许保留的内容

| 宿主内容 | 理由 |
| --- | --- |
| Actix listener、HTTP/WS 单端口组合、静态通配路由 | WASM 无法 bind 宿主监听端口 |
| WebSocket 握手、心跳、帧类型、frame/message size limit | 传输引擎职责 |
| JWT 验签、认证策略、权限、属主隔离 | 安全边界，必须集中裁决 |
| `TrafficFilterChain`、链路加密过滤器、审计 | 安全/传输边界 |
| connection registry、endpoint registry、owner purge | 引擎事实 |
| text/binary 原始帧收发、有界发送队列 | 通用传输原语 |
| `host-pty` 的 spawn/write/resize/kill/ring-fetch/is-running | WASM 无 PTY 能力 |
| 插件停用/服务器停机时的资源回收 | 生命周期与安全清理 |

### 2.4 宿主禁止出现的内容

- 终端/会话/设备/任务/同步业务枚举、DTO 或解析器。
- 对插件端点路径、插件 ID、业务 API 名称的硬编码。
- 根据 payload 字段决定动作、刷新、广播目标或会话状态。
- 宿主 `PtyRing` 直读时以 `session_id` 建立业务映射。
- 为旧移动端、旧插件或旧 wire 保留 fallback。

---

## 3. 目标接口设计

### 3.1 `host-websocket` 公共接口

现有客户端域与服务端域原语保留：

- 客户端：`connect`、`send-text`、`send-binary`、`close`、`is-connected`。
- 服务端：`register-endpoint`、`send-text-to-client`、`send-binary-to-client`、`broadcast-text`、`broadcast-binary`、`close-client`、`unregister-endpoint`、`list-clients`、`list-endpoints`。
- 事件：`open/error/close`、`client-connect/client-disconnect` 属主私有 topic；帧通过 `events-ws` 回调。

新增一个通用连接上下文查询原语（WIT 形状）：

```wit
connection-context: func(endpoint-id: string, client-id: string) -> result<string, string>;
```

返回值只包含连接/认证事实，不包含会话或插件业务派生字段：

```json
{
  "clientId": "c-123",
  "endpointId": "wse-456",
  "owner": "com.example.plugin",
  "addr": "192.168.1.10:54321",
  "authenticated": true,
  "connectedAt": 1730000000000,
  "authContext": {
    "subject": "device-or-client-subject",
    "deviceName": "optional-name",
    "fingerprint": "optional-fingerprint"
  }
}
```

约束：

- `connection-context` 仅端点属主可调用，权限仍为 `ws:server`。
- 永不返回 JWT、session token、公钥、私钥或配对记录。
- `auth: none` 时 `authenticated=false`，`authContext` 省略或字段为空。
- `connection-context` 是安全上下文，不是设备列表、会话列表或在线判定接口。
- 该新增函数及本专项的破坏性删除统一进入 desktop ABI **v28**；旧产物不迁移、不兼容，实例化期显式提示按 v28 重建。

### 3.2 宿主内部通用连接模型

连接条目只保留：

```text
ConnectionRecord {
  connection_id,
  endpoint_id,
  owner,
  peer_addr,
  authenticated,
  connected_at,
  auth_context
}
```

删除或禁止：

```text
ChannelKind::{Terminal, Event}
bound_session
subscribed_sessions
session_id
device_online
device_name 作为广播选择条件
fingerprint 去重策略
```

`WsSessionRegistry` 只提供：

- 注册/注销连接。
- 按 `connection_id`、`endpoint_id`、`owner` 查询。
- 发送文本/二进制帧。
- 端点范围广播。
- 关闭单个连接/端点/属主资源。
- 返回原始连接事实。
- 插件停用时回收本人全部连接。

所有“发给谁、显示什么、是否在线、是否刷新”的选择由插件或调用方完成。

### 3.3 插件端点协议

宿主不定义插件 payload 的 JSON schema；插件端点收到的 text/binary 帧对宿主是不透明字节。

`com.bedcode.terminal-session` 声明两个业务端点（路径由插件 manifest 决定，宿主不硬编码）：

```json
{
  "wsEndpoints": [
    { "path": "session-control", "auth": "jwt" },
    { "path": "terminal", "auth": "jwt" }
  ]
}
```

#### `session-control`

- 文本帧是插件自定义动作 JSON。
- 插件直接返回响应 JSON，不再套宿主 `Message::SessionControl` 信封。
- 请求关联、错误形状、刷新通知和动作词表全部归 `ws_control.rs`。
- 宿主只负责：JWT 首帧认证、帧过滤、按属主投递、连接生命周期。

#### `terminal`

- 文本帧承载插件定义的输入、订阅、ACK/控制命令。
- 二进制帧承载插件定义的输出、ACK 或协议数据。
- 会话 id、游标、合帧窗口、实时/批量模式、截断重同步、`session_stopped` 全部由插件维护。
- 宿主不解析二进制帧，不读取 `session_id`，不决定输出属于哪个会话。

### 3.4 认证与连接事件

1. 插件端点使用 `auth: "jwt"` 时，宿主只接受固定安全握手帧：

   ```json
   {"type":"auth","token":"<jwt>"}
   ```

   这不是业务 `Message`，而是宿主安全边界的极小认证契约。
2. 宿主负责验签、过期校验、认证策略和超时；认证失败/超时 close 4001。
3. 宿主在 `client-connect` 事件中只发布连接标识、地址、认证结果；插件需要设备身份时调用 `connection-context`。
4. 插件在 `client-connect` / `client-disconnect` 事件中自行：
   - 更新认证记录 touch/close；
   - 维护设备在线派生视图；
   - 维护会话与连接的关联；
   - 发出业务事件。
5. 宿主删除 `conn.rs` 中对 `notify_connection_touch`、`notify_connection_close`、设备连接事件的直接调用。

---

## 4. 插件侧实现要求

### 4.1 `terminal-session` 新增 WS 终端域

建议新增 `plugins/terminal-session/rust/src/ws_terminal.rs`，或将现有 `ws_control.rs` 拆为：

```text
ws_control.rs   会话控制端点
ws_terminal.rs  终端输入、订阅、输出、ACK
```

插件负责：

- 连接级状态：认证后的连接 id、会话绑定、游标、订阅代次、模式、关闭状态。
- 终端输入：调用 `host-pty.write`；特殊键由插件现有 `keys.rs` 翻译为字节。
- 终端输出：按 pty_id 调 `host-pty.ring-fetch`，按插件协议合帧后调用 `host-websocket.send-binary-to-client`。
- 终端生命周期：消费 `<owner>::pty:exit`，在排空后向对应客户端发送插件定义的停止帧。
- 连接断开：回收该连接的订阅/游标/发送任务，不让宿主维护会话订阅表。
- 多客户端隔离：每连接独立游标与背压；一个慢客户端不得阻塞其他客户端或 PTY 产出。

### 4.2 禁止在 `on-client-message` 内同步做长输出泵

`events-ws.on-client-message` 只做：

1. 解析当前插件协议帧；
2. 更新插件连接状态；
3. 投递一个插件内部任务或状态机事件；
4. 立即返回。

输出泵、ACK 处理、ring 拉取不能在宿主调用插件的同步回调中形成长链。插件任务通过已有异步/定时能力运行，调用 `send-binary-to-client` 时避免 actix arbiter 与 guest 回调互等。

### 4.3 PTY 宿主广播声明退役

删除 `host-pty.spawn` 配置中的 `hostBroadcastSessionId`：

- 删除 SDK `PtySpawnConfig` 字段与构造器。
- 删除宿主 `pty.rs` 的 `broadcast_session_id` 字段、唯一性检查、映射和 `broadcast_handle_for_session`。
- 删除 `session_gateway`、旧 WS 终端订阅器和 `pty/output_sink.rs` 中对该映射的依赖。
- 插件直接使用自己的 `session record.pty_id` 调 `ring-fetch`。
- `PtyRing` 继续作为宿主引擎内部缓冲；PTY 输出字节跨 WASM 边界只经 `host-pty.ring-fetch`，输入仍经 `host-pty.write`。

这使 PTY 引擎不再知道“会话 id”或“宿主 WS 广播会话”。

### 4.4 业务事件迁移

删除宿主 `broadcast-sync` 及其产品载荷后：

- 插件自身内部事件：`host-bus.publish`，使用属主私有 topic，例如 `<plugin-id>::session:changed`。
- 需要其他插件消费的事件：使用 public topic（例如 `terminal-session:changed`），不伪装成属主私有 topic；跨插件仍经 `host-bus` 门禁。
- 桌面插件前端事件：`host-events.emit`，事件名和 JSON 载荷由插件定义。
- 宿主只提供事件传输，不定义 `SyncEvent`、`SyncPayload`、会话摘要或任务状态枚举。
- 所有插件内 `broadcast_sync(...)` 调用迁移为 bus/emit；迁移完成后删除 `host-events.broadcast-sync`、SDK `HostEvents::broadcast_sync`、`SyncEvent`、`SyncPayload` 和 `broadcast` 权限位（若无其他合法消费者）。

---

## 5. 文件级迁移矩阵

| 当前文件/模块 | 动作 | 终态 |
| --- | --- | --- |
| `server/websocket/routes.rs` | 重写 | 只保留 `/ws/plugin/{plugin_id}/{path}` 通用握手、frame limit、endpoint owner/上限检查 |
| `server/websocket/conn.rs` | 深度收缩 | 通用连接 actor；删除 session/device/business auth record 分支；保留认证策略、过滤、限流、生命周期 |
| `server/websocket/registry.rs` | 重构 | 通用 connection/endpoint registry；删除 ChannelKind、Event/Terminal、设备去重/在线方法 |
| `server/websocket/channel/plugin.rs` | 保留并泛化 | 唯一宿主通道 adapter：raw text/binary、owner、auth、事件；不出现业务类型 |
| `server/websocket/channel/event.rs` | 删除 | 旧 `/ws/event` 不再存在 |
| `server/websocket/channel/terminal.rs` | 删除 | 终端协议归插件 |
| `server/websocket/message.rs` | 删除 | 不再有宿主业务 `Message` |
| `server/websocket/services/` | 删除 | `session_control`、`terminal_service` 归插件 |
| `server/websocket/subscription.rs` | 删除 | 输出订阅/游标/ACK 归插件 |
| `server/websocket/terminal_ws/**` | 删除 | 控制帧、TB v3、WatchMode、SessionStopped 归插件 |
| `server/websocket/session.rs` | 删除或替换 | 替换为通用 `ConnectionState/ConnectionContext`，不含 session/device 派生字段 |
| `server/websocket/connection_types.rs` | 删除 | 通用连接 DTO 放 `host-websocket`/server connection 模块 |
| `server/websocket/websocket_manager.rs` | 拆分 | `WebSocketServer` 生命周期 + 通用 `ConnectionDirectory`；删除 `BusinessMessage` 方法 |
| `server/websocket.rs` | 收紧 facade | 只导出通用路由/服务器/连接接口；产品模块不再 `pub mod` |
| `host_api/ws.rs` | 扩展 | 新增 `connection-context`；删除任何业务端点特判；继续复用通用 registry |
| `host_api/pty.rs` | 收缩 | 删除 `hostBroadcastSessionId` 与 session 映射；保留 PTY 通用原语 |
| `host_api/events.rs` | 删除产品同步面 | `broadcast-sync` 迁出/删除；保留通用 emit/notify |
| `events/host_sync_event.rs`、`events/sync_handler.rs` | 删除 | 宿主不再持同步产品载荷和 WS 广播策略 |
| `commands.rs`、`server.rs` | 清理 | 删除 `get_connected_devices`/`DeviceConnectionInfo` 产品 DTO；如需连接事实，改用通用 host-connection 面 |
| `enums/control.rs`、`enums/sync.rs` 及 WS 专用 re-export | 清理 | 无 WS 消费者后删除；不为旧路径保留垫片 |
| `plugins/terminal-session/rust/src/ws_control.rs` | 保留并扩展 | 直接处理插件端点帧 |
| `plugins/terminal-session/rust/src/ws_terminal.rs` | 新增 | 终端 WS 协议与输出泵 |
| `plugins/terminal-session/rust/src/output.rs` | 改造 | 从命令拉取扩展为 WS 连接级 ring-fetch/发送状态机 |
| `plugins/terminal-session/rust/src/launch.rs` | 改造 | 不再声明 `hostBroadcastSessionId` |
| `plugins/terminal-session/rust/src/session/**`、task/actions | 改造 | 事件改 bus/emit；会话/任务 WS 协议归插件 |
| `packages/plugin-sdk-desktop/rust/wit/bedcode.wit` | 破坏性更新 | 新增 connection-context；删除 broadcast-sync/PTY 广播字段；ABI v28 |
| `packages/plugin-sdk-desktop/rust/src/**` | 同步更新 | HostWebsocket、HostEvents、HostPty、wire/event 类型与默认导出同步 |

> 具体文件可因编译边界调整而重命名，但不得以“重命名”掩盖业务依赖；最终以依赖扫描锁验收。

---

## 6. 实施阶段与依赖

### P0：基线与静态边界锁

- 记录当前 `git status`/在途 diff，不覆盖其他任务改动。
- 建立宿主 WS 业务依赖清单与目标删除清单。
- 建立最终源码结构锁清单，但不在当前基线立即启用目标态断言；随 P1/P3/P4 对应改造同票启用：
  - `server/websocket/**` 实现段不得出现 `Message`、`SessionControlAction`、`TerminalAction`、`SyncPayload`、`SessionStopped`、`hostBroadcastSessionId`。
  - `host_api/pty.rs` 不得出现 `broadcast_handle_for_session` 或 `broadcast_session_id`。
  - 宿主不得出现 `broadcast_sync` 调用方（迁移完成后）。
- 锁必须区分测试夹具和实现段，避免自匹配。

### P1：通用 host-websocket 契约与连接核心

1. 在 desktop WIT/SDK 增加 `connection-context`，ABI 升 v28。
2. 宿主实现权限门、属主校验、auth context 脱敏与查询。
3. 将 `conn.rs` 收缩为通用连接 actor。
4. 将 `registry.rs` 收缩为 connection/endpoint registry。
5. 将 `websocket_manager.rs` 拆为服务器生命周期与连接目录。
6. 先完成通用插件通配端点的接线；旧终端/事件路由的删除统一在 P3 硬切，不在本阶段形成可交付的半切换状态。
7. 保留并强化 `channel/plugin.rs`：它只转原始帧，不读业务 payload。
8. 暂不删除仍被旧路由引用的设备在线、连接历史 touch/close、Event/Terminal 广播策略；先隔离为 legacy adapter，随 P3/P4 旧路由硬切一并删除。

依赖：无。

### P2：terminal-session 插件端点与终端数据面

1. manifest 声明 `terminal` 与 `session-control` 两个 `ws:server` 端点，均使用 `auth: jwt`。
2. 插件实现 `ws_terminal`：输入、订阅、游标、ACK、合帧、截断重同步、停止通知。
3. 插件用 `host-pty.write` 处理输入，用 `host-pty.ring-fetch` 拉输出，用 `host-websocket.send-binary-to-client` 推帧。
4. 插件用 `connection-context` 获取脱敏认证身份。
5. 插件用 bus/emit 发布会话、设备、任务事件。
6. 插件新链路停止声明 `hostBroadcastSessionId`；宿主映射暂只为尚未删除的 legacy 订阅器编译存在，P4 随旧订阅器一并删除。
7. 完成输出性能基线后再进入硬切。

依赖：P1 的 `connection-context` 与通用端点稳定；性能不达标时只允许保留**无业务语义**的通用字节流优化，不得把 terminal/session 逻辑放回宿主。

### P3：业务事件与连接事实先行迁移

1. 插件所有 `broadcast_sync` 调用迁移到 `host-bus.publish` / `host-events.emit`；按 §4.4 区分属主私有 topic 与 public topic。
2. 插件消费 WS 生命周期事件，自行完成认证记录 touch/close、设备派生视图和业务事件发布。
3. 删除 `HostSyncEvent`、`SyncEventHandler`、`SyncPayload` 宿主桥；删除 `host-events.broadcast-sync` WIT/SDK/host 实现及无消费者的 `broadcast` 权限位。若仍有非 WS 消费者，先迁移到 bus/emit，再删除，不保留产品同步桥。
4. 删除 `get_connected_devices` 与 `DeviceConnectionInfo`；插件从 `host-connection.connections-list` 派生设备视图。
5. 更新所有插件产物和 manifest 权限清单。
6. 旧 `/ws/event` 与 `/ws/terminal/session/{id}` 仍只作为待删除编译路径存在，不作为可交付兼容面。

依赖：P2 插件端点通过行为闭环。

### P4：宿主硬切与旧协议删除

在同一可交付变更中完成：

1. 删除 `/ws/event`。
2. 删除 `/ws/terminal/session/{session_id}`。
3. 删除 `message.rs`、`channel/event.rs`、`channel/terminal.rs`、`services/`、`subscription.rs`、`terminal_ws/`。
4. 删除 `hostBroadcastSessionId`、`broadcast_handle_for_session` 及 `host_api/pty.rs` 的 session 映射。
5. 删除 `server/websocket.rs` 对产品模块的公开导出。
6. 删除宿主 `Message` 相关测试与旧 wire fixture。
7. 删除旧 `SyncPayload`/`SessionControl`/`Terminal` WS 路径测试；新测试只覆盖插件端点和通用 transport。
8. 删除已无消费者的 WS 专用 `enums` re-export；不为旧路径保留垫片。
9. 旧路径请求返回宿主通用 404，不提供 alias 或 fallback。

依赖：P3；P3/P4 不得拆成两个可交付版本。

### P5：清理、文档与全量门禁

- 更新 desktop code-map、ADR 0022 修订记录、插件开发检查清单、双语 CHANGELOG。
- 记录移动端明确不兼容，不写“移动端零改动”。
- 清理旧路径、旧 ABI、旧产物和死测试。
- 运行全量 Rust、插件、SDK、前端（若触及前端）验证。
- 运行 `cargo fmt`、`cargo clippy`、根目录 eslint。
- 收尾运行 `lens_diagnostics mode=all`，不得有 blocker。

---

## 7. 行为契约

### 7.1 连接生命周期

- 连接注册成功后才有 `client-connect`。
- `client-connect` 必须早于该连接首个业务帧回调。
- `client-disconnect` 必须早于连接条目最终摘除，且每连接恰好一次。
- 认证失败/认证超时连接不得发布 `client-connect`。
- 插件停用、端点注销、服务器停机均 close 4005 或约定通用关闭码，并发布断开事件。
- 事件不缓冲、不重放；插件使用 `list-clients` / `connection-context` 自愈。

### 7.2 帧与背压

- text/binary 原样转发，宿主不 UTF-8 转换业务载荷。
- 同连接帧按到达顺序投递。
- 每连接发送队列有界；队列满显式报错或由插件处理，不静默丢弃。
- 二进制输出走 `list<u8>`/原始帧，不转 JSON 数组，不在宿主构造终端帧。
- 过滤链在插件端点 inbound/outbound 均执行；链路加密是否适用仍由安全策略决定，不由业务协议决定。

### 7.3 认证与凭据

- 宿主只做 JWT/认证策略裁决，不做配对码、QR、设备记录和业务认证编排。
- `connection-context` 永不返回 token。
- 插件端点未声明、属主未激活、端点不存在、帧超限时 fail-visible；不返回“成功但无数据”。
- 认证失败、插件 trap、连接断开都必须清理连接与插件任务。

### 7.4 终端流

- 插件的 `ring-fetch` 游标按连接独立保存。
- 环截断由插件显式发送重同步协议；宿主不替客户端重放或拼接。
- PTY 输出环永不因某个 WS 客户端背压而阻塞生产端。
- 一个连接的输出任务异常不得影响同端点其他连接。
- 终端会话停止由插件消费 `pty:exit` 后决定何时、何帧通知客户端。

### 7.5 业务事件

- 会话/任务事件由插件定义 payload 与 topic。
- 宿主不按事件变体决定是否广播、排除哪个设备或生成刷新通知。
- 跨插件通信只经 `host-bus` / `host-api-call`；不经宿主 WS 全局广播。
- 桌面 UI 事件由插件 `host-events.emit`，宿主只投递原始事件名与 JSON。

---

## 8. 依赖方向与源码锁

### 8.1 允许的依赖图

```text
core/app
  └── server/websocket (通用 transport)
        ├── actix / ws protocol
        ├── server/core/filter + security
        └── endpoint/connection registry

wasm_core/host_api/ws
  └── server/websocket (通用 transport primitives)

plugins/terminal-session
  ├── host-websocket
  ├── host-pty
  ├── host-bus / host-events
  └── 私有会话/终端/认证记录数据
```

禁止：

```text
server/websocket -> enums::{control, sync, auth, special_key}
server/websocket -> session_gateway
server/websocket -> session / device / task 业务类型
server/websocket -> 固定 plugin-id / endpoint / API 名称
host_api/pty -> session-id 映射
host events -> SyncPayload / SessionSummary / TaskStatus
```

### 8.2 结构锁验收

实现完成后，源码扫描必须证明：

- `server/websocket/` 的非测试实现段没有产品业务类型和业务方法名。
- `server/websocket.rs` 不再 `pub mod message/services/terminal_ws/subscription`。
- `/ws/event` 与 `/ws/terminal/session/` 路由字符串只允许出现在迁移说明/测试反例中，不得出现在实现。
- `hostBroadcastSessionId`、`broadcast_handle_for_session` 在生产源码中为零。
- `broadcast_sync`、`SyncPayload`、`HostSyncEvent` 在宿主生产源码中为零。
- 宿主 `ws` 核心不出现 `session_id`、`subscribed_sessions`、`SessionStopped`、`WatchMode`。
- 插件端点声明存在，且 `ws:server` 权限、owner 隔离、endpoint 上限测试齐全。

---

## 9. 测试与验证

### 9.1 单元测试矩阵

#### 宿主通用 transport

- 握手成功/失败、auth none/jwt、认证超时 4001。
- endpoint owner 隔离、跨插件句柄/端点/客户端拒绝。
- frame/message limit、端点客户端上限、发送队列满 fail-visible。
- 文本/二进制原样收发、帧顺序、连接 close code 与 `wasClean`。
- `client-connect` / `client-disconnect` 恰好一次及先后时序。
- `connection-context` 脱敏：不含 token、secret、公钥；跨属主拒绝。
- 插件停用回收只影响本人，服务器停机关闭全部插件端点连接。

#### 插件 terminal-session

- `session-control` 未知动作、缺参数、认证失败、响应帧。
- `terminal` 输入写入真实 PTY；Ctrl-C/Ctrl-D 等插件键翻译闭环。
- ring-fetch 正常顺序、历史、截断、ACK、游标单调、慢客户端隔离。
- 多客户端独立游标；断开/替换/退订后无残留任务。
- `pty:exit` 后尾帧排空与插件停止帧顺序。
- `connection-context` 身份映射到插件认证记录 touch/close。
- 事件改 bus/emit 后，插件自身消费者可收到；宿主不再构造 `SyncPayload`。

### 9.2 集成测试

- 真实 Actix server + 真实 terminal-session WASM 插件 + 真实 PTY 的 WS 闭环。
- 插件端点 JWT 成功、失败、超时。
- text/binary echo 与 endpoint broadcast。
- 端点注销、插件停用、服务器停机的连接回收。
- 无旧路由：请求 `/ws/event` 与 `/ws/terminal/session/x` 得到 404；不存在宿主业务响应。
- 旧 ABI 产物实例化失败并给出 v28 重建提示；不尝试加载或迁移。

### 9.3 性能门禁

- 复用现有 PTY 输出性能基线，新增“插件 `ring-fetch` + WIT binary + WS send”路径基准。
- 至少覆盖 1 MB/s 常态输出与 10 MB/s 压力场景。
- 记录 CPU、内存、队列深度、帧大小、ring 截断次数。
- 禁止以 JSON 数组承载高频输出；禁止无界队列；禁止宿主恢复 terminal/session 业务分支作为性能补丁。
- 若插件路径不达标，只允许新增通用 `ByteStream`/原始帧优化原语，并另立 ADR 记录；不得恢复业务耦合。

### 9.4 收尾命令

```bash
# 以下命令均从仓库根目录执行
(cd bedcode-desktop/src-tauri && cargo test)
(cd bedcode-desktop/plugins/terminal-session/rust && cargo test)
(cd bedcode-desktop/packages/plugin-sdk-desktop/rust && cargo test)
(cd bedcode-desktop && pnpm run test:run)
pnpm exec eslint .
```

- 改动 Rust 后执行 `cargo fmt --check` 与 `cargo clippy`。
- 改动 `gen/android` 时另执行 `./gradlew :app:compileUniversalDebugKotlin`；本专项默认不涉 Android。
- 所有 cargo 测试必须使用 rustup shim；测试后关闭 mock server、vitest worker、gradle daemon 等残留进程。
- pi agent 收尾执行 `lens_diagnostics mode=all`，无 blocker 才算完成。

---

## 10. 风险与控制

| 风险 | 控制 |
| --- | --- |
| 插件每帧调用 WIT 造成性能退化 | 二进制直传、批量 ring-fetch、有界队列、性能基准；失败只准保留通用字节流原语 |
| guest 回调内调用 WS 发送导致互等/自锁 | 插件消息处理只投递状态机任务；输出泵独立运行；沿用 ambient runtime 规则 |
| 去掉宿主 session 订阅后出现断连泄漏 | 插件连接状态机 + 断开事件 + 任务 abort；集成测试覆盖每个出口 |
| 认证身份缺失导致插件无法记录连接 | 新增脱敏 `connection-context`；端点事件后可查询；不把 token 传插件 |
| 事件迁移造成插件间通知丢失 | 插件 activate 期订阅 bus；提供快照查询；测试事件顺序与自愈 |
| 旧调用方被硬切 | 这是用户明确接受的范围；不添加 fallback，ABI v28 显式拒绝旧产物 |
| 移动端被打断 | 用户明确不要求兼容；spec/ADR 如实登记，不宣称移动端零改动 |
| 删除 `hostBroadcastSessionId` 影响 PTY 其他消费者 | 先完成全仓消费方扫描；目标仅保留插件 `ring-fetch`，任何新直读需求另立通用 stream spec |

---

## 11. 完成定义（Definition of Done）

- [ ] `server/websocket/**` 不再包含会话、终端、设备、同步任务业务解释。
- [ ] 宿主只保留通用 WS transport、认证策略、过滤、限流、连接/端点注册和生命周期。
- [ ] 只保留 `/ws/plugin/{plugin_id}/{path}` 业务入口；旧 `/ws/event` 与 `/ws/terminal/session/{id}` 已删除并返回 404。
- [ ] `Message`、`SessionControlAction`、`TerminalAction`、`SyncPayload`、`SessionStopped` 等宿主 WS 业务类型已删除或完全退出 WS 生产路径。
- [ ] `connection-context` 已进入 WIT/SDK/host 实现，凭据脱敏与属主隔离测试通过。
- [ ] `com.bedcode.terminal-session` 已拥有 session-control 与 terminal 两个端点，真实 PTY 输入/输出/ACK/断线闭环通过。
- [ ] `hostBroadcastSessionId` 与 `broadcast_handle_for_session` 已删除，PTY 引擎不再知道 session id。
- [ ] 插件事件已迁移到 bus/emit，宿主 `broadcast-sync`/`HostSyncEvent`/`SyncPayload` 生产路径已删除。
- [ ] 设备派生视图和认证记录 touch/close 由插件负责，宿主只提供通用连接事实。
- [ ] desktop ABI 已升至 v28，所有随包插件产物重建；旧产物显式失败，不静默兼容。
- [ ] 宿主 WS 源码结构锁、插件行为测试、集成测试、性能基准全部通过。
- [ ] desktop code-map、ADR 0022、插件检查清单、CHANGELOG 与本 spec 状态同步。
- [ ] 移动端明确标记为不兼容/不纳入本专项，不写“移动端零改动”。

---

## 12. 参考

- `docs/adr/0022-plugin-host-interface-primitive-boundary.md`
- `docs/knowledge/plugin-development-checklist.md`
- `.scratch/2026-09-18-ws-base-service/spec.md`
- `.scratch/2026-09-23-session-engine-downsink/spec.md`
- `.scratch/2026-09-24-host-crypto-business-downsink/spec.md`
- `bedcode-desktop/docs/code-map.md`
- `bedcode-desktop/src-tauri/src/server/websocket/`
- `bedcode-desktop/src-tauri/src/wasm_core/host_api/ws.rs`
- `bedcode-desktop/src-tauri/src/wasm_core/host_api/pty.rs`
- `bedcode-desktop/plugins/terminal-session/rust/src/ws_control.rs`
