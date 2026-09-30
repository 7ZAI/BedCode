# 跨端真实互连集成测试方案（移动端 ↔ 桌面端）

> 状态: **implemented**（2026-09-30 实施完成；实施记录与偏差见 `issues/01-07`）
> 日期: 2026-09-30
> 决策记录（2026-09-30，用户确认）：
> 1. 形态选 **A（单进程真实互连）**，B/C 仅记录不实施
> 2. 两端 lib 改名根治同名冲突：`bedcode_lib` → `bedcode_desktop_lib`（桌面）/ `bedcode_mobile_lib`（移动）——作为 **L0 前置步骤**（§3-A 关键工程点 1 + §7）
> 前置文档: `归档文档/2026-08-16-desktop-integration-tests/spec.md`（桌面 L1/L2，已完成）、
> `归档文档/2026-08-16-mobile-integration-tests/spec.md`（移动 L1/L2，已完成）、
> `归档文档/2026-09-09-desktop-e2e-webdriver/spec.md`（桌面 WebDriver E2E，ready-for-agent）、
> `docs/knowledge/mobile-desktop-auth.md`（认证/协议单一事实源）、ADR 0031/0033/0034
> 实施后的三处偏差（一律以实施记录为准）：① §3-A2 / §6-4「fixture 构建机制跨工程不可引用」**不成立**——认证中心是真实 wasm 应用产物（随包目录，路径引用即可）；② §6-7「轮换需宿主命令面接线否则标 blocked」**不成立**——轮换触发面是插件互调 `auth-grant`，已真被覆盖；③ §5「场景 5 三种 deny_kind 各一」**跨端不可观测**——`deny_kind` 是宿主日志字段不是 wire 字段（客户端一律 401，这是有意的）。另 §6-1「Android 无 libbedcode_lib 硬编码」实查为假：手工保留且入库的 `android-backup/.../generated/Rust.kt` 需同步改名。
> **§5 / §7 的「可进 CI（test.yml 增 job）」未实施**（2026-09-30 用户定案：维持现有 CI，按需手动跑）。`test.yml` 保持接入前原样；跨端契约漂移改由 AGENTS §10 的「改跨端协议必须跑 cross-end-tests」人工门禁兜。

---

## 1. 问题陈述（为什么需要「真实互连」）

现状是**两端各自用协议级 mock 对端**验证 wire 契约（desktop-integration-tests L1 = 真实服务器 + 通用客户端；mobile-integration-tests L1 = mock 桌面服务器 + 移动端真实客户端）：

| 端 | 真实侧 | mock 侧 |
|---|---|---|
| 桌面 | Actix 服务器（`server_integration` / `pty_session_chain` / `ws_auth_rules`） | 通用 reqwest / tokio-tungstenite 客户端（**不是移动端代码**） |
| 移动 | AuthHttpClient / SessionHttpClient / TerminalLinkManager / WsClient | 协议级 mock 桌面服务器（`tests/common/mod.rs`） |

**盲区**：没有任何一个测试让「桌面端真实服务器」与「移动端真实客户端代码」在无 mock 情况下互连。由此产生两类只有真实互连才能暴露的缺陷：

1. **双向契约失真**：桌面端 mock 移动端按自己理解构造帧，移动端 mock 桌面端按文档/自身 `Message` 枚举构造应答——两套 mock 各自自洽，若文档与任一端实现偏离，**两端测试全绿但真实互连必坏**（移动 spec L2 落地记录 4「重连 bug 测试全绿下存活」正是这类断链的先例）。
2. **时序/生命周期联调盲区**：真实链路里配对 → 认证 → 会话 → 终端流的跨端时序、桌面端插件激活/停用对移动端连接的影响、JWT 轮换宽限期（ADR 0033 密钥环）等，mock 无法覆盖。

移动 spec 的 L3 曾将「双端联动 E2E」列为不优先（需 adb 模拟器 + 真实实例），但**那只是针对「真机/模拟器双进程」的重形态**。本方案证明存在一条轻量路径——**同一测试进程内，桌面端真实服务器 + 移动端真实客户端代码互连**，零 adb、零 WebView、`cargo test` 直接可跑、可进 CI。

