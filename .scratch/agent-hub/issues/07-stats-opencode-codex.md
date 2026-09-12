# 07: 统计补全——opencode SQLite 适配 + codex 预留 + 收尾

**What to build:** opencode 适配器接入：读 `~/.local/share/opencode/opencode.db`（session 表扁平列直读 tokens_input/output/reasoning/cache_read/cache_write 与 cost，epoch ms 时间戳归一），以 db 文件 mtime 触发重扫，进同一套归一 schema——统计看板与会话日志视图自动覆盖第三家；codex 适配器按官方文档格式预留骨架（本机已装未初始化，探测状态正确展示，实机初始化后校准）。收尾：手动触发扫描、空态与授权缺失降级文案（i18n）、数据保留策略（全量保留 + 手动清空）。

**Blocked by:** 06

**Status:** ready-for-agent

- [ ] opencode 真实会话（本机 12 个）进入看板与日志视图；mtime 变更触发增量重扫且幂等
- [ ] codex 适配器骨架 + 「已装 · 未初始化」状态展示正确
- [ ] 空 CLI / 授权拒绝的空态与降级文案走 i18n（zh-CN/en 同步）
- [ ] `pnpm run test:run`、`cargo test`、`pnpm exec eslint .` 0 error 全绿
