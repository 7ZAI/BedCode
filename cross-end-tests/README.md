# cross-end-tests — 跨端真实互连集成测试

同一测试进程内，让**桌面端真实服务器**与**移动端真实客户端代码**互连。零 mock、
零 adb、零 WebView、零模拟器。

```bash
# 前置：两端随包 wasm 产物都须已构建（2026-10-15 起移动侧协议客户端也在 wasm 插件里）
cd bedcode-desktop && pnpm run plugins:build
cd bedcode-mobile && pnpm run plugins:build

cd cross-end-tests && cargo test
```

产物缺失时测试**显性失败**（不是 `[skip]`）——本工程的唯一驱动方式就是真实产物，
静默跳过会把「未验证」伪装成「通过」。

## 两进程形态（desktop-harness，2026-10-15 起）

单进程 rig 有一个**结构性边界**：`inventory` 是进程级注册，桌面与移动对端差异域
（host-http / host-websocket / host-peer / host-mdns）各有一份实现，同一进程里只能
装一套（`HostModule::fallback` 让位）⇒ 移动真插件只能跑在桌面宿主实现上 ⇒ 涉及
移动 host-http/websocket 语义的 5 个场景 `#[ignore]`（见 `terminal_ws_flow.rs` 头注）。

**两进程形态把这堵墙拆掉**：桌面侧拆成独立进程（`desktop-harness` bin：真 AppContext +
真认证中心插件 + 真 Actix + 真 PTY，stdin/stdout JSON-RPC 控制面），移动侧测试进程
只链移动包 ⇒ 移动宿主域实现无竞争安装 ⇒ 移动插件的 WS / 页面窄转发走**移动生产实现**。

```bash
# 前置（两进程场景额外）：先建 harness 二进制（default 模式；两进程模式不构建它）
cd cross-end-tests && cargo build -p bedcode-crossend-desktop-harness

# 两进程场景 = 本包 `two-proc` 构建模式：desktop-lib 是 optional 依赖、被 feature
# 整体出图 ⇒ server-crate 的 common-host 无人开启 ⇒ 移动 whitelist 干净。
# 只跑 cross-end-tests（`-p`）：连桌面侧成员一起构建会 feature 统一而复现污染。
cd cross-end-tests \
  && cargo test -p cross-end-tests --no-default-features --features two-proc \
     --test two_proc_terminal_ws --test two_proc_selfcheck -- --nocapture
```

- 场景：`two_proc_terminal_ws`（C-001..C-009：桌面命令面建会话 → 移动插件真 WS 链路
  prewarm/subscribe/send-input/停止传播 → 页面窄转发零丢弃 + 背压压力路径）
- 自检：`two_proc_selfcheck`（装配守卫 + spawn→READY→rpc→shutdown）
- 移动插件 host-http 面**无头不可测**（egress 闸门对无 AppHandle 上下文有意 fail-closed，
  `host_domains/http.rs` 有用例钉着；`tauri::AppHandle` 泛型造不出 mock）——会话生命周期
  走桌面命令面 `session.ws.control`（生产同一实现），移动 HTTP wire 契约由激活场景
  `event_channel_flow.rs`（reqwest）+ 移动单端 `session_http_flow.rs`（就地端口 impl）承接。
  安全相关待裁决项见 spec §5.1（L1 桌面端目标在无头 CI 下是否放行）。

## 为什么需要它

两端各自的集成测试都用**协议级 mock 对端**：

| 端 | 真实侧 | mock 侧 |
|---|---|---|
| 桌面 | Actix 服务器（`src-tauri/tests/pty_session_chain.rs` 等） | 通用 reqwest / tokio-tungstenite 客户端 |
| 移动 | 移动 wasm 插件产物（终端协议 / 会话 HTTP / 认证编排）+ 宿主传输面 | 假桌面服务器（`tests/common/mock_plugin_ws` 一类夹具） |

两套 mock 各自自洽。若文档或任一端实现偏离，**两端测试全绿但真实互连必坏**。
本工程消灭这个盲区：请求字节由移动端真实客户端（真插件）生成，应答字节由桌面真实插件生成。

