# 跨端测试覆盖缺口补测方案（事件通道 / HTTP 代理加密面 / mDNS）

**日期**：2026-10-01
**前置工程**：`.scratch/2026-09-30-cross-end-integration-tests/`（跨端 rig 本体，已交付 8/8 绿）
**状态**：resolved · 2026-10-01（三组场景全部落地：票 01/02/03/04 全部 resolved；
`cross-end-tests` 8 → **11 个二进制全绿**）

---

## 1. 问题陈述

`cross-end-tests`（2026-09-30 交付）的 8 个二进制把「认证 → 会话 HTTP → 终端流」主干
钉死了，但**以移动端真实代码为准盘点后，还有三条移动端生产链路零跨端覆盖**——它们
全绿也不能证明真实互连成立：

1. **事件通道 `session-control`（7 类事件帧）**——移动端任务队列/会话模式/任务历史
   UI 的信号面，从未让一帧事件真实往返；
2. **HTTP 代理面 `http_request`（Egress + JWT 注入 + 链路加密信封）**——移动端前端
   所有 HTTP 的必经之路，测试客户端全走直连 reqwest 明文，绕过了它；
3. **mDNS 发现/广播 + `/api/health` 探测**——用户「发现并连上桌面」的入口，被
   `set_target` 直接接线绕过。

对应风险形态（AGENTS §8 fail-visible 判据）：真实互连断裂在测试全绿下长期存活——
「移动端 UI 收不到任务事件」「加密请求在桌面解密失败」「发现不了桌面」三类断裂，
现有套件一道也抓不住。

## 2. 现状覆盖矩阵（基于代码，非文档）

| 面 | 覆盖 | 证据 |
|---|---|---|
| 认证 HTTP（配对/QR/reauth/生物反例） | ✅ | `tests/pairing_auth_flow.rs` |
| fail-closed（无中心/伪造凭证，HTTP+WS） | ✅ | `tests/fail_closed_flow.rs` |
| 密钥环轮换宽限期（HTTP+WS） | ✅ | `tests/jwt_rotate_reconnect.rs` |
| 会话 HTTP（start/list/stop/remove/input + 1002 + remove 幂等） | ✅ | `tests/session_http_flow.rs` |
| 终端流 WS（订阅→真实 PTY→页面→终态） | ✅ | `tests/terminal_ws_flow.rs` |
| 输出压力（零缺口/重锚 fail-visible/背压） | ✅ | `tests/terminal_output_pressure.rs` |
| 生命周期（停用/重激活） | ✅ | `tests/lifecycle_flow.rs` |
| 台子（端点登记/停机不答） | ✅ | `tests/harness_selfcheck.rs` |
| **事件通道 session-control（7 类事件）** | ❌ | `rg 'event_ws|session-control' tests/` 零命中 |
| **http_request 代理面（含加密信封）** | ❌ | 测试客户端直连 reqwest（`mobile/.../auth/http.rs:301`、`session/http.rs:75`） |
| **mDNS 发现/广播** | ❌ | `desktop_ctx.rs:110` 只 `new` 未启动广播；移动端 discovery 未接线 |
| **/api/health 探测** | ❌ | 未断言响应形状 |
| peer-net 文件传输（移动端↔桌面 P2P） | ❌ | 见 §8 范围外 |
| 生物认证正向 / QR 扫码 UI | ❌ | 诚实边界（Android Keystore / UI 步骤），见 §8 |

## 3. 缺口详述（代码级证据）

### 3.1 事件通道 session-control

- 移动端解析面：`bedcode-mobile/src-tauri/src/handler/plugin_event.rs:39-52` 定义 7 类
  事件名（`session:created` / `session:stopped` / `session:removed` /
  `task:status-changed` / `session:mode-changed` / `task:queue-changed` /
  `task:scheduled-changed`）→ `to_mobile_event` → `MobileEvent` → 前端 `ws_sync_*`。
- 连接面：`bedcode-mobile/src-tauri/src/connection/event_ws.rs`——认证成功后
  supervisor 自动建连 `session-control` 端点 + 极简首帧 auth + 意外断开退避自愈。
  `run_supervisor(app_handle: Option<AppHandle>)`（`event_ws.rs:46`）**无头可建连**。
- 桌面广播面：`com.bedcode.terminal-session` 在声明端点 `session-control` 广播
  `{"type":"event","event":"<name>","payload":{...}}`（插件侧 `ws_event` / 任务域）。
- **现状**：cross-end-tests 只连过 `terminal` 端点；事件通道一次没跑过。

### 3.2 HTTP 代理面 http_request（Egress + JWT 注入 + 链路加密信封）

- 入口：`bedcode-mobile/src-tauri/src/commands/http_proxy.rs:173`
  `execute_proxy(request, app: Option<&AppHandle>)`——**命令与集成测试共用**，
  `app=None` 时仅 L3 弹窗路径 fail-closed，无头可驱动。
