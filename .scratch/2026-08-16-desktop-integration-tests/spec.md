# 桌面端集成测试方案（L1 + L2）

> 状态: **已完成**（2026-08-16，6 张 tickets 全部 resolved，7 个 commit）
> 目标: 补齐单元测试（538 绿）与真实链路之间的空档——跨模块、跨进程（Actix runtime）、跨语言（Rust ↔ 前端契约）的集成验证。

## 背景与现状

桌面端核心业务是**局域网 HTTP + WS 服务器**（Actix，默认端口 8766），移动端通过 WS 协议配对、认证、控制 PTY 会话。当前测试现状：

| 层 | 现状 |
|---|---|
| Rust 单元测试 | 538 个全绿；`tauri` `test` feature + `mock_app` 已在用（api_bridge.rs / frontend_output_handler.rs） |
| Rust 集成测试 | `src-tauri/tests/` 仅 `build_manifest_smoke.rs`（恒真断言，为满足 build.rs link-arg 约束）——**实际空白** |
| 前端 | vitest + happy-dom，组件/composables/store 单测（~20 文件） |
| E2E | `package.json` 有 `test:e2e`（playwright）脚本，但无 config、无 e2e 目录——纯预留 |

**核心红利**：`start_http_server(port, &NetworkConfig)`（`server/app.rs:264`）是纯 Rust 函数，不依赖 AppHandle；`configure_routes`（`server/app.rs:165`）注册 health_check / terminal_ws / local_terminal_ws / terminal_bg_image 等路由。核心业务链路可像普通后端服务一样**进程内真实启动**测试。

**依赖已确认（零新增 dev-dependency）**：
- `tokio-tungstenite = "0.24"`（生产依赖，WS 客户端）
- `reqwest = "0.12"`（生产依赖，HTTP 客户端）
- `tauri` `test` feature（2026-08-16 已加）

---

## L1：Rust 进程内集成测试（`src-tauri/tests/server_integration.rs`）

### 原理

真实启动 Actix HTTP+WS 服务器（测试内分配端口）→ 用真实客户端（reqwest / tokio-tungstenite）从外部连入 → 走完整请求链路（中间件 → 路由 → service → actor → PTY），断言端到端行为。覆盖单元测试抓不到的跨模块集成 bug（路由注册遗漏、中间件顺序、actor 消息协议、全局状态联动）。

### 文件结构与运行方式

```
src-tauri/tests/
├── build_manifest_smoke.rs     # 保留（build.rs link-arg 约束）
└── server_integration.rs       # 新增：本方案主体
```

- **单个 `#[tokio::test]` 函数 + 场景子步骤**（或 `static SERVER_TEST_LOCK: Mutex<()>` 串行化）——`WebSocketManager::global()`、`AppContext`（OnceLock）跨测试共享，禁止并行起停服务器（教训：metrics 测试并行污染）
- 端口：`start_http_server(0, ...)` 由 OS 分配（需实现时确认 0 是否被 accept；否则用空闲端口探测），避免 8766 与真实实例冲突
- 收尾：`handle.stop(true)` 优雅停机 + 防御性 `WsSessionRegistry` 清理

### 初始化策略（两条路径，实现时按依赖确认）

1. **首选**：`tauri::test::mock_builder()` 建 mock AppHandle → `AppContextBuilder::build_and_init()`（与生产 `run()` 同路径组装全部服务：db / session_manager / plugin_host / file_service / pairing_service / qr_manager / mdns_advertiser / sync_tx / resource_dir / system_info）。AppContext 是 `OnceLock`，**每个测试进程只 init 一次**，所有场景共享
2. 备选：若实际 handler 仅依赖各服务 `global()` 单例（如 PairingService、WsSessionRegistry），可跳过 AppContext 直接初始化惰性单例——实现时用日志/回溯确认依赖路径后选定

### 测试场景（按优先级）

