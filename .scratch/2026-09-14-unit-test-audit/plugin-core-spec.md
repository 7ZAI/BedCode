# 单元测试审计 — plugin 核心模块

- 审计范围：`bedcode-desktop/src-tauri/src/plugin/` 6 个核心文件
- 审计时间：2026-09-14
- 生产代码未修改（`git diff --stat -- src/plugin/` 空）

---

## 1. 摘要（TL;DR）

- 基线：`cargo test --lib "plugin::"` → **248 passed / 0 failed / 1.13s**
- 6 个核心文件共约 **85 个测试**，覆盖分支/错误路径整体扎实（host.rs、registry.rs、message_bus.rs、wasm_runtime.rs、component.rs 均达标）
- **1 个 P0 缺口**：`api_bridge.rs`（382 行 1 测试）—— 3 个 storage 命令 + 1 个 terminal_send_input 命令里的**权限门禁逻辑**（`is_activated` + `permission().check`）只写在此桥接层，未被任何测试断言；改动能静默通过 CI
- **2 个 P1 缺口**：component.rs / wasm_runtime.rs 中若干测试在 `dirs::home_dir()` 返回 None 或 `BEDCODE_PLUGIN_DEBUG` 未设置时**静默 return**，无 `#[ignore]`/`.skip` 计数，回归时不可见

---

## 2. 基线命令与结果

```bash
$ cd bedcode-desktop/src-tauri
$ cargo test --lib "plugin::"
test result: ok. 248 passed; 0 failed; 0 ignored; 0 measured; 367 filtered out; finished in 1.13s
```

`git status --short -- bedcode-desktop/src-tauri/src/plugin/` → 空（干净）。

---

## 3. 逐文件判定表

