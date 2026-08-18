# 07 — 桌面本地通道迁移：TB v2 + 快照重订阅

**What to build:** 桌面端本地终端消费链路（`/ws/terminal/local` + `useTerminalOutputStream.ts` + `TerminalPreview.vue`）从字节游标模型迁移到快照模型：帧头解析升级 TB v2（seq）；游标 `cursor` → `last_rendered_seq`；forceResubscribe 增量重订阅 → 快照重订阅（跳过 ≤ last_rendered_seq）；`onTruncated(min_offset)` → `onTruncated(min_seq)`。写入管线（rAF 合并 + DEC 2026 + 64KB 拆块）不动。

**Spec:** §5.3、§5.4、§5.5、§5.6（验收 4/5）

**Blocked by:** 06

**Status:** done（已提交 4aa0a0b4）

- [x] TB v2 帧解析 + 连续性校验改 seq
- [x] 快照重订阅路径（跳过已渲染、截断提示语义）
- [x] TerminalPreview 接线调整
- [x] 测试：useTerminalOutputStream 单元（帧解析/缺口/重订阅/截断）+ 桌面终端回归

## Comments