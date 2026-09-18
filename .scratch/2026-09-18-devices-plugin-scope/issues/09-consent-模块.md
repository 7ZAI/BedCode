# 09: consent 模块 + 互调 API（B4）

**What to build:** peer 首连确认决策能力 + 互调 API 声明（`auth.decide-consent` / `auth.list-trusted-devices`），供 file-transfer 消费（票 10）；ADR 0017「未声明 api 不可调」验证。

**Blocked by:** 08

**Status:** ready-for-agent

- [ ] consent 决策单测（正反例）全绿：允许 / 拒绝 / 已信任免确认 / 一次性确认
- [ ] 互调闭环测试：消费方经互调调用 `auth.decide-consent` 成功返回；未声明 api 的调用被拒（ADR 0017）
- [ ] api 已在 manifest `api` 字段声明（声明即契约）