---

## 2. 可行性依据（2026-09-30 代码现状核对）

### 2.1 桌面端：服务器可无头启动（先例已存在）

- `server/core/app.rs::start_http_server(port, &NetworkConfig)` 是**纯 Rust 函数，不依赖 AppHandle**，返回 `ServerHandle` + server future，可进程内启动、优雅停机
- `AppContext` 进程级 `OnceLock`，**无头模式先例已落地**：`tests/pty_session_chain.rs` 用
  `AppContextBuilder::new()...app_handle(None)` 组装真实服务，并**激活真实认证中心插件产物**（terminal-session fixture WASM）——`/api/auth/*`、WS 插件端点、PTY 编排全部真实运行
- 认证裁决 fail-closed（ADR 0031）：无中心在册即全拒；正向认证路径已在 pty_session_chain 跑通

### 2.2 移动端：客户端层可无 AppHandle 构造

| 组件 | 构造方式 | AppHandle 依赖 | 证据 |
|---|---|---|---|
| `ConnectionManager` | `new()` 公开轻量构造；`connect_without_emit()` 测试专用入口 | 无（emit 走 Option） | `connection/manager.rs:125,238` |
| `AuthHttpClient`（`auth/http.rs`） | 依赖 ConnectionManager + timeouts | **无** | `use crate::connection::manager::ConnectionManager` |
| `SessionHttpClient`（`session/http.rs`） | 依赖 ConnectionManager + `state::get_global_token` | **无** | 同上 |
| `TerminalLinkManager`（`terminal_link.rs`） | 事件已 trait 化（`TerminalEventSink`），测试注入记录替身 | 无（生产 AppHandle impl 该 trait） | `terminal_link.rs:72-82` |
| `WsClient`（`connection/ws_client.rs`） | 本地 TcpListener + accept_async 模式已有单测先例 | 无 | 移动 L1 基建 |

移动端 `state.rs` 的全局 token / manager 单例是进程级 `OnceLock`+`RwLock`，测试可用 `set_global_token` / `clear_global_token` 控制（移动 tests 惯例已确立）。

### 2.3 协议面现状（本方案要连的真实 wire）

- **HTTP**（移动端消费，桌面插件提供）：`/api/auth/*`（配对/验证/QR/JWT）、`/api/sessions/*`（start/stop/remove/input）、`/api/configs`；响应为 `{code,message,data?}` 信封
- **WS**（2026-09-26 硬切后只承载 `com.bedcode.terminal-session` 两条插件端点）：
  `/ws/plugin/com.bedcode.terminal-session/session-control` + `.../terminal`
  帧**永不加解密**（链路加密已退役，不得回接）；首消息 `{"type":"auth","token":...}` JWT 认证，无 auth_ok 回帧，首条业务帧可达即生效（`pty_session_chain.rs:179-184` 已确立）

---

## 3. 方案形态（推荐 A，备选 B/C）

### A. 单进程真实互连（**本方案主体**，L1）

新建**独立测试工程** `cross-end-tests/`（monorepo 根下第三个玩家），同一测试进程内：
- 进程内无头启动桌面端真实 Actix 服务器（OS 分配端口）+ 激活真实认证中心插件 fixture
- 用**移动端真实客户端代码**（AuthHttpClient / SessionHttpClient / TerminalLinkManager）连入
- 零 mock：不 mock 对端行为、不 mock invoke、不 mock 事件（TerminalEventSink 替身只记录，不伪造协议应答）

```
cross-end-tests/
├── Cargo.toml            # 独立包；依赖两端 crate（L0 改名后 lib 名天然不同，零重命名映射）
└── tests/
    ├── common/
    │   ├── mod.rs        # 桌面无头装配（AppContext + 认证中心 fixture）+ 移动端客户端装配
    │   └── desktop_ctx.rs# 复刻 pty_session_chain 的 AppContextBuilder 无头模式
    ├── pairing_auth_flow.rs   # 探测 → 配对 → 认证闭环
    ├── session_http_flow.rs   # 会话 CRUD + 输入（HTTP 面）
    ├── terminal_ws_flow.rs    # 终端流闭环（WS 面，真实 PTY 输出）
    └── reconnect_fail_flow.rs # JWT 重连 + fail-closed 失败路径
```