| # | 场景 | 断言要点 | 覆盖链路 |
|---|------|---------|---------|
| 1 | HTTP 健康检查 | `health_check` 返回 200 + 预期 JSON | 路由注册 → handler |
| 2 | HTTP 鉴权契约 | 未带 JWT 访问受保护 `/api/*` → 401；带非法 token → 401；带合法 token → 放行 | jwt_auth 中间件在真实路由上的行为 |
| 3 | 配对全链路 | WS 连接 → `Message::Auth{stage:RequestPairing}` → 收到 `VerifyCode` 响应 → `verify_pairing_code` → `Authenticated` + 返回 session_token | ws actor → pairing_service → auth_service → JWT 签发 |
| 4 | 认证后客户端注册 | 认证后 `WsSessionRegistry` 出现该 client，`authenticated=true` | registry ↔ actor 联动 |
| 5 | 未认证拒绝 | 未认证连接发业务消息 → 被拒/报错 | actor 状态机 |
| 6 | PTY 会话链路 | 认证后创建会话 → 写入命令（echo）→ 收到输出事件 → 关闭会话 | ws → session_manager → openpty → 输出广播 |
| 7 | 多客户端广播 | 两客户端认证，一端发 `broadcast_to_others` 语义消息，另一端收到、发送端排除 | registry 广播路由 |
| 8 | 优雅停机 | `handle.stop(true)` 后新连接拒绝、端口释放 | 生命周期 |

> 消息格式对齐 `server/ws/message.rs`（桌面端）的 `Message` JSON 结构；构建器参考移动端 `connection/request.rs` 的 `AuthRequest`（协议对称，字段名以桌面端 serde 为准）。

### 关键实现要点

- 等待异步事件：`tokio::time::sleep + yield_now`；**禁止**在 `#[tokio::test]`（current_thread）body 里 `std::thread::sleep`（tokio task 不被轮询）
- PTY 场景在 Windows 用真实 openpty（pty_process 已有 2 个真实 openpty 单测垫底，链路已验证可跑）
- 断言超时统一 `timeout()` 包装，避免 CI 卡死
- 日志：测试内 `tracing` subscriber 输出到 test harness，失败时可查链路

### 验收标准

- [ ] `cargo test --test server_integration` 全绿，且**不是恒真断言**（每场景至少一个真实往返断言）
- [ ] 与现有 `cargo test --lib`（538）并行跑无冲突（端口隔离 + 串行锁生效）
- [ ] 场景 3/4/6/7 覆盖配对 → 认证 → PTY → 广播主链路
- [ ] 集成测试发现既有 bug 时不现场修复，按惯例记入 `.scratch/` bug 台账，统一修复

---

## L2：前端集成测试（vitest 层）

### 目标

单元测试（mock invoke 返回 fixture）之上补两层：
1. **契约防线**：前端 mock 的 fixture 与 Rust 侧 DTO **字段级对齐**（snake_case 命名、类型、可选性），防止前后端字段漂移（历史上出现过 `active_task` 字段缺失、`approvalTimeoutSec` 模板访问等契约 bug）
2. **组合集成**：真实 Pinia + 真实 composable + 组件挂载（`@vue/test-utils`），只 mock `@tauri-apps/api/core` 的 `invoke` 边界——验证 store/composable/组件之间的协作，而非单个函数

### 契约 mock 工厂（新增）

```
src/__tests__/fixtures/
├── index.ts              # 统一导出 + fixture 类型
├── server.ts             # ServerStatusInfo / NetworkConfig / ServerMetrics（对齐 useServer.ts 类型）
├── pairing.ts            # PairingCode / PendingDevice / QrToken 相关
├── session.ts            # 会话列表 / SessionLaunchConfig
└── plugin.ts             # 插件清单 / 插件状态
```