| 文件 | 行数 | 测试数 | assert 语句 | 判定 | 关键缺口 |
| --- | ---: | ---: | ---: | :---: | --- |
| `host.rs` | 3011 | 34（+ host_impl/* 分模块） | 127 | ✅ 优 | 个别错误消息仅 `contains()`，措辞改动会误判 |
| `wasm_runtime.rs` | 2084 | 17（+ host_impl/*） | 62 | ✅ 优 | `test_debug_mode_trap_includes_line_info` 无 `BEDCODE_PLUGIN_DEBUG` 时**静默 return**（P1） |
| `wasm_runtime/component.rs` | 1227 | 10 | 21 | ✅ 优 | `resolve_preopen_dirs_expands_home_variable` / `expand_preopen_declarations_*` 在 `home_dir()==None` 时**静默 return**（P1）；`LoadedWasmPlugin::new` ABI/激活失败路径无测 |
| **`api_bridge.rs`** | 382 | **1** | **2** | ❌ **差** | 见 §4 |
| `registry.rs` | 541 | 10 | 49 | ✅ 优 | — |
| `message_bus.rs` | 501 | 13 | 22 | ✅ 优 | 无并发生序测试（可接受） |

判定准则（`unit-test-discipline` 精神）：分支覆盖、错误路径、断言强度、无恒真断言、无只测 mock。

---

## 4. api_bridge.rs（P0 缺口）

**现状**：382 行代码 / 20+ Tauri `#[command]` 包装 / 1 个测试（`frontend_load_report_always_ok`，仅 2 行 `is_ok()`）。

**测试注释（line 341-365）说明**：几乎所有 command 的最后一个参数是 `State<'_, Arc<PluginHost>>`，`State` 无 `From<T>`、构造需要 `StateManager`，因此"不硬造测试"。这一解释部分合理，但**低估了真实的覆盖缺口**：

### 4.1 未被覆盖的生产逻辑

| 命令 | 未覆盖的生产逻辑 | 风险 |
| --- | --- | --- |
| `plugin_storage_get/set/delete` | `is_activated` 检查 + `permission().check("storage")` 门禁 + 两条错误消息字符串（`"Plugin {} is not activated"` / `"Plugin {} has no storage permission"`） | 门禁被误删/短路时 CI 不报警；权限绕过可能上线 |
| `plugin_terminal_send_input` | 同上（`terminal:input`） + `AppContext::global().session_manager()` 调用链 | 未激活插件可能被注入输入 |
| `plugin_dev_reload` | `#[cfg(debug_assertions)]` / `#[cfg(not(debug_assertions))]` **两个分支均无测试** | 生产环境调用路径未验证返回错误 |
| `plugin_preauthorize/activate/deactivate/uninstall/mark_error` | 错误日志分支（`tracing::error!`）—— 无日志级别/字段断言 | 无法从测试确认门禁日志可观测性 |
| `plugin_install_from_file` | 错误日志、`AppError` 传播 | 同上 |
| `plugin_fs_auth_respond` | 纯委托给 `FsAuthChecker::respond`，**桥接层无独立逻辑**——可接受无测 | — |
| `plugin_list_loaded/get_info/activate_state/list_commands/...` | 纯委托、无独立逻辑 | 由 host.rs / registry.rs 覆盖，可接受 |

### 4.2 具体测试缺口

- **storage 门禁错误字符串**（`"not activated"` / `"has no storage permission"`）在 host.rs 中有语义等价断言，但**桥接层字符串本身**未被任何测试比对；改措辞会静默通过
- **terminal_send_input**：即使启用 tauri test feature，`AppContext::global()` 无头测试中会 panic；建议**提取纯门禁函数**（e.g. `check_terminal_input_permission(host, plugin_id)`）后单测，而非依赖 Tauri runtime
- **`plugin_dev_reload` 生产分支**（`Err("Hot reload only available in dev mode")`）从未被测试触发，因 release 构建下该 `#[cfg]` 分支编译掉；建议用编译时 flag 或把错误消息提取到常量后单测字符串

### 4.3 修复路径（推荐）

1. **最小改动**：在 `api_bridge.rs` 顶部抽取纯函数
   ```rust
   fn check_storage_permission(host: &Arc<PluginHost>, plugin_id: &str) -> crate::Result<()> { ... }
   fn check_terminal_input_permission(host: &Arc<PluginHost>, plugin_id: &str) -> crate::Result<()> { ... }
   ```
   然后单测两个函数（激活/未激活 × 有/无权限 4 分支），桥接层退化为薄封装
2. **次选**：`Cargo.toml` 增加 `tauri` 的 `test` feature，用 `mock_builder` 构造 `State`（改动大、影响面广，AGENTS.md §5 谨慎）
3. **`plugin_dev_reload` 生产分支**：把错误消息提取常量 `const RELOAD_DISABLED_MSG: &str = "Hot reload only available in dev mode";`，单测常量

Issue 已创建：`issues/30-api-bridge-permission-gates-tests.md`

---

## 5. component.rs 与 wasm_runtime.rs 的静默 SKIP（P1）

### 5.1 现状

`wasm_runtime/component.rs` 中：
- `resolve_preopen_dirs_expands_home_variable`（line ~1133）：`if let Some(home) = dirs::home_dir() else { return; }`
- `expand_preopen_declarations_expands_and_keeps_ungranted`（line ~1148）：同样 `if let Some(home) = ... else { return; }`

`wasm_runtime.rs` 中：
- `test_debug_mode_trap_includes_line_info`（line 1884）：`if !plugin_debug_mode() { eprintln!("SKIP: ..."); return; }`

### 5.2 风险

- 测试全部"通过"，但实际**未执行**关键断言（`assert_eq!(out.len(), 1)` / `assert!(err.contains("file:line"))`）
- 无 `#[ignore]` 标记，`cargo test` 输出中不显示，容易被忽略
- CI 环境（无 `$HOME`、release 构建无 debug profile）下这些测试可能**一直空跑**

### 5.3 修复路径

- 用 `#[ignore]` + 环境变量 gate 或 `should_panic`/显式 skip 计数
- 或使用 `std::env::temp_dir()` 构造伪 HOME 而非依赖 `dirs::home_dir()`
- 至少加一条 `eprintln!("SKIP")` 计数（当前已有，但没在输出中被醒目展示）

Issue 已创建：`issues/31-preopen-debug-test-silent-skip.md`

---

## 6. 修复优先级

| 优先级 | 项 | 文件 | 建议工作量 |
| --- | --- | --- | --- |
| **P0** | api_bridge 权限门禁测试 | `api_bridge.rs` | 中等（抽纯函数 + 4 分支单测） |
| P1 | 静默 SKIP 改造 | `component.rs` + `wasm_runtime.rs` | 小（改 3 处测试） |
| P2 | `LoadedWasmPlugin::new` ABI 不匹配 / activate 失败路径 | `component.rs` | 小-中 |
| P2 | host.rs 错误字符串硬编码 → 提取常量后比对 | `host.rs` | 中（改动面大） |

---

## 7. 结论

- **整体测试质量良好**：5/6 文件覆盖扎实，`host.rs` / `registry.rs` / `message_bus.rs` 可作为团队其他模块的参考
- **1 处硬缺口**：`api_bridge.rs` 的权限门禁是真正的生产逻辑（不是薄透传），必须补测
- **2 处隐性风险**：静默 SKIP 让 P1 级测试在某些 CI 环境下"永远通过"，需改造
