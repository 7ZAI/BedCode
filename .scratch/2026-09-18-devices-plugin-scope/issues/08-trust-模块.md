# 08: trust 模块（B3）

**What to build:** 设备信任列表统一视图（DB pairings + peer trust_store 映射），列表 / 撤销行为与宿主实现等价，数据持久化。

**Blocked by:** 07

**Status:** ready-for-agent

- [ ] 列表 / 撤销行为与宿主实现等价（对照测试）
- [ ] 数据持久化，重启后一致；撤销后立即生效
- [ ] 插件 cargo test 全绿
