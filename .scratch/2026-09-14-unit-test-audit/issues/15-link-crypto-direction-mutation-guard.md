# 15 — link_crypto 方向隔离变异守卫（变异测试 23/24 漏过）

**What to build:** 为 `derive_http_traffic_keys` 补方向隔离金样向量，使密钥方向混淆可被变异测试捕获，而非仅依赖 roundtrip 自洽性。

**Blocked by:** 无

**Status:** done（2026-09-15）

- [ ] `derive_http_traffic_keys` 金样向量：固定 IKM `= [7u8; 32]` + path `= "/api/sessions"` → 断言 `request` 密钥精确 hex
- [ ] 同上断言 `response` 密钥精确 hex
- [ ] `derive_ws_session_ciphers` 金样向量：固定 client_eph 公钥 → 断言 c2s/s2c 36B 上下文精确 hex
- [ ] 金样向量测试注释标注「移动端 TS 复刻必须逐字节一致」（协议兼容锚点）
- [ ] 变异验证：交换 `HTTP_INFO_REQUEST`/`HTTP_INFO_RESPONSE` 后金样测试失败
- [ ] `cargo test --lib server::link_crypto` 通过

## 证据

变异测试：`src/server/link_crypto.rs:311-312` 交换 `HTTP_INFO_REQUEST` 与 `HTTP_INFO_RESPONSE`：

```bash
python3 -c "
p='src/server/link_crypto.rs'; s=open(p).read()
s=s.replace('HTTP_INFO_REQUEST, 32)', 'HTTP_INFO_RESPONSE, 32)')
s=s.replace('HTTP_INFO_RESPONSE, 32)', 'HTTP_INFO_REQUEST, 32)')
open(p,'w').write(s)"
cargo test --lib server::   # → 24 个 link_crypto 测试仅 1 个失败
```

**漏过的测试**（23 个全绿）：
- `http_codec_roundtrip_and_tamper_rejection` — roundtrip，加密解密用同一 `HttpTrafficKeys` 对象，swap 后自洽
- `http_full_request_response_cycle_through_filter` — 完整链路，两端各自派生（同一被污染函数），结果一致
- `http_fail_closed_paths` — 失败路径不依赖方向正确性
- `http_get_with_empty_body_negotiates_and_encrypts_response` — GET 空 body 路径自洽

唯一守卫：`http_keys_deterministic_and_direction_isolated` 的 `assert_ne!(a.request, a.response)`。

## 根因

Roundtrip 测试验证「加密解密自洽」而非「方向语义正确」。`assert_ne!(a.request, a.response)` 仅检查「两方向不同」而非「request 用 request info 派生」。若两个 info 常量被同时交换，`request ≠ response` 仍成立（只是语义反转），不等断言通过。

金样向量是唯一能捕获此变异的守卫——固定输入锁死输出字节，任何 info 常量改动都会改变派生结果。

## 与票据 11 的关系

票据 11（前次会话）聚焦「断言弱点」（WS fallback 分支、幂等命名），本票据聚焦「方向隔离变异守卫」。两者互补：11 加固现有断言，15 补金样向量。若票据 11 已含金样向量方案，可合并。

## 修复方向

参考已有 `ws_handshake_interop_with_client_replication`（`link_crypto.rs:1108`）的「移动端 TS 复刻必须逐字节一致」模式：

```rust
#[test]
fn http_keys_gold_vector_locked() {
    let ikm = [7u8; 32];
    let keys = derive_http_traffic_keys(&ikm, "/api/sessions").unwrap();
    // 锁死精确字节（协议兼容锚点）
    assert_eq!(hex::encode(keys.request), "DEADBEEF...");
    assert_eq!(hex::encode(keys.response), "CAFEBABE...");
}
```

## 影响面

仅新增测试，零生产代码改动。金样向量测试锁定协议兼容性——若未来改 HKDF 参数，金样测试会失败，提示需同步更新移动端 TS 实现。

## Comments

- 2026-09-14 审计发现（第二轮，变异测试），见 `../http-ws-spec.md` §5.2
- 关联：`link_crypto.rs:61` 注释「HKDF info 常量——协议兼容性表面，issue 05 移动端 TS 实现必须逐字节一致」
- 变异体已 `cp /tmp/link_crypto.rs.bak` 完全回滚，`git diff --stat` 确认为空
