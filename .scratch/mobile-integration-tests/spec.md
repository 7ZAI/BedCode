# 移动端集成测试方案（L1 + L2）

> 状态: **完成**（L1 2026-08-16 晚完成；L2 2026-08-16 深夜完成）
> 进度: ✅ L1 全部完成——`tests/ws_protocol_integration.rs` 7/7 + `tests/file_server_http.rs` 8/8 全绿；全量 366 单测无回归。✅ L2 全部完成——fixtures 8 文件 + `src/__tests__/integration/` 5 文件 26 测试全绿，全量 205 前端测试无回归
> 目标: 补齐移动端 366 个单元测试与真实链路之间的空档——WS 客户端全链路（协议 → 客户端 → router → handler → 前端事件）、file_service HTTP 服务器契约、前端组合集成。

## 背景与现状

移动端是**WS 客户端**（连接桌面端主机），与桌面端的"服务器进程内测试"对称但不同：

| 层 | 现状 |
|---|---|
| Rust 单元测试 | 280 个（44 文件 `#[cfg(test)]`），`tauri test` feature 已有 |
| Rust 集成测试 | **无 `src-tauri/tests/` 目录**（对比桌面端已有 4 个集成测试二进制） |
| 前端 | vitest + happy-dom，composables/store 单测（connectionProbe / useTerminalBuffer 等 7 个文件） |
| 已有 E2E 类基建 | dev-shell + headless Chrome CDP 冒烟脚本（`.scratch/ocr-plugin/devshell-smoke.mjs` 模式可复用） |

### 移动端独有的有利条件（已逐一确认源码）

1. **`ConnectionManager::new()` 是公开轻量构造**（`Arc<Self>`）→ 测试可建独立实例，**不需要**桌面端的全局串行锁（桌面端 `AppContext` OnceLock 只能 init 一次）
2. **`connect_without_emit(address, port, name)` 已存在**——专为测试设计的连接入口（不带 AppHandle）
3. **`build_router` 只依赖 `event_tx: broadcast::Sender<MobileEvent>`**——无 AppHandle 依赖（桌面端 `mock_app()` 与 Wry 不兼容的坑在移动端不存在）
4. **`subscribe()` 公开**——测试直接订阅 `MobileEvent` 断言事件流（`MobileEvent` 定义见 `src/router/event.rs`：Output / AuthSuccess / AuthFailed / PairingRequest / PairingVerified / Paired / ServerClosed / Error / Ack / SyncSession* / SyncConfig*）
5. **`ws_client.rs` 单测已确立本地服务器模式**（`TcpListener` + `accept_async`）——升级到协议级即可
6. **协议对称**：移动端 `model/message.rs` 与桌面端 `server/ws/message.rs` 同为 `#[serde(tag = "type", content = "payload")]`——**mock 服务器应答直接用移动端 `Message` 枚举构造再 `to_json()`**，形状天然正确，无需手写 JSON
7. `FileService::new` 轻量（registry + 后台任务 + actix server），无 AppHandle 依赖，tokio runtime 内可构造

### 已知障碍与 prefactor

| 障碍 | 处理 |
|------|------|
| `AuthHandler` 认证成功分支 `get_plugin_manager().expect("PluginManager not initialized")` → **测试环境 panic**（PluginManager::new 需要真实 Wry AppHandle，无法在纯 Rust 测试构造） | **prefactor**：改 `try_get_plugin_manager()`（已存在，返回 Option）——未初始化时跳过插件生命周期通知，语义正确（无插件环境本就该跳过） |
| 认证成功后 `get_file_service()` 惰性初始化会真启动 actix HTTP server + 后台 sweeper | 可接受（tokio runtime 内），属真实链路的一部分；`resend_if_active` 在无连接/无挂载时自动跳过 |
| `set_global_token` 是全局 RwLock，认证成功后写入 | 同测试文件内场景间用 `clear_global_token()` 收尾（tests/ 下每个文件是独立进程，跨文件无污染） |
| Windows 测试二进制 manifest | 移动端 build.rs 已有与桌面端相同的 `cargo::rustc-link-arg=/MANIFESTINPUT` 注入（作用于 package 所有链接产物）；保留 `build_manifest_smoke.rs` 恒真测试对齐桌面端，验证注入生效 |

