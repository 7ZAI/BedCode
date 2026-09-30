# 票 04 — 场景 2：JWT 重连 + 轮换宽限期（ADR 0033 密钥环）

**状态**：resolved · 2026-09-30
**类型**：task

## 落点

`cross-end-tests/tests/jwt_rotate_reconnect.rs`（5 条契约 S-001…S-005）。

## spec §6-7 的「blocked 风险」不成立

spec 担心「rotate-key 需宿主命令面接线（auth 专项票 05 未完成）→ 轮换部分标 blocked」。
**实查**：轮换触发面是**插件互调** `com.bedcode.terminal-session.auth-grant` 的
`jwt` / `rotate-key` 动作（该 api 已在 plugin.json 声明），无头装配直接驱动即可，
不需要宿主命令面。ADR 0033 的轮换宽限期因此**真被跨端覆盖了**，没有降级。

## 覆盖

| 契约 | 覆盖 |
|---|---|
| S-001 | 轮换前 token 可换发 + 带 `kid`（前置） |
| S-002 | 轮换返回 `{rotated:true, kid, previousKid}`，kid 推进、previousKid 点名上一代 |
| S-003 | **旧 token 轮换后仍可换发**（宽限期，不撤销）；且换发产物带**新** kid（签发侧已切换） |
| S-004 | kid 推进的跨端可观测性（经 `auth-grant jwt/verify` 读回 claims） |
| S-005 | **轮换前的 token 仍能过 WS 端点认证**：把全局 token 换回旧的那张，终端链路仍进 live，且真实 PTY 输出字节到达移动端页面通道 |

S-005 是本场景的跨端核心——「设备没重新认证就重连」正是生产里最常见的路径，
此前只在插件单测里成立。

## 验证

`cargo test --test jwt_rotate_reconnect` → 1 passed。
