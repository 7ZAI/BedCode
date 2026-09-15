# 08: 集成隔离验证 + 双端回归 + ADR 补记（D5）

**What to build:** 端到端验收关闭。验证需求①（业务隔离）：双插件各自 browse 同一服务类型互不干扰——A 插件的事件流不进 B 插件的队列（按定向 topic 断言）；验证 host 与插件 advertise 共存于单一守护；双端回归门禁全绿（cargo test / vitest / eslint / i18n）；peer-net 集成回归等价。验收通过后补记 ADR（D5 定案）：修订 ADR 0022 相关表述——host-mdns 契约扩展（advertise 原语、事件定向投递、属主仲裁）、基础服务零业务代码红线。

**Blocked by:** 04, 05, 06, 07

**Status:** done (2026-09-15)

## Acceptance criteria

- [ ] 双插件 browse 隔离集成断言：A、B 各自 browse，A 的事件 topic 集合不含 B 实例的消息（物理隔离成立）
- [ ] host（owner=host）与插件 advertise 共存于单守护，互不注销对方
- [ ] 双端 cargo test 全绿；桌面 `pnpm run test:run` 与移动端 vitest 通过；根目录 eslint 0 error；i18n key 双端同步（如有 UI 文案）
- [ ] peer-net 集成测试回归全绿（发现 / 在线判定 / 能力通告行为等价）
- [ ] ADR 0022 修订入库：host-mdns v2 契约（advertise 原语 / 事件定向 / 属主仲裁）+ 基础服务零业务代码红线