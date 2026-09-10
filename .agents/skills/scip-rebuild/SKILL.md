---
name: scip-rebuild
description: |
  Rebuild BedCode's SCIP Rust index (type-precise def/ref queries, zero resident memory) when it goes stale.
  Use before reference-dense Rust work — impact assessment before refactoring, listing callers or
  definitions of a symbol after edits — when scipq stale or scipq rebuild-if-stale reports an outdated
  index, or once per session as the fixed-interval refresh.
---

# SCIP 索引重建

`rust-analyzer scip` 生成的类型精确代码引用索引（`scipq` 查询，零常驻内存）。索引是**快照**，
代码变更后引用数据就 stale 了；本 skill 覆盖「何时重建 + 怎么重建 + 怎么验证」。
查询用法（`syms` / `defs` / `refs` / `refs-exact` 等）见 `.pi-lens/scip/README.md`，本 skill 不管。

## 什么时候重建（触发分支）

| 场景 | 动作 |
| --- | --- |
| 引用密集任务前（重构前列调用方、评估影响面、刚改完 Rust 代码要查 def/ref） | 先 `scipq stale` 检查，stale 才重建 |
| 会话开始（固定间隔刷新） | `scipq rebuild-if-stale [hours]` 一次，fresh 则零成本跳过 |
| `scipq stale` / `rebuild-if-stale` 报过期 | 按下方流程重建 |

## 流程

1. **检查新鲜度**：`scipq stale [hours]`（默认 24h）——输出 desktop / mobile 两个 workspace
   各自的索引年龄与最近源码改动时间，**exit 0 = fresh，exit 1 = stale**（无索引也报 stale）。
   → 完成标准：明确知道两个 workspace 各自是 fresh 还是 stale。

2. **按策略刷新**（stale 才执行，二选一）：
   - 固定间隔：`scipq rebuild-if-stale [hours]` —— 间隔内跳过，超龄自动调 `rebuild.sh` 重建。
   - 按需强制：`scipq rebuild` —— 等价直接跑 `.pi-lens/scip/rebuild.sh`，两个 workspace 全量重建。
   → 完成标准：两个 workspace 各打印 `indexing` 与 `converting to sqlite` 且脚本 exit 0。

3. **验证**：`scipq stats` 计数正常（documents / symbols / mentions 非零），再 `scipq stale` 应报
   fresh（exit 0）。日常抽查可用 `scipq defs <符号>` 命中刚改过的代码。
   → 完成标准：desktop / mobile 的 `*.db` 均更新，stale 检查 fresh，查询能命中新符号。

## 成本模型（参考）

- 每次重建两个 workspace 都全量跑：各一次 `rust-analyzer scip`（约 1 分钟、内存峰值 2-4GB、
  **进程即退**）+ 一次 `scip expt-convert`（秒级）；重建后所有查询零常驻内存。
- 没有文件监听 hook、不会每次代码变更自动重建——所以引用密集工作前主动检查/刷新
  （重建本身成本高，别为了每次小改动频繁跑）。
- 底层脚本 `.pi-lens/scip/rebuild.sh`（`scipq rebuild` 就是调它），个别需求可直接执行。
