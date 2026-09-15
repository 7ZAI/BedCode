# 21 — 修复 DB 迁移测试零守卫力（变异测试全存活）

**What to build:** 修复 `database.rs` 的 4 个迁移测试，使其对迁移代码破坏有真实守卫力（当前变异测试证明 4 个测试在迁移被完全破坏后仍全绿）。

**Blocked by:** 无

**Status:** done（2026-09-15 修复）

- [ ] 验证 `init_schema()` 在测试中实际被调用（当前可能测试在初始化时就创建了正确库，迁移路径未执行）
- [ ] `old_db_is_migrated_and_preserves_data`：构造真正的旧库（缺失新列/约束），断言迁移后旧数据保留 + 新列存在
- [ ] `migration_is_idempotent`：对已迁移库重复调用 `init_schema()`，断言无错误 + 数据不变
- [ ] `fresh_db_uses_new_check_constraint_with_linux`：断言 CHECK 约束存在且值合法
- [ ] `migrated_db_accepts_linux_value`：断言 `linux` 值被接受、非法值被拒绝
- [ ] 变异验证：破坏 `run_migrations()` 后至少 1 个测试失败
- [ ] `cargo test --lib db::` 通过

## 证据

变异测试：破坏 `database.rs` 的 `init_schema()` / `run_migrations()` → **7 个测试全部 PASS**，因为 4 个迁移测试全部走私有 helper `init_and_migrate()`（`database.rs:124-172`）——它用 `execute_batch(include_str!("schema.sql"))` + 手抄的 pairings 列迁移/CHECK 约束迁移逻辑，**从不调用生产 `run_migrations()`**。即测试验证的是「新建/temp 库正确」+「复制版迁移逻辑正确」，生产迁移代码被破坏时测试无法感知。

## 根因

测试用私有 helper `init_and_migrate()` 复制了生产迁移逻辑（schema.sql 全量执行 + pairings 列迁移 + session_configs CHECK 迁移），并直接对该复制结果断言。生产 `init_schema()` / `run_migrations()` 的迁移路径从未被测试触达。部分测试（如 `old_db_is_migrated_and_preserves_data`）构造了含旧约束的真实老库，但迁移仍由复制版 helper 执行，与生产实现解耦。

## 修复方向

1. 构造旧库：手动创建缺失新列/约束的 SQLite 库，再调用 `init_schema()` 验证迁移
2. 断言迁移后 schema 变化（列存在、约束存在、数据保留）
3. 对已迁移库重复调用 `init_schema()` 验证幂等性
4. 变异验证确保测试有守卫力

## 影响面

仅新增/修改测试，零生产代码改动。

## Comments

- 2026-09-14 审计发现，见 `../db-spec.md` §5.1
- 变异测试由子 agent 执行，全部变异体已回滚
