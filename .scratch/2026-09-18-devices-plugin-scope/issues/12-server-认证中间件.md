# 12: server 认证中间件（C3）

**What to build:** server 连接建立验签执行留在宿主中间件（密码学引擎不移动），认证策略取认证中心 capability 导出（06 框架：exported_capabilities 探测 / call_capability_export）；认证中心未激活时策略回退宿主。

**Blocked by:** 09（策略导出就绪）

**Status:** ready-for-agent

- [ ] jwt_auth 中间件单测全绿（验签仍执行于宿主）
- [ ] 连接建立流程集成测试通过（验签 → 策略取认证中心 → 放行/拒绝）
- [ ] 认证中心未激活降级可用（策略回退宿主，无单点）
