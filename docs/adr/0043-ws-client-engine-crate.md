# 移动端 WS 出站连接引擎抽根：bedcode-ws-client-engine 能力 crate

## 状态

**已实施（2026-10-09，`.scratch/2026-10-09-ws-client-engine-crate/spec.md`）**。
本任务**零 ABI / 零 WIT 变更**（不触碰移动端 `bedcode.wit` 与 SDK `abi.rs`）、5 原语签名与
错误文案逐字保留、事件 topic 与帧信封形状逐字保留——纯结构抽根，行为零变化。

## 背景

移动端 `host-websocket` 客户端域（ADR 0041）的全部机制（句柄表 / 属主仲裁 / reader-writer
双任务 / 心跳 / 自动重连 / 帧信封 / 停用回收，约 1,350 行）原先住在 fork crate
`bedcode-mobile/packages/bedcode-wasm-core/src/manager/runtime/host_impl/ws.rs` 里。
ADR 0041 D1 对这一份与桌面 `bedcode-server-websocket` 客户端段的「同构但分叉」显式接受的
重复，同时写明**条件触发转 B**：出现第三个消费端，或客户端段逻辑再演进两轮以上，届时抽
client-only 子 crate 到根 `packages/` 并以 ADR 固化边界。票 12 终端订阅协议客户端迁插件、
票 17 整核搬移两轮演进之后条件已到，本轮落地。

同时，同族先例已铺路：ADR 0035（能力域 crate 化：默认形态 = 纯引擎 + 端口抽象）、
ADR 0040（双端共享实现核）、ADR 0042（mDNS 引擎抽根/共享，M1–M5 同款手法）。

## 决策

### D1 · 新建独立 crate `packages/bedcode-ws-client-engine`（不复用 feature-gate 形态）

与 ADR 0042 的 discovery-engine 不同，本轮**不**在既有 crate 上加 feature：那份可共享的
实现本来就在**桌面 crate**里，而移动端实现是与桌面分叉的另一份。故抽根对象是移动端这一份，
落点是新的独立 crate。桌面 `bedcode-server-websocket` 的客户端段**不并入**——它仍与 actix
服务器栈 + `bedcode-server-core` 过滤器链同 crate 耦合，ADR 0041 D1 的桌面侧条件未满足；
两端「同构但分叉」的显式重复在客户端段继续存在，但移动端这一份自此是**任何宿主可直接引用**
的通用 crate。

默认形态 = **纯引擎 + 端口抽象**：零 WIT、零 SDK（双端 `bedcode_plugin_api*`）、零平台
（tauri）、零机制内核（host-kit）依赖；机制级依赖只有
tokio / tokio-tungstenite / futures-util / serde / serde_json / uuid / tracing / async-trait。

### D2 · 平台差异面全部经 `ports::WsClientPorts` 注入（7 方法）

| 差异面 | 端口方法 | 为什么不能住引擎 |
| --- | --- | --- |
| 权限门（`ws:client`，fail-closed） | `check_permission` | 授权表是宿主安全闸门（AGENTS §5.1.3 ②），出站是 SSRF 面 |
| 事件 / 帧投递（总线 topic） | `publish` / `publish_binary` | 总线与会话隔离是宿主机制；topic 已由引擎拼好（属主私有命名空间） |
| 后台任务（reader/writer/重连） | `spawn` | wasmtime fiber 内不保证 runtime 上下文，裸 `tokio::spawn` 会 panic；任务必须挂宿主运行时 |
| jwt 代发 token | `global_token` | 凭据不落插件（C4）；宿主只交出「代发时那一刻的 token 值」 |
| 重连退避策略与钳制边界 | `reconnect_policy` / `reconnect_bounds` | 全局退避单一事实源在宿主（下限钳制是自愈风暴的教训沉淀），引擎只按策略推进循环 |