`Cargo.toml` 依赖声明（L0 之后）：

```toml
[dependencies]
# 依赖键 = lib name（- 转 _），代码内 use bedcode_desktop_lib:: / bedcode_mobile_lib:: 与 lib name 一致
bedcode-desktop-lib = { package = "bedcode-desktop", path = "../bedcode-desktop/src-tauri" }
bedcode-mobile-lib  = { package = "bedcode-mobile",  path = "../bedcode-mobile/src-tauri" }
```

**关键工程点**：

1. **L0 两端 lib 改名（前置，消除同名冲突）**：`bedcode_lib` → `bedcode_desktop_lib`（桌面）/ `bedcode_mobile_lib`（移动）。
   改名后 cross-end-tests 直接依赖两端，零重命名映射。**影响面已全部核实**（§6-1）：
   仅两端各自内部（src/tests/bench），无外部依赖方、无 Android 硬编码、无脚本/CI 硬编码。
   改名清单（L0 验收 = 两端 `cargo test` 全量绿 + 移动端 `gradlew` 编译通过）：
   - `[lib] name`：两端 Cargo.toml
   - 代码引用（机械替换）：桌面 `src/main.rs:47` `bedcode_lib::run()`、移动 `src/main.rs:47` 同；
     桌面 tests 8 文件 + bench 1 文件、移动 tests 6 文件的 `use bedcode_lib::`
   - **字符串常量连带（非机械替换，手工审查后改）**：
     a. 桌面 `src/system/config.rs` 4 处 EnvFilter 字符串 `bedcode_lib=debug` → `bedcode_desktop_lib=debug`
        （默认值 :393/:856 + 文档字符串 :49/:366）
     b. 桌面 `src/wasm_core/host_api/log.rs` 插件日志 target `bedcode_lib::wasm_core::plugin_log`
        → `bedcode_desktop_lib::...`（:27-62 常量 + :292 测试断言）；移动端对等位置同理
     c. 文档同步：`docs/knowledge/logging.md:20,28`、`plugin-wasm-logging.md:18`、
        `plugin-development-checklist.md:105`（target `bedcode_lib::plugin::plugin_log` 契约）、
        `docs/knowledge/build-process.md` 产物名表（libbedcode_lib.so/.dll/.dylib/.a，5 处）
   - **验证项**：tauri 产物命名跟随 lib name（Android jniLibs 的 .so 文件名、桌面 .so/.dll）、
     mobile 端 `./gradlew :app:compileUniversalDebugKotlin`、桌面 `pnpm run tauri:build` 冒烟（可选）
   - **已知行为变化（记录，勿静默）**：用户既有日志 EnvFilter 配置 `bedcode_lib=debug` 失效，
     需改新名（开发调试配置，非产品功能）；插件日志 target 变化影响日志过滤配置
2. **fixture 构建机制复用**：桌面端 wasm 闭环 fixture（terminal-session 产物 + wasip3 工具链，
   `RUSTUP_TOOLCHAIN=nightly-2026-09-16` 单一事实源 `scripts/wasip3-toolchain.sh`）当前在桌面端
   tests/ 内构建（cfg(test)），**跨工程不可直接引用**。处理：① 优先把 fixture 构建抽为脚本/共享
   模块（scripts 下）；② 或 cross-end-tests 引用桌面端 crate 的 pub fixture 构建辅助（需桌面端
   暴露 pub API）；③ 兜底：cross-end-tests 内复制构建逻辑（最不推荐，易漂移）。
3. **AppContext 进程级单例**：每个 test 二进制（独立进程）只 init 一次桌面端 AppContext；
   场景间串行（沿用 server_integration 教训：单测试函数 + 子场景，或静态锁）。
