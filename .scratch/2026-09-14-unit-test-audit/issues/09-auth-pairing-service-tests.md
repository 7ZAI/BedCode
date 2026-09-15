# 09 — auth_service.rs 生物认证三函数 + pairing_service.rs 单次使用测试

**What to build:** `src/server/services/auth_service.rs` 161 行中有 118 行是认证核心逻辑（`issue_biometric_challenge`、`verify_biometric_challenge`、`bind_biometric_credential`），但 3 个测试全部只测 5 行的 `format_device_display_name`。`src/server/services/pairing_service.rs` 的 `verify_and_consume_code`（单次使用 + 过期清除）零测试。补覆盖这些安全关键路径。

**Blocked by:** 无

**Status:** done（2026-09-15 修复）

- [ ] 新增 `test_issue_biometric_challenge_empty_fingerprint_returns_not_bound`：断言 `issue_biometric_challenge("")` 返回 `CredentialNotBound`
- [ ] 新增 `test_issue_biometric_challenge_unpaired_device`：未配对设备返回 `NotPaired`
- [ ] 新增 `test_verify_biometric_challenge_consumes_nonce_single_use`：同一 `(fingerprint, nonce)` 连续调用两次，第二次必须返回 `ChallengeInvalid`
- [ ] 新增 `test_verify_biometric_challenge_expired_challenge`：挑战值超过 60s 过期，返回 `ChallengeExpired`
- [ ] 新增 `test_verify_biometric_challenge_wrong_signature`：签名错误返回 `SignatureInvalid`
- [ ] 新增 `test_bind_biometric_credential_empty_key_unbinds`：传空串应解绑，返回 `false`
- [ ] 新增 `test_bind_biometric_credential_valid_key_binds`：非空串应绑定，返回 `true`，且 DB 中 `public_key` 非空
- [ ] 新增 `test_bind_biometric_credential_db_failure_maps_error`：DB 故障返回 `BiometricAuthError::Database`
- [ ] 新增 `test_verify_and_consume_code_single_use`：同 code 连续调用两次，第二次必须 false
- [ ] 新增 `test_expired_code_cleared`：过期 code 被清除，`get_pairing_code` 返回 None
- [ ] `cargo test --lib server::services::auth_service::` + `cargo test --lib server::services::pairing_service::` 通过

## 证据

- `auth_service.rs:134-161` 3 个 test 全部只测 `format_device_display_name`（5 行）
- 认证核心函数：`issue_biometric_challenge`（:31）、`verify_biometric_challenge`（:55）、`bind_biometric_credential`（:98）
- `pairing_service.rs:51-71` `verify_and_consume_code` 零测试（单次使用 + 过期清除）
- 生产代码缺陷被测试掩盖：`bind_biometric_credential` 无 publicKey 格式校验，任何非空字符串都写入 DB
- TOCTOU 时序：`verify_biometric_challenge` 先消费挑战（`:64-71`）再取 pairing（`:73-78`），若并发 unbind 会出现「挑战已被消费但 pairing.public_key 已空」的中间态

## 根因

3 个测试全部只测 5 行的辅助格式化函数，118 行认证核心逻辑零断言保护。任何一个函数被写坏（忘记消费 challenge、跳过指纹空串校验、公钥空串被当作绑定成功）都能通过所有测试。

## 修复方向

1. 为每个业务函数补 3-4 个用例，覆盖正反路径
2. 用 mock challenge 存根断言挑战单次有效
3. 补 DB 依赖的错误映射测试

## 影响面

修复后，生物认证与配对码的安全边界有回归网。当前任何安全回归只能靠 E2E 或线上问题暴露。

## Comments

- 2026-09-14 审计发现，见 `../http-ws-spec.md` §5.11
- 关联生产代码缺陷：`bind_biometric_credential` 无 publicKey 格式校验（需另开 issue 修生产代码）
