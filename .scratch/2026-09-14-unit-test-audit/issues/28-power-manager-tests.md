# 28 — power.rs 测试缺口：PowerManager 状态机未覆盖

**What to build:** 为 `PowerManager` 添加状态转换测试，覆盖 enable/disable/is_active/set_enabled 行为。

**背景:** `power.rs` 当前仅有 2 个测试，全部在 `linux_logind` 模块测试 D-Bus 消息构造。`PowerManager` 本身的状态转换逻辑（激活/禁用/用户开关/重复激活保护）零测试。

**参考:** `.scratch/unit-test-audit/system-spec.md` §2

**测试清单:**

- [ ] `is_active()` 初始为 `false`
- [ ] `enable()` 后 `is_active()` 为 `true`
- [ ] `disable()` 后 `is_active()` 为 `false`
- [ ] 重复 `enable()` 第二次 → 不重复激活（`tracing::debug` 跳过路径）
- [ ] `set_enabled(false)` → `is_enabled()` 为 `false`，已激活的自动 `disable()`
- [ ] `set_enabled(true)` 后 `enable()` → 正常工作
- [ ] `set_enabled(false)` 后 `enable()` → 不激活（跳过）

**注意:** 这些测试不需要 mock D-Bus/nosleep — 可以在 `#[cfg(test)]` 中添加测试专用构造函数或使用已有的 `new()` 方法在测试环境中执行（enable/disable 在测试环境会尝试连接但失败，行为可观测）。

**Status:** done（2026-09-15）
