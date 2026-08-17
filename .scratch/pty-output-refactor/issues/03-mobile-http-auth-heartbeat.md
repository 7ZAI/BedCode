# 03 — 移动端 HTTP 认证客户端：配对/QR/生物/reauth + JWT 管理

**What to build:** 移动端 Rust 新增 reqwest（rustls），建立 HTTP 认证客户端：`POST /api/auth/{pairing,verify,qr-connect,biometric-challenge,biometric-verify,reauth}` 全流程；JWT 存于 `auth/manager.rs`（`get_global_token` 保留，D3）；连接生命周期重构：`connect_and_pair` 改为 HTTP 实现（认证成功后触发 04 的常驻事件 WS 建立），删除 WS 认证握手（`handler/auth.rs` 收敛为仅处理 `authenticated`/`failed` 回复）；新增 `get_ws_token()` / `get_ws_url()` 命令供前端按需开 WS。**无 presence 心跳任务**（设备在线由事件 WS 存活判定，见 D8）。

**Spec:** §4.5、§4.6（验收 1/2/3/4）

**Blocked by:** 01, 02

**Status:** ready-for-agent

- [ ] Cargo.toml 加 reqwest(rustls)；HTTP 客户端模块 + 错误映射 AppError
- [ ] AuthManager：配对/验码/QR/生物/reauth 的 HTTP 实现 + JWT 存取刷新
- [ ] `connection/manager.rs`：connect_and_pair 重构为 HTTP；WS 认证握手删除；lifecycle 状态机（新增 Authed）；认证成功后回调 04 的事件 WS 建立
- [ ] 命令：`get_ws_token` / `get_ws_url`
- [ ] 测试：HTTP 认证流程（可 mock 服务端或复用集成测试基建）、token 刷新

## Comments

- 2026-08-17 修订：**取消 presence 心跳任务**（原验收 6 的 TTL 语义删除），设备在线由 01/04 的事件 WS 存活判定承担。