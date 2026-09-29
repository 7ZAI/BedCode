# 01: 宿主认证中心注册表（单中心 + 唯一性仲裁 + fail-closed 裁决核心）

**What to build:** 新增 `wasm_core/host_api/auth_center.rs`——单中心注册表（K1/K4）+ `authorize()` 裁决核心（K3）。本票只做**宿主内数据与判定**，不碰 WIT、不接线调用点（票 02/03 做）。

**Blocked by:** None

**Status:** todo

- [ ] `AuthCenterEntry { center_id, owner, methods }` + `static CENTER: OnceLock<Mutex<Option<..>>>`
- [ ] `register(owner, methods) -> Result<String>`：已有中心 → `Err` 且**点名在册属主**；铸 `authc-<uuid>`；`tracing::info!` 留痕
- [ ] `unregister(owner) -> Result<()>`：非属主 → `Err("not owner of auth center")`
- [ ] `center() -> Option<AuthCenterEntry>` / `is_registered() -> bool` / `center_id() -> Option<String>`
- [ ] `purge_for_plugin(plugin_id)`：只碰本人，返回是否回收（`activation.rs:693-702` 那一组接线，本票先留桩接线点）
- [ ] `pub(crate) enum AuthDecision { Allow, Deny(String) }` + `decision_for(center_present, outcome)` **纯判定函数**（可注入 `now`/无 I/O），便于单测与 fail-closed 锁
- [ ] 锁用 `std::sync::Mutex`（瞬时、无跨 await），同 `host_api/mdns.rs` 口径；**不引入 async 锁**
- [ ] 模块声明加进 `host_api.rs`（`pub(crate) mod auth_center;`）
- [ ] inline 单测（unit-test-discipline：正例/反例/边界/副作用）：
  - 注册成功返回 `authc-` 前缀句柄
  - **反例**：重复注册被拒且错误文本含在册属主 id
  - 反例：非属主注销被拒
  - 反例：`purge_for_plugin("别人")` 不影响在册中心
  - 边界：停用后 `is_registered()` 变 false、`center()` 变 None
  - `decision_for`：无中心 → Deny；调用失败 → Deny；中心 Err → Deny(原因透出)；中心 Ok → Allow
  - **fail-closed 反例锁**：`decision_for` 任何「拿不到裁决」的分支都不得返回 Allow（穷举分支断言）

## 关键实现事实

- 宿主**不解释** methods 语义（B1 不命中），只当声明列表存
- `authorize()` 本票**不**调 guest（要 `&PluginHost`），票 03 才接 `call_auth_policy`——本票交付判定核心，票 03 交付 IO
- 不新增权限位（K8 复用 `PERMISSION_AUTH`），故不碰 SDK 权限词汇漂移锁
- 不 bump ABI（本票零 WIT 改动）

## 验收

- `cd bedcode-desktop/src-tauri && cargo test auth_center` 全绿
- 根 `pnpm exec eslint .` 0 error（本票无前端改动，预期无变化）
