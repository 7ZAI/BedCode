# 09 — 移动端常驻事件 WS 的链路加密接入（Rust 侧）

**What to build:** 移动端 `/ws/event` 常驻连接实现在 `src-tauri/src/connection/event_ws.rs`（tokio-tungstenite + JWT 首消息），不在 WebView 内——issue 05/07 的 TS 实现覆盖不到。需要把 link-crypto 协议以**共享 crate**（建议 `packages/link-crypto` 或并入 bedcode-peer-net 同级）实现一次，桌面端 server 与移动端 event_ws 客户端共同消费，消除第三份协议实现；随后 event_ws 握手时携带 crypto 提案、派生会话密码、帧加解密。

**Blocked by:** 04, 05

**Status:** ready-for-human（代码完成双端测试全绿；真机联调验证待执行）

- [x] 共享 crate 抽出协议核心，桌面端 server 切换为消费者且全部既有测试不红（packages/link-crypto；金样互验测试通过证明字节面无损）
- [x] 移动端 event_ws 握手协商 + 帧/消息加解密（establish_event_ws 提案/回执/派生/安装 + WsClient IO 钩子 seal/open）
- [x] strict 模式（allowPlaintextFallback=false）下 event WS 不再是明文旁路（桌面拒绝协商时 strict 断连报错、非 strict 明文续跑；安装失败一律断连）
- [x] cargo test（双端）：桌面 lib 550 + link_crypto_http 3 绿；移动 lib 299 + 集成全绿；移动 vitest 301 绿

> 实施备注：
> - 共享 crate 位置 `packages/link-crypto`（与 peer-net 同级，两端 path 依赖）
> - TS↔Rust 桥：`set_link_crypto_context` 命令（persist/pin 刷新/App 启动三时机推送，失败静默）
> - 语义分层：桌面未回执 = 服务端仍明文 → 非 strict 可续跑；回执后派生失败 = 桌面已注册密码表 → 必须断连
> - 真机联调项归入 issue 08 真机清单第 4 组（后台杀进程重连 reauth+rekey）一并执行