---

## L1：Rust 进程内集成测试（`src-tauri/tests/`）

```
src-tauri/tests/
├── build_manifest_smoke.rs        # 恒真断言，对齐桌面端（Windows manifest 注入验证 + 占位）
├── common/
│   └── mod.rs                     # 协议级 mock 桌面端服务器 + 工具函数（cargo 惯例，不作为测试二进制）
└── ws_protocol_integration.rs     # 主体：WS 客户端全链路
```

### 协议级 mock 服务器设计（`tests/common/mod.rs`）

- `TcpListener::bind("127.0.0.1:0")`（OS 分配端口）+ `accept_async`——升级 ws_client.rs 既有模式
- 收到文本 → `serde_json::Value` 解析 → 记录到 `received: Arc<Mutex<Vec<serde_json::Value>>>`
- **自动应答状态机**（协议形状对齐桌面端 `server/ws/message.rs`）：
  - `Auth{stage: RequestPairing}` → 回 `Auth{stage: VerifyCode, pairing_code: "123456"}`（message_id 回填请求 id）
  - `Auth{stage: VerifyCode, pairing_code}` → 回 `Auth{stage: Authenticated, session_token: "test-jwt-token"}`
  - 其余消息记录但默认不应答（测试按需用 `send_message()` 主动推送）
- 应答构造用**移动端 `Message` 枚举**（`Message::auth(None, AuthPayload{...})` → `to_json()`）——两端协议对称的保证
- 提供 `send_message(&Message)` / `close()` / `received()` 供测试驱动

### 测试场景（`ws_protocol_integration.rs`）

| # | 场景 | 驱动 | 断言要点 |
|---|------|------|---------|
| 1 | 连接握手 | `ConnectionManager::new()` + `connect_without_emit` | `get_status() == Connected`；mock 收到连接 |
| 2 | 配对全链路 | `AuthRequest::request_pairing` → mock 回 VerifyCode → `verify_pairing_code("123456")` → mock 回 Authenticated | event_tx 依次收到 `PairingVerified`、`AuthSuccess`；`get_global_token() == "test-jwt-token"`；后续消息自动注入 token（mock 端断言收到的 token 字段） |
| 3 | 终端输出推送 | 认证后 mock `send_message(Message::output(...))` | event_tx 收到 `MobileEvent::Output{session_id, data, index}`，内容逐字段一致 |
| 4 | 服务端主动断开 | mock `close()` | 收到 `MobileEvent::ServerClosed` 或 WsClientEvent::ServerClosed；状态回落 |
| 5 | 未连接拒绝 | 断开后 `send()` | `Err(AppError::WebSocket("Not connected"))` |

> 实际落地为 7 场景：握手 / 配对全链路 / 终端输出推送 / 请求-响应匹配 / ServerClosed 业务消息 / TCP 断开感知（WsClient 事件层，Close 帧）/ 未连接拒绝。

### 关键实现要点

- `#[tokio::test]`（current_thread）内异步等待用 `tokio::time::sleep` + 断言统一 `timeout()` 包装，禁止 `std::thread::sleep`
- 事件等待：`subscribe()` 后 `broadcast::Receiver::recv().await` + timeout（broadcast 无订阅者时消息丢弃——先 subscribe 再触发）
- 场景间 `clear_global_token()` 收尾；token 注入断言放在 mock 端（收到消息的 `payload.token` 字段）
- 每个 `.rs` 文件是独立测试二进制（进程隔离），`common/mod.rs` 由各测试文件 `mod common;` 引入
- 测试内 `tracing` subscriber 输出到 test harness，失败时可查链路

### 验收标准（L1）

- [x] `cargo test --test ws_protocol_integration` 全绿，每场景至少一个真实往返断言（非恒真）
- [x] 与现有 `cargo test`（280 单测）并行跑无冲突（端口 0 分配 + 独立进程）
- [x] 场景 2/3 覆盖配对 → 认证 → 终端输出主链路
- [x] prefactor 仅限 `get_plugin_manager` → `try_get_plugin_manager` 一处，语义不变

---

## L2：前端组合集成（vitest 层）

