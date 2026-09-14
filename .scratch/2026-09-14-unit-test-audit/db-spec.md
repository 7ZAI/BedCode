# 桌面端 DB 模块单元测试审查报告

> 状态: **审计完成，1 张修复票据待处理**（2026-09-14，23:00）
> 范围: `bedcode-desktop/src-tauri/src/db/`（3 文件 + schema.sql）
> 测试规模: **7 个**（4 `#[test]` + 3 `#[test]`）
> 分支: `dev`

---

## 1. 摘要（Verdict）

**7 个测试对迁移/schema 正确性的守卫力为零。**

- 变异测试实锤：破坏生产迁移函数、schema.sql、operations.rs、prune cap → **7 个测试全部 PASS**。测试名声称「迁移幂等」「旧库迁移保留数据」，但实际无守卫力。
- `models.rs`（157 行）零测试。
- `operations.rs`（623 行，25+ pub 方法）仅 3 测试，覆盖 `add_pairing` 合并逻辑和 `close_open_connection_event_by_fingerprint`，其余 22+ 方法零覆盖。
- 6 张表、79 行 schema.sql 无回归锁。

---

## 2. 审查基线

```bash
cd bedcode-desktop/src-tauri
cargo test --lib db::   # → 7 passed; finished in ~0.05s
```

---

## 3. 审查方法

1. **实跑基线**：7 测试全绿。
2. **变异测试**：改坏生产代码 → 观察测试是否变红。
   - 变异 A：破坏 `database.rs` 的 `init_schema()` / `run_migrations()` → 7 测试全绿
   - 变异 B：破坏 `schema.sql` 的 CREATE TABLE 定义 → 7 测试全绿
   - 变异 C：破坏 `operations.rs` 的 prune cap 逻辑 → 7 测试全绿
   - **结论：所有变异体均存活**，测试对迁移/schema/操作正确性零守卫力。
3. **代码阅读**：确认测试断言仅覆盖 `add_pairing` 的合并行为（uid_hash 分组合并/分离）和 `close_open_connection_event_by_fingerprint` 的 disconnect 回填，其余无覆盖。

> 变异体已在审查结束前完全回滚，`git diff --stat src/db/` 为空。

---

## 4. 总判定表

| 文件 | 行数 | 测试数 | 判定 | 关键问题 |
|---|---|---|---|---|
| `database.rs` | 286 | 4 | 🔴 **形同虚设** | 4 个迁移测试对迁移破坏零守卫力 |
| `operations.rs` | 623 | 3 | 🟡 部分有效 | 3 测试覆盖 2 个方法；25+ pub 方法中 22+ 零覆盖 |
| `models.rs` | 157 | 0 | 🔴 零测试 | 纯类型定义，风险较低 |
| `schema.sql` | 79 | 0 | 🔴 零测试 | 6 张表无回归锁 |

---

## 5. 逐文件结论

### 5.1 `database.rs` — 🔴 4 个迁移测试形同虚设

4 个测试：`fresh_db_uses_new_check_constraint_with_linux`、`old_db_is_migrated_and_preserves_data`、`migration_is_idempotent`、`migrated_db_accepts_linux_value`。

变异测试证明：破坏 `init_schema()` / `run_migrations()` 后 4 个测试全绿。测试名声称「迁移幂等」「旧库保留数据」，但实际断言未覆盖迁移执行路径——测试可能在初始化时就创建了正确的库，迁移代码根本没被执行到。

### 5.2 `operations.rs` — 🟡 部分有效

3 个测试覆盖：
- `add_pairing` 合并逻辑：`add_pairing_merges_by_uid_hash_when_fingerprint_changes` + `add_pairing_keeps_distinct_uid_hashes_separate`
- `close_open_connection_event_by_fingerprint`：`close_open_connection_event_by_fingerprint_backfills_disconnect`

25+ pub 方法中 22+ 零覆盖，包括：`update_pairing_last_seen`、`update_pairing_token`、`verify_session_token`、`remove_pairing`、`verify_pairing`、`record_connection_event`、`create_session_config`、`get_session_configs`、`delete_session_config`、`update_session_config`、`get_quick_actions` 等。

---

## 6. 修复优先级

| 优先级 | 票据 | 内容 |
|---|---|---|
| P0 | 21 | 修复迁移测试零守卫力（变异测试验证） |

---

## 7. 审计纪律记录

1. **变异测试已回滚**：`git diff --stat src/db/` 为空。
2. **所有行号从磁盘读取**（bash），非 read 工具快照。
3. **测试残留进程**：cargo test 完成后无后台进程残留。
