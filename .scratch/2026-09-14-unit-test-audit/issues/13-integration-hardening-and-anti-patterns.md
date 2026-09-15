# 13 — 集成测试补强 + 反模式改造（GCM 篡改 + token-fingerprint + tracing 耦合 + flaky + 注释漂移 + 装饰性测试）

**What to build:** 两类改进：(a) 集成测试补强关键安全路径（GCM 密文层篡改、JWT token-fingerprint 交叉校验、publicKey 格式校验）；(b) 测试反模式改造（tracing 耦合、flaky 测试、注释漂移、装饰性测试）。

**Blocked by:** 无

**Status:** done（2026-09-15 修复）

- [ ] 新增 `test_link_crypto_http_gcm_ciphertext_tamper_rejected`：篡改 GCM 密文字节（非信封 JSON），断言返回 400 且 error body 携带 GCM auth failure 特征
- [ ] 新增 `test_ws_auth_rules_jwt_fingerprint_mismatch_rejected`：JWT claims 的 fingerprint 与客户端声明不匹配，应被拒绝
- [ ] 修改 `test_http_auth_biometric_bind_public_key`：补 publicKey 格式校验断言（非 Base64 应被拒绝）
- [ ] 修改 `http_filter.rs` 日志断言：不直接检查 `tracing` 的 WARN/INFO 标签与 target 字面量，改用结构化日志捕获或断言响应行为
- [ ] 修改 `http_filter.rs` 请求 ID 测试：改用确定性 ID 生成器或加 `sleep(2s)` 避免跨秒归零
- [ ] ~~修改 `link_crypto.rs` 密钥缓存 TTL 测试~~ —— **复核排除**：`REQUEST_KEY_TTL=30s`（:566），缓存测试用可注入 `Instant` 纯函数判定（:1356-1377），无 sleep 时序，本条不成立（见证据区划线项）
- [ ] 修改 `tests/ws_session_route.rs:6,11`：文件头注释从 "TB v2" 改为 "TB v3"（实现 :158/:279/:299 已是 v3）
- [ ] ~~修改 `tests/broadcast_shutdown.rs:558` `assert_no_sync_data` 的因果顺序~~ —— **复核：现状已满足**（:545-556 先 `wait_for_sync_data` 等 B 收到广播，才在 :558 断言 A 不收到）。保留 500ms 窗口作为可选项：更稳健的做法是让断言窗口可调注入或改为事件序证明
- [ ] 修改 `port_checker.rs` 两个测试：加真实端口占用验证（绑定端口后断言不可用）
- [ ] 修改 `tests/build_manifest_smoke.rs`：`assert!(true)` 是文件头注释声明的「保留最小测试用例以满足 build.rs 的 `cargo:rustc-link-arg-tests` 约束」的**故意恒真**（非疏忽）；建议改为验证注入链路真实生效（如检查 build.rs 注入的环境变量/符号可解析）而非裸 `assert!(true)`，或保留并注释说明
- [ ] `cargo test` 全量通过

## 证据

- `tests/link_crypto_http.rs:154-189` 篡改测试只测信封 JSON 结构损坏，**没有测试 GCM 密文本身的字节篡改**（真正的攻击路径）
- `tests/ws_auth_rules.rs` 场景 3：JWT 认证未交叉校验 `claims.fingerprint` 与客户端声明（生产代码缺陷被测试掩盖）
- `tests/http_auth_biometric.rs` T7：`bind_biometric_credential` 生产代码**无 publicKey 格式校验**，任何非空字符串都写入 DB
- `http_filter.rs:429-548` 日志断言直接检查 `tracing` 的 WARN/INFO 标签、status=XXX 子串、tracing span 字面量（`http_request`/`request_id=`）——耦合过紧
- `http_filter.rs:259-266` 请求 ID 用 `subsec_nanos()` 检验"不同"（`assert_ne!(r1, r2)`），跨秒归零（恰好同秒同一分数值）理论会失败（flaky 风险低但存在）
- ~~`link_crypto.rs:1328` 密钥缓存 TTL=1s 与 50ms 断言窗口（flaky）~~ —— **复核排除**：当前 `REQUEST_KEY_TTL=30s`（:566），缓存测试（:1356-1377）用可注入的 `Instant` 纯函数判定，无 sleep 时序，此项不成立，已从本票移除
- `tests/ws_session_route.rs:6,11` 注释写 "TB v2" 但实现是 TB v3（注释漂移）
- `tests/broadcast_shutdown.rs:558` `assert_no_sync_data` 因果证据弱
- `port_checker.rs:111-129` 两个测试零/软断言（装饰性）
- `tests/build_manifest_smoke.rs:6-13` `assert!(true)` 恒真断言

## 根因

1. 集成测试篡改路径覆盖不足（只测信封 JSON 损坏，未测 GCM 密文层）
2. 生产代码缺陷被测试掩盖（token-fingerprint 交叉校验、publicKey 格式校验）
3. 测试耦合内部实现（tracing 日志格式、subsec_nanos 时序）
4. 注释与实现漂移（TB v2 vs TB v3）
5. 装饰性测试（零断言、恒真、条件断言）

## 修复方向

1. 补 GCM 密文层篡改测试
2. 补 JWT token-fingerprint 交叉校验负向测试
3. 补 publicKey 格式校验断言
4. 改造 tracing 耦合断言（改用结构化日志或行为断言）
5. 修复 flaky 测试（sleep 或可注入时钟）
6. 修正注释漂移
7. 改造装饰性测试（加真实断言）

## 影响面

修复后，集成测试能抓安全回归，测试基础设施不再耦合内部实现，flaky 减少，注释与实现一致，装饰性测试有真实断言。

## Comments

- 2026-09-14 审计发现，见 `../http-ws-spec.md` §5.13 与 §5.14
- 部分改进涉及生产代码缺陷修复（token-fingerprint 交叉校验、publicKey 格式校验），需另开 issue