### 与桌面端的核心差异：**事件驱动**

移动端前端状态机主要靠 Rust 端 `listen` 事件推进（`ws_connecting` / `ws_connected` / `ws_paired` / `ws_output` / `ws_sync_*` 等，见 `useMobileCommands.ts` 的 `initMobileEventListeners`），而非 invoke 返回值。所以 mock 边界是**两层**：

- mock `@tauri-apps/api/core` 的 `invoke`（命令返回值走 fixtures）
- mock `@tauri-apps/api/event` 的 `listen`（**脚本化事件序列**驱动 composable 状态机）

### 契约 mock 工厂（`src/__tests__/fixtures/`）

对齐移动端 Rust DTO（每个文件头注明 Rust 源 + serde 规则，沿袭桌面端 05 票的 drift 教训）：

| 文件 | 对齐源 |
|------|--------|
| `auth.ts` | `enums/auth.rs`（AuthPayload / AuthStage snake_case） |
| `terminal.ts` | `model/message.rs` TerminalPayload / `router/event.rs` MobileEvent::Output |
| `session.ts` | `enums/sumary.rs` SessionSummary、`enums/control.rs` SessionControlPayload |
| `sync.ts` | 同步事件载荷（session/config/task_status） |
| `file_service.ts` | file_service 枚举 |

### 组合集成测试（`src/__tests__/integration/`）

| # | 文件 | 组合内容 | 断言要点 |
|---|------|---------|---------|
| 1 | `connection-flow.test.ts` | 真实 `useMobileConnection` + 连接视图组件 | 连接 → 状态机流转；超时/取消/重连（12s 超时 + MAX_AUTO_RECONNECT_ATTEMPTS=3 用 fake timers 压缩） |
| 2 | `pairing-flow.test.ts` | `useMobileConnection` + 配对组件 | 脚本化事件（ws_pairing_request → ws_paired）→ isPaired 流转 → pairedDevices 持久化 |
| 3 | `terminal-flow.test.ts` | xterm 实例 + `useTerminalBuffer` + `useMobileConnection.sendInput` | 脚本化 `ws_output` 事件 → xterm 渲染 → 输入回传参数构造 |
| 4 | `session-flow.test.ts` | sessionConfigs/activeSessions + 会话视图 | 配置加载 → 启动会话 → 状态联动 → 停止 |
| 5 | `plugin-flow.test.ts` | 插件加载器 + 文件服务挂载 | 插件清单 → 启用 → 挂载状态联动 |

### 验收标准（L2）

- [x] 全部 fixture 与对应 Rust DTO 字段对齐（人工对照表或运行时断言）——`drift.ts` 的 `assertDtoFields` 运行时断言 + 类型级 `Equals`，各 fixture 文件头注明 Rust 源文件；`maxOffset` 语义（订阅时快照覆盖字节偏移）已在 fixture 注释说明
- [x] 5 个集成场景全绿，每场景至少 2 个 composable/store 协作断言——26 测试全绿（connection 8 / pairing 5 / terminal 3 / session 6 / plugin 4）
- [x] `npm run test:run` 全量通过（205），重跑 3 次无 flaky

### L2 落地记录（2026-08-16）

新增文件（均未提交）：
- `src/__tests__/fixtures/`：drift.ts（从桌面端对齐）+ auth / terminal / session / sync / file_service + index.ts
- `src/__tests__/integration/helpers.ts`：flushAsync / loadFreshModule / resetLocalStorage / clearEventHandlers（非测试文件）
- `src/__tests__/integration/`：connection-flow / pairing-flow / terminal-flow / session-flow / plugin-flow