任务面 `WsTask` 只暴露 `cancel()`（幂等），**不暴露宿主运行时句柄类型**；退避策略面
`ReconnectPolicy`（`start` / `get_delay` / `on_success`）与移动端 `HostEnginePorts` 的
既有端口逐字同形，宿主适配器只做类型包装、不复制任何退避逻辑。

### D3 · 移动端收窄为薄适配器 + `MobileWsClientPorts`

fork crate `host_impl/ws.rs`（1,350 行）重写为薄适配器（约 330 行）：5 条原语
（`ws_connect` / `ws_send_text` / `ws_send_binary` / `ws_close` / `ws_is_connected`）与
`purge_for_plugin` **函数签名不变**（component.rs 接线 / host_impl 聚合入口零改动），body
改为调引擎域函数。`MobileWsClientPorts` 五面实现：权限门读 manifest
`granted_permissions`（构造时结算）/ `bus.publish(topic, "host", …)` / `spawn_with_error_boundary_on`
（宿主 panic 错误边界）+ `RuntimeTask`（`abort` 幂等）/ `host_ports.global_token()` /
`host_ports.reconnect_bounds()` 与 `reconnect_policy()` 投影。

**同步↔异步桥留在宿主侧**：WIT host fn 是同步上下文而引擎 `connect` 是 async（握手 await），
`guarded_host_call`（panic 边界）+ `block_on_async`（重入安全桥）两件事都是宿主运行时形态，
故在适配层，引擎不碰。

### D4 · wire 词汇自持 + 漂移锁（源文件级比对）

`src/wire.rs` 自持四类 wire 契约副本：状态事件名（`ws:open|error|close|reconnect-scheduled`）、
属主私有 topic 拼法（`<plugin-id>:ws:<event>`）、帧信封 kind 与头长
（`kind(1) + handle 长度 u16 BE(2)`）、权限字面量 `ws:client`。**插件的消费面**
（`parse_ws_frame` / `WsIncomingFrame` / `HostWs` trait / topic 助手）仍住移动 SDK
`host/ws.rs`——引擎与它逐字一致由 `wire::drift_lock` 钉死：读**源文件**比对
（不依赖编译期依赖关系，故「零 SDK 依赖」前提与锁并存），事件名常量块与 topic 助手块按
文本块比、帧形状与权限字面量按行比；任一侧漂移（改值 / 改定义 / 改注释）即红。

### D5 · 边界与行为红线（三条）

1. **只抽移动端实现**（见 D1）；桌面侧不动。
2. **不做 wss / 不做编排**（ADR 0041 D5 保持）：仅 `ws://`；心跳与退避重连为**引擎参数**
   （`heartbeatSecs` / `autoReconnect` config），订阅协议 / 重订阅 / ack-resync 编排归插件。
3. **服务端域不跟演**（ADR 0018 / 0041 D2 保持）：只有客户端 5 函数 + `purge_for_plugin`；
   不引入 `ws:server` 权限词汇。

行为契约（权限门 fail-closed / 属主隔离 / 句柄生命周期 / close code 语义 / 队列满 fail-fast /
purge 回收 / 入参校验 / 帧信封形状 / 重连窗口钳制）**随实现原样搬入引擎**并由引擎单测覆盖；
引擎不含任何产品概念（无「会话」「终端」「设备」「配对」名词），url / headers / protocols
与帧内容纯字节透传。

### D6 · 治理与防回接面同批更新

- crate 内 `src/boundary_lock.rs`（4 例）：生产源码零平台 / SDK / wasm-core / 机制内核 /
  WIT 绑定层针脚（含 `MessageBus` / `WasmPluginState` 等宿主类型名）+ 生产清单零内部 crate
  且机制级依赖仍在场 + **单测必须在 `src/` 内**（治理 crate 规则：crate 根不得有 `tests/` /
  `benches/` / `examples/` 或 `[[test]]` 段）；扫描面按 `#[cfg(test)] mod` 声明处推导。
