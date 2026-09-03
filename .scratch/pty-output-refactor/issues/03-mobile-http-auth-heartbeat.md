# 03 — 移动端 HTTP 认证客户端：配对/QR/生物/reauth + JWT 管理

**What to build:** 移动端 Rust 新增 reqwest（rustls），建立 HTTP 认证客户端：`POST /api/auth/{pairing,verify,qr-connect,biometric-challenge,biometric-verify,reauth}` 全流程；JWT 存于 `auth/manager.rs`（`get_global_token` 保留，D3）；连接生命周期重构：`connect_and_pair` 改为 HTTP 实现（认证成功后触发 04 的常驻事件 WS 建立），删除 WS 认证握手（`handler/auth.rs` 收敛为仅处理 `authenticated`/`failed` 回复）；新增 `get_ws_token()` / `get_ws_url()` 命令供前端按需开 WS。**无 presence 心跳任务**（设备在线由事件 WS 存活判定，见 D8）。

**Spec:** §4.5、§4.6（验收 1/2/3/4）

**Blocked by:** 01, 02

**Status:** ready-for-agent

- [x] Cargo.toml 加 reqwest(rustls)；HTTP 客户端模块 + 错误映射 AppError
- [x] AuthManager：配对/验码/QR/生物/reauth 的 HTTP 实现 + JWT 存取刷新
- [x] `connection/manager.rs`：connect_and_pair 重构为 HTTP；WS 认证握手删除；lifecycle 状态机（新增 Authed）；认证成功后回调 04 的事件 WS 建立
- [x] 命令：`get_ws_token` / `get_ws_url`
- [x] 测试：HTTP 认证流程（可 mock 服务端或复用集成测试基建）、token 刷新

## Comments

- 2026-08-17 修订：**取消 presence 心跳任务**（原验收 6 的 TTL 语义删除），设备在线由 01/04 的事件 WS 存活判定承担。
- 2026-08-18 完成：HTTP 认证全链路落地（reqwest rustls+json、ApiEnvelope 解析、六端点方法、manager 六方法 HTTP 化、lifecycle Authed、connect 去 WS、connect_without_emit 保留测试路径、reconnect→HTTP reauth 退避、get_ws_token/get_ws_url）。03 与 04 须一起合入方可发布（当前会话/终端/配置 WS 命令 send 报 Not connected、生物 bind/unbind 暂不可用）。**04 衔接契口**：`AuthManager::apply_auth_success` 经 `ConnectionManager::event_tx()` 广播 `MobileEvent::AuthSuccess`（WS 回复路径由 `AuthHandler` Authenticated 分支补发 set_authed）；`establish_ws_client(addr,port,path)` 私有 helper 供 04 建事件 WS（`WS_EVENT_PATH=/ws/event` + `AuthRequest::reauthenticate` 首消息）。