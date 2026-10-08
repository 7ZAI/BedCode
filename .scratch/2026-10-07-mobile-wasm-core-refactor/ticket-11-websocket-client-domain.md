# 票 11 · host-websocket 客户端域（阶段 3 首票 · 票 12 的铺路票）

> 状态：**已实施（2026-10-08）**。三点开放裁决用户已拍板：引擎形态 **A（自建）**、**不做 wss**、
> `connect-timeout-secs` 上限 **5s**（§5/§10 与实际一致的按推荐执行；偏差见 §9 + ADR 0041）。
> 实施要点：world `plugin` 补 `import host-websocket;`（方案文档漏列，编译期拦下）；下行帧走
> 既有 `events-binary` 导出 + `<owner>:ws:message` 二进制 topic（§9 偏差表第三行兑现）；消费
> 插件须同时持 `bus` 权限位（集成测试实证，WIT/SDK 注释已注明）；migration §3.2 联动跳过
> （文档已不存在）。上一票：票 10（`.scratch/.../ticket-10-command-face-retirement.md`）。
> 全部事实取自 2026-10-08 工作区实测（行号可复现），不凭记忆。

## 1. 目标

给移动端插件提供**通用出站 WebSocket 连接**能力（WIT `host-websocket` 客户端域 5 函数），
让票 12（`terminal_link.rs` 终端订阅协议客户端迁插件）有连接面可用——票 12 要求「输出帧帧级
直传、禁止逐帧 JSON 化」（spec C3 性能红线），而帧级直传必须有插件可持有的 WS 连接原语。

本票**只做传输面**，不做终端协议、不做会话控制、不动认证（C4 安全边界留宿主）。

## 2. 事实盘点（实测）

### 2.1 移动端现状：本域完全不存在

| 位置 | 现状 |
| --- | --- |
| `packages/plugin-sdk-mobile/rust/wit/bedcode.wit` | import 接口 14 组，**无 `host-websocket`**（有 `host-terminal` 1 函数待票 15 退役） |
| `packages/plugin-sdk-mobile/rust/src/host/` | 12 个域模块（bus/config/database/events/fs/http/log/mdns/peer/platform/storage/terminal），**无 `ws.rs`** |
| `packages/plugin-sdk-mobile/rust/src/permission.rs` | 19 个权限位，**无 `ws:*`**（`VALID_PERMISSIONS` + `PERMISSION_API_MAP` 都要扩） |
| `src-tauri/.../host_impl/` | 13 个实现模块，**无 `ws.rs`**（`purge_for_plugin` 聚合入口已存在，`deactivate_inner` 已调用） |
| `component.rs` | 13 组 `impl bedcode::plugin::host_X::Host` + 13 条 `add_to_linker`（新增域要两处同批） |

### 2.2 桌面端终态：实现分两层，能力域与服务器栈耦合

- WIT `host-websocket` **15 函数** = 客户端 5（`connect` / `send-text` / `send-binary` / `close` /
  `is-connected`）+ 服务端 9（`register-endpoint` 等）+ `connection-context`（v28 安全上下文查询）。
- 能力域实现：`bedcode-desktop/packages/bedcode-server-websocket/src/plugin_binding.rs`（1,216 行）。
  客户端段 = 第 74–410 行左右：`CLIENTS` 句柄表（handle → owner + writer 通道 + 状态）、
  `ws_connect`（`IntoClientRequest` + 握手 + `spawn_with_error_boundary` 起 reader/writer 两任务）、
  `ws_send_text` / `ws_send_binary` / `ws_close` / `ws_is_connected` / `enqueue`（队列满 → 错）。
  权限门 `PERMISSION_WS_CLIENT`、事件 `<owner>::ws:open|error|close`。
- 端口 adapter：`bedcode-wasm-core/src/host_api/ws.rs`（163 行，纯转发 + `install` / `purge_for_plugin`）。

### 2.3 关键阻塞：整 crate 不可复用

`bedcode-server-websocket` 依赖 `actix-web` + `bedcode-server-core`（过滤器链 / 链路加密 /
`TransportFace`）——它是**服务器**面 crate。移动端是客户端、不跑 actix、也没有链路加密层。
直接 path 依赖会拖进整个服务器栈；复制则双份漂移（spec D1 反对）。

**但客户端段自身依赖很薄**：`tokio_tungstenite` + 句柄表 + 两个 spawn 任务 + 属主事件 + 权限门。
移动端已有 `tokio-tungstenite 0.24` / `futures-util 0.3` / `uuid v4` ⇒ **零新依赖**可实现。

### 2.4 移动端现有 WS 代码不是通用引擎

