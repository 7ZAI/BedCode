# 03: WIT / SDK — 退役 `device-token-issue` / `device-token-verify`，ABI 32 → 33

**What to build:** 破坏性契约变更。`host-auth` 退役 2 函数，ABI bump，双端 WIT 副本同步，旧产物实例化期点名重建。

**Blocked by:** 01

**Status:** todo

- [ ] `bedcode-desktop/packages/plugin-sdk-desktop/rust/wit/bedcode.wit` · `host-auth` 删 `device-token-issue` 与 `device-token-verify`
- [ ] `bedcode-mobile/packages/plugin-sdk-mobile/rust/wit/bedcode.wit` **同步**（双端 WIT 真源，ADR 0022 / 0019）；移动端 WIT 本就不含这两个函数（桌面独有），故此处为**核对**而非删除，须留痕
- [ ] `packages/plugin-sdk-desktop/rust/src/abi.rs` `ABI_VERSION` 32 → **33**，`assert_eq!(ABI_VERSION, 33)` 同步
- [ ] SDK 绑定层删除两个 host 函数的 guest 侧桩与 `HostAuth` trait 方法
- [ ] 宿主 `wasm_core/host_api/auth.rs` 删 `auth_device_token_issue`(:323) / `auth_device_token_verify` 及 `component.rs:190` 的接线
- [ ] 宿主 `wasm_core/manager/capability.rs` 若有对应能力登记项，同步清理
- [ ] **fail-visible ②**：`LoadedWasmPlugin::stale_artifact_rebuild_hint` 加条件 —— v32 产物实例化期拿到点名缺失 interface +「按 v33 SDK 重建」
- [ ] 双端偏离登记追加到 ADR 0022「双端偏离」节（本组 2 函数桌面独有，mobile ABI 保持 **11**）
- [ ] 移动端 **ABI 不 bump**、SDK 不跟演、不投影（移动端是客户端，不承载服务端网关与认证中心角色）

## 关键实现事实

- ABI 现值 **32**（`abi.rs:191`，测试断言在 `:231`）
- 这是 ABI 序列里的**第 N 次破坏性变更**。先例：v27 整 interface 退役（`host-session` /
  `host-terminal`）、v28 / v31 字段与函数删除。措辞与 hint 文案对齐 v31 先例
- `host-auth` 保留面（不得误删）：`secret-get/set/delete/keys` · `auth-setting-set` ·
  `biometric-credential-bound` / `biometric-verify-signature` / `biometric-credential-bind` ·
  `link-identity-parts` · v32 的 4 个 `auth-center-*` / `auth-methods-*`
- **移动端不跟演是既有的双端偏离**（`host-auth` 系列自 v15 起即桌面独有），
  本票只是把「这 2 个函数也在桌面独有面内」写进登记
- `stale_artifact_rebuild_hint` 现有条件在
  `bedcode-desktop/src-tauri/src/wasm_core/manager/runtime/component.rs`

## 验收

- `cd bedcode-desktop/packages/plugin-sdk-desktop && cargo test` 全绿
- 移动端 SDK / `cargo test` 全绿（确认未受影响）
- 桌面 `cargo test` 中 ABI 版本相关用例全绿
- 用 v32 SDK 构建的旧产物 → 实例化期拿到点名错误（**非 trap、非静默降级**），有回归锁
- 移动端 `packages/plugin-sdk-mobile` grep 确认不含被退役函数名