关键设计决策与坑：
1. **模块级单例 reset**：useMobileConnection 模块加载即 init()（22+ 个 listen await），模块级 ref 跨用例残留——每次用例 `vi.resetModules()` + 动态 import（freshConnection helper）；vi.mock 注册不受 reset 影响
2. **旧模块监听器残留**：重新加载模块时旧模块的 listen handler 不会自动 unlisten（无卸载钩子）——emit 会同时触发新旧 handler 造成 invoke 双调用；必须 `clearEventHandlers` 后再加载
3. **事件驱动状态**：connecting 等状态由 ws_* 事件驱动而非 connect() 返回——断言前需先 emit 事件；connect() 只设置 disconnected + isConnecting
4. **重连 bug（已修复）**：发现 connect() 置 `autoReconnectAborted=true` 且无重置路径 → 手动连接后的意外断开不触发前端 ws_reconnect。**修复**：onConnected 回调复位 `autoReconnectAborted=false` + `autoReconnectAttemptCount=0`（取消标记只用于取消「进行中」的重连等待，连接成功建立后必须复位）；边界语义保留——ws_connected 未到达（连接未成功）前的意外断开仍不重连。集成测试覆盖：完整重连闭环（unexpected_disconnect → ws_reconnect → ws_reconnected → JWT 认证 → ws_paired）+ 边界场景
5. **invoke 参数均为对象形状**（`{ token }` / `{ sessionToken }` / `{ code }`），非位置参数
6. **HTTP API 响应 camelCase**（wslDistro/workingDir）、ws_sync_* 事件内嵌 DTO snake_case——两套形状并存；HTTP 响应手写 camelCase 对象，事件走 fixture
7. **drift 断言与 skip_serializing_if 的兼容**：断言要求键集合恒全——Option 字段默认 `undefined`（键在、JSON 序列化时省略，精确模拟 skip 语义）
8. **SubscribeResult maxOffset 裁剪**：flushPending 跳过 `end_offset <= maxOffset` 的缓冲帧（快照已覆盖）——fixture 默认 maxOffset=0（订阅时快照未覆盖任何字节）
9. **订阅竞态**：doSubscribe 内部 await startGlobalListener（异步）——确认前回放帧测试需先显式 `await store.startGlobalListener()` 消除时序竞态
10. **视图组件不挂载**：DevicesView 依赖过重（QR/mdns/生物识别等）——集成测试用真实 composable + 轻量事件驱动（原计划的“连接视图组件”改为事件序列驱动，组合覆盖目标不变）
11. **xterm stub**：遵循 writeCoalescer.test.ts 惯例（stub 需 `element` 属性——writeBytes 守卫）；writeCoalescer 默认直写（ENABLE_RAF_COALESCE=false）事件回调内立即 write
12. **vue-tsc 既有错误**：`src/plugin/` 3 个 OcrApi 错误为基线/他人改动（OCR 票据 09），与 L2 无关

---

## L3（可选，backlog）

- 进程级 E2E 比桌面端更贵（需 adb 设备/模拟器 + 桌面端实例双端联动），**不优先**
- 轻量路径：**dev-shell headless Chrome 冒烟脚本**（`.scratch/ocr-plugin/devshell-smoke.mjs` 已确立 CDP 驱动模式）固化为可回归套件，跑「连接 → 配对 → 会话 → 输出」主流程

## 执行顺序

1. ✅ prefactor：`AuthHandler` `get_plugin_manager()` → `try_get_plugin_manager()`
2. ✅ L1 基建：`tests/common/mod.rs` mock 服务器 + `ws_protocol_integration.rs` 场景 1–2（握手 + 配对）
3. ✅ L1 场景 3–6（输出 / 请求响应 / 断开 / 未连接拒绝）
4. ✅ L2 fixtures 工厂（对齐移动端 DTO）
5. ✅ L2 组合集成 1–5（核心是事件序列驱动）
6. ✅ 全量回归：`cargo test` + `npm run test:run`（移动端），出报告

## 风险与坑

1. **AuthHandler 全局依赖**：`get_plugin_manager()` panic 已列入 prefactor；`get_file_service()` 惰性初始化会真启动 actix server（随机端口，测试进程退出自动回收）——若 announce 逻辑干扰断言，考虑 `try_get_file_service` 类似方案
2. **broadcast 无订阅者丢消息**：subscribe 必须先于触发动作
3. **token 全局残留**：场景间 `clear_global_token()`
4. **mock 服务器应答时序**：测试用事件通道或 sleep+轮询控制，统一 timeout 包装
5. **Windows manifest**：若测试二进制启动崩溃（0xc0000139），检查 build.rs 注入是否对 tests/ 产物生效（保留 build_manifest_smoke.rs 兜底）
