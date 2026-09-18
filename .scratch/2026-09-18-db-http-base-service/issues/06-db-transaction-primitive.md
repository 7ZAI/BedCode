# 06 — DB 事务原语 `execute-batch`（杜绝跨调用裸事务）

**What to build:** 修复审计发现 D2（内核数据完整性风险）。为 DB 域新增事务原语 `execute-batch`（主库与插件独立库各一）：单次调用内宿主持有连接锁、事务内顺序执行多语句，任一句失败整体回滚，返回受影响行数合计。WIT 为接口增量追加（不 bump ABI 版本，属函数级追加；实现时确认既有 ABI 递增惯例）。

**配套防线：** 文档化「禁止跨调用裸 BEGIN/COMMIT/SAVEPOINT」；如实现成本可控，对 `execute`/`execute-params` 做事务控制语句白名单检测（完整 SQL 以 BEGIN/SAVEPOINT/COMMIT/ROLLBACK 类开头/结尾 → 拒绝并引导 `execute-batch`）。此防线是 D2 完整性关键：当前连接 Mutex 按语句释放，插件裸事务窗口内**内核自己的写入会插进插件事务**，插件 ROLLBACK 会一并丢弃内核写入。

**注意边界：** 事务持有连接期间其他插件/内核调用会等待——`execute-batch` 必须有语句数/总耗时兜底（复用 05 的超时护栏，避免长事务阻塞内核）。05 与 06 互相独立但实现顺序建议 05 先行（护栏兜底事务面）。

**Blocked by:** None（建议排在 05 后开工）。

**Status:** `ready-for-agent`

- [x] `execute-batch`（主库 + 插件库）：事务内多语句原子执行 + 失败回滚 + 影响行数合计
- [x] 事务控制语句白名单检测（拒绝裸 BEGIN/COMMIT 跨调用模式）+ 单测正反例
- [x] 事务超时/语句数兜底（复用 05 护栏）+ 正反例单测
- [x] WIT + SDK（guest 绑定 + TS 面）函数级追加同步；ABI 惯例确认
- [x] 单测：多语句原子可见（成功全提交 / 任一失败全回滚）；跨调用裸事务被拒
- [x] 桌面 cargo test 全绿

## Comments