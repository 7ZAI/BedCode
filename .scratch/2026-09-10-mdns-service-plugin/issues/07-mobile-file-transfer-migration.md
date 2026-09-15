# 07: 移动端 file-transfer 迁移

**What to build:** 与桌面 05 同构的消费插件迁移落移动端，双端消费行为一致：activate 自调 browse + 订阅定向 topic、deactivate stop-browse、移除全局 `mdns:found`/`mdns:lost` 订阅；设备列表派生视图保留在前端缓存。移动端设备发现（对等网络语境）与迁移前行为等价。

**Blocked by:** 05, 06

**Status:** done (2026-09-15)

## Acceptance criteria

- [ ] 迁移行为与桌面 05 同构（自建 browse + 定向订阅 + 移除全局订阅 + 停用回收）
- [ ] 移动端 file-transfer 测试通过（Rust + vitest）
- [ ] 真机/模拟器冒烟：可发现桌面节点、无自播回显、离线判定正确