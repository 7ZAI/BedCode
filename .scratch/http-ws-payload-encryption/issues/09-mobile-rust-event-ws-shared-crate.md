# 09 — 移动端常驻事件 WS 的链路加密接入（Rust 侧）

**What to build:** 移动端 `/ws/event` 常驻连接实现在 `src-tauri/src/connection/event_ws.rs`（tokio-tungstenite + JWT 首消息），不在 WebView 内——issue 05/07 的 TS 实现覆盖不到。需要把 link-crypto 协议以**共享 crate**（建议 `packages/link-crypto` 或并入 bedcode-peer-net 同级）实现一次，桌面端 server 与移动端 event_ws 客户端共同消费，消除第三份协议实现；随后 event_ws 握手时携带 crypto 提案、派生会话密码、帧加解密。

**Blocked by:** 04, 05

**Status:** needs-triage

- [ ] 共享 crate 抽出协议核心，桌面端 server 切换为消费者且全部既有测试不红
- [ ] 移动端 event_ws 握手协商 + 帧/消息加解密
- [ ] strict 模式（allowPlaintextFallback=false）下 event WS 不再是明文旁路
- [ ] cargo test（双端）