`src-tauri/src/connection/`（4,442 行，含 `ws_client.rs` 638 行 + ws_client 子模块）是**产品形
客户端**：请求-响应管理器、心跳、重连、消息路由到 `Message` 业务枚举，且只拼 `ws://` 地址
（`ws_connection.rs:54`）。它不是「连任意 ws:// 并按句柄收发帧」的引擎，而且它本身就是票 12 的
迁出对象——**不能**拿它当引擎垫在 host-websocket 下面（会形成「宿主产品层 = 引擎」的倒挂）。

## 3. 范围裁决（三条硬边界）

1. **只做客户端域 5 函数，服务端域 9 函数 + `connection-context` 一律不做**。移动端不跑 WS
   服务器（ADR 0018 移动端是消费端）。接口名保持 `host-websocket`（同名词跨端对齐），但函数集
   是桌面的**真子集**——WIT 注释必须写明「客户端子集，与桌面 15 函数的差异见 spec §1.2」。
2. **权限位只加 `ws:client`，不引入 `ws:server`**。理由沿桌面判据：出站连接是 SSRF 面（插件可代
   宿主访问任意地址），必须独立成位、fail-closed（未声明即拒），不与未来服务端能力混位。
3. **事件走属主定向总线 topic**（`<owner>::ws:open|error|close`），复用 `host_impl/mdns.rs` 的
   定向 publish 范式（票 03 范式），**不经 `host-events`**（那是业务事件面，ADR 0022 B6 禁区）。
   帧数据不经事件回灌（事件只带连接事实），下行数据走 `send-text` / `send-binary` 原语。

另附两条「本票不做」的显式声明：
- **不做 `wss://`**：与现役形态一致（移动端只拼 `ws://`，局域网明文 + peer-net 层 TLS）。
  若将来移动端需 wss，需带 CA / 自签证书配置扩展，属独立立项。
- **不做重连 / 心跳 / 断线续传**：客户端域是**传输原语**，编排（退避重连、订阅协议、ack/resync）
  归插件（票 12 的 `terminal_link` 迁入插件后自带）。宿主只提供「断线事实事件」。

## 4. ABI 影响面：移动端 13 → 14（纯增量）

当前 `ABI_VERSION = 13`（票 07 接收编排下沉）。本票**新增接口**，不改既有接口函数集
⇒ **纯增量，v13 插件二进制不受影响**，老产物照常加载（只是没有 ws 能力）。

⚠️ 协商是**单向**的（仅拒绝高于宿主的版本，票 07 §5.1 已记录）：v13 产物在 v14 宿主上加载
成功但**没有 ws 能力**，且无任何报错。对本票影响可控（当前唯一消费者 file-transfer 无 ws 需求），
但票 12 落地时必须同批处理：要么插件侧显式检查（`host-ws` 能力探测）要么接受「插件未升级 =
终端面不可用」。本票**只记录该风险，不实现探测**。

### 4.1 移动端侧同步点（5 处 + WIT + 版本，缺一即编译红或静默失效）

| # | 文件 | 改动 | 漏了会怎样 |
| --- | --- | --- | --- |
| 1 | `packages/plugin-sdk-mobile/rust/wit/bedcode.wit` | 新增 `interface host-websocket`（5 函数 + 属主事件文档） | 编译红（guest 调不到） |
| 2 | `packages/plugin-sdk-mobile/rust/src/host/ws.rs`（新）+ `host/mod.rs` | 新增 `HostWs` trait（签名与 WIT 一一对应） | 编译红 |
| 3 | `packages/plugin-sdk-mobile/rust/src/wasm_host.rs` | `impl HostWs for WasmPluginState`（5 方法转发 `host_impl::ws`） | 编译红 |
| 4 | `packages/plugin-sdk-mobile/rust/src/permission.rs` | `PERMISSION_WS_CLIENT` 常量 + `VALID_PERMISSIONS` + `PERMISSION_API_MAP` | **静默失效**：manifest 声明 `ws:client` 会被拒（词汇外） |
| 5 | `src-tauri/.../host_impl/ws.rs`（新）+ `host_impl/mod.rs` | 引擎实现（句柄表 / 权限门 / 属主仲裁 / 事件 / `purge_for_plugin` 分支）+ `mod ws;` | 编译红 |
| 6 | `src-tauri/.../component.rs` | `impl bedcode::plugin::host_websocket::Host for WasmPluginState` + `add_to_linker::<WasmPluginState, D>` 一行 | 编译红（实例化期找不到 import） |
| 7 | `packages/plugin-sdk-mobile/rust/src/abi.rs` | `ABI_VERSION` 13 → 14 + 版本史注释 | **静默失效**：v14 产物在 v13 宿主被拒（但反之不拒，见上） |
| 8 | `plugins/file-transfer/manifest.json` | 本票**不改**（无 ws 需求）——票 12 才加 | — |

