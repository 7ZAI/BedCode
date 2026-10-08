# 移动端 host-websocket 客户端域：自建通用出站连接引擎（桌面 15 函数的真子集）

## 状态

**已定案（2026-10-08，`.scratch/2026-10-07-mobile-wasm-core-refactor/` 票 11）**。
移动端 ABI **13 → 14**（纯增量）。三条开放点经用户裁决（2026-10-08）：引擎形态选 A（自建）、
不做 `wss://`、`connect-timeout-secs` 上限 5s。

## 背景

- 票 12（`terminal_link.rs` 终端订阅协议客户端迁插件）要求输出帧**帧级直传、禁止逐帧
  JSON 化**（spec C3 性能红线 / ADR 0022）——帧级直传必须有插件可持有的 WS 连接原语。
- 桌面终态的 `host-websocket` 是 15 函数（客户端 5 + 服务端 9 + `connection-context`），
  实现落在 `packages/bedcode-server-websocket`（能力域 crate），客户端段与 **actix 服务器栈
  + `bedcode-server-core` 过滤器链**同 crate 耦合——移动端是纯客户端、不跑 actix、没有链路
  加密层，**整 crate 不可复用**；客户端段自身依赖很薄（`tokio-tungstenite` + 句柄表 +
  reader/writer 两任务 + 属主事件 + 权限门），移动端已有全部依赖 ⇒ 零新依赖可实现。
- 移动端现有 `connection/ws_client.rs` 是**产品形客户端**（请求-响应管理器 / 心跳 / 重连 /
  业务 `Message` 路由、只拼 `ws://`），且它本身就是票 12 的迁出对象——不能拿它当引擎垫在
  host-websocket 下面（会形成「宿主产品层 = 引擎」的倒挂）。

## 决策

### D1 · 引擎形态 = 移动端自建（方案 A）

`host_impl/ws.rs` 自持句柄表 + reader/writer 任务，直接用既有 `tokio-tungstenite`（约 600 行）。
与桌面客户端域**同构但分叉**：桌面那份与 actix 服务器栈耦合、要过过滤器链，移动端不需要；
共享化要先拆服务器栈依赖，成本远大于收益。

**条件触发转 B**（抽 client-only 子 crate 到根 `packages/`）：出现第三个消费端，或客户端段
逻辑再演进两轮以上——届时以 ADR 固化边界。在此之前的「一份同构实现」是显式接受的重复。

### D2 · 函数集 = 客户端 5 函数真子集

`connect` / `send-text` / `send-binary` / `close` / `is-connected`。服务端域 9 函数与
`connection-context` **不存在于移动端**（ADR 0018 移动端是消费端，不跑 WS 服务器），
防回接锁 `mobile_host_websocket_client_domain_lock.rs` 锁 WIT 函数名与权限词汇。
接口名保持 `host-websocket`（同名词跨端对齐），函数集是桌面的真子集。

### D3 · 权限位只加 `ws:client`

出站连接是 SSRF 面（插件可代宿主访问任意 `ws://` 地址），独立成位、fail-closed（未声明即拒），
不与未来服务端能力混位。移动端权限词汇**没有** `ws:server`（同锁管控）。
同步点四处：SDK `permission.rs` 常量 → `VALID_PERMISSIONS` → `PERMISSION_API_MAP` →
宿主 `host_impl/ws.rs` 权限门真源（移动端无打包 CLI / 前端合法集合两点）。

### D4 · 投递通道：状态事件 JSON topic + 下行帧二进制 topic（与 spec 的 events-ws 偏差）

- **状态事件**走消息总线属主私有 topic（JSON，`host-bus` 的 `subscribe`）：
  `<plugin-id>:ws:open|error|close`。宿主不缓冲、不重放，须 activate 期订阅；
  丢失后自愈靠 `is-connected`。
- **下行帧**走二进制属主私有 topic `<plugin-id>:ws:message`（`host-bus` 的
  `subscribe-binary`），载荷为帧信封 `kind(1) + handle 长度 u16 BE(2) + handle + 原始字节`。
  零 JSON 编解码（C3 红线）；同一连接内帧按到达序投递。
- **与 spec 票 11 原文的偏差**：spec 写「可选导出 `events-ws`」，实施改走既有
  `events-binary` 导出（v9 已有，宿主动态探测）——零新导出、零 ABI 面，帧走总线二进制通道
  而非新的 guest 回调接口。帧信封形状由宿主 `frame_envelope` 与 SDK `parse_ws_frame`
  **双点配对维护**，形状变更须双端同批。
- **总线订阅需同时持有 `bus` 权限位**（集成测试实证：仅 `ws:client` 时订阅被
  host-bus 权限门拒绝、属主事件静默不可达）——消费插件须同时声明两个权限位。

### D5 · 不做 `wss://`、不做重连编排

- `wss://` 显式拒绝（明确错误而非让握手失败掩盖原因）：与现役形态一致——移动端只拼
  `ws://`（局域网明文 + peer-net 层 TLS）；将来需要 wss 时 CA / 自签证书配置属独立立项
  （用户裁决 2026-10-08）。
- 客户端域是**传输原语**：退避重连、心跳、订阅协议、ack/resync 编排一律归插件
  （票 12 迁 `terminal_link` 时自带）；宿主只提供断线事实事件。`connect` 同步阻塞至握手
  完成（host fn 同步上下文，同桌面 mutex 形态），`connect-timeout-secs` 缺省即上限
  5s、越界截断（ADR 0029：不长时间挂起插件实例）。

### D6 · ABI 13 → 14 纯增量 + 单向协商的已知风险

新增接口不改既有函数集 ⇒ v13 插件二进制不受影响。协商是**单向**的（仅拒绝高于宿主的版本）：
v13 产物在 v14 宿主加载成功但**没有 ws 能力**且无报错。本票不实现能力探测；票 12 落地时
必须同批处理（插件侧显式检查，或接受「插件未升级 = 终端面不可用」）。

## 后果

- 移动端 host_impl 域 13 → 14，聚合回收 `purge_for_plugin` 纳入 ws（停用下线全部连接）。
- 门禁：`host_impl/ws.rs` 内联单测八类（权限门 fail-closed / 属主隔离 / 生命周期真实握手 /
  close code 语义 / 队列满 fail-fast / purge 回收 / 入参校验 / 帧信封形状）+ 边界锁
  （服务端域函数名与 `ws:server` 词汇禁令，变异自检 2/2）+ 真实 WASM 组件全链路集成
  （component-test `ws-client` feature：connect → send-text → 对端回帧 → close 事件
  wasClean=true → is-connected 翻 false）。
- spec 原文「ABI 11→12」过期修正为 13→14（票 06/07 已各 bump 一次）；
  「`docs/implementation-plans/mobile-wasmtime-component-migration.md` §3.2 差异表」联动项
  跳过——该文档已不存在，移动端 WIT 差异事实以移动端 `bedcode.wit` 注释与本 ADR 为真源。
