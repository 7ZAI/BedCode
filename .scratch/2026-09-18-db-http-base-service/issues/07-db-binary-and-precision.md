# 07 — DB 二进制与 i64 精度限制文档化（低优先级，无 ABI 变更）

**What to build:** 审计发现 D3/D4 的最低成本处理——**不改 WIT / 不新增 ABI 面**，把两条既有限制固化为文档化契约：

1. **BLOB 二进制**：宿主查询 BLOB 列输出 hex 字符串、参数绑定无字节数组类型（数组/对象参数 fallback 为 JSON 字符串）。插件需要二进制存储时只能 hex 存 TEXT 或走 host-fs 文件。此限制写入 SDK 注释/SDK 文档（含 host-database 与 host-plugin-database 的 guest trait 文档，及 WASM 插件指南类文档）。
2. **i64 精度**：查询 i64 → JSON number → TS 侧双精度丢精度（>2^53）。文档化建议：插件主键/大整数用 TEXT 或自增整数，不依赖 >2^53 的 JSON number 往返。

真实的二进制参数/字节数组支持（bytes 类型）**不做**：涉及 WIT ABI 变更，留到与 host-websocket（WS 票据）的 ABI 合并窗口一并评估，避免双 bump。

**Blocked by:** None — 纯文档任务。

**Status:** `ready-for-agent`

- [x] SDK 文档/注释补充 BLOB hex 输出与参数绑定类型限制（desktop SDK；移动端 SDK 若同构同步标注，不改契约）
- [x] i64 精度限制文档化 + 主键建议（TEXT / 自增整数）
- [x] 核对既有文档（knowledge/插件开发指南类）是否有 DB 能力描述需同步
- [x] 无代码行为变更（仅文档）；eslint 无影响，cargo 无需重跑（若改动涉及 SDK crate 注释，跑对应 cargo check）

## Comments