# 10: file-transfer 消费迁移（C1）

**What to build:** file-transfer 的 peer consent / 信任管理改经互调认证中心 API（`auth.decide-consent` / `auth.list-trusted-devices`），移除本地实现；行为与迁移前等价。

**Blocked by:** 09

**Status:** ready-for-agent

- [ ] file-transfer 单测 + 对等网络集成测试全绿
- [ ] 与迁移前行为等价（对照测试：同一场景同一决策）
- [ ] 双轨并存期宿主实现保留作对照基线（无单点，降级兜底）
