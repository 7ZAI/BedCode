# 06: 测试重做 + 回归锁 + 性能探针复测

**What to build:** 宿主/插件两侧既有测试大面积引用被退役的宿主 `JwtService`，需逐处重做；并补防回接锁。

**Blocked by:** 04, 05

**Status:** todo

## 必须重做的既有测试（引用面已核实）

- [ ] `wasm_core/manager/host/tests/system_component_test.rs`（8 处 `JwtService`）——
      `valid_token` 构造方式失效；`enforce_connection_policy` 的 `no_center` / `unavailable` /
      `policy` 三态用例需改用中心签发的 token
- [ ] `wasm_core/manager/runtime/tests/ws_e2e.rs`（9 处）
- [ ] `wasm_core/manager/runtime/tests/ws_output_perf.rs`（2 处）
- [ ] `auth_center_perf.rs`（6 处）—— A 段（宿主原生验签）本专项后**不存在**，探针只留 B 段
- [ ] `utils/auth/jwt.rs` 全部 `#[cfg(test)]` 随文件退役

## 跨实现锁的处置

- [ ] **删或反转** `utils/auth/jwt.rs:337` `host_jsonwebtoken_matches_plugin_fixed_vector`
      —— 宿主不再产出 token，「宿主与插件产出逐字节相同」这一不变量**失去意义**
- [ ] 替换为「中心实现 vs RFC 7515 §A.1 官方向量」（插件侧
      `pairing/jwt.rs:365` 已有该用例，确认其覆盖足够）

## 新增回归锁

- [ ] **防回接锁（静态）**：宿主生产路径不得出现 JWT 密码学字样
      （`JwtService` / `verify_token_with_expiry` / `generate_device_token` / `verify_device_token`），
      手法参照 `wasm_core/manager/host/tests/l2_gating_test.rs` 的文本扫描
- [ ] **防回接锁（能力）**：注册表在册中心的 `methods` 与导出能力面不得再要求宿主代签
- [ ] **迁移锁**：旧密钥签发的 token 走 `handle_reauth` 被拒，且错误文案可读
- [ ] **闭环锁**：真实产物加载 → 配对 → 签发 → HTTP `/api/*` → WS 端点 → 撤销后拒绝
- [ ] **轮换锁**（票 05 通过时）：跨代验签正反例
- [ ] **闸门纪律**：`auth_center_perf` 必须保持 `#[ignore]` —— 它让中心整段在册，
      而注册表是**进程级单槽**，`hold_registry_desk` 只串行化写表不覆盖用例体，
      并行会随机打红 `system_component_test.rs:232` 的 `assert!(!registry::is_registered())`

## 性能复测

- [ ] 用 `auth_center_perf` 探针复测：A 段消失后，合计不得劣化
      （基线：中心往返 95.7–113.5 µs/op，合计 101.7–120.6 µs/op，dev profile）
- [ ] 记录新数据入 ADR 0033

## 验收

- `cd bedcode-desktop/src-tauri && cargo test` **全量**绿（lib + 全部集成 target）
- `cd bedcode-desktop/wasm-apps/terminal-session/rust && cargo test` 全绿
- 每条新锁做**变异自检**（把被锁行为改坏，用例必须转红）
- 探针 `#[ignore]` 状态下常规套件不受影响（1067 基线 + N ignored）
