# 22 — sync_handler.rs 11 个 handle_* 分支补测试（P0）

**What to build:** 为 `sync_handler.rs` 的 11 个 `handle_*` 分支补测试，消除跨端同步协议回归风险。

**Blocked by:** 需先抽象 WS 广播接口（当前 handler 直接依赖 actix runtime）

**Status:** done（2026-09-15 修复：SyncBroadcaster trait + Fake 9 测试）

- [ ] 抽象 WS 广播接口为 trait，handler 依赖 trait 而非 actix runtime
- [ ] 为每个 `handle_*` 分支构造匹配的 SyncEvent，断言正确的广播消息
- [ ] 覆盖所有 SyncEvent variant → handler 分支映射
- [ ] 变异验证：改坏一个 `handle_*` 分支 → 对应测试失败
- [ ] `cargo test --lib events::sync_handler` 通过

## 证据

`sync_handler.rs` 405 行，0 测试。11 个 `handle_*` 分支是跨端同步协议的核心路由——变体匹配错误会静默丢弃或误路由事件。

## 影响面

需重构 handler 依赖（actix → trait），涉及中等改动。

## Comments

- 2026-09-14 审计发现，见 `../events-spec.md` §5.5
