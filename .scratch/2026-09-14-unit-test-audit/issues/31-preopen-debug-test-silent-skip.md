# 31 · component.rs / wasm_runtime.rs 测试静默 SKIP

- **优先级**：P1
- **影响模块**：
  - `bedcode-desktop/src-tauri/src/plugin/wasm_runtime/component.rs`（line ~1133、~1148）
  - `bedcode-desktop/src-tauri/src/plugin/wasm_runtime.rs`（line ~1884，`test_debug_mode_trap_includes_line_info`）
- **审计来源**：`.scratch/unit-test-audit/plugin-core-spec.md` §5
- **状态**：done（2026-09-15 修复）

## 问题

三处测试在条件不满足时直接 `return`，不标 `#[ignore]`、不计入失败：

1. `component.rs:1118-1120` `resolve_preopen_dirs_expands_home_variable`
   ```rust
   let Some(home) = dirs::home_dir() else { return; };
   ```
2. `component.rs:1146-1148` `expand_preopen_declarations_expands_and_keeps_ungranted` — 同上
3. `wasm_runtime.rs:1884-1888` `test_debug_mode_trap_includes_line_info`
   ```rust
   if !plugin_debug_mode() {
       eprintln!("SKIP: BEDCODE_PLUGIN_DEBUG 未设置，跳过行号冒烟（调试模式是手工开关）");
       return;
   }
   ```

## 风险

- 测试"通过"但断言未执行，回归不可见
- CI（无 `$HOME` 或 release 构建）下这些测试**永远空跑**
- 与 `unit-test-discipline` 的 G5（无恒真断言）精神相悖

## 建议修复

- 用 `std::env::temp_dir()` 构造伪 HOME 而非依赖 `dirs::home_dir()`（前两条）
- 或改成 `#[ignore]` + 环境变量 gate，显式标记"仅在特定条件下运行"
- 对 `test_debug_mode_trap_includes_line_info`：改为 `#[ignore = "requires BEDCODE_PLUGIN_DEBUG=1"]` + 环境变量触发子进程模式，或从 CI 单独跑
- 若坚持静默 SKIP，至少在测试末尾加一个**计数断言**（如 `assert!(TEST_SKIPPED_COUNTER.swap(false), "skip path is a no-op test")`）——反例：`SKIP` 打印到 stdout 但不出现在结果统计中

## 验收

- [ ] `cargo test --lib plugin::wasm_runtime::tests` 输出中 SKIP 计数可见
- [ ] 无 `$HOME` 环境下 `resolve_preopen_dirs_expands_home_variable` 仍能实际执行断言
- [ ] `test_debug_mode_trap_includes_line_info` 在无 `BEDCODE_PLUGIN_DEBUG` 时要么 `#[ignore]`、要么走 CI 显式子进程验证