- **对齐机制**：每个 fixture 文件头注明对应的 Rust DTO 源文件（如 `server/ws/message.rs`、`dtos/*.rs`）+ serde 字段命名规则；可选强校验：运行时 `expect` 断言字段名集合与类型定义一致（防新增字段未同步）
- 现有单测改为从 fixtures 工厂取数（收敛数据定义，消除各测试文件重复字面量）

### 组合集成测试（新增 `src/__tests__/integration/`）

| # | 文件 | 组合内容 | 断言要点 |
|---|------|---------|---------|
| 1 | `pairing-flow.test.ts` | 真实 Pinia + `usePairing` + 配对视图组件 | 生成配对码 → 展示 → 轮询待配设备 → 配对成功状态流转 |
| 2 | `server-flow.test.ts` | `useServer` + settingsStore + 服务设置组件 | 启动/停止服务器 → 指标轮询 → 网络配置持久化回显 |
| 3 | `terminal-flow.test.ts` | xterm 实例 + `useTerminalOutputStream` + 会话控制 composable | 输出流 → xterm 渲染 → 输入回传参数构造 |
| 4 | `plugin-flow.test.ts` | 插件加载器 + pluginStore | 插件清单加载 → 启用/停用状态联动 |

- 环境：沿用 happy-dom；`invoke` mock 统一走 fixtures 工厂（`vi.mock('@tauri-apps/api/core')`，现有 useServer.test.ts 已确立此模式）
- **不 mock**：Pinia、vue-router（测试内创建真实实例）、composables 内部逻辑

### 验收标准

- [ ] 全部 fixture 与对应 Rust DTO 字段对齐（人工对照表或运行时断言）
- [ ] 4 个集成场景测试全绿，且每个至少含 2 个 composable 或 store 的协作断言
- [ ] `npm run test:run`（vitest run）全量通过，无 flaky（重跑 3 次稳定）

---

## 后续扩展：L3（进程级 E2E，可选，不在本次范围）

- **轻量探活（推荐先做）**：脚本启动构建产物 → 轮询 HTTP 端口（桌面端自身是 HTTP 服务器，curl 配对端口验证 200）→ 断言日志无 ERROR → 杀进程。可进 CI
- **重方案**：tauri-driver + Playwright（`test:e2e` 脚本已预留）。需安装 tauri-driver、capabilities 加 `core:webview`、CI 图形会话；xterm 自绘 UI 自动化收益有限，**L1/L2 落地后再评估**

---

## 风险与坑（实现前必读）

1. **全局单例串行化**：`WebSocketManager::global()`、`AppContext`（OnceLock）、`WsSessionRegistry::global()` 跨测试共享 → 单测试函数 + 子场景，或全局 Mutex
2. **AppContext 只可 init 一次**：`OnceLock`，mock AppHandle 必须在首个场景前完成组装；备选路径（仅 global 单例）需先确认 handler 依赖
3. **current_thread runtime**：`#[tokio::test]` 内异步等待用 `tokio::time::sleep`；服务端线程（WebSocketManager 的 Actix 线程）是独立 runtime，不受影响
4. **端口冲突**：测试端口用 0（OS 分配）或随机高位端口，禁止硬编码 8766
5. **Windows PTY**：openpty 在 Windows 走 ConPTY 封装，真实进程创建有环境依赖（PATH 等），场景 6 失败时先确认是测试环境问题还是链路 bug
6. **tauri mock 限制**：`mock_builder` 无法启动真实 WebView，所有前端相关命令只能验证"注册存在 + 参数/返回值契约"，不验证真实 UI 交互（属 L3 范畴）

## 执行顺序

1. L1 场景 1–2（HTTP 契约，最简，先打通测试基建：启动/停机/端口）
2. L1 场景 3–5（配对 + 认证 + 注册，核心业务）
3. L1 场景 6–8（PTY + 广播 + 停机）
4. L2 fixtures 工厂 + 存量单测迁移
5. L2 组合集成测试 1–4
6. 全量回归：`cargo test`（桌面）+ `npm run test:run`，出报告
