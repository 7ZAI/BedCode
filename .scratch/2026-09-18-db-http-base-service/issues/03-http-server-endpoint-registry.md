# 03 — 服务端插件端点注册治理（接线死代码注册表）

**What to build:** 修复审计发现 H3。把既有的 `HttpEndpointEntry` 注册表从死代码变成真实治理面：插件 activate 时按其声明注册 HTTP 端点路径，deactivate/卸载时按属主回收；HTTP 路由先查注册表再转发，未注册路径 404；两插件同一路径冲突 = 注册拒绝 + 明确错误。保持旧前缀 ANY 兼容策略，确保已装插件（auto-task 的 `/api/plugin/com.bedcode.auto-task/...`）零迁移。

实现时需定案的兼容细节：已声明 vs 未声明的插件端点行为（默认目标：注册表接线后不破坏 auto-task 现有端点，可能以「未声明 = 前缀内放行」过渡或要求全部声明——二选一留证）。可互借 WS 阶段 A 的 `ChannelType` 开放化与属主回收模式。

**Blocked by:** None.

**Status:** `ready-for-agent`

- [x] activate 声明 → 注册；deactivate/卸载 → 属主回收（含插件崩溃后重启的注册表一致性）
- [x] 路由先查注册表，未注册 404；路径冲突注册拒绝（注册/回收单测正反例，含冲突、重复注册幂等）
- [x] auto-task 端点回归（现有端点不被本票破坏）；既有注册表单测更新为接线语义
- [x] 桌面 cargo test 全绿

## Comments