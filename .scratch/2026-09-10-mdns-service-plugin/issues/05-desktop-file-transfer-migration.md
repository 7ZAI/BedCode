# 05: 桌面 file-transfer 迁移（D2）

**What to build:** 首个消费插件迁移（D2 定案：一期同步迁移、无迁移垫片）。file-transfer 从「订阅宿主全局发现事件」迁移为「activate 时自调 browse 对等网络服务类型 → 订阅属主定向事件 topic → deactivate 时 stop-browse（宿主 purge 兜底）」；设备列表派生视图（去重 / TTL / 展示名 / 能力位）继续保留在前端缓存，wire 形状既有字段不变；全局 `mdns:found`/`mdns:lost` 订阅移除。设备发现端到端行为与迁移前等价。

**Blocked by:** 03, 04

**Status:** done (2026-09-15)

## Acceptance criteria

- [ ] activate 时自调 browse 并订阅 `mdns:found.<file-transfer>` / `mdns:lost.<file-transfer>`；deactivate 时 stop-browse（宿主 purge 兜底不泄漏句柄）
- [ ] 移除全局 `mdns:found` / `mdns:lost` 订阅与处理分支
- [ ] 设备列表内容 / 在线判定与迁移前等价（冒烟验证可发现对等节点、无自播回显）
- [ ] file-transfer 相关测试通过（Rust + 前端 vitest）