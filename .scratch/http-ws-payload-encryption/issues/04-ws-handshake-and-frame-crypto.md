# 04 — WS 半边：握手扩展与终端/事件通道帧加解密

**What to build:** `terminal_ws.rs` 两类通道（session 终端 + event）的首消息 auth 协议扩展可选字段 `crypto:{v:1, ek}`：服务器校验 JWT 后生成新鲜 s_eph，`auth_ok` 回带 `crypto:{v:1, ek}`；双 ECDH 派生——master IKM = `ECDH(m_eph, s_eph) || ECDH(m_eph, Kd)`，transcript salt = `"bc-link-crypto/v1" || m_ek_b64 || s_ek_b64`，expand 出 c2s/s2c 各 32B key + 4B nonce prefix（复用 01 的派生函数）。会话密码上下文按连接存取（actor 内持有或全局表 keyed by addr），供既有四个过滤 hook 消费：text 帧 ↔ JSON 信封（类型保持原则）、binary 帧 ↔ `[ver u8][seq u32][ct]`，nonce = `prefix || u64BE(seq)` 严格递增校验。**解密失败语义从「丢帧」升级为 Close(4003) 关连接**（出站同理）——需调整现有 hook 的 Reject 分支行为并保留 warn 日志。未带 crypto 字段维持明文现状（老客户端兼容）；WsLocal 豁免由 01 兜底。

参考：spec「WS 会话加密」「失败语义」节；心跳 Ping/Pong 与 Close 不过滤不加密。

**Blocked by:** 01

**Status:** ready-for-agent

- [ ] 集成测试：协商握手 → text/binary 加密帧双向往返 → 对端解密得原文（参考 tests/ws_auth_rules 风格）
- [ ] 无 crypto 字段的老客户端全流程回归通过（既有 ws 测试不红）
- [ ] 篡改帧 → 连接以 4003 close code 关闭并有 warn 日志，非静默丢帧
- [ ] seq 乱序 / 重复帧 → 关连接
- [ ] 事件通道广播路径同样加解密（websocket_manager 广播出站口验证）
- [ ] Rust 侧固定金样向量产出（供 05 移动端交叉验证）
- [ ] `cargo test --lib` 全绿

## Comments

- 2026-08-26 实现：FilterContext 增加 negotiation 字段传递 HTTP 协商头（偏离 spec「不改 trait」表述——最小可选字段扩展）；WS 二进制帧 seq 采用 **u64**（spec 草案写 u32，长会话计数余量更足）。编译/测试验证随分支统一进行。