4. **移动端全局 token**：场景间 `clear_global_token()`；每次配对/认证后写入真实 token。
5. **CI 可跑**：无 adb / 无 WebView / 无 Xvfb，`cargo test --manifest-path cross-end-tests/Cargo.toml`
   直接进 test.yml（§7）。

### B. 双进程轻量 E2E（L2，后续）

桌面端真实二进制（或 headless 模式）跑一个进程；移动端客户端层打包成小 headless 二进制跑另一
进程，经真实 TCP 互联。仍无 adb，但引入进程编排（spawn/端口传递/超时）。比 A 更接近生产形态
（真实进程边界、真实 Tauri 启动），成本显著更高。**A 落地后评估**。

### C. 真机/模拟器 E2E（L3，记录不实施）

移动 spec 已明示不优先（需 adb + 桌面真实实例双端联动）。保持 backlog，不展开。

---

## 4. 场景设计（L1，按优先级）

每个场景 = 一个独立 test 二进制（进程隔离），内部单测试函数 + 串行子步骤（沿用既有教训）。

| # | 场景 | 驱动 | 断言要点 | 覆盖盲区 |
|---|------|------|---------|---------|
| 1 | HTTP 探测 + 配对认证闭环 | AuthHttpClient 真实连桌面无头服务器；桌面生成配对码 → 移动端 verify | 六端点 wire shape 真实往返；`{code,message,data}` 信封；session_token 真实可复验 | 双向契约失真（探测/配对面） |
| 2 | JWT 重连认证 | 场景 1 拿到的真实 token 重连（桌面侧 `enforce_connection_policy` 验签） | 重连成功、token 注入后续请求；**换 key 轮换宽限期**（ADR 0033：上一代密钥 7 天内仍可验签） | 轮换/重连时序 |
| 3 | 会话 HTTP 面 | SessionHttpClient：start / stop / remove / input | 真实会话登记（terminal-session 插件库）；specialKey 透传；业务码 1002 → Auth 错误映射 | HTTP 面契约失真 |
| 4 | 终端流 WS 闭环 | 移动端 TerminalLinkManager 连 session-control + terminal 两端点；HTTP POST input → 桌面真实 PTY（bash）→ WS 输出帧回放 | subscribe → 回放 → 实时裸字节 → ack 水位 → ring_resync 重锚 → session_stopped；**PTY 真实输出字节到达移动端** | 终端流跨端时序（最致命盲区） |
| 5 | fail-closed 失败路径 | 无中心在册 / 非法 token / 篡改帧 | 一律拒绝（`deny_kind` no_center/unavailable/policy），无降级路径；移动端收到显性错误 | 认证边界真实行为 |
| 6 | 停用/激活联动（可选） | 桌面端停用 terminal-session 插件 → 移动端连接行为 | 端点拒绝显性化（fail-visible 形态 ①），移动端状态回落 | 生命周期联调 |

> 场景 1/2/4 是 P0（覆盖「配对 → 认证 → 终端输出」主链路）；3/5/6 是 P1。
> 超时预算：参考 pty_session_chain 实测 0.34s 的量级，预算 5s 上限，CI 慢机放宽到 15s。

---

## 5. 验收标准

- [ ] **L0**：两端 lib 改名完成，桌面 `bedcode_desktop_lib` / 移动 `bedcode_mobile_lib`；两端 `cargo test`
  全量绿、移动端 `gradlew` 编译通过、产物名与日志 target 文档同步（build-process.md / logging.md /
  plugin-wasm-logging.md / plugin-development-checklist.md）；tauri 产物命名实测跟随
