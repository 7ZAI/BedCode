# 30 · api_bridge.rs 权限门禁无测试覆盖

- **优先级**：P0
- **影响模块**：`bedcode-desktop/src-tauri/src/plugin/api_bridge.rs`
- **审计来源**：`.scratch/unit-test-audit/plugin-core-spec.md` §4
- **状态**：done（2026-09-15 修复）

## 背景

`api_bridge.rs` 382 行 / 20+ Tauri `#[command]` 包装 / **仅 1 个测试**（`frontend_load_report_always_ok`，仅覆盖无 `State` 参数的诊断命令）。文件内测试注释（line 341-365）解释"State 无法在单测中构造"，但该解释低估了真实缺口：**storage/terminal_send_input 的权限门禁是桥接层独有的生产逻辑**，不是 host.rs 的透传。

## 未覆盖的生产逻辑

| 命令 | 未覆盖 |
| --- | --- |
| `plugin_storage_get/set/delete` | `is_activated` 检查 + `permission().check("storage")` 门禁 + 错误消息字符串 |
| `plugin_terminal_send_input` | 同上（`terminal:input`）+ `AppContext::global()` 依赖 |
| `plugin_dev_reload` | `#[cfg(debug_assertions)]` / `#[cfg(not(debug_assertions))]` 两分支均无测 |
| `plugin_install_from_file` / `plugin_preauthorize` / `plugin_activate` / `plugin_deactivate` / `plugin_uninstall` / `plugin_mark_error` | 错误日志分支的字段/级别无断言 |

风险：门禁被误删/短路时 CI 不报警；权限绕过可能上线。

## 建议修复

**方案 A（推荐，最小改动）**：抽取纯函数后单测

```rust
fn check_storage_permission(host: &Arc<PluginHost>, plugin_id: &str) -> crate::Result<()> { ... }
fn check_terminal_input_permission(host: &Arc<PluginHost>, plugin_id: &str) -> crate::Result<()> { ... }
```

然后为两个函数写 4 分支测试（激活/未激活 × 有/无权限），桥接层退化为薄封装。

**方案 B**：为 `plugin_dev_reload` 的生产分支提取错误消息常量，单测常量本身。

**方案 C**：`Cargo.toml` 启用 `tauri` 的 `test` feature + `mock_builder`——改动面大，暂不推荐（AGENTS.md §5 最小改动）。

## 验收

- [ ] `plugin_storage_*` 三命令的 `is_activated` + `permission().check` 门禁被纯函数单测覆盖（4 分支）
- [ ] `plugin_terminal_send_input` 的 `terminal:input` 门禁被纯函数单测覆盖（4 分支）
- [ ] `plugin_dev_reload` 生产分支错误消息有常量+单测
- [ ] `cargo test --lib plugin::api_bridge` 通过
- [ ] `pnpm exec eslint .` 0 error（如涉及前端契约不变）
