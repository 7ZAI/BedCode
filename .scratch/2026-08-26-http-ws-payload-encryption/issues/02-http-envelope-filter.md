# 02 — HTTP 半边：信封协议与请求级响应密钥缓存

**What to build:** 在 01 的过滤器骨架内实现 spec「HTTP 单发加密」全流程。入站：识别请求头 `X-BedCode-Crypto: v1 <ek_b64>` → `shared = ECDH(Kd_priv, ek)` → `salt = ASCII(path)` 派生 k_req/k_resp（info 区分方向）→ 解信封 `{v,n,ct}`（随机 12B nonce，AAD = `"v1" || direction || path_len || path`）→ handler 收到明文。出站：用 k_resp 加密响应体为同格式信封，响应头回 `X-BedCode-Crypto: v1` 标记。核心难点：filter 是全局单例、出入站两次独立调用，而 k_resp 只能从该请求的 ek 派生——过滤器内部维护并发安全短 TTL 缓存 `(peer_addr, ek_b64) → {k_req, k_resp, path}`，入站写入、出站命中即删、30s 清扫兜底。解密失败走既有 Reject → 400 通路。`/api/auth/*` 本期保持明文（pinning 引导，Kd 下发在 03）。

参考：spec「HTTP 单发加密」「实现要点（请求级状态传递）」节；`middleware/http_filter.rs` 已负责整体缓冲与 Reject→400，无需改动。

**Blocked by:** 01

**Status:** ready-for-agent

- [ ] actix 集成测试（沿 `GLOBAL_CHAIN_LOCK` 串行惯例）：加密请求 → handler 断言收到明文 → 响应体可用 k_resp 解出原文
- [ ] 缺头 / 坏信封 / 篡改 ct → 400 且 handler 未触达，拒绝详情含过滤器名
- [ ] AAD 路径绑定：/a 端点的信封投递到 /b → 解密失败
- [ ] 缓存并发测试（模拟多 worker 并发互不串键）与 TTL 清扫单测；命中即删防重放复用
- [ ] nonce 随机性抽查（同一派生上下文两个信封 nonce 不同）
- [ ] `cargo test --lib` 全绿

## Comments

- 2026-08-26 实现：FilterContext 增加 negotiation 字段传递 HTTP 协商头（偏离 spec「不改 trait」表述——最小可选字段扩展）；WS 二进制帧 seq 采用 **u64**（spec 草案写 u32，长会话计数余量更足）。编译/测试验证随分支统一进行。
