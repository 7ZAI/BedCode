# 03 — HTTP 代理命令面 `http_proxy.rs`（http_request / http_cancel + JWT + 加密 + 超时 + 报文对齐）

**What to build:** 新增 `src-tauri/src/commands/http_proxy.rs`：通用 HTTP 代理命令 `http_request(request_id, method, url, headers, body, timeout_ms)` + `http_cancel(request_id)`。共享 `reqwest::Client`（State 持有，连接池复用）；JWT 注入（`/api/auth/*` 白名单除外）；链路加密信封（ticket 01 的 derive_http_keys + 既有 bedcode-link-crypto）；超时默认 30s（D5）；取消用 reqwest `AbortHandle`（State 存 `Mutex<HashMap<request_id, AbortHandle>>`，完成/取消/超时后 remove 防泄漏）；结构化日志带 `request_id` 字段（§8 日志红线）。

**Spec:** §3 目标架构、§5.5（调用机制借鉴、报文逐项对齐）、§5（兼容性 5 条）、§9 D1/D3/D5

**Blocked by:** 01, 02, 04

**Status:** done

## 前置决策：L1 与 httpProbe 时序（handoff §3.1，实现前先定，建议方案 a）

`useMobileConnection` 流程 = `setApiBaseUrl(address, port)` → `httpProbe(address, port)`（**此时 ConnectionManager.target 未设**，ws_connect 才 set_target）→ 连接。L1「目标 host:port = 当前 target」在 probe 阶段不命中。

- **方案 a（建议）**：`http_request` 加 `kind: "desktop"|"external"` 参数——desktop 类请求 Rust 校验 host ∈ {当前 target ∪ 最近一次 ws_connect/probe 目标}；阻止任意外网 URL 借 desktop 逃逸。
- 方案 b：httpProbe 走专用命令只放行 GET /api/health。
- 方案 c：其他。
- 按方案 a 实现；若实现中发现更优路径，先停下与用户确认，不擅自改。

## 关键实现事实（handoff §2/§3 已核实）

- JWT 注入源：全局 token `get_global_token()`；`/api/auth/*` 前缀不带 Bearer（spec §5 第 4 条）。
- HTTP 加密判定 = `enabled ∧ encrypt_http ∧ kd_public_b64.is_some()`（对齐前端 `isChannelEncryptionActive('http')`；encrypt_http 由 ticket 04 补进 state）；GET/HEAD 无 body 仍带协商头（与现状一致）；加密失败 fail-closed。
- **报文逐项对齐（§5.5 结论 2.3 / §5 第 5 条）**：对比现状 tauriFetch 报文——`Origin`（桌面端安全过滤可能依赖）、默认 `User-Agent`、POST/PUT 无 body 时 `Content-Length: 0`、Range 头时 `Accept-Encoding: identity`、forbidden headers（Connection/Cookie/Host/Origin/Referer + `proxy-*`/`sec-*` 前缀）丢弃；用捕获报文 diff 单测锁住。
- 响应形状（建议，可微调）：Rust 返回 `{status, statusText?, headers, body_text}`；前端 `request()` 保持 ApiResult 归一化（HTTP 错误 `code=status` / 网络错误 `code=-1` / 取消语义对齐）。
- pin 刷新收束（handoff §3.5）：HTTP auth 响应携带 kdPublicB64 → Rust 代理解析后 `update_link_crypto_pin` + emit `ws_link_crypto_pin` 给前端（前端 localStorage 保留作设置页展示，裁决在 Rust）。

## 实现清单

- [x] 共享 `reqwest::Client` 入 State + `http_request` / `http_cancel` 命令
- [x] L1 时序决策落地（方案 a：kind 参数 + 目标集合校验；或与用户确认后方案）
- [x] Egress 校验接入（调 ticket 02 egress.rs 判定；Deny → `EXTERNAL_URL_NOT_DECLARED`/`EXTERNAL_URL_DENIED`，请求不发）
- [x] JWT 注入（`/api/auth/*` 白名单）、链路加密信封（GET/HEAD 仍带协商头、失败 fail-closed）、pin 刷新收束
- [x] 超时 30s + AbortHandle 取消 + request_id map 清理（完成/取消/超时兜底，防膨胀）
- [x] 报文对齐（Origin/UA/Content-Length:0/Accept-Encoding:identity/forbidden headers）+ 报文 diff 单测
- [x] 结构化日志：`request_id = %value` 字段形式，消息不带字段拼接
- [x] `lib.rs` / `commands.rs` 注册
- [x] 单测：JWT 注入、auth 白名单、加密信封字节一致性、错误映射、超时、取消、并发 request_id 互不串扰（多路复用正确性）

## 验证

- `cargo test`（src-tauri）http_proxy 相关用例全绿（含并发多路复用单测）
- 报文 diff 单测锁定与现状 tauriFetch 报文一致

## Comments

- 2026-09-11 完成：`src/commands/http_proxy.rs`（`http_request` / `http_cancel`；核心 `execute_proxy(request, Option<&AppHandle>)` 供集成测试直接调用，app=None 时 L3 弹窗 fail-closed 拒绝）。关键决策与事实：
  - **L1 时序（方案 a 落地）**：kind=desktop 校验 `host:port ∈ desktop_targets` 且 scheme 必须 http（https 外网不得借 desktop 逃逸）；目标由前端 `egress_declare_desktop_target`（setApiBaseUrl 时调用）声明——覆盖 httpProbe 先于 ws_connect 的时序缺口。kind=external（默认）走 egress 全层判定 + 弹窗。
  - **响应协商头**：桌面端实测响应头是 `X-BedCode-Crypto: v1`（固定值，不带公钥——spec §4，前端 `=== 'v1'` 判定）；请求头 `v1 <ek_b64>`。响应加密复用请求协商的同一会话（不重新协商临时密钥）——集成测试曾误用新临时对导致 AEAD 失败，已按协议修正。
  - **报文对齐**：forbidden 头（connection/cookie/host/origin/referer/upgrade/keep-alive + proxy-*/sec-*）丢弃；POST/PUT 无 body 补 `Content-Length: 0`；Range → `Accept-Encoding: identity`；**不伪造 Origin**（桌面端 `allow_any_origin` 实测不校验，handoff 推测不成立）。
  - **pin 刷新收束**：auth 响应 `data.kdPublicB64` → `update_link_crypto_pin` + emit `ws_link_crypto_pin`（与既有事件形状一致）。
  - **取消**：oneshot + `tokio::select!`（借鉴 plugin-http 模式）；PENDING map 完成/取消/超时后 remove 防泄漏。
  - 共享 Client 带 `no_proxy()`（系统代理劫持局域网目标的实测教训，与 AuthHttpClient 一致）。
- 验证：纯函数 4 单测 + 集成测试 `tests/http_proxy_flow.rs` 7 用例全绿（JWT 注入/auth 白名单/加密信封字节级互通（mock 用 link-crypto 解密请求 + 加密响应）/报文对齐/并发 10 请求/取消/Egress fail-closed）；全量 292 passed。
- 坑：① `#[tokio::test]` 独立 current_thread runtime 会 abort 测试内 spawn 的 actix server → mock server 改跑独立线程 multi_thread runtime 常驻；② actix `web::Json` 提取器要求 Content-Type → mock 用 Bytes；③ 测试全局 state 污染用 `static SERIAL: std::sync::Mutex` 串行化。
