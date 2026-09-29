# 07: 文档反转 + ADR 0031 修订 + CHANGELOG 双语

**What to build:** 本专项反转了若干被写进契约注释的事实，必须逐处订正——否则后来人会按错误文档做出错误判断。

**Blocked by:** 04

**Status:** todo

## 必须反转的错误陈述（已逐条核实）

- [ ] `bedcode-desktop/packages/plugin-sdk-desktop/rust/wit/bedcode.wit` · `auth-policy` 接口注释：
      现写「插件密钥域与宿主不同，**无法也不应验签**」+「**密钥不出宿主**」。
      前半句成立（`plugin_secrets` 属主隔离），后半句**不准确**——插件自己生成
      `jwt.key`（`pairing/keys.rs:42-43` guest 内 `getrandom`）并自带完整 HS256 实现。
      改为：本专项后中心**自持密钥并自验签**
- [ ] `wasm-apps/terminal-session/rust/src/auth_http/mod.rs:310-311` 注释
      「验签（宿主 `JwtService` 同一路径…，密钥不出宿主）」→ 改为本地实现
- [ ] `wasm-apps/terminal-session/rust/src/auth_http/jwt.rs:3` 模块头
      「**密钥不出宿主**：签发与验签执行经 host-auth `device-token-issue` / `device-token-verify`」→ 反转
- [ ] `wasm-apps/terminal-session/rust/src/policy/mod.rs:4-5` 同款陈述 → 反转
- [ ] `bedcode-desktop/src-tauri/src/utils/auth/auth_center.rs` 模块头
      「验签执行留宿主中间件（密码学引擎不移动，spec §3「不动」表）」→ 反转并指向 ADR 0033
- [ ] `docs/adr/0031-auth-center-registration-and-composable-grant.md` 修订记录追加指向 0033
      （**不改原文** —— ADR 是时间切片，改写会丢失决策现场）
- [ ] ADR 0031 §Consequences「凭据与密码学不动」标注已被 0033 修订

## 同步更新

- [ ] `docs/adr/0022-plugin-host-interface-primitive-boundary.md`「双端偏离」节追加本组 2 函数
- [ ] `docs/knowledge/mobile-desktop-auth.md` —— 认证链路章节：签发/验签归属、迁移需重配
- [ ] `bedcode-desktop/docs/code-map.md` Core Modules 段：`utils/auth/` 职责变化
- [ ] `bedcode-mobile/docs/code-map.md` —— 若有认证链路描述
- [ ] `docs/knowledge/plugin-development-checklist.md` —— 中心角色新增「自持签发密钥」
- [ ] `AGENTS.md` §8 认证红线段 —— 「认证链路只走既有 auth 模块」补一句归属口径
- [ ] `CHANGELOG.md` + `CHANGELOG_zh.md` 双语条目（含 ABI 33 与存量需重配的**破坏性变更**提示）
- [ ] 本 spec 回填实施记录

## 纪律

- AGENTS §0：**改文档前先核对事实**，本文档列出的行号在实施后可能漂移，逐条 `rg` 复核
- CHANGELOG 破坏性变更必须**显式写明存量用户影响**（已配对设备需重新配对）

## 验收

- 全仓 `rg "密钥不出宿主|无法也不应验签|验签执行留宿主"` **零命中**（或仅存于历史 ADR 原文）
- 两端 code-map 与实际代码一致
- CHANGELOG 双语都有条目，且都写明「存量已配对设备需重新配对」