**权限四同步点**（桌面五同步点的移动端子集）：SDK `permission.rs` 常量 → `VALID_PERMISSIONS` →
`PERMISSION_API_MAP` → 宿主 `host_impl/ws.rs` 的权限门真源。移动端**无**打包 CLI / 前端合法集合
那两点（无 plugin manifest-gen 校验链），故移动端是四点。

## 5. 引擎从哪来：两个选项（推荐 A，待用户确认）

### A · 移动端自建通用出站连接引擎（推荐）

`src-tauri/src/plugin/wasm_runtime/host_impl/ws.rs` 自持句柄表 + reader/writer 任务，
直接用已有 `tokio-tungstenite`。预估 **350–450 行**（含属主事件、权限门、purge）。

- 收益：零新依赖、不拖 actix 栈、不动桌面任何文件（本专项 Out of scope 含桌面改动）、
  与移动端已有 host_impl 范式一致、无跨 crate 耦合。
- 代价：与桌面客户端域**存在一份实现重复**（约 340 行同构逻辑）。但两端形态已实质分叉：
  桌面需 actix 服务器栈共存 + 过滤器链适配，移动端不需要；共享化要先拆服务器栈依赖，
  成本远大于收益。

### B · 从 `bedcode-server-websocket` 抽 client-only 子 crate 到根 `packages/`

与能力域上提路线（`.scratch/2026-10-07-capability-crates-to-root-packages`）同轨。

- 收益：双端一份客户端引擎。
- 代价：要剥离 actix / server-core 依赖（本 crate 的 `lib.rs` 明确「只向下依赖 server-core /
  server-base」），抽出的子 crate 仍需为移动端补一套过滤器链适配；改动面落在桌面能力域
  （本专项 Out of scope，需另立项 + 双端评估 ADR 0019）。收益仅在「出现第三端」时才放大。

**推荐 A**，并在文档里记条件触发：若出现第三个消费端，或客户端段逻辑再演进两轮以上，
则按 B 立项抽 crate（届时用 ADR 固化边界）。

## 6. 实施步骤（8 步，每步可验证）

1. **ADR 裁决**：本票引擎形态（A/B）落 ADR（移动端 host-websocket 客户端域形态），并按 §3 追加
   ADR 0022 移动端批次条目（只加传输原语、B1-B6 零命中）。
2. **WIT 新增接口**：`bedcode.wit` 加 `host-websocket`（5 函数），注释写明客户端子集语义、
   属主事件 topic 形态、SSRF 面与 `ws://` 限制。
3. **SDK trait + 权限位**：`host/ws.rs` + `host/mod.rs` 导出 + `permission.rs` 三处。
4. **宿主引擎实现**：`host_impl/ws.rs`（句柄表 `HashMap<String, ClientEntry>` 挂在
   `WasmPluginState` 或独立 `WsClientState`；权限门 `require_ws_permission`；属主仲裁；
   定向事件；`purge_for_plugin` 分支）+ `host_impl/mod.rs` 登记。
5. **component.rs 两处接线**：`impl host_websocket::Host` + `add_to_linker` 一行。
6. **ABI bump**：`abi.rs` 13 → 14 + 版本史注释（对齐 §4.1 表格）。
7. **测试**（见 §7）。
8. **文档联动**：mobile code-map（新增 host-websocket 域段 + 同步点表格）、spec 状态推进、
   CHANGELOG 双语、`docs/implementation-plans/mobile-wasmtime-component-migration.md` §3.2
   差异表（WIT 接口组数 +1）。

## 7. 门禁

### 7.1 针对性单测（开发中随改随跑）

`host_impl/ws.rs` 内联 `#[cfg(test)]`：
1. **权限门 fail-closed**：未声明 `ws:client` 的插件调 5 函数全拒（每函数一例，验错误文本含
   `permission denied: ws:client`）。
2. **属主隔离**：A 插件的句柄给 B 插件查 / 发 / 关 → 全部拒绝（跨插件越权是本域最大风险面）。
3. **句柄生命周期**：connect → is-connected true → close(1000) → is-connected false；
   close 命中返回 true、二次 close 返回 false。
4. **close code 语义**：`wasClean=true` 当且仅当 code ∈ {1000, 1001}（与桌面同款契约）。
5. **队列满 fail**：writer 通道满 → `send-text` 报错而非阻塞（不阻塞实例是 ADR 0029 硬要求）。
6. **purge 回收**：`purge_for_plugin(A)` 后 A 的连接全部下线、B 的不受影响；停用后
   `is-connected` 不可查（句柄不存在）。
