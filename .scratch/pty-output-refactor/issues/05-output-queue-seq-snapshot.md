# 05 — 输出队列 seq 化 + 快照订阅（废除偏移量）

**What to build:** `session_output.rs` 重构：`OutputEvent` 删除 start_offset/end_offset；`UnifiedOutputQueue` 的 min/max/snapshot_offset 改 min_seq/max_seq/snapshot_seq；`get_range(cursor)` 字节裁剪改为按 seq 取整段（无裁剪、无断点续传语义）；`subscribe()` 快照化：占位 subscriber → 快照 snapshot_seq → 读 `[min_seq..snapshot_seq]` 历史 → 发 `history_end` 控制帧 → 写锁内排空 pending + 原子激活（pending 天然全部 > snapshot_seq，删除旧 skip 逻辑）；删除 `start_seq` 参数、`SubscribeMode`（incremental/reset）、订阅响应 mode/offset 字段。`snapshot_offset`（2J 点）保留为可选配置 `history_start_mode`（默认 `min`，D4）。

**Spec:** §5.2、§5.5

**Blocked by:**

**Status:** ready-for-agent

- [ ] OutputEvent/输出队列 offset→seq 全量替换（含 forward.rs 引用）
- [ ] `subscribe()` 快照协议（占位→历史→history_end→排空→激活）
- [ ] SubscribeMode/start_seq/裁决逻辑删除；`history_start_mode` 配置项
- [ ] 测试：快照顺序严格 `[历史][history_end][实时]`（含占位期 on_output 竞态）、seq 范围正确性、无重复无遗漏、空历史场景

## Comments