# 29 — qr_token.rs + biometric.rs 剩余缺口补测试（前提已修正）

**What to build:** `qr_token.rs`（159 行）与 `biometric.rs`（211 行）**并非零测试**（初版审计误判，已更正）：qr_token 有 4 个 `#[tokio::test]`（generate/verify 单次消费、无效 token、clear、TTL=0 过期），biometric 有 5 个测试（挑战生成/消费、错配/未知指纹、clear、过期、`verify_signature_roundtrip`）。剩余缺口仅为边界/并发用例与契约显式化，补测范围相应收窄。

**Blocked by:** 无

**Status:** done（2026-09-15）

- [ ] `qr_token.rs` 并发单次使用：两个并发 `verify(&token)` 只允许一个成功（当前为顺序测试，未覆盖并发竞态）
- [ ] `qr_token.rs` 边界：TTL 恰好等于 elapsed（`is_expired` 用 `>=`，`elapsed == ttl` 视为过期）——当前 `test_qr_token_expired` 只测 `ttl=0` 的宽松过期，未锁边界语义
- [ ] `qr_token.rs` `get_active()` 过期路径：生成过期 token 后 `get_active` 返回 None（当前只测 verify 路径的过期）
- [ ] ~~`qr_token.rs` token 与设备指纹绑定~~ —— **复核排除**：`QrTokenManager` 只存 `QrToken{token, created_at, ttl_secs, used}`，无设备指纹概念；指纹校验在 `utils::auth::pairing` / `auth_service`，不属于本模块
- [ ] `biometric.rs` `BiometricChallengeManager::verify_and_consume` 并发消费：同一 nonce 并发验证仅一次成功（与 qr 同类竞态）
- [ ] `biometric.rs` 签名验证负向：`verify_biometric_signature` 用错误签名/错误公钥验签失败（现有 `test_verify_signature_roundtrip` 只测正确路径）
- [ ] `cargo test --lib utils::auth::` 通过

## 证据

实测（2026-09-14 复核）：

- `qr_token.rs:118` `#[cfg(test)]`，4 个测试：`test_qr_token_generate_and_verify`（:123）、`test_qr_token_invalid_token`（:138）、`test_qr_token_clear`（:145）、`test_qr_token_expired`（:153）
- `biometric.rs:135` `#[cfg(test)]`，5 个测试：`test_challenge_generate_and_consume`（:143）、`test_challenge_mismatch_and_unknown`（:154）、`test_challenge_clear`（:162）、`test_challenge_expired`（:170）、`test_verify_signature_roundtrip`（:185）
- 初版断言「qr_token 零测试 / biometric 仅 1 测试」错误，原因：`grep "#\[test\]"` 漏数 `#[tokio::test]` + 未展开测试模块统计

## 修复方向

并发用例用 `tokio::join!` / `spawn` 并发两个 verify，断言恰好一个 Ok 一个 Err；边界用例构造 `ttl_secs=1` + `sleep(1s)` 或注入时钟（`Instant` 可直接注入 `QrToken::new(ttl)` 后固定 created_at 的变体较难，可用 `tokio::time::pause`）；`get_active` 过期用例直接生成 ttl=0 后调用。

## 影响面

仅新增测试，零生产代码改动。

## Comments

- 2026-09-14 审计发现（初版前提误判，本版已修正），见 `../utils-spec.md` §3
- 审计教训：统计测试数必须同时匹配 `#[test]` 与 `#[tokio::test]`，不能只 grep `#[test]`