- [ ] `cross-end-tests` 工程存在，`cargo test` 全部场景真实往返（**无恒真断言**，每场景至少一次「移动端代码发出的请求 → 桌面端真实处理 → 响应回到移动端」断言）
- [ ] 场景 1/2/4 覆盖主链路：配对 → 认证 → 会话 → PTY 真实输出到达移动端
- [ ] 场景 5 验证 fail-closed：三种 `deny_kind` 至少各一
- [ ] 与两端既有全量测试并行无冲突（端口 0 分配 + 进程隔离 + 串行锁）
- [ ] 认证中心使用**真实 fixture 产物**（非 stub），wasmHash 注入链路有效
- [ ] 可进 CI：test.yml 增 job（或并入现有 desktop job），ubuntu-latest 直接可跑
- [ ] 发现既有 bug → 记入 `.scratch/test-coverage-bugs.md`（沿用惯例，不现场修复）
- [ ] 收尾：两端各自全量回归（`cargo test` + `pnpm run test:run`）+ 本工程全量；测试后清理残留进程/端口（AGENTS §3）

---

## 6. 风险与坑

1. **L0 改名（已定案，非风险项，以下为残余影响面结论）**：影响面已核实——
   - 无外部依赖方（wasm-apps / plugins / packages 均不依赖宿主 lib；rg 全仓核实）
   - Android 工程（gen/android）无 `libbedcode_lib.so` 硬编码；tauri.conf.json 无 mainBinaryName 覆盖
     （产物按 package 名/默认规则命名，lib name 变化由 tauri 构建自动跟随——**需 L0 实测确认**）
   - 无脚本/CI 硬编码（scripts/、.github/ 零引用）
   - 残余风险（低，均为可预期变化）：① 插件日志 target 契约变化（`bedcode_lib::plugin::plugin_log`
     → 新名），3 份文档同步 + 用户日志过滤配置需改；② 用户既有 EnvFilter 配置失效（开发调试配置）；
     ③ Windows manifest 注入（build.rs link-arg）作用于 package 链接产物，cross-end-tests 作为
     外部工程不受覆盖——CI（ubuntu）无此问题，本地 Windows 跑测试二进制需另行处理（记录即可）
   - 极端回退（改名受阻时）：仅改一端 lib name（如只改桌面端）即可消除同名冲突，影响面减半；
     或依赖键重命名 + 抽移动端客户端为独立 crate（precedent：`packages/link-crypto`、`packages/peer-net`）
2. **移动端 crate 在 Linux 上作为依赖编译**：CI 已证明 `cargo test`（移动端）可跑，但依赖引入会
   增加编译面（tauri/android 相关 feature 需核对默认关闭）；首次编译时间预计分钟级，接受。
3. **AppContext 单例 + fixture 激活**：复刻 pty_session_chain 的 `app_handle(None)` 模式；注意
   AppContext 的 `try_global()` 未初始化即 panic（server_integration 已锁此边界），跨端测试必须
   先装配。
4. **fixture 构建机制跨工程**（§3-A2）：当前在桌面端 tests/ 内 cfg(test) 构建，需抽取或暴露，
   否则 cross-end-tests 无法加载真实认证中心产物——**这是本方案第一个落地动作**。
5. **PTY 真实进程平台差异**：Linux 上 bash 可用（CI 同）；Windows 需 ConPTY + PATH（标注，CI 不涉及）。
6. **移动端事件订阅时序**：broadcast 无订阅者丢消息——先 subscribe 再触发；TerminalEventSink
   替身必须在连接建立前注入。
7. **轮换宽限期断言**（场景 2）：需要认证中心 `rotate-key` 互调 + 宿主命令面接线（ticket 05 未完成
   项）——若未接线，场景 2 降级为「单一密钥重连」断言，轮换部分标记 blocked。

---

## 7. 执行顺序

1. **L0 两端 lib 改名**（§3-A 关键工程点 1 清单）：改 `[lib] name` + 代码引用 + 字符串常量连带
   + 文档同步 → 两端 `cargo test` 全量回归 + 移动端 `gradlew` 编译验证 + tauri 产物命名实测
2. **Spike（已降级为编译验证）**：`cross-end-tests` 最小骨架同时依赖两端 crate 编译通过
   （`/tmp/cross-end-spike`）；验证移动端 AuthHttpClient 无 AppHandle 可构造、桌面端
   `start_http_server` 无头可启动