7. **入参校验**：`url` 非 `ws://` 开头 → 明确错误；`connect-timeout-secs` 越界 → 明确错误。

### 7.2 结构锁（新文件，2 用例）

`tests/mobile_host_websocket_client_domain_lock.rs`：
- **不得跟演桌面服务端域**：移动端 WIT 里不得出现 `register-endpoint` / `broadcast-*` /
  `send-*-to-client` / `close-client` / `unregister-endpoint` / `list-clients` /
  `list-endpoints` / `connection-context` 任一函数名（ADR 0018 移动端不跑服务器；跟演即越界）。
- **`ws:server` 权限位不得在移动端出现**（与上一条配对：权限词汇同锁）。
- 变异自检：注回 WIT 服务端函数 → 转红；`permission.rs` 加 `PERMISSION_WS_SERVER` → 转红。

### 7.3 集成测试（`src-tauri/tests/`，ws_e2e 式）

起本地 `tokio-tungstenite` 监听（`127.0.0.1` 随机端口）→ 真实插件经 `host-websocket`
connect → send-text → 服务端回帧 → close；断连后断言 `ws:close` 属主事件到达且 payload 含
连接事实。**用例必须走真实插件闭环**（`plugins_dir` 用随包产物），不拿裸 host_fn 当验证。

### 7.4 收尾全量（AGENTS §10）

- 宿主 `cargo test --no-fail-fast` 全量（基线：lib 374 passed / 6 failed，6 个是并行会话在途的
  `egress.rs`，与票 06–10 同数）；
- 新锁 + 前四把锁（发送编排 / 接收编排 / 发现投影 / 命令面）复跑全绿；
- SDK crate `cargo test`（权限表新增位若有测试需同步）；
- 插件 crate `cargo test` **零改动仍跑**（证真源未被牵连）+ `--rust-only` wasm32 门禁
  （spec §6 硬门禁：native 绿 ≠ 可交付；本票插件零改动，但要证明 ABI bump 未破坏插件构建）；
- `cross-end-tests` 全量（双端 WIT 有共有接口新增，按 ADR 0019 双端同步评估口径）；
- 前端 `pnpm run test:run` + 根 eslint（本票预期零前端改动，写明跳过理由）；
- 真机：Android 上连桌面实测（票 12 的前置），列入票 20 验收。

## 8. 风险与回退

| 风险 | 吸收 |
| --- | --- |
| **connect 同步阻塞**：桌面语义是「阻塞至握手完成」，移动端 host fn 是同步上下文（同桌面 mutex 形态），握手期阻塞整个插件实例 | 收紧 `connect-timeout-secs` 上限（建议 ≤ 5s，超界报错而非长时间挂起）；文档写明代价；ADR 0029 的按需异步化（`func_wrap_async`）是后续方向，本票不实施 |
| **SSRF 面**：插件可代宿主连任意 `ws://` 地址（含内网） | 权限位 fail-closed + 仅属主句柄 + 连接事实事件可观测；不做 URL 白名单（与桌面同款，桌面亦无）——若要收紧属独立立项 |
| 连接泄漏 | 停用经既有 `purge_for_plugin` 回收 + 节点/进程退出兜底；单测覆盖 |
| 双份实现漂移（选项 A） | WIT 注释 + code-map 记「与桌面客户端域同构但分叉」；条件触发即转 B（§5） |

## 9. 与 spec 的偏差（点名，实施时按本票口径）

| spec 票 11 原文 | 本票核实后口径 |
| --- | --- |
| 「ABI 11→12」 | 实际是 **13 → 14**（票 06/07 已各 bump 一次：11→12→13） |
| 「客户端域 5 函数」 | 确认，但**必须显式声明服务端域不做**（spec 未点明，落地时容易跟演桌面 15 函数） |
| 「+ 可选导出 `events-ws`」 | **本票不做**：帧数据不经事件回灌（§3.3），只有连接事实事件走既有总线；`events-ws` 导出若确有需求另立项 |
| 未提权限位 | 新增 `ws:client` 一点（移动端四同步点，见 §4.1） |

## 10. 需用户裁决的开放点

1. **引擎形态选 A 还是 B**（§5，推荐 A）。
2. **`wss://` 是否本票就要**：核实结论是现役链路只拼 `ws://`，故推荐不做；但若移动端后续要连
   带 TLS 的桌面端，需在本票就把 CA / 自签证书配置拉进来（否则票 12 会返工）。
3. **`connect-timeout-secs` 上限取值**（建议 5s；与桌面默认值是否对齐需确认，桌面 WIT 未写死默认）。