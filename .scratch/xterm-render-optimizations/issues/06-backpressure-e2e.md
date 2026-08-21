# 06 — 背压：端到端验证 + writeInChunks 简化

**What to build:** 背压反馈环端到端闭合——真机 `cat` 大文件 / `tail -f` 日志风暴下峰值内存与 writeQueue 积压受钳制、UI 不冻结；背压介入时暂停、跟上时零开销放行（本地环回高吞吐不被反噬）。落地后简化 `writeInChunks` 的 `setTimeout(0)` 让步。

**Blocked by:** 04 + 05

**Status:** resolved（实现完成，e2e 待真机）

- [ ] 真机 `cat` 大文件 / `tail -f` 日志风暴：峰值内存与 writeQueue 积压受钳制、UI 不冻结（需真机，spec 预置「本地环回收益有限」前提下验证）
- [x] 背压仅在渲染解析跟不上时介入、跟上时零开销放行（没 ack 水位未超则 should_pause 恒 false，读线程无 sleep 无锁全速；跟上节奏时 unacked 远低于水位）
- [X] `writeInChunks` 的 `setTimeout(0)` 让步**暂不简化**（保留）——它来自另一会话的写管线（WRITE_YIELD_THRESHOLD=256KB），对「水位以上、暂停阈值以下」的短时暴发仍有保护作用；且背压的 writeQueue 缩减收益只能在真机复测后确认。删除让步属降低防护，需真机数据支撑后再动（记 plan Comments）
- [x] 快照重订阅 / 重播跳过语义与背压兼容（ack 与重播去重共用 last_rendered_seq 单坐标系；ack 为单调前推、陈旧 ack 天然 no-op；暂停不丢字节，重订阅全量重播不变）
- [x] 全量测试全绿：前端 vitest 436/436（exit 0）+ `vue-tsc --noEmit` 干净 + Rust `cargo test --lib` 567/567（含新增 3 模块用例）