> **守门意义（2026-10-15 实测）**：终端链路下沉 wasm 插件后，本套件曾因引用退役的宿主
> `terminal_link` 而不编译、且未接入 CI —— 期间「重放前不清屏 → 移动端整屏重复」类
> 回归从两端 CI 底下派过。现在本包接入 `test.yml`（job `cross-end`），且移动侧夹具统一走
> `common/mobile_plugin.rs`（真产物 + 无头宿主，见下）。

## 结构

```text
cross-end-tests/
├── Cargo.toml              # 独立包；同时依赖两端 lib（bedcode-desktop-lib / bedcode-mobile-lib）
├── desktop-harness/        # 两进程形态的桌面进程（workspace 成员；依赖图只含桌面侧）
│   ├── src/lib.rs          # 装配面（desktop_ctx 同构）+ rpc_dispatch + 装配守卫
│   └── src/main.rs         # READY <port> + stdin/stdout JSON-RPC 控制面循环
├── src/lib.rs              # 空 lib（crate 级说明）
└── tests/                  # 单进程 rig 场景（default 模式）+ 两进程场景（two-proc 模式）：
    ├── common/             # desktop_ctx（桌面无头装配）/ mobile_ctx / mobile_plugin（移动真产物世界）
    ├── two_proc_common/    # 两进程：desktop 驱动（spawn harness + JSON-RPC）；mobile_* 复用 common/
    ├── two_proc_selfcheck.rs    # 两进程台子自检（装配守卫 + spawn→READY→rpc→shutdown）
    ├── two_proc_terminal_ws.rs # 终端闭环两进程版（C-001..C-009）
    ├── harness_selfcheck.rs    # 台子自检
    ├── pairing_auth_flow.rs    # 配对 / QR / 重认证
    ├── jwt_rotate_reconnect.rs # 密钥环轮换宽限期（# [ignore]：单进程装不下两端宿主域实现）
    ├── session_http_flow.rs    # 会话 HTTP 面（# [ignore]，同因）
    ├── terminal_ws_flow.rs     # 终端闭环（# [ignore]；两进程版 = two_proc_terminal_ws）
    ├── fail_closed_flow.rs     # fail-closed（# [ignore]，同因）
    ├── lifecycle_flow.rs       # 停用/激活联动（# [ignore]，同因）
    ├── event_channel_flow.rs   # 事件通道 session-control（激活）
    ├── http_proxy_flow.rs      # HTTP 代理面 + 链路加密信封（激活）
    └── mdns_health_flow.rs     # mDNS 发现/广播 + /api/health（激活）
```

**两种构建模式（Cargo features，见 Cargo.toml `[features]`）**：`default = ["desktop"]`
跑单进程场景；`--no-default-features --features two-proc` 把 desktop-lib（optional 依赖）
整体出图，跑两进程场景。两模式互斥无法被 cargo 强制，混用会被
`two_proc_selfcheck` 的装配守卫显性拦下。

**移动侧夹具口径（`common/mobile_plugin.rs`）**：加载 `bedcode-mobile/src-tauri/resources/plugins/mobile/**/*.wasm`
（`pnpm run plugins:build` 落点）→ `MockPorts` 只供给端事实（主连接目标 = 本进程桌面服务器、
token = 配对真发）→ 真实宿主面（WS 客户端引擎 / HTTP egress / 页面窄转发 / 权限判定）全部真实现 →
测试经插件命令面（`terminal-session.*`）驱动，经 `get-state` 与页面 Channel 观测。

> 已删除的旧场景：`terminal_output_pressure.rs` / `terminal_resync_ordering.rs`（驱动退役的
> 宿主 `terminal_link`）。其中压力/连续性契约并入 `terminal_ws_flow.rs` 的 C-005/C-009；“重锚
> 信号先于重放字节”类契约在无头 harness 不可观测（插件事件出口需真 AppHandle），消费侧契约
> 由移动端前端 vitest（`wasm-apps/bedcode/src/terminal/__tests__/store.test.ts`）承接。

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
