# 02 — device_bridge::stop_browse 对称清理 endpoint memo

**What to build:** `stop_browse` 当前只清 `sessions`，`endpoints` memo 永不清理；百次 toggle 后进程内静态表持续增长。对称清空，收敛内存不变量。

设计依据见同目录 `../spec.md` §1.3、§4 D6。

**Type:** task
**Status:** resolved
**Blocked by:** None — can start immediately.

- [x] `stop_browse` 清 `sessions` 的同时清空 `endpoints`
- [x] 保留 `DeviceSnapshotEntry` / 快照持久化逻辑不动
- [x] 既有 `#[cfg(test)]` 单测（`resolve_endpoint_prefers_explicit_and_updates_memo`）适配新语义：断言 stop_browse 后 memo 为空
- [x] cargo test 全绿

## 取舍注明（spec §4 D6）
endpoint memo 为跨激活重连复用而保留；本票采用「全清」收敛内存。若回归证明重连体验受损，改「无活跃 session 时清」折中。

## 验收
- cargo test 全绿；连续激活/停用循环后 `endpoints`/`sessions` 静态表大小稳定。

## Comments

实现（2026-09-06）：
- `stop_browse` 清 `sessions` 的同时清空 `endpoints` memo，取舍注明写入 doc 注释（spec D6；回归受损时回退「无活跃 session 时清」）。
- `DeviceSnapshotEntry` / 快照持久化逻辑未动。
- 测试：新增 `stop_browse_clears_sessions_and_endpoint_memo`（MockMdns 记录退订调用、断言 sessions/endpoints 全清 + 幂等不重复退订）；既有 `resolve_endpoint_prefers_explicit_and_updates_memo` 适配——引入 `statics_lock` 互斥（stop_browse 全清是进程级静态操作，并行测试须串行化），保留 forget_session 激活期内保留 memo 语义。cargo test 23/23 全绿。