- Egress：`egress.rs:295 add_desktop_target(host, port)` 静态单例，无头可声明桌面目标；
  `kind="desktop"` 经 L1 校验（`http_proxy.rs:180-201`），非声明目标一律拒。
- JWT 注入：`http_proxy.rs:155 should_inject_jwt`（`/api/auth/*` 白名单除外）。
- 链路加密信封：`http_proxy.rs:264-283`——已 pin（`state::is_http_encryption_active`
  + KD 公钥）且非 auth 路径时：`generate_ephemeral` + `derive_http_keys(eph, kd, path)`
  + `encrypt_http_body(plain, aad=Inbound)` → 信封；桌面侧解密在
  `bedcode-desktop/packages/bedcode-server-core/src/link_crypto.rs`
  （`ln_body(&request_key, &ctx.data, &http_aad(Inbound, route))`）。
- 承载的生产端点（`bedcode-mobile/src/composables/useHttpApi.ts`，调用方：
  `useMobileConnection.ts` / `SessionsView.vue` / `plugin/context.ts` /
  `useTerminalResize.ts`）：`/api/sessions/{id}/resize`、`/api/configs`、
  `/api/quick-actions`、`/api/file-tree(-children)`、`/api/file-content`、
  `/api/diff-tree`、`/api/file-diff`、`/api/git/{branches,status,checkout}`、
  `/api/plugin/com.bedcode.terminal-session/{session-mode, task-queue/*,
  session-settings, task-history/*, supported-agents}`——**契约从未跨端跑过**；
  其中 **resize 无 Rust 侧客户端**（`session/http.rs` 只有残留检查
  `ws_resize_terminal`，见 :328），只能经代理面。

### 3.3 mDNS + /api/health

- 双端 service type 同值 `_bedcode._tcp.local.`（`bedcode-mobile/src-tauri/src/mdns/types.rs:7`）；
  移动端 `discovery.rs` 浏览，桌面 `MdnsAdvertiser` 广播。
- `desktop_ctx.rs` 只 `new` 了 advertiser 未 `start_advertise`；移动端 discovery 从未在
  rig 里跑（`set_target` 直连绕过）。
- `/api/health`：`useHttpApi.ts` `httpProbe()` 连接前探测（响应 `{status,port,uptime_secs}`），
  跨端从未断言。

## 4. 方案形态

**在现有 rig 上新增场景二进制，不改工程结构**（`AppContext` 进程级 OnceLock 单例，
每个场景独立测试二进制、进程隔离——既有约定照旧）。桌面装配复用
`desktop_ctx::init_app_context`（认证中心真实产物），移动端装配复用 `mobile_ctx`
（事件/输出记录替身口径：只记录已真实发生的数据，不伪造协议应答）。

补测的**请求字节仍由移动端真实客户端生成、应答由桌面真实插件生成**——与 L0 同一条
铁律，只是把客户端从「直连 reqwest 的 AuthHttpClient/SessionHttpClient」换成
「http_proxy 代理面」和「event_ws 事件通道」。

## 5. 场景设计（按优先级，每场景 = 一个独立测试二进制）

### 5.1 事件通道（最高优先：UI 主信号面）

`event_channel_flow.rs`：
- 前置：真实配对换 token → 启动 `event_ws` supervisor（app=None）→ 事件通道就绪
- E-001 会话生命周期事件：桌面经 `plugin_api_call`（`com.bedcode.terminal-session.*`
  互调，与桌面 UI 同一入口）驱动建会话/停会话/删会话 → 移动端收到
  `session:created` / `session:stopped` / `session:removed` 事件帧并解析为
  `MobileEvent`（断言载荷字段逐字：`session_id` / `status` / `source_device` 等）
- E-002 任务事件：驱动 `task:status-changed` / `task:queue-changed` /
  `task:scheduled-changed` / `session:mode-changed` 各一帧 → 断言载荷形状
  （`queue_count` / `action` / `task_status` / `auto_approve`）
- E-003 事件不重放契约：断连期间事件丢失、重连后由前端对账补齐——验证
  `ws_event_channel_ready` 在通道建立后发射（无头以事件替身承接）
- E-004 认证前帧丢弃：畸形/未认证首帧不进 `MobileEvent`（防呆防御路径）

### 5.2 HTTP 代理面 + 链路加密信封

`http_proxy_flow.rs`：
- P-001 Egress L1：`add_desktop_target` 声明桌面目标后，`execute_proxy(kind="desktop")`
  指向 rig 服务器 → 200；**未声明**目标 → `AppError::Egress` 显性拒绝
- P-002 JWT 注入：`/api/sessions` 带全局 token 注入 Bearer 放行；`/api/auth/*`
  白名单不注入；空 token 的 `/api/sessions` 拒绝
- P-003 链路加密信封：认证时 pin 桌面 KD 公钥（`state::update_link_crypto_pin`）+
  `enable_http_encryption` → `execute_proxy` 对 `/api/sessions` 发加密信封 → 桌面
  `link_crypto.rs` 真实解密 → 200 + 响应密文被移动端真实解密为明文（**双端真实
  加解密互连，含 AAD 路由绑定**）