- 桌面 `capability_crates_no_product_ids.rs`：`SCANNED_CRATES` 登记（抽根即入语义管辖面）。
- fork crate `tests/fork_boundary_lock.rs`：双端共享锚点白名单 4 家 → **5 家**
  （+ `bedcode-ws-client-engine`，与 ADR 0042 的 discovery-engine 同理由移入）。
- 移动宿主 `tests/mobile_host_websocket_client_domain_lock.rs`：修陈旧路径
  （原指宿主 `src/plugin/wasm_runtime/host_impl/ws.rs`，host_impl 迁入 fork crate 后即失效——
  陈旧路径会让锁静默扫不到任何文件）+ 扫描面扩到引擎 `wire.rs` / `engine.rs` + 目标文件
  存在性断言（防路径再次漂移导致锁空转）。

## 影响面

- **新增** `packages/bedcode-ws-client-engine`：`src/{lib,engine,ports,wire}.rs` +
  `engine/tests.rs`（行为契约 13 例）+ `wire/drift_lock.rs`（3 例）+ `boundary_lock.rs`（4 例）。
- **移动端 fork crate** `bedcode-mobile/packages/bedcode-wasm-core`：`Cargo.toml`
  （+ `bedcode-ws-client-engine` path 依赖；`tokio-tungstenite` 保留在生产段——test-support
  的 mock WS 服务器仍用它，而宿主以普通依赖方式开该 feature）、
  `host_impl/ws.rs` 薄适配器重写（-1,465 / +268 行 diff）、`test_support/mock_plugin_ws.rs`
  仅格式整理。
- **文档**：本 ADR、移动 code-map（WS 域段与防回接锁索引）、桌面 code-map（锁索引登记项）、
  CHANGELOG 双语。
- **零 ABI / 零 WIT**：移动端 `bedcode.wit` 与 SDK `abi.rs` 未被本任务触碰（纯结构抽根；
  ABI 版本号的演进由各自票据记账）。

## 验证

- 新 crate `cargo test`：**20 全绿**（引擎 13 + 漂移锁 3 + 边界锁 4）。
- fork crate `cargo test --features test-support`：**285 + 4 + 4 全绿**（含
  `ws_client_domain_full_loop_with_real_component` 真实组件全链路回归与薄适配器 3 例）。
- 移动宿主 `cargo test`：全部集成目标通过（含 ws 客户端域边界锁；结果见 CHANGELOG 条目）。
- 变异自检 3/3：漂移锁（改副本值 → 红）/ 边界锁（生产代码注入宿主针脚 → 红）/
  适配器映射（权限门旁路 → 红），全部还原。
- 桌面 `capability_crates_no_product_ids` 对新增 crate 的扫描面生效（登记项断言）。

## 与既有 ADR 的关系

- **ADR 0041 D1**：本 ADR 是该条目「条件触发转 B」的落地固化；D2–D5 边界原样继承。
- **ADR 0035**：能力域 crate 默认形态（纯引擎 + 端口抽象）的延续；
  `capability_crates_no_product_ids` 登记表同步 +1。
- **ADR 0040 / 0042**：双端共享实现核的第三种手法落点（前两者为 feature-gate 共享 /
  双端共享实现核；本轮为「从分叉实现抽 client-only 子 crate」）；fork 边界锁共享锚点
  白名单与 ADR 0042 同措辞扩到 5 家。
- **ADR 0018 / 0019**：移动端契约独立不被破坏（本任务 WIT/ABI 零变更）；双端锁版与
  「先改锁再动 WIT」纪律不触发（本轮不动 WIT）。
- **ADR 0022**：引擎零产品语义（§5.1 B1–B6 零命中）；停用回收 / 属主隔离 / 事件定向投递
  三项既有语义逐字保留。
- **ADR 0042 的对照**：那一轮在桌面 crate 上加 feature 共享；本轮因实现分叉（桌面那份与
  actix 栈耦合）而新建 crate——两轮形态不同但共享同一端口范式
  （`DiscoveryPorts` / `WsClientPorts`），后续若桌面客户端段解耦，可评估并入本 crate。
