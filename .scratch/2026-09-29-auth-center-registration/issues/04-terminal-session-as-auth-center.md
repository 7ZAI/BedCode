# 04: 中心侧实现（terminal-session 注册/注销 + `auth-grant` 组合分派）

**What to build:** `com.bedcode.terminal-session` 激活时注册为认证中心并声明 4 种方式；新增互调 api `auth-grant` 按 method 分派（组合式认证的服务端面）。

**Blocked by:** 02, 03

**Status:** todo

- [ ] 激活时 `auth-center-register(["pairing_code","qr","biometric","jwt"])`；失败按 K3 语义留 `error!`（认证面将全断，必须可见）
- [ ] 停用时 `auth-center-unregister()`；宿主 `purge_for_plugin` 兜底（防 guest 未跑 deactivate）
- [ ] 互调 api `auth-grant`（契约 **spec §4.3.1**；沿用仓内 `params` 为 camelCase 具名对象
      的既有约定，`api_call.rs:1-16`）：
      `fn auth_grant(method: String, params: serde_json::Value) -> Result<Value, String>`
- [ ] 逐条固定 **method → 既有实现** 的映射表（写进代码注释，票验收项）：
      `pairing_code`→`pairing/` · `qr`→配对 QR 实现 · `biometric`→`auth_http/biometric.rs` ·
      `jwt`→`auth_http/jwt.rs`。映射错了不会编译报错，**必须逐条写注释并在激活时做一致性自检**
- [ ] 组合逻辑**不做**在中心（哪个方式优先、何时回退是调用方的业务判断，B2）
- [ ] **既有 `/api/auth/*` 端点与私有库 `auth_records` 真源零改动**（新增出口，不是搬家）
- [ ] 中心侧自检：注册表里的 methods 与实际可分派的 method 集合一致（不一致 → 激活失败，fail-visible）
- [ ] `wasm-apps/terminal-session` 补 `pnpm run build` + `wasmHash` 注入验证

## 关键实现事实

- 中心与「会话」耦合是**当前**的实现选择，不是架构约束——单中心注册表天然允许未来换人（换插件 id 即可），ADR 0031 Considered Options E 已登记
- `auth-grant` 分派的是**既有实现**，不新造认证方式；组合发生在**调用方**（其他插件组合多个 method）
- 401 拒绝原因必须点名（`no auth center registered` vs `auth center unavailable: …` vs 中心业务原因），否则排障要再翻日志

## 验收

- `cd bedcode-desktop/wasm-apps/terminal-session/rust && cargo test` 全绿
- `cd bedcode-desktop/wasm-apps/terminal-session && pnpm run build` 通过（wasmHash 注入）
