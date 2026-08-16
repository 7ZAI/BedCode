# 05 — 契约 fixtures 工厂

**What to build:** 前端测试数据的单一真源：所有 mock invoke 返回的数据集中到一个 fixtures 工厂，每个 fixture 与对应 Rust DTO 字段级对齐（字段名/类型/可选性，含对齐机制防新增字段漂移），存量前端单元测试从各自手写字面量迁移到工厂取数。

**Blocked by:** None — can start immediately

**Status:** resolved

- [x] fixtures 工厂覆盖：服务器状态/网络配置/指标、配对码/待配设备、会话配置、插件清单，字段与 Rust DTO 一致（文件头注明对应 DTO 源与命名规则）
- [x] 对齐机制可检出字段漂移（新增字段未同步时测试失败或显式告警）
- [x] 存量测试（useServer / usePairing / 相关 store 测试）改用工厂取数，行为不变
- [x] `npm run test:run` 全绿

## Answer

实现于 c0fce1bde（2026-08-16）。`src/__tests__/fixtures/`：13 个 DTO 工厂（server/pairing/session/plugin，含 PendingDevice）+ `assertDtoFields` 运行时键集断言（工厂产出即校验）+ 类型级 `Equals` 编译期断言。机制实际检出并修复 2 处真实漂移：`ServerStatusInfo.uptime_secs`、`WslDistro.is_default/version`。6 个存量测试迁移到工厂取数，vitest 397 全绿、vue-tsc 0 错误。

**评审修正**：消除 drift.test 与工厂内断言的双层重复（it.each 改为调工厂不抛错，键集验证点收敛到工厂内）；补 PendingDevice fixture（无 wire 消费者，纯对齐回归）。