3. **基建**：`cross-end-tests` 骨架 + `common/desktop_ctx.rs`（AppContext 无头 + 认证中心 fixture
   加载，**优先抽取 fixture 构建机制**）+ `common` 移动端装配
4. **场景 1**（配对认证闭环）——打通首条真实链路
5. **场景 2**（JWT 重连）——含轮换宽限期（受 §6-7 阻塞条件约束）
6. **场景 4**（终端流 WS 闭环）——最致命盲区，真实 PTY 输出
7. **场景 3/5/6**（会话 HTTP / fail-closed / 生命周期）
8. **收尾**：全量回归（两端 + 跨端）、CI 接入评估（test.yml 新 job）、
   `docs/knowledge/mobile-desktop-auth.md` 补「跨端测试」章节、CHANGELOG 双语条目（如落地）

---

## 8. 与既有测试体系的关系（不重复建设）

| 既有设施 | 本方案 | 关系 |
|---|---|---|
| desktop L1（server_integration / pty_session_chain） | 桌面侧基建 | **复用**：AppContext 无头模式、fixture 加载、串行锁、`authenticate_endpoint` 帧模式 |
| mobile L1（tests/common/mod.rs mock 服务器） | 移动侧 | **保留**（单端快速回归仍需）；本方案是其「真实对端」补充，不替换 |
| mobile L2（前端事件驱动集成） | 前端 | 无关（本方案是 Rust 层互联） |
| desktop-e2e-webdriver spec | 桌面 E2E | 互补：它覆盖真实进程 IPC/UI（桌面单端），本方案覆盖跨端协议（无 UI）；共享 CI 门禁分层 |
| `unit-test-discipline` skill | 全部新测试 | **强制**：行为契约 → 测试矩阵 → G1-G6 → 变异自检 |

---

## 9. L0 改名变更清单（实施核对表）

| # | 文件 | 改动 | 类型 |
|---|---|---|---|
| 1 | `bedcode-desktop/src-tauri/Cargo.toml` | `[lib] name = "bedcode_desktop_lib"` | 机械 |
| 2 | `bedcode-mobile/src-tauri/Cargo.toml` | `[lib] name = "bedcode_mobile_lib"` | 机械 |
| 3 | 桌面 `src/main.rs:47` | `bedcode_lib::run()` → `bedcode_desktop_lib::run()` | 机械 |
| 4 | 移动 `src/main.rs:47` | `bedcode_lib::run()` → `bedcode_mobile_lib::run()` | 机械 |
| 5 | 桌面 tests 8 文件 + bench 1 文件 | `use bedcode_lib::` → `use bedcode_desktop_lib::` | 机械 |
| 6 | 移动 tests 6 文件 | `use bedcode_lib::` → `use bedcode_mobile_lib::` | 机械 |
| 7 | 桌面 `src/system/config.rs`（:49/:366/:393/:856） | EnvFilter 字符串 `bedcode_lib=debug` → `bedcode_desktop_lib=debug` | **手工审查** |
| 8 | 桌面 `src/wasm_core/host_api/log.rs`（:27-62/:292） | 插件日志 target 前缀 → `bedcode_desktop_lib::...` | **手工审查** |
| 9 | 移动端对等日志 target 文件（若有） | 同上 → `bedcode_mobile_lib::...` | **手工审查** |
| 10 | `docs/knowledge/build-process.md`（:272/:321-325/:393/:520） | 产物名表 `libbedcode_lib.*` → `libbedcode_desktop_lib.*` + `libbedcode_mobile_lib.*` | 手工 |
| 11 | `docs/knowledge/logging.md`（:20/:28）、`plugin-wasm-logging.md`（:18）、`plugin-development-checklist.md`（:105） | 日志 target 契约 → 新名 | 手工 |
| 12 | cross-end-tests `Cargo.toml` | 依赖键 = lib name（§3 示例） | 新增 |

> 实施时以 `rg -n bedcode_lib` 全仓复查为准（本表是 2026-09-30 核实快照）。
> 改名必须两端各自独立验证后再做 cross-end-tests——L0 是后续一切的地基。
