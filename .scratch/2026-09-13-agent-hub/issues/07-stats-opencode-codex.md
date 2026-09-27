# 07: 统计补全——opencode SQLite 适配 + codex 预留 + 收尾

**What to build:** opencode 适配器接入：读 `~/.local/share/opencode/opencode.db`（session 表扁平列直读 tokens_input/output/reasoning/cache_read/cache_write 与 cost，epoch ms 时间戳归一），以 db 文件 mtime 触发重扫，进同一套归一 schema——统计看板与会话日志视图自动覆盖第三家；codex 适配器按官方文档格式预留骨架（本机已装未初始化，探测状态正确展示，实机初始化后校准）。收尾：手动触发扫描、空态与授权缺失降级文案（i18n）、数据保留策略（全量保留 + 手动清空）。

**Blocked by:** 06

**Status:** resolved（2026-09-27 实施，方案 A 补齐）

- [x] opencode 真实会话（本机 54 个）进入看板与日志视图；库文件变更触发增量重扫且幂等
- [x] codex 适配器骨架 + 「已装 · 未初始化」状态展示正确
- [x] 空 CLI / 授权拒绝 / 缺 `sqlite3` / 库不存在的空态与降级文案走 i18n（zh-CN/en 同步）
- [x] `pnpm run test:run`、`cargo test`、`pnpm exec eslint .` 0 error 全绿

## Answer（2026-09-27）

实施记录见子票 [`.scratch/2026-09-27-agent-hub-audit-fixes/issues/11-ticket07-opencode-codex.md`](../../2026-09-27-agent-hub-audit-fixes/issues/11-ticket07-opencode-codex.md) 的
「Answer」小节（通道选择 / 分页与截断策略 / 四条实况处置 / 变异自检 / 未覆盖风险）。

要点留档：

- **§2「使用统计」列为 v1、§4.5「opencode = SQLite」的承诺已兑现**（`ADAPTERS` 由
  `["claude","pi"]` 变为四家），母 spec §2 **未降级**。
- **SQLite 读取通道**：复用插件既有 `host-process` 同步跑 `sqlite3 -json -readonly`
  （新增 WIT 原语要 ABI bump + 双端同步，超出该轮边界）。`session` 表扁平列与
  `usage_session` 一对一，事件流从 `message × part` 联表现查。
- **§9 待实机校准项更新**：opencode 四条实况已按实机处理（`model` 是 JSON 串 /
  `cost` 全 0 / 9 个零 token 会话 / epoch ms）。codex 会话格式**仍未校准**（本机
  0 个 rollout 文件），骨架依据官方 rollout 文档实现——§10 待办 3 保持 open。
- **数据保留策略**：全量保留不自动过期 + 手动清空（`agent-hub.clear-usage-data`，
  两表同事务）。
