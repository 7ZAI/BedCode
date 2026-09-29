# 03: 网关裁决面切换 + fail-closed + 退役 5 处旧发现路径（K2/K3/K7）

**What to build:** 两条裁决调用点改查注册表；删除全部 fail-open 降级与旧发现机制；桥接门切注册表。**这是本次事故的直接修复点。**

**Blocked by:** 01, 02

**Status:** todo

- [ ] `utils/auth/auth_center.rs::enforce_connection_policy` 重写：无中心 → `Err`（点名 `no auth center registered`）；中心调用失败 → `Err`（**不** `log_fallback` 放行）
- [ ] 删除 `host.rs:709 auth_center_candidates()` 全函数及其全仓引用
- [ ] 删除 `auth_center.rs` 的 `candidates.first()` 启发式与「无候选 → 放行」「传输失败 → 放行」两分支
- [ ] 删除 `SESSION_MARKER_API` / `api_registered()`；`session_active()` 改查 `auth_center::is_registered()`（K7）
- [ ] 调用点 1：`server/http/middleware/jwt_auth.rs:49`（HTTP `/api/*`）
- [ ] 调用点 2：`server/websocket/channel/plugin.rs:252`（WS 插件端点首消息，close 4001 带原因）
- [ ] `activation.rs:693-702` 那一组补 `auth_center::purge_for_plugin(plugin_id)`
- [ ] **不做中心 id 缓存**（spec §5.2）：裁决必须每次实时问中心（策略撤销要即时生效，
      缓存裁决 = 撤销后仍放行的安全洞）；`center()` 查表本身 O(1)，无需缓存
- [ ] 三类拒绝带**结构化** `deny_kind = no_center | unavailable | policy`（spec §5.1），
      禁止把区分信息只拼进消息字符串（AGENTS §8）
- [ ] **退役字眼加载即抛**（fail-visible ③）：`auth_center_candidates` / `SESSION_MARKER_API` / `log_fallback` 不得在宿主出现

## 关键实现事实

- 宿主仍执行 HS256 验签（K5/D3 不动密码学）——**先验签再问中心**的顺序不变
- 中心 `Err(reason)` 是**业务拒绝**（透出原因），调用失败是**基础设施失败**（Deny 带 `auth center unavailable` 前缀），两者在日志上必须可区分
- fail-closed 意味着：中心插件未激活时本机认证面全断——UI 侧要能提示（票 04 中心未注册时给可见信号）

## 验收

- `cd bedcode-desktop/src-tauri && cargo test` 全绿
- 事故复现验证：两个插件导出 `auth-policy`、只注册一个 → 裁决落在注册者（票 05 锁）
