# cross-end-tests — 跨端真实互连集成测试

同一测试进程内，让**桌面端真实服务器**与**移动端真实客户端代码**互连。零 mock、
零 adb、零 WebView、零模拟器。

```bash
# 前置：桌面随包 wasm 产物（认证中心）须已构建
cd bedcode-desktop && pnpm run plugins:build

cd cross-end-tests && cargo test
```

产物缺失时测试**显性失败**（不是 `[skip]`）——本工程的唯一驱动方式就是真实产物，
静默跳过会把「未验证」伪装成「通过」。

## 为什么需要它

两端各自的集成测试都用**协议级 mock 对端**：

| 端 | 真实侧 | mock 侧 |
|---|---|---|
| 桌面 | Actix 服务器（`src-tauri/tests/pty_session_chain.rs` 等） | 通用 reqwest / tokio-tungstenite 客户端 |
| 移动 | AuthHttpClient / SessionHttpClient / TerminalLinkManager | 假桌面服务器（`tests/common/mod.rs`） |

两套 mock 各自自洽。若文档或任一端实现偏离，**两端测试全绿但真实互连必坏**。
本工程消灭这个盲区：请求字节由移动端真实客户端生成，应答字节由桌面真实插件生成。

## 结构

```text
cross-end-tests/
├── Cargo.toml              # 独立包；同时依赖两端 lib（bedcode-desktop-lib / bedcode-mobile-lib）
├── src/lib.rs              # 空 lib（crate 级说明）
└── tests/
    ├── common/
    │   ├── desktop_ctx.rs  # 桌面无头装配：AppContext + 真实认证中心产物 + Actix 服务器
    │   └── mobile_ctx.rs   # 移动端客户端装配：目标设备 / 全局 token / 事件与输出记录替身
    ├── harness_selfcheck.rs    # 台子自检（场景红了先排除台子坏了）
    ├── pairing_auth_flow.rs    # 配对 / QR / 重认证（正例 + 4 类反例）
    ├── jwt_rotate_reconnect.rs # 密钥环轮换宽限期（ADR 0033：旧 token 轮换后仍可用）
    ├── session_http_flow.rs    # 会话 HTTP 面 + 1002 错误信封 + remove 幂等
    ├── terminal_ws_flow.rs     # 终端流闭环（真实 bash PTY 输出到达移动端）
    ├── terminal_output_pressure.rs # 终端输出压力：零缺口 / 重锚 fail-visible / 背压
    ├── fail_closed_flow.rs     # fail-closed（无中心在册 / 伪造凭证）
    ├── lifecycle_flow.rs       # 桌面停用 / 激活对移动端连接的联动
    ├── event_channel_flow.rs   # 事件通道 session-control（7 类事件 + 不重放 + 自愈）
    ├── http_proxy_flow.rs      # HTTP 代理面（Egress + JWT 注入 + 链路加密信封）
    └── mdns_health_flow.rs     # mDNS 发现/广播 + /api/health 探测
```

**每个场景 = 一个独立测试二进制**：`AppContext` 是进程级 `OnceLock` 单例，场景之间
无法重装，只能靠进程隔离。

## 装配约束（踩过的坑）

1. **加密面必须走非环回地址**：桌面端链路加密过滤器**按设计豁免环回对端**
   （`link_crypto::is_exempt`）。本包两端同主机，连 `127.0.0.1` 会静默跳过加密分支，
   `http_proxy_flow` 因此连本机自己的 LAN IP；取不到非环回 IPv4 时**显性 panic**
   （不静默 skip，否则加密面会被当成已验证）。
2. **服务器启动有两个入口**：`start_server`（`app::serve`，轻）与
   `start_server_via_supervisor`（生产同款：写端口状态 + 起 mDNS 广播 + 重置指标）。
   `mdns_health_flow` 必须用后者——否则既不广播，`/api/health` 还会报默认端口 8765。
3. **收尾要在断言失败时也执行**：`WebSocketManager` 的 actix runtime 跑在**非 daemon**
   OS 线程上；运行中的 `mdns_sd::ServiceDaemon` 被直接 drop 会 join 收包线程约 2 分钟。
   两者叠加曾让一个失败断言跑 120s；现用 `catch_unwind` + 外层收尾 + `resume_unwind`，
   失败路径实测 1.0s（各场景另有 `TempDirGuard` 保证 panic 时也清 `/tmp`）。
4. **headless 链路加密装配**：`enable_link_crypto_http()` 必须早于配对——认证响应里的
   `kdPublicB64` 就是它建的那条身份公钥，pin 不能手工造假（假 pin 桌面解不开 → 假红）。

## 只读观测依赖（不伪造任何应答）

本包额外依赖 `bedcode-server-websocket`（插件 WS 端点表 + 连接认证态）与
`bedcode-server-core`（headless 链路加密装配 + 加密计数器快照）。两者都**只读**
传输/加密事实，用来给「就绪」「真的加解密过」这类判据提供不经被测代码的观测面；
任何应答仍由桌面端真实插件生成。

## 为什么依赖键等于 lib 名

L0（2026-09-30）把两端 lib 从同名 `bedcode_lib` 改为 `bedcode_desktop_lib` /
`bedcode_mobile_lib` 之前，这两行只能二选一——同名 crate 无法同时出现在一个依赖图里。
路径依赖的依赖键默认取 package 名，故 Cargo.toml 里要显式写 `package = "..."`。

## 记录替身口径

`EventRecorder`（`TerminalEventSink`）/ `OutputRecorder`（`Channel<InvokeResponseBody>`）
**只记录已真实发生的数据**（链路状态事件、PTY 输出字节），不伪造任何协议应答。
生产形态是 Tauri WebView，测试里没有 WebView，替身只被「读」用于断言。

## 覆盖不到的边界（诚实清单）

- `deny_kind` 三态（`no_center` / `unavailable` / `policy`）是宿主**日志字段**不是
  wire 字段——客户端一律看到 401（这是有意的，不泄露部署信息）。三态分类的覆盖在
  宿主 `utils/auth/auth_center` 单测。
- 生物认证正向路径：移动端私钥在 Android Keystore，无头进程构造不出真设备密钥。
- QR 的「桌面扫码确认」UI 步骤：直接驱动插件 `qr-code-generate` 互调（即桌面 UI 的
  同一入口）生成 token。
- peer-net 文件传输（移动端↔桌面 P2P）：走 peer-net 引擎的 mDNS + P2P 直连，
  **不经桌面 HTTP/WS 服务器**，本 rig（Actix HTTP/WS）结构上不含它。
- 移动端前端 `useHttpApi.ts` 的每个调用点：跨端覆盖的是它们共用的 Rust 代理面
  `execute_proxy`，请求构造与 `ApiResult` 归一化仍由前端单测负责。
- `SyncConfig*` 三个事件变体在 `session-control` 上无帧源（插件只广播 3 类会话事件 +
  4 类任务/模式事件），如实不测。
- mDNS 场景依赖**本机组播**：无组播环境（部分 CI 容器）下 15s 内发现不到即 panic，
  不静默 skip。

## 产物落点与磁盘

本包有**自己的** `target/`（依赖图与两端都不同，合并会互相驱逐缓存）。
首次全量构建分钟级、峰值十几 GB——构建前按 AGENTS §3 检查磁盘。

spec：`.scratch/2026-09-30-cross-end-integration-tests/spec.md`
