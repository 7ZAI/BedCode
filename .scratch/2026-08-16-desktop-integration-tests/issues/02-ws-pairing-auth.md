# 02 — WS 配对与认证链路

**What to build:** 移动端配对体验的服务端半边在真实网络链路上可用：测试内启动服务器后，用真实 WebSocket 客户端模拟移动端完成「请求配对 → 收到配对码 → 验证配对码 → 认证成功拿到会话 token → 服务端注册为已认证设备」的完整流程；同时验证未认证连接无法执行需要认证的业务操作（被拒且不进入已认证集合）。

**Blocked by:** 01 — 集成测试基建 + HTTP 契约

**Status:** resolved

- [x] 真实 WS 连接发出配对请求（协议消息对齐服务端 `Message` 结构）后收到配对码响应
- [x] 验证配对码后收到认证成功响应，含会话 token（非空）
- [x] 认证完成后服务器已注册该客户端且标记为已认证（注册表可查询）
- [x] 未认证连接发送业务消息被拒绝，且不会出现在已认证客户端列表中
- [x] 错误配对码被拒并返回明确错误（不产生已认证客户端）

## Answer

实现于 cdcc91184（2026-08-16）。配套 prefactor：`AppContext.app_handle` Option 化（Wry 类型不可 mock）全仓 16 处适配 + `PluginHost::new` Option 化；生产路径行为不变（lib 538 无回归）。

`tests/ws_pairing_auth.rs`（独立测试二进制，进程隔离）：单测试函数 4 场景串行——配对全链路（RequestPairing→VerifyCode→Authenticated+非空 token）、认证后 registry 注册（authenticated=true）、未认证拒绝（AUTH_REQUIRED）、错误配对码拒绝。服务全部真实实现 + 内存 SQLite，零新增依赖。

**与 ticket 措辞的差异**：配对码不在 VerifyCode 响应中返回（生产经 `pairing-code-generated` 前端事件投递，无头模式跳过）——测试改从 `PairingService::get_current_code()` 读取共享实例，文件头已注释。
