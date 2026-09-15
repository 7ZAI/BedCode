# 桌面端 Enums 模块单元测试审查报告

> 状态: **审计完成，1 张修复票据待处理**（2026-09-14，23:30）
> 范围: `bedcode-desktop/src-tauri/src/enums/`（9 文件，1440 行）
> 测试规模: **35 个**
> 分支: `dev`

---

## 1. 摘要（Verdict）

**测试高度集中在 `special_key.rs`，其余 8 文件零测试。**

- `special_key.rs`（828 行，32 测试）：全模块测试的 91%，覆盖 serde 序列化、legacy 格式解析、修饰键组合、ANSI 转义序列。质量尚可。
- `session.rs`（96 行，2 测试）：覆盖 `TaskStatus` 默认值和 serde 往返。
- `sync.rs`（116 行，1 测试）：覆盖 `SyncType` 单一测试。
- **6 个文件零测试**：`auth.rs`(101)、`control.rs`(128)、`plugin.rs`(6)、`pty_status.rs`(15)、`shell.rs`(102)、`summary.rs`(48)。

---

## 2. 审查基线

```bash
cargo test --lib enums::   # → 35 passed
```

---

## 3. 总判定表

| 文件 | 行数 | 测试 | 判定 | 关键问题 |
|---|---|---|---|---|
| `special_key.rs` | 828 | 32 | 🟢 有效 | 覆盖全面：serde/legacy/修饰键/ANSI 转义 |
| `session.rs` | 96 | 2 | 🟡 部分 | 仅 TaskStatus；其余 variant 未测 |
| `sync.rs` | 116 | 1 | 🟡 部分 | 仅 SyncType；其余类型未测 |
| `auth.rs` | 101 | 0 | 🔴 零测试 | AuthStage/AuthPayload 零覆盖 |
| `control.rs` | 128 | 0 | 🔴 零测试 | SessionControlAction 零覆盖 |
| `shell.rs` | 102 | 0 | 🔴 零测试 | ShellType/ShellConfig 零覆盖 |
| `summary.rs` | 48 | 0 | 🔴 零测试 | SessionSummary 零覆盖 |
| `pty_status.rs` | 15 | 0 | 🟡 可接受 | 纯枚举，6 variant |
| `plugin.rs` | 6 | 0 | 🟡 可接受 | 单 variant 枚举 |

---

## 4. 修复优先级

| 优先级 | 票据 | 内容 |
|---|---|---|
| P2 | 23 | auth.rs + control.rs + shell.rs 补 serde 往返测试 |

---

## 5. 审计纪律记录

- 测试计数：35（grep 精确匹配）
- 零测试文件：6（>50 行的有 4 个：auth/control/shell/summary）
