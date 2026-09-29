# 05: 回归锁（本次事故专属）+ fail-visible 形态 ②

**What to build:** 4 条锁把这次事故钉死，外加旧产物点名。**没有这票，票 03 的修复会被下一个人改回去。**

**Blocked by:** 03, 04

**Status:** todo

- [ ] **多候选锁**（真源回归，复刻事故）：同时激活两个导出 `auth-policy` 的插件、只注册其一 → 裁决必须落在**注册者**；未注册者即使 id 排序在前也不被选中
- [ ] **fail-closed 锁**：未注册中心 → `enforce_connection_policy` 必须 `Err`（不是 `Ok`）
- [ ] **防回接锁** `retired_auth_center_discovery_is_not_reintroduced`：`auth_center_candidates` / `SESSION_MARKER_API` / `log_fallback` 放行分支字眼不得在宿主出现（放 `wasm_flow_test.rs` 或 `system_component_test.rs`，同族既有锁的位置）
- [ ] **停用锁**：中心停用 → 认证面立即拒绝（不静默放行）
- [ ] **三类拒绝可区分锁**（spec §5.1）：`no_center` / `unavailable` / `policy` 三条路径
      各自的 `deny_kind` 断言——「中心没起来」与「用户撤销了设备」是两类不同问题，
      合并成一句文案就等于把 F2/F3/F4 重新混成今天这个「看不懂的拒绝」
- [ ] 错误文本**不得含凭据片段**（JWT / claims / 指纹以外的敏感值）——凭据红线（AGENTS §8）
- [ ] fail-visible ②：`stale_artifact_rebuild_hint` 增加条件——manifest 声明 `auth` 且产物早于 v32 → 实例化期点名「按 v32 SDK 重建以注册认证中心」
- [ ] 变异自检：把「无中心 → 放行」改回去，确认上述锁转红

## 关键实现事实

- 现有闭环测试（`system_component_test.rs:189`）只激活**单个**候选，所以「取第一个」恰好正确——**多候选场景必须新增**，这是本次事故能上生产的直接原因
- 防回接锁的价值：它是唯一能挡住「优化成取第一个 / 加个 fallback」的机制

## 验收

- 每条锁单独做**变异自检**（改坏实现 → 对应锁转红 → 改回）
- `cargo test` 全绿
