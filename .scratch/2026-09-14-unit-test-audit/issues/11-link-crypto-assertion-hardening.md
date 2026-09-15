# 11 — link_crypto.rs 断言弱点加固（WS fallback + 金样向量 + 幂等）

**What to build:** `link_crypto.rs` 有 24 个测试，整体是工业级质量，但存在 3 处严重断言弱点：(a) WS 帧的 `!ws_has_ciphers` 分支（明文 fallback 语义）生产代码仅 2 处调用 `should_process`（`on_inbound` / `on_outbound` 各自 1 处），且 `on_inbound/on_outbound` 本身从未被测试触达；(b) `http_keys_deterministic_and_direction_isolated` 无金样向量，只测自洽；(c) `registration_into_fresh_chain_is_named_and_idempotent_per_instance` 名为"幂等"但只调用一次 register。补强这些断言。

**Blocked by:** 无

**Status:** done（2026-09-15）

- [ ] 新增 `test_ws_frame_no_ciphers_fallback_branch`：peer 未注册 ciphers + `allow_plaintext_fallback=true` → `Continue` 且 `ctx.data` 不变；`allow_plaintext_fallback=false` → `Reject` 且 `msg.contains("allow_plaintext_fallback")`
- [ ] 新增 `test_derive_http_traffic_keys_matches_golden_vector`：为 `derive_http_traffic_keys(&ikm, "/api/sessions")` 增加 `assert_eq!(hex::encode(&a.request), "<来自移动端 linkCrypto.ts 的金样 hex>")`，金样与共享 crate 的 `derive_http_keys_matches_frontend_golden` 保持一致
- [ ] 改名为 `test_sync_registration_is_idempotent`：新增 `sync_registration(); sync_registration(); chain.list_names()` 只出现一次的断言
- [ ] 新增 `test_identity_corrupt_file_refuses_regeneration_verifies_file_unchanged`：额外断言错误后 `dir.join(IDENTITY_FILE)` 仍为原 corrupt 内容（未重建）
- [ ] 新增 `test_request_key_cache_capacity_eviction`：容量护栏回归测试，超 `HTTP_KEY_CACHE_MAX=1024` 时逐出最早项
- [ ] 新增 `test_request_key_cache_ttl_sweep_on_store`：TTL 清扫路径在 store 时触发
- [ ] `cargo test --lib server::link_crypto::` 通过

## 证据

- `link_crypto.rs:699-705` WS 帧的 `!ws_has_ciphers` 分支（明文 fallback 语义）：生产代码仅 2 处调用 `should_process`（`on_inbound` :841 / `on_outbound` :851）；测试另有 10+ 处直接调 `should_process`（:1223-1300，覆盖环回/白名单/子开关），但 `on_inbound` / `on_outbound` 本身从未被测试触达
- `link_crypto.rs:1060-1070` `identity_corrupt_file_refuses_regeneration` 只断言错误消息 `contains("corrupt") || contains("refusing")`，未断言文件状态未重建
- `link_crypto.rs:1084-1096` `http_keys_deterministic_and_direction_isolated` 无金样向量
- `link_crypto.rs:1304-1314` `registration_into_fresh_chain_is_named_and_idempotent_per_instance` 名为"幂等"但只调用一次 register
- `link_crypto.rs:1356-1377` `request_key_cache_is_take_once_and_ttl_bounded` 只测纯函数，`store_http_keys` 的容量护栏（`HTTP_KEY_CACHE_MAX=1024` :598）与 TTL 清扫路径零覆盖

## 根因

24 个测试中，3 处断言弱点：
1. WS fallback 语义在测试层完全空白（开发者删除/篡改这段代码 CI 仍然全绿）
2. 金样向量缺失导致 HKDF 参数被误改 CI 仍绿
3. 幂等语义是 `sync_registration` 里 `if !REGISTERED.swap(true)` 唯一未被测试的机制

## 修复方向

1. 为 WS fallback 补正向 + 负向测试
2. 加金样向量（与共享 crate 保持一致）
3. 改名并补幂等断言
4. 补文件状态未变更断言
5. 补容量护栏与 TTL 清扫测试

## 影响面

修复后，link_crypto 的 3 处关键盲区有回归网。当前这些路径任何回归 CI 都抓不到。

## Comments

- 2026-09-14 审计发现，见 `../http-ws-spec.md` §5.1
- 金样向量需与移动端 `linkCrypto.ts` 保持一致（跨端契约）