- P-004 生产端点抽样契约：`/api/sessions/{id}/resize`（唯一无 Rust 客户端端点）、
  `/api/configs`、`/api/quick-actions`、`/api/plugin/.../task-queue/list` 至少各一条
  真实往返（响应 envelope `{code,message,data}` 形状断言，与黄金形状锁口径一致）
- P-005 取消：`http_cancel(request_id)` 打断在途请求（可选，若实现简单）

### 5.3 mDNS + health

`mdns_health_flow.rs`：
- M-001 桌面广播 ↔ 移动端发现：启动桌面 advertiser（`start_advertise` 无头可行）→
  移动端 `discovery` browse 同 `_bedcode._tcp.local.` → 发现到桌面记录
  （含 address/port/deviceName 字段）
- M-002 `/api/health`：移动端 `httpProbe` 真实往返，断言 `{status, port, uptime_secs}`
  形状与端口一致

## 6. 验收标准

1. 新增场景二进制在 `cross-end-tests && cargo test` 全绿；既有 8 个二进制不回归
2. 每条契约有行为断言（正例+反例，unit-test-discipline G1-G6），拒绝恒真/只测 mock
3. 无头装配口径与 `desktop_ctx`/`mobile_ctx` 既有约定一致（真实产物、显性失败）
4. `AGENTS.md` §3 黄金命令不变；`docs/knowledge/mobile-desktop-auth.md`
   「跨端真实互连测试」章表格同步新增三行
5. 收尾跑全量：宿主 cargo test / 移动 cargo test / 两端 vitest / cross-end /
   eslint 0 error（按 §10 DoD）

## 7. 风险与坑

- **事件通道首帧认证**：`event_ws` 极简首帧 `{"type":"auth","token":"<jwt>"}`，无加密
  提案、不等待回执；认证失败由宿主 close 4001 表达——测试须区分「通道没建」与
  「建了但被 4001 关」。
- **链路加密需要真实 KD 协商**：pin 来源于认证链（`auth/manager.rs:260`），测试须走
  真实配对流程拿 pin，不能手工造假公钥（假 pin 会让桌面解不开）。
- **加密只对「已 pin ∧ encrypt_http ∧ 非 auth 路径」生效**（`http_proxy.rs:161`）：
  断言前先确认真实开关状态，否则 P-003 可能变成「明文 200」的恒真。
- **桌面 advertiser 的 mDNS 组播**：无头可跑但依赖本机 5353/组播环境；若 CI 容器
  无组播，须按 AGENTS §10 写明原因而非静默 skip（与「缺产物显性失败」同口径）。
- **事件驱动时序**：事件通道是异步广播，断言用 `mobile_ctx::wait_event` 轮询口径
  （既有 `WAIT_TIMEOUT=15s` 宽松预算）。
- **进程级单例**：新增二进制各自独立进程（OnceLock 单例），场景间不互相污染；
  与既有 8 个二进制同口径。

## 8. 范围外（记录，不拆票）

| 项 | 理由 |
|---|---|
| peer-net 文件传输（移动端↔桌面 P2P） | 走 peer-net 引擎 mDNS+P2P 直连，**不经桌面 HTTP/WS 服务器**——现 rig（Actix HTTP/WS）结构上不含它；补它需把真实 peer-net 引擎接进 rig 或走 L2 双进程形态，单独立项 |
| 生物认证正向路径 | 移动端私钥在 Android Keystore，无头进程构造不出真设备密钥（README 既有诚实边界） |
| QR 桌面扫码确认 UI 步骤 | 直接走 `qr-code-generate` 互调（桌面 UI 同一入口）生成 token，扫码动作本身无法无头复现（README 既有诚实边界） |
| `deny_kind` 三态分类 | 宿主日志字段非 wire 字段，客户端一律 401；三态覆盖在宿主 `utils/auth/auth_center` 单测（README 既有边界） |

## 9. 待用户裁决点

> 2026-10-01：用户以「开工」授权开工，三组按本节优先级（事件通道 > 加密信封 > mDNS/health）
> 全部实施完毕。裁决点 2（peer-net 是否另立项）与裁决点 3（`http_cancel`）的处置：
> 前者仍未立项（§8 范围外），后者**已纳入**并覆盖（实现简单，见票 02 P-005）。

1. ~~§5 三组场景的优先级与范围是否认可~~ → 已按此顺序实施
2. peer-net 文件传输是否另行立项（§8 第一行）→ **仍待裁决**
3. ~~P-005 `http_cancel` 是否纳入~~ → 已纳入

---

## Comments

- 2026-10-01：覆盖面审计结论基于实际代码（非文档）：8 个二进制覆盖矩阵、三个零覆盖
  大面、证据位置全部经 rg/read 核实（见票 01/02/03 正文的 `文件:行` 引用）。
