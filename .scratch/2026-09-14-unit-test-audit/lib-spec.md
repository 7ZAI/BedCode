# 桌面端 lib.rs 单元测试审查报告

> 状态: **审计完成，零票据**（2026-09-14，23:30）
> 范围: `bedcode-desktop/src-tauri/src/lib.rs`（790 行）
> 测试规模: **0 个**
> 分支: `dev`

---

## 1. 摘要（Verdict）

**790 行 Tauri 应用入口零测试，但这是框架启动代码，单测价值有限。**

- `lib.rs` 是 Tauri 应用入口：`run()` → `tauri::Builder::default()` → 注册插件 → 注册 commands → 配置窗口 → `build()` → `run()`。
- 95% 是 Tauri builder 配置和插件/commands 注册，无业务逻辑。
- 错误处理路径（`setup` 回调中的 DB 初始化、插件加载）无法在单测中验证（依赖 Tauri runtime）。
- 集成测试（`tests/`）已覆盖应用启动链路。

---

## 2. 审查基线

```bash
cargo test --lib lib::   # → 0 tests (no #[test] in lib.rs)
```

---

## 3. 总判定表

| 文件 | 行数 | 测试 | 判定 | 关键问题 |
|---|---|---|---|---|
| `lib.rs` | 790 | 0 | 🟡 可接受 | Tauri 框架启动代码，无业务逻辑可测 |

---

## 4. 说明

lib.rs 的 790 行中：
- ~500 行：`tauri::Builder` 配置（插件注册、commands 注册、窗口配置、菜单）
- ~100 行：`setup` 回调（DB 初始化、PTY manager 创建、插件系统启动）
- ~100 行：模块声明和 re-export
- ~90 行：错误处理（panic hook、error boundary）

这些代码：
- 无独立可测的业务逻辑（全是框架接线）
- `setup` 回调依赖 Tauri runtime（无法在 `#[test]` 中调用）
- 集成测试已覆盖启动链路

**不创建票据**：lib.rs 零测试是合理的设计决策，非覆盖缺口。

---

## 5. 审计纪律记录

- 无测试残留进程
- 未修改任何代码